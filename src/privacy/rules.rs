//! Vendored, data-driven secret-detection rule pack (CLO-514, FR-60).
//!
//! The TOML corpus is embedded at build time via `include_str!`, parsed with
//! serde, and compiled once into a [`regex::RegexSet`] (the runtime prefilter)
//! plus a per-rule [`regex::Regex`] used to extract the secret capture group.

use std::sync::OnceLock;

use regex::{Regex, RegexSet};
use serde::Deserialize;

/// The vendored rule corpus, embedded at build time.
const VENDORED_RULES: &str = include_str!("rules.toml");

/// A rule as written in the TOML pack. Unknown fields are tolerated for
/// forward compatibility (a future `--secret-rules` file may carry more).
#[derive(Debug, Deserialize)]
struct RawRule {
    id: String,
    regex: String,
    #[serde(default)]
    keywords: Vec<String>,
    entropy: Option<f64>,
    min_digits: Option<u32>,
    #[allow(dead_code)]
    confidence: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawPack {
    #[serde(default)]
    rules: Vec<RawRule>,
}

/// A single compiled detection rule.
#[derive(Debug)]
pub struct CompiledRule {
    #[allow(dead_code)] // surfaced in tests; reserved for debug logging of matched rule
    pub id: String,
    pub regex: Regex,
    #[allow(dead_code)]
    pub keywords: Vec<String>,
    pub entropy: Option<f64>,
    pub min_digits: Option<u32>,
}

/// The compiled rule engine: a `RegexSet` over every rule (one-pass prefilter)
/// plus the individual `Regex`es for capture extraction.
#[derive(Debug)]
pub struct RuleEngine {
    set: RegexSet,
    rules: Vec<CompiledRule>,
}

impl RuleEngine {
    /// Parse and compile a TOML rule pack. Returns a clear, rule-attributed
    /// error (never panics) when the TOML is invalid or any regex fails to
    /// compile - this is what keeps a malformed vendored pack out of the field
    /// (AC8). An empty/attribution-only pack compiles to a zero-rule engine.
    pub fn compile(toml_src: &str) -> Result<Self, String> {
        let pack: RawPack =
            toml::from_str(toml_src).map_err(|e| format!("secret rule pack: invalid TOML: {e}"))?;

        let mut patterns = Vec::with_capacity(pack.rules.len());
        let mut rules = Vec::with_capacity(pack.rules.len());
        for raw in pack.rules {
            let regex = Regex::new(&raw.regex)
                .map_err(|e| format!("secret rule '{}': invalid regex: {e}", raw.id))?;
            patterns.push(raw.regex);
            rules.push(CompiledRule {
                id: raw.id,
                regex,
                keywords: raw.keywords,
                entropy: raw.entropy,
                min_digits: raw.min_digits,
            });
        }

        let set = RegexSet::new(&patterns)
            .map_err(|e| format!("secret rule pack: failed to build RegexSet: {e}"))?;
        Ok(Self { set, rules })
    }

    /// Rules whose pattern matches somewhere in `text`, found in a single
    /// `RegexSet` pass. `RegexSet` IS the prefilter - callers run the
    /// individual `Regex` only for the indices returned here.
    pub fn matching_rules(&self, text: &str) -> impl Iterator<Item = &CompiledRule> {
        self.set
            .matches(text)
            .into_iter()
            .map(move |i| &self.rules[i])
    }

    #[allow(dead_code)] // used by tests to assert the corpus size
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// The process-wide vendored engine, compiled at most once. Returns an error
/// (mapped by the caller to `GcmError::Config`) rather than panicking if the
/// vendored pack somehow fails to compile - though a unit test guarantees it
/// does, so the error path is unreachable in a shipped binary.
pub fn vendored() -> Result<&'static RuleEngine, String> {
    static ENGINE: OnceLock<RuleEngine> = OnceLock::new();
    if let Some(engine) = ENGINE.get() {
        return Ok(engine);
    }
    let engine = RuleEngine::compile(VENDORED_RULES)?;
    let _ = ENGINE.set(engine);
    Ok(ENGINE.get().expect("engine just set"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendored_pack_compiles_with_expected_corpus() {
        let engine = vendored().expect("vendored rule pack must compile");
        // At least the providers the spec enumerates.
        assert!(engine.len() >= 12, "only {} rules", engine.len());
    }

    #[test]
    fn malformed_regex_is_a_clear_error_not_a_panic() {
        let bad = r#"
[[rules]]
id = "broken"
regex = "([unclosed"
keywords = ["x"]
"#;
        let err = RuleEngine::compile(bad).unwrap_err();
        assert!(err.contains("broken"), "error should name the rule: {err}");
        assert!(err.contains("invalid regex"), "got: {err}");
    }

    #[test]
    fn empty_pack_degrades_gracefully() {
        let engine = RuleEngine::compile("# attribution header only\n").unwrap();
        assert_eq!(engine.len(), 0);
        assert_eq!(engine.matching_rules("anything").count(), 0);
    }

    #[test]
    fn regexset_prefilter_selects_only_matching_rules() {
        let engine = vendored().unwrap();
        // concat! so the literal token never appears contiguously (GitHub push protection).
        let line = format!(
            "export GITLAB_TOKEN={}",
            concat!("glpat", "-ABCDEFGH1234ijklmnop")
        );
        let hits: Vec<&str> = engine
            .matching_rules(&line)
            .map(|r| r.id.as_str())
            .collect();
        assert!(hits.contains(&"gitlab-pat"), "hits: {hits:?}");
    }
}
