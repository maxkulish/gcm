# Spec: Replace best-effort secret scanner with a rule-pack + entropy detection engine

**Created**: 2026-06-23
**Linear**: [CLO-514](https://linear.app/cloud-ai/issue/CLO-514) (FR-60 new; hardens FR-50 from CLO-490)
**Estimated scope**: L (~8 files, ~6 sub-tasks)

## 1. Problem Statement

gcm's optional pre-egress secret scanner (`src/privacy.rs`, shipped in CLO-490 for FR-50) is a
git-secrets-class matcher. `secret_ranges()` (`src/privacy.rs:248`) is built from two hand-coded
detectors:

- `prefixed_token_ranges` (`src/privacy.rs:278`) — a fixed prefix list (`AKIA`/`ASIA`, `ghp_`/`gho_`/…,
  `github_pat_`, `sk-`) with per-family char classes and minimum lengths.
- `assignment_value_ranges` (`src/privacy.rs:300`) — a 7-word keyword allowlist (`api_key`, `secret`,
  `token`, …); a `key = value` / `key: value` is only flagged when the **left side contains an
  allowlisted word**.

Both consumers — abort mode (`scan_text` → `GcmError::SecretDetected { count }`, `src/privacy.rs:74`)
and redact mode (`redact_secrets`, `src/privacy.rs:228`) — read the `Vec<Range<usize>>` this function
returns. The whole engine is inline Rust constants, so the corpus cannot grow without a code change.

The concrete failure: a generically-named, prefix-less credential is invisible. `GITLAB="3cjcjg988jrskbxx"`
has no known prefix and `GITLAB` is not in the keyword allowlist, so it passes `--secret-scan=abort`
untouched and reaches the LLM provider. Anyone whose secret is not one of the ~10 hardcoded shapes is
unprotected, which defeats the purpose of a pre-egress scan. This affects every gcm user who opts into
`--secret-scan`/`GCM_SECRET_SCAN` (off by default; `src/privacy.rs:12-39`, wired in `src/main.rs:202`).

The fix is to replace the inline matcher with a **data-driven, engine-backed** detector built on the
gitleaks/Kingfisher model: a vendored TOML rule pack (embedded at build time), executed by the pure-Rust
`regex` crate, with a Shannon-entropy gate that catches high-randomness values regardless of name or
prefix — while suppressing the false positives (UUIDs, git SHAs, lockfile integrity hashes) that naive
entropy scanning produces. `secret_ranges()` (or its successor) keeps returning `Vec<Range<usize>>`, so
`scan_text` and `redact_secrets` are unchanged at the seam and off/redact/abort behavior is preserved.

## 2. Acceptance Criteria

- [ ] **AC1 — generic credential caught.** A diff containing `GITLAB="3cjcjg988jrskbxx"` (or any
  `IDENT = "high-entropy-value"`) is reported under `--secret-scan=abort` (exits non-zero, no provider
  request sent) and replaced with `[REDACTED: secret]` under `--secret-scan=redact`.
- [ ] **AC2 — vendored providers detected.** Unit tests assert a representative live-shaped token for each
  vendored provider is caught: AWS (`AKIA…`), Google (`AIza…`), GitHub (existing `ghp_`/`github_pat_`),
  GitLab (`glpat-…`), Slack (`xox[bpsa]-…`), Stripe (`sk_live_…`/`rk_live_…`), Anthropic (`sk-ant-…`),
  OpenAI (`sk-…`), xAI (`xai-…`), plus the generic-api-key rule.
- [ ] **AC3 — false-positive controls hold.** None of the following is flagged (no sensitive keyword
  adjacent): a canonical UUID (`8-4-4-4-12`), a 40-char git/SHA-1 hex, a 64-char SHA-256 hex, and a
  `package-lock.json`-style `"integrity": "sha512-…"` line.
- [ ] **AC4 — pragma honored.** A line ending in `# gcm:allow` or `// gcm:allow` produces no detection,
  even when it contains a real token shape.
- [ ] **AC5 — data-driven.** Rules live in a vendored TOML pack embedded via `include_str!`; adding a new
  provider rule is a TOML edit with **no Rust code change**. The vendored file header preserves upstream
  license attribution (gitleaks MIT, Kingfisher Apache-2.0).
- [ ] **AC6 — dependency budget.** Exactly one new runtime crate (`regex`); no Hyperscan/`vectorscan`, no
  `rayon`, no network calls. Shannon entropy is hand-rolled (no crate).
- [ ] **AC7 — no regression.** Every detection the old engine made still fires (the redact/abort/off
  contract and existing AC-S1..S3 acceptance checks stay green); existing unit + acceptance suites pass;
  new regression tests added for AC1–AC4.

  **Note on prefix migration narrowing:** The old engine's prefix detector used permissive minimum
  lengths (e.g. `ghp_` min_len 24 = 20+ trailing chars). The vendored rule pack uses gitleaks
  live-shape lengths (e.g. GitHub `ghp_` requires exactly 36 trailing chars), which is narrower for
  sub-canonical bare tokens. This is an intentional precision improvement: canonical-length tokens
  (provider-documented lengths) still fire; sub-canonical bare tokens that the old engine caught are
  accepted as a deliberate narrowing. The keyword-anywhere compatibility pass (see §3) preserves
  detection for keyword-named assignments regardless of prefix length.
- [ ] **AC8 — vendored rules validated, never panic.** A malformed regex in `rules.toml` fails a unit
  test with a clear, attributed error; rule load/compile never panics at runtime (a runtime load failure
  surfaces as `GcmError::Config`, not a panic). An empty/attribution-only `rules.toml` degrades gracefully
  (the generic + entropy detectors still run; no panic).
- [ ] **AC9 — UTF-8 safe.** Multi-byte input (Cyrillic, emoji) adjacent to a detected secret never
  panics; all returned ranges fall on `char` boundaries so `redact_secrets` byte-slicing is safe.

**Verification method**: `cargo test` (unit + `tests/onboarding.rs`), `cargo build --release`, and
`scripts/acceptance.sh` (extended with new AC-S* cases). Each AC maps to a row in §5.

## 3. Constraints

**Must**:
- Keep the integration seam returning `Vec<Range<usize>>` over the original input text so `scan_text`
  (`src/privacy.rs:74`) and `redact_secrets` (`src/privacy.rs:228`) work unchanged; off/redact/abort and
  `GcmError::SecretDetected { count }` semantics are preserved.
- Compile the rule pack (parse TOML → `regex::RegexSet` + per-rule `Regex`) **once**, not per scanned
  string (a single gcm run calls the scanner ~6 times across `prepare_grouping`/`prepare_diff`). Store the
  compiled engine in `Privacy` or a process-wide `OnceLock`.
- **`RegexSet` IS the prefilter** — do not hand-roll a separate keyword/substring prescan for the rule
  pack. Call `RegexSet::matches(text)` once (a single combined DFA pass), then run `Regex::captures()`
  only for the rule indices it reports. The `keywords` schema field is retained as data (forward-compat,
  external-rules-ready, and used by the *generic detector's* keyword fast-path), not as a runtime prefilter
  stage for the regex rules.
- Rule schema fields: `id` (string), `regex` (string), `keywords` (string list), `entropy` (float, opt),
  `min_digits` (int, opt), `confidence` (string, opt). Unknown fields tolerated forward-compatibly.
- The capture group whose entropy/`min_digits` is tested is the rule's secret group (group 1 if present,
  else whole match), matching gitleaks/Kingfisher convention.
- Entropy is Shannon computed over `value.chars()` (**not** bytes — multi-byte UTF-8 must not inflate the
  score), with a minimum length gate (≈16) before computing.
- **Thresholds must be mathematically reachable for the min-length gate.** The realized Shannon entropy of
  a length-`L` string is bounded by `log2(L)`: 4.0 at L=16, 4.32 at L=20, only reaching 4.5 at L≥23.
  Therefore the raw "base64 ≈ 4.5" bar is **unreachable** for the 16–22-char tokens FR-60 must catch. The
  detector MUST gate primarily on **length-normalized entropy** `H / log2(L)` (length-invariant, range
  0–1), with the charset-aware raw value as a secondary floor only where it is reachable. Defaults live as
  named, inline-documented constants and are calibrated by the acid-test below, not copied verbatim from
  the ticket's (length-naive) numbers.
- **Charset classifier order must be hex → alphanumeric/base36 → base64** (hex ⊂ alnum ⊂ base64). The
  intermediate alphanumeric/base36 class is required: without it a lowercase-alnum token is classified
  base64 and measured against the stricter (and, for short L, unreachable) bar. Each wider class gets a
  correspondingly higher raw floor; hex is checked first so hex tokens don't inherit the base64 bar.
- **Calibration acid-test (binding):** AC1's value `3cjcjg988jrskbxx` (L=16, lowercase-alnum) has realized
  Shannon entropy **≈3.33 bits** and normalized ratio **≈0.83**. The chosen thresholds MUST flag it — this
  is the entire point of FR-60 — and a unit test asserts this exact value is detected. Conversely, a benign
  16-char low-entropy value (e.g. a repeated/dictionary-ish string, normalized ≲0.6) MUST NOT be flagged.
  Pure-hex of known lengths (UUID, 32/40/64) is kept out by the **structural FP suppression** (§AC3),
  *not* by the hex entropy floor (a 40-char git SHA scores ≈3.8 > 3.0 and would otherwise flag).
- Vendored rules must be **lookaround-free**: the `regex` crate supports neither lookaround (`(?=)`/`(?!)`)
  nor backreferences. Any gitleaks/Kingfisher rule relying on them must be transcribed to a lookaround-free
  regex with the exclusion logic moved into Rust filters in `detect.rs`.
- **Boundaries must use zero-width anchors, never consumed delimiter characters.** gitleaks expresses
  boundaries with lookbehind/lookahead; transcribing those into a *consuming* class (e.g.
  `[^A-Za-z0-9](token)[^A-Za-z0-9]`) breaks adjacent matches because `regex::find_iter` is
  non-overlapping — for `,sk-A,sk-B,` the first match eats both shared commas and the second secret is
  missed (and thus leaks). Use `\b`/`\B`, `^`/`$` (multiline), or no boundary; if a leading class is
  unavoidable, **never consume the trailing delimiter** that could anchor the next match. A regression
  test covers two secrets separated by a single delimiter (both must be detected/redacted).
- The generic-assignment + entropy detector must catch `IDENT = "value"` / `IDENT: value` for **any**
  identifier (not just the keyword allowlist); keyword-named assignments keep a lower-entropy fast path.
  It must tolerate a leading diff `+`/`-`/indentation before `IDENT` (diffs are `+`-prefixed) and must
  **skip git metadata lines** (`diff --git`, `index 0123abc..789def`, `similarity index`, `@@` hunk
  headers, `+++`/`---` path lines) so their high-entropy hex is not mistaken for a secret.
- Returned ranges must fall on `char` boundaries (UTF-8 safety for `redact_secrets` byte-slicing, AC9).
- Precise provider rules take **precedence** over the generic-entropy detector: when both hit overlapping
  spans, `merge_ranges` collapses them to one range so redaction emits a single marker and the abort
  `count` is not double-inflated.
- Rule load/compile failure (should be impossible for a test-validated vendored pack) surfaces as
  `GcmError::Config` — the codebase uses a custom `GcmError` enum (`src/error.rs`), not `anyhow`.
- The `# gcm:allow` / `// gcm:allow` pragma leaders are as specified in the ticket; broader comment leaders
  (SQL `--`, HTML `<!-- -->`, C `/* */`) are a deliberate future extension, out of scope here.
- Preserve upstream license attribution text in the vendored TOML header (MIT gitleaks, Apache-2.0
  Kingfisher).

**Must-not**:
- Add any runtime dependency other than `regex`. No Hyperscan/`vectorscan-rs`, no `rayon`, no ML/BPE
  tokenization, no live API validation, no network calls.
- Load rules from a user-supplied external file path (rules are vendored + embedded only; the schema must
  not *preclude* a future `--secret-rules <path>` override, but no such flag is added here).
- Let the entropy ignore-set (lockfiles/minified/fixtures) suppress **precise rule** detections — the
  ignore-set narrows the *generic-entropy* detector only; named-provider rules fire everywhere.
  **Line→path attribution decision:** the ignore-set applies only where a line is reliably attributable to
  a file — the diff body scanned by `prepare_diff`. Track the current path from the **`diff --git a/<p>
  b/<p>` header**, *not* from `+++ b/<path>` alone: a deleted file emits `+++ /dev/null`, so `+++`-only
  tracking would mis-attribute the removed (`-`) lines (including a deleted `package-lock.json`'s removed
  integrity hashes) to `/dev/null`. The `diff --git` line carries both paths and survives add/delete.
  (Removed `-` lines are still scanned — they are part of the diff text sent to the provider and can leak.)
  For the non-diff text `prepare_grouping` scans (`file_list`/`status`/`stat`), there is no per-line path,
  so the ignore-set is a no-op there and the generic detector runs unfiltered — acceptable because
  lockfile/minified noise lives in diff bodies, not in status/stat summaries.
- Panic on a malformed vendored rule (a bad regex must surface as a clear error, not a runtime panic in
  the field; since the pack is vendored, a compile-time-vendored bad regex should be caught by a test).
- **Deliberate scope limit — bare/standalone generics.** The generic detector keys off an `IDENT = value`
  / `IDENT: value` assignment; a prefix-less, high-entropy string with no identifier (inside a JSON array
  `[ "3cjcjg…" ]`, or a positional call arg `login("3cjcjg…")`) is intentionally **not** flagged by the
  generic detector — scanning every bare string would flood the user with false positives. This is an
  accepted gap, documented so test authors don't expect it. Note: prefixed provider tokens (`sk-ant-…`,
  `glpat-…`, etc.) are still caught **anywhere** by their precise rules, regardless of assignment context.

**Prefer**:
- Split the growing `privacy.rs` into a `src/privacy/` module directory (mirroring the existing
  `src/provider/` precedent): e.g. `mod.rs` (Privacy/PathFilter/SecretScanMode), `rules.rs` (schema +
  loader + compiled engine), `entropy.rs` (Shannon + charset classification), `detect.rs` (pipeline +
  generic detector + FP suppression). Keep `PathFilter` and `SecretScanMode` behavior byte-identical.
- Reuse gitleaks `config/gitleaks.toml` regexes and per-rule entropy/keywords, and Kingfisher's GitLab
  `glpat-` rule (with `min_entropy`/`min_digits`) as transcription sources.
- Keep redaction range-merging (`merge_ranges`, `src/privacy.rs:348`) so overlapping detections collapse.

**Escalate when**:
- A vendored upstream regex uses a feature the `regex` crate does not support (lookaround/backreferences)
  and a faithful lookaround-free transcription with Rust-side exclusion is impossible — surface the
  specific rule and proposed rewrite.
- The dependency-budget constraint (AC6) would have to be broken to satisfy any other requirement.

## 4. Decomposition

1. **Engine foundation** — Add `regex` to `Cargo.toml`. Create the `src/privacy/` module split. Define the
   TOML rule schema + serde structs, a loader that `include_str!`s the pack and compiles it to a
   `RegexSet` + per-rule `Regex` once, and the hand-rolled Shannon entropy fn (over `.chars()`) with the
   charset classifier (**hex → alnum/base36 → base64**), **length-normalized** gate `H/log2(L)`, and named
   threshold constants calibrated to the §3 acid-test. Unit-test: pack parses, every rule regex compiles,
   entropy + normalized values for known strings (incl. the AC1 value ≈3.33/≈0.83). Files: `Cargo.toml`,
   `src/privacy/mod.rs`, `src/privacy/rules.rs`, `src/privacy/entropy.rs`.
2. **Vendor the rule pack** — Transcribe the initial corpus (AWS, Google `AIza`, GitHub, GitLab `glpat-`/
   `glptt-`/`GR1348941`, Slack `xox[bpsa]-`, Stripe `sk_live_`/`rk_live_`, Anthropic `sk-ant-`, OpenAI
   `sk-`, xAI `xai-`, generic-api-key) into `src/privacy/rules.toml` with MIT/Apache-2.0 attribution
   header. Files: `src/privacy/rules.toml`.
3. **Detection pipeline + prefix migration** — `RegexSet::matches()` as the single-pass prefilter → run
   `Regex::captures()` only for matched rule indices → entropy/`min_digits` gate, returning
   `Vec<Range<usize>>`. **Explicitly migrate** each old prefix detector (`AKIA`/`ASIA`, `ghp_`/`gho_`/…,
   `github_pat_`, `sk-`) onto a vendored rule and add a regression test asserting each old-engine detection
   still fires (AC7). Files: `src/privacy/detect.rs`, `src/privacy/mod.rs`.
4. **Generic assignment + entropy detector** — Match `IDENT = "value"` / `IDENT: value` for any identifier
   (tolerating a leading diff `+`/`-`/indent; skipping git-metadata lines); accept the value only if length
   ≥ ~16 AND normalized entropy clears the charset-aware threshold; keyword-named idents use a lower
   threshold fast path. This is the direct `GITLAB="…"` fix (AC1) — verify against the §3 acid-test. Files:
   `src/privacy/detect.rs`.
5. **False-positive controls + pragma** — Suppress canonical UUID and pure-hex 32/40/64 unless a sensitive
   keyword is adjacent; apply `min_digits` as a secondary gate; honor inline `# gcm:allow` / `// gcm:allow`
   (drop any range on a pragma line); extend the **entropy detector's** default ignore-set (lockfiles,
   `*.min.js`, `testdata/`, fixtures) via diff line→path attribution, leaving precise rules everywhere.
   Files: `src/privacy/detect.rs`, `src/privacy/mod.rs`.
6. **Tests + acceptance** — Unit tests for AC1–AC4 and no-regression; extend `scripts/acceptance.sh` with
   end-to-end AC-S* cases for `GITLAB="…"` under abort+redact and for the UUID/git-SHA non-detection.
   Files: `src/privacy/*` test modules, `scripts/acceptance.sh`.

**Dependency order**: 1 first. Then 2, 3, 4 can proceed in parallel (3 and 4 depend on 1; 2 depends on the
schema from 1). 5 depends on 3 + 4. 6 depends on all. Critical path: 1 → 3 → 5 → 6.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | `GITLAB="3cjcjg988jrskbxx"` under abort | `Err(SecretDetected{count>=1})`, non-zero exit, empty provider capture | unit test in `detect.rs` + `scripts/acceptance.sh` new AC-S |
| 2 | same value under redact | value replaced by `[REDACTED: secret]`, exit 0 | unit `redact_secrets` test + acceptance |
| 3 | one live-shaped token per vendored provider (AWS/GCP/GitHub/GitLab/Slack/Stripe/Anthropic/OpenAI/xAI/generic) | each produces ≥1 range | table-driven unit test in `detect.rs` |
| 4 | canonical UUID `550e8400-e29b-41d4-a716-446655440000`, no keyword | no detection | unit test (FP controls) |
| 5 | 40-char git SHA + 64-char SHA-256, no keyword | no detection | unit test (FP controls) |
| 6 | `"integrity": "sha512-<base64>"` lockfile line | no detection | unit test (FP controls) |
| 7 | `API_KEY=ghp_… # gcm:allow` and `const k = "sk-…"; // gcm:allow` | no detection | unit test (pragma) |
| 8 | rule pack loads + every regex compiles | loader returns engine, no error/panic | unit test in `rules.rs` |
| 9 | add a dummy provider rule to `rules.toml`, no Rust edit, rebuild | new shape detected | manual / fixture test proving data-driven (AC5) |
| 10 | existing AC-S1..S3 + all current unit tests | green | `cargo test` + `scripts/acceptance.sh` |
| 11 | dependency audit | only `regex` added; no rayon/hyperscan/network | `git diff Cargo.toml` + `cargo tree` review (AC6) |
| 12 | malformed regex injected into a test rule pack | loader returns clear attributed error, no panic | unit test in `rules.rs` (AC8) |
| 13 | empty/attribution-only `rules.toml` | generic + entropy detectors still run, no panic | unit test (AC8) |
| 14 | secret adjacent to multi-byte text (e.g. `токен=sk_live_…`, emoji) | detected, ranges on char boundaries, no panic | unit test (AC9) |
| 15 | keyword fast-path: `API_KEY=lowEntropyShortish` flagged vs `RANDOM_IDENT=lowEntropyShortish` not | keyword path lower threshold; generic path requires entropy | unit test |
| 16 | git metadata line `index 0123abc..789def 100644` | no detection (metadata skipped) | unit test |
| 17 | real `glpat-…` inside a `package-lock.json` diff hunk | still caught (precise rule ignores the entropy ignore-set) | unit test |
| 18 | entropy calibration: `3cjcjg988jrskbxx` (H≈3.33, norm≈0.83) | detected by generic+entropy gate | unit test (binding acid-test, AC1) |
| 19 | benign 16-char low-entropy value (normalized ≲0.6) under generic detector | NOT detected | unit test (FP) |
| 20 | two secrets on one line separated by a single delimiter (`,sk-A,sk-B,`) | BOTH detected/redacted (no swallowed delimiter) | unit test (boundary regression) |
| 21 | deleted `package-lock.json` hunk (`+++ /dev/null`), removed integrity hashes | path attributed via `diff --git`; integrity hashes not flagged | unit test (path attribution) |

**Edge cases to verify**:
- High-entropy but legitimate non-secret values just under threshold stay unflagged (tune defaults so a
  typical UUID/git-SHA does not exceed the generic threshold once FP rules apply).
- A real token shorter than the min-length gate still caught by its precise prefix rule (precise rules
  must not be gated out by the generic length floor).
- Overlapping detections (a prefix rule and the generic detector both hit the same span) collapse via
  `merge_ranges` so redaction emits one marker, and the abort `count` is not double-inflated.
- Pragma at end of a line that also contains a non-secret keeps detecting on *other* lines.
- Entropy ignore-set narrows only the generic detector: a real `glpat-…` inside `package-lock.json` is
  still caught by its precise rule.
- Multi-byte/UTF-8 input does not produce a panic on byte-range slicing (`redact_secrets` slices on byte
  offsets — ranges must fall on char boundaries).
