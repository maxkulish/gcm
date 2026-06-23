//! Two-stage secret detection pipeline (CLO-514, FR-60).
//!
//! 1. **Precise provider rules** - the vendored [`RuleEngine`]: a `RegexSet`
//!    one-pass prefilter selects candidate rules, each rule's `Regex` extracts
//!    the secret group, gated by the rule's optional raw-Shannon entropy and
//!    `min_digits`. Precise rules fire everywhere (the path ignore-set never
//!    suppresses them).
//! 2. **Generic assignment + entropy detector** - catches `IDENT = value` /
//!    `IDENT: value` for *any* identifier (the `GITLAB="..."` fix). The value
//!    is accepted only if it clears a length gate and a charset-aware
//!    *normalized* entropy floor; a sensitive-keyword identifier uses a lower
//!    threshold fast path. Structural false positives (UUID, git/SHA hex) are
//!    suppressed unless a sensitive keyword is adjacent.
//!
//! Both stages return byte ranges into the original text, on `char`
//! boundaries, merged so overlapping detections collapse to one redaction.

use std::ops::Range;
use std::sync::OnceLock;

use regex::Regex;

use super::entropy::{normalized_entropy, shannon_entropy, Charset};
use super::rules::RuleEngine;

/// Minimum value length before the generic detector computes entropy.
const GENERIC_MIN_LEN: usize = 16;
/// Shorter values are still accepted when the identifier is a sensitive
/// keyword (keyword fast path), down to this floor.
const KEYWORD_MIN_LEN: usize = 8;

/// Identifier substrings that mark a value as security-sensitive, lowering
/// the bar. Keep this aligned with the legacy assignment detector's allowlist
/// so the keyword fast path preserves AC7 without widening to broad fragments
/// like `key`, `api`, or `auth` (for example, `monkey` is not sensitive).
const SENSITIVE_KEYWORDS: &[&str] = &[
    "api_key",
    "apikey",
    "access_key",
    "secret",
    "token",
    "password",
    "private_key",
];

/// Inline pragmas that mute detection on a line.
const PRAGMAS: &[&str] = &["# gcm:allow", "// gcm:allow"];

/// All detected secret spans in `text`, merged and sorted.
pub fn secret_ranges(text: &str, engine: &RuleEngine) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    ranges.extend(precise_ranges(text, engine));
    ranges.extend(generic_ranges(text));
    drop_pragma_ranges(text, &mut ranges);
    merge_ranges(ranges)
}

/// Redact every detected span with a fixed marker.
pub fn redact_secrets(text: &str, engine: &RuleEngine) -> String {
    let ranges = secret_ranges(text, engine);
    if ranges.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for range in ranges {
        if range.start < cursor {
            continue;
        }
        out.push_str(&text[cursor..range.start]);
        out.push_str("[REDACTED: secret]");
        cursor = range.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// Stage 1: precise provider rules over the whole text.
fn precise_ranges(text: &str, engine: &RuleEngine) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    for rule in engine.matching_rules(text) {
        for caps in rule.regex.captures_iter(text) {
            // Test capture group 1 if the rule defines one, else the whole match.
            let m = caps.get(1).or_else(|| caps.get(0)).unwrap();
            let value = m.as_str();
            if let Some(min_h) = rule.entropy {
                if shannon_entropy(value) < min_h {
                    continue;
                }
            }
            if let Some(min_d) = rule.min_digits {
                if value.chars().filter(|c| c.is_ascii_digit()).count() < min_d as usize {
                    continue;
                }
            }
            out.push(m.start()..m.end());
        }
    }
    out
}

/// Stage 2: generic assignment + entropy detector, line by line.
///
/// Two passes:
/// 1. **Line-start assignment** — `IDENT = value` / `IDENT: value` anchored to
///    line start (after diff `+`/`-`/indent). Catches `GITLAB="…"` (AC1).
/// 2. **Keyword-anywhere compatibility** — scans for `SENSITIVE_KEYWORDS`
///    anywhere on the line (the old engine's `lower.find(key)` behavior), so
///    `const password = "…"` and `let token = "…"` are still caught (AC7
///    no-regression).
fn generic_ranges(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut current_path: Option<String> = None;
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        update_diff_path(line, &mut current_path);
        if !is_diff_metadata(line) {
            let path_ignored = current_path
                .as_deref()
                .map(entropy_path_ignored)
                .unwrap_or(false);

            // Pass 1: line-start assignment (the primary generic detector).
            if let Some((value_range, ident)) = assignment_value(line) {
                let value = &line[value_range.clone()];
                if accept_generic(value, &ident, path_ignored) {
                    out.push(offset + value_range.start..offset + value_range.end);
                }
            }

            // Pass 2: keyword-anywhere compatibility (old engine's
            // `lower.find(key)` behavior). Only fires when the line-start
            // pass did not already catch the value, so we don't double-count.
            if let Some((value_range, _)) = keyword_anywhere_assignment(line) {
                let value = &line[value_range.clone()];
                // The keyword-anywhere pass is a compatibility shim for the old
                // engine — it uses the same KEYWORD_MIN_LEN gate with no
                // entropy floor, matching the old behavior exactly.
                if value.chars().count() >= KEYWORD_MIN_LEN {
                    out.push(offset + value_range.start..offset + value_range.end);
                }
            }
        }
        offset += line.len();
    }
    out
}

/// The assignment matcher: `IDENT = value` / `IDENT: value`, tolerating a
/// leading diff `+`/`-`/indentation. Returns the value's in-line byte range
/// and the lowercased identifier.
fn assignment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^[+\- \t]*([A-Za-z_][A-Za-z0-9_.\-]*)\s*[:=]\s*["']?([^\s"',]+)"#)
            .unwrap()
    })
}

fn assignment_value(line: &str) -> Option<(Range<usize>, String)> {
    let caps = assignment_re().captures(line)?;
    let ident = caps.get(1)?.as_str().to_ascii_lowercase();
    let value = caps.get(2)?;
    Some((value.start()..value.end(), ident))
}

/// Accept a generic-assignment value as a secret?
fn accept_generic(value: &str, ident: &str, path_ignored: bool) -> bool {
    let len = value.chars().count();
    let keyworded = ident_is_sensitive(ident);

    // Structural false positives: a bare UUID or git/SHA-length hex is not a
    // secret unless a sensitive keyword names it.
    if !keyworded && is_structural_fp(value) {
        return false;
    }

    if keyworded {
        // Keyword fast path: a named credential (api_key/secret/token/...) needs
        // only modest length, no entropy floor - this matches the old engine's
        // keyword-assignment detector so low-entropy named credentials are not a
        // regression (AC7). Immune to the entropy-only path ignore-set.
        return len >= KEYWORD_MIN_LEN;
    }

    // Generic (unnamed) path: gated by length + normalized entropy, and
    // disabled inside ignored files (lock files, minified, fixtures).
    if path_ignored || len < GENERIC_MIN_LEN {
        return false;
    }
    normalized_entropy(value) >= Charset::classify(value).normalized_floor()
}

/// Scan for a `SENSITIVE_KEYWORDS` match anywhere on the line, then extract
/// the next `=` / `:` value. This is the old engine's `lower.find(key)`
/// compatibility pass (AC7 no-regression for declaration-prefixed and
/// quoted-object-key forms like `const password = "…"` or `"api_key": "…"`).
///
/// Returns the value's in-line byte range and the matched keyword, or `None`
/// if no keyword is found or no value follows.
fn keyword_anywhere_assignment(line: &str) -> Option<(Range<usize>, String)> {
    let lower = line.to_ascii_lowercase();
    for kw in SENSITIVE_KEYWORDS {
        if let Some(kw_start) = lower.find(kw) {
            // Find the next `=` or `:` after the keyword.
            let after_kw = &line[kw_start + kw.len()..];
            if let Some(sep_pos) = after_kw.find(['=', ':']) {
                // Skip the separator and any whitespace after it.
                let after_sep_raw = &after_kw[sep_pos + 1..];
                let trimmed = after_sep_raw.trim_start();
                let trimmed_bytes = after_sep_raw.len() - trimmed.len();

                // Determine the value end: if quoted, find the closing quote;
                // otherwise, find the next delimiter.
                let value_end = if let Some(inner) = trimmed.strip_prefix('"') {
                    // Find closing double-quote.
                    inner.find('"').map(|pos| pos + 1).unwrap_or(trimmed.len())
                } else if let Some(inner) = trimmed.strip_prefix('\'') {
                    // Find closing single-quote.
                    inner.find('\'').map(|pos| pos + 1).unwrap_or(trimmed.len())
                } else {
                    // Unquoted: up to whitespace, comma, semicolon, or bracket.
                    trimmed
                        .find(|c: char| {
                            c.is_whitespace()
                                || c == ','
                                || c == ';'
                                || c == '"'
                                || c == '\''
                                || c == ')'
                                || c == ']'
                                || c == '}'
                        })
                        .unwrap_or(trimmed.len())
                };

                if value_end == 0 {
                    continue;
                }

                // Skip if the value starts with a digit (likely a number,
                // not a secret) — matches old engine behavior.
                let value_str = &trimmed[..value_end];
                if value_str
                    .as_bytes()
                    .first()
                    .is_none_or(|b| b.is_ascii_digit())
                {
                    continue;
                }

                let value_start = kw_start + kw.len() + sep_pos + 1 + trimmed_bytes;
                return Some((value_start..value_start + value_end, kw.to_string()));
            }
        }
    }
    None
}

fn ident_is_sensitive(ident: &str) -> bool {
    SENSITIVE_KEYWORDS.iter().any(|k| ident.contains(k))
}

/// Canonical UUID (8-4-4-4-12) or pure hex of MD5/SHA-1/SHA-256/git-SHA length.
fn is_structural_fp(value: &str) -> bool {
    uuid_re().is_match(value) || pure_hex_hash(value)
}

fn uuid_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
            .unwrap()
    })
}

fn pure_hex_hash(value: &str) -> bool {
    matches!(value.len(), 32 | 40 | 64) && value.chars().all(|c| c.is_ascii_hexdigit())
}

/// Diff lines whose content must not be scanned by the generic detector.
fn is_diff_metadata(line: &str) -> bool {
    let t = line.trim_end_matches(['\n', '\r']);
    t.starts_with("diff --git ")
        || t.starts_with("index ")
        || t.starts_with("similarity index ")
        || t.starts_with("@@ ")
        || t.starts_with("+++ ")
        || t.starts_with("--- ")
        || t.starts_with("new file mode ")
        || t.starts_with("deleted file mode ")
        || t.starts_with("rename from ")
        || t.starts_with("rename to ")
}

/// Track the current file path from a `diff --git a/<p> b/<p>` header (robust
/// to deleted files, which emit `+++ /dev/null`).
fn update_diff_path(line: &str, current: &mut Option<String>) {
    let t = line.trim_end_matches(['\n', '\r']);
    if let Some(rest) = t.strip_prefix("diff --git ") {
        // rest = "a/<path> b/<path>"; take the b/ side, fall back to a/.
        if let Some(b_idx) = rest.rfind(" b/") {
            *current = Some(rest[b_idx + 3..].to_string());
        } else if let Some(a) = rest.strip_prefix("a/") {
            *current = Some(a.split(" b/").next().unwrap_or(a).to_string());
        }
    }
}

/// Default ignore-set for the *generic-entropy* detector only: high-noise
/// machine files where random-looking strings are almost never secrets.
fn entropy_path_ignored(path: &str) -> bool {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.ends_with(".min.js")
        || base.ends_with(".lock")
        || base == "package-lock.json"
        || base == "yarn.lock"
        || base == "Cargo.lock"
        || base == "pnpm-lock.yaml"
        || base == "composer.lock"
        || base == "Gemfile.lock"
        || base == "poetry.lock"
        || path.contains("testdata/")
        || path.contains("fixtures/")
        || path.contains("__snapshots__/")
}

/// Drop any range that lies on a line carrying a `# gcm:allow` / `// gcm:allow`
/// pragma.
fn drop_pragma_ranges(text: &str, ranges: &mut Vec<Range<usize>>) {
    if ranges.is_empty() {
        return;
    }
    let mut muted: Vec<Range<usize>> = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        // Mute only when the line ENDS with the pragma (per AC4), so the literal
        // text buried mid-line cannot silently suppress a real secret.
        let trimmed = line.trim_end();
        if PRAGMAS.iter().any(|p| trimmed.ends_with(p)) {
            muted.push(offset..offset + line.len());
        }
        offset += line.len();
    }
    ranges.retain(|r| !muted.iter().any(|m| r.start >= m.start && r.start < m.end));
}

/// Sort and merge overlapping/adjacent ranges so redaction emits one marker and
/// the abort count is not double-inflated (precise + generic on the same span).
pub fn merge_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => {
                last.end = last.end.max(range.end);
            }
            _ => merged.push(range),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> &'static RuleEngine {
        super::super::rules::vendored().unwrap()
    }

    fn detects(text: &str) -> bool {
        !secret_ranges(text, engine()).is_empty()
    }

    // AC1: the prefix-less, generically-named credential.
    #[test]
    fn ac1_generic_gitlab_assignment_detected() {
        assert!(detects(r#"GITLAB="3cjcjg988jrskbxx""#));
    }

    #[test]
    fn ac1_redacts_generic_value() {
        let out = redact_secrets(r#"GITLAB="3cjcjg988jrskbxx""#, engine());
        assert!(!out.contains("3cjcjg988jrskbxx"));
        assert!(out.contains("[REDACTED: secret]"));
    }

    // AC2: every vendored provider shape.
    #[test]
    fn ac2_all_vendored_providers_detected() {
        // Token bodies for shapes GitHub push-protection validates (gitlab,
        // slack, stripe) are assembled via concat! so the literal secret
        // pattern never appears contiguously in source; gcm scans the joined
        // value identically at runtime.
        let cases = [
            ("aws", "AKIAIOSFODNN7EXAMPLE".to_string()),
            ("gcp", "AIzaSyA1234567890abcdefghijklmnopqrstuv".to_string()),
            (
                "github",
                "ghp_0123456789abcdefghijklmnopqrstuvwxyz".to_string(),
            ),
            (
                "gitlab",
                concat!("glpat", "-ABCDEFGH1234ijklmnop").to_string(),
            ),
            (
                "slack",
                concat!("xox", "b-123456789012-abcdefghijklmnop").to_string(),
            ),
            (
                "stripe",
                concat!("sk", "_live_0123456789abcdefghijABCD").to_string(),
            ),
            (
                "anthropic",
                "sk-ant-api03-abcDEF0123456789ghiJKLmno".to_string(),
            ),
            ("openai", "sk-abcDEF0123456789ghiJKLmnopQRS".to_string()),
            ("xai", "xai-abcDEF0123456789ghiJKLmnopQRS".to_string()),
        ];
        for (name, token) in &cases {
            assert!(detects(token), "{name} token not detected: {token}");
        }
    }

    // AC2 variant coverage: all Slack family variants and Stripe rk_live_.
    #[test]
    fn ac2_slack_variants_detected() {
        let slack_variants = [
            concat!("xox", "b-123456789012-abcdefghijklmnop"),
            concat!("xox", "a-123456789012-abcdefghijklmnop"),
            concat!("xox", "p-123456789012-abcdefghijklmnop"),
            concat!("xox", "r-123456789012-abcdefghijklmnop"),
            concat!("xox", "s-123456789012-abcdefghijklmnop"),
        ];
        for variant in &slack_variants {
            assert!(detects(variant), "Slack variant not detected: {variant}");
        }
    }

    #[test]
    fn ac2_stripe_rk_live_detected() {
        let token = concat!("rk", "_live_0123456789abcdefghijABCD");
        assert!(detects(token), "Stripe rk_live_ not detected: {token}");
    }

    #[test]
    fn ac2_generic_api_key_rule_detected() {
        assert!(detects(r#"api_key = "Zx9Q2mLkP0wEr5Ty8UiO""#));
    }

    // AC3: structural false positives stay quiet.
    #[test]
    fn ac3_uuid_not_detected() {
        assert!(!detects("id = 550e8400-e29b-41d4-a716-446655440000"));
    }

    #[test]
    fn ac3_git_sha_and_sha256_not_detected() {
        assert!(!detects(
            "commit = a94a8fe5ccb19ba61c4c0873d391e987982fbbd3"
        ));
        assert!(!detects(
            "digest = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        ));
    }

    #[test]
    fn ac3_lockfile_integrity_hash_not_detected() {
        let line = r#"  "integrity": "sha512-abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ+/=="#;
        assert!(!detects(line));
    }

    // AC4: pragma mutes the line.
    #[test]
    fn ac4_pragma_mutes_detection() {
        assert!(!detects(
            "token = ghp_0123456789abcdefghijklmnopqrstuvwxyz # gcm:allow"
        ));
        assert!(!detects(
            r#"const k = "sk-abcDEF0123456789ghiJKLmnopQRS"; // gcm:allow"#
        ));
    }

    // AC7: old prefix-detector shapes still fire (no regression).
    #[test]
    fn ac7_legacy_prefix_shapes_still_detected() {
        // Keyword-anywhere forms (old engine's `lower.find(key)` behavior).
        assert!(detects("AWS=AKIAABCDEFGHIJKLMNOP"));
        assert!(detects("token=ghp_abcdefghijklmnopqrstuvwxyz123456"));
        assert!(detects("github_pat_11ABCDE0123456789abcdefghij_klmnopqrstuvwxyzABCDEFGHIJ0123456789klmnopqrstuv"));

        // Declaration-prefixed forms (regression: old engine caught these via
        // keyword-anywhere, new engine must too).
        assert!(detects("const password = \"abcdefgh\""));
        assert!(detects("let token = \"abcdabcd\""));
        assert!(detects("export API_KEY=abcdefgh"));
        assert!(detects("\"api_key\": \"abcdefgh\""));

        // Bare prefix tokens (no keyword context) — exercised by the precise
        // rule using canonical gitleaks live-shapes. The gitleaks corpus uses
        // provider-documented token lengths (e.g. GitHub ghp_ has exactly 36
        // trailing chars), which is narrower than the old engine's permissive
        // min_len. This is an intentional precision improvement: canonical-
        // length tokens still fire; sub-canonical bare tokens that the old
        // engine caught are now accepted as a deliberate narrowing.
        assert!(detects("ghp_abcdefghijklmnopqrstuvwxyz123456ABCDEF")); // 36 trailing
        let stripe_token = concat!("sk", "_live_0123456789abcdefghijABCD");
        assert!(detects(stripe_token));
    }

    // AC7 regression: the old keyword-assignment detector flagged ANY 8+ char
    // value after a sensitive keyword, with no entropy floor. The keyword fast
    // path must preserve that, or low-entropy named credentials silently leak.
    #[test]
    fn ac7_keyworded_low_entropy_value_still_detected() {
        assert!(detects("password=aaaaaaaa"));
        assert!(detects("token=abcdabcd"));
        assert!(detects("my_api_key=aaaaaaaa"));
        assert!(detects("access_key_id=aaaaaaaa"));
    }

    #[test]
    fn broad_keyword_substrings_do_not_trigger_fast_path() {
        assert!(!detects("monkey=aaaaaaaa"));
        assert!(!detects("api_version=aaaaaaaa"));
        assert!(!detects("auth_method=aaaaaaaa"));
    }

    // Boundary regression: two secrets separated by one delimiter, both caught.
    #[test]
    fn adjacent_secrets_both_detected() {
        let text = "k=sk-abcDEF0123456789ghiJKL,sk-zzzYYY9876543210wwwVVVu";
        let ranges = secret_ranges(text, engine());
        assert!(ranges.len() >= 2, "expected 2 ranges, got {ranges:?}");
    }

    #[test]
    fn git_metadata_hex_not_detected() {
        assert!(!detects("index 0123abc..789def0 100644"));
    }

    #[test]
    fn benign_low_entropy_assignment_not_detected() {
        assert!(!detects("NAME = aaaaaaaaaaaaaaaaaa"));
    }

    // AC4 precision: the pragma mutes only when the line ENDS with it, so the
    // literal "# gcm:allow" buried mid-line cannot silently suppress a real
    // secret elsewhere on the same line.
    #[test]
    fn pragma_only_mutes_at_end_of_line() {
        let secret = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
        let line = format!(r#"note = "see # gcm:allow"; token = {secret}"#);
        assert!(detects(&line), "mid-line pragma text must not suppress");
    }

    #[test]
    fn precise_rule_in_lockfile_still_detected() {
        // A real glpat- inside package-lock.json is caught by its precise rule
        // even though the generic-entropy detector is disabled there. Token
        // assembled via concat! to avoid GitHub push-protection on the fixture.
        let token = concat!("glpat", "-ABCDEFGH1234ijklmnop");
        let diff = format!(
            "diff --git a/package-lock.json b/package-lock.json\n+  \"token\": \"{token}\"\n"
        );
        assert!(detects(&diff));
    }

    #[test]
    fn deleted_lockfile_path_uses_diff_git_not_dev_null() {
        let diff = "diff --git a/package-lock.json b/package-lock.json\n--- a/package-lock.json\n+++ /dev/null\n-random_value = 3cjcjg988jrskbxx\n";
        assert!(!detects(diff));
    }

    #[test]
    fn utf8_adjacent_secret_no_panic() {
        let text = "ключ = sk-abcDEF0123456789ghiJKLmnopQRS";
        let _ = secret_ranges(text, engine()); // must not panic on byte slicing
        assert!(detects(text));
    }

    #[test]
    fn utf8_redaction_slices_on_char_boundaries() {
        let text = "префикс🔐 token = sk-abcDEF0123456789ghiJKLmnopQRS суффикс";
        let out = redact_secrets(text, engine());
        assert!(out.contains("префикс🔐 token = [REDACTED: secret] суффикс"));
    }
}
