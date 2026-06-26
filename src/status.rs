//! Read-only configuration / provider introspection for `gcm status` (CLO-515).
//!
//! The command answers "what will gcm do right now, and why" without any network
//! call, diff read, or LLM request. The hard part is **source attribution**: a
//! value alone is not enough, so each provider reports where its key, model, and
//! (for Ollama) endpoint came from.
//!
//! Attribution mirrors the precedence the runtime actually applies, computed here
//! **without** calling [`crate::config::apply_to_env`] (which would copy inline
//! config keys into the environment and destroy attribution):
//!   * **key**:   env var (non-blank) > inline config `key` > not set
//!   * **model**: `--model` flag (selected provider only) > per-provider env > default
//!   * **selected provider**: `--provider` flag > `GCM_PROVIDER` > `config.default` > Groq
//!   * **Ollama endpoint**: `GCM_OLLAMA_BASE_URL` > `OLLAMA_HOST` (normalized) >
//!     config `endpoint` > default `http://localhost:11434`
//!
//! All attribution helpers are pure (they take the loaded config plus an
//! `env_lookup` closure) so they are unit-testable without touching process env,
//! mirroring the `config_path_from` / `env_plan(is_set)` style elsewhere.

use std::path::PathBuf;

use serde::Serialize;

use crate::cli::Cli;
use crate::config::{self, Config};
use crate::output::SCHEMA_VERSION;
use crate::provider::{ollama, resolve_model_with_source, ModelSource, ProviderId};

/// Canonical provider order for output (matches the wizard's `cloud_then_ollama`).
const PROVIDER_ORDER: [ProviderId; 5] = [
    ProviderId::Groq,
    ProviderId::Google,
    ProviderId::Openai,
    ProviderId::Anthropic,
    ProviderId::Ollama,
];

/// The full `gcm status` payload. Versioned (`v`) like the commit `Envelope` but a
/// distinct shape - it is NOT an `output::Envelope` (that enum is commit-only).
/// JSON consumers should ignore unknown fields so this can grow without a `v` bump.
#[derive(Debug, Serialize)]
pub struct StatusReport {
    pub v: i32,
    pub version: &'static str,
    pub paths: PathsStatus,
    pub providers: Vec<ProviderStatus>,
    /// Set only when `GCM_PROVIDER` holds an unknown value (reported, not fatal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PathsStatus {
    /// `env var GCM_CONFIG` or `default dir`.
    pub config_dir_source: String,
    /// Resolved `config.toml` path, or `None` if no OS config dir is available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_file_path: Option<PathBuf>,
    /// Whether the config file exists on disk.
    pub config_file_exists: bool,
}

#[derive(Debug, Serialize)]
pub struct ProviderStatus {
    /// Canonical lowercase token (`groq`, `google`, ...).
    pub name: ProviderId,
    /// The effective selected provider this invocation (flag > env > config > groq).
    pub selected: bool,
    /// Whether the provider is activated (see [`is_activated`]).
    pub activated: bool,
    /// Key source for cloud providers; `None` for key-free Ollama.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_source: Option<String>,
    /// Ollama endpoint; `None` for cloud providers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Ollama endpoint source; `None` for cloud providers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_source: Option<String>,
    /// Resolved effective model.
    pub model: String,
    /// Model source: `default` / `env var <NAME>` / `flag`.
    pub model_source: String,
    /// For Ollama only: false when the model routes off-machine (a `:cloud` model).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zero_egress: Option<bool>,
}

/// Entry point for the `status` subcommand. Pure introspection: loads the config
/// and reads the environment, builds the report, prints it (JSON or human), and
/// always returns exit code 0 (misconfiguration is reported as fields, not a
/// failure). Dispatched at the top of `run()` before any repo/provider/LLM work.
pub fn run_status_subcommand(args: &Cli) -> i32 {
    let config = config::load();
    let report = build_report(
        args.provider,
        args.model.as_deref(),
        config.as_ref(),
        |var| std::env::var(var).ok(),
    );

    if args.json {
        // serde cannot fail on our owned types; fall back to a minimal object.
        let json = serde_json::to_string(&report)
            .unwrap_or_else(|_| "{\"v\":1,\"version\":\"unknown\"}".to_string());
        println!("{json}");
    } else {
        print_human(&report);
    }
    0
}

/// Build the report from explicit inputs (pure; the body of
/// [`run_status_subcommand`]), so the whole shape is unit-testable without env.
fn build_report(
    cli_provider: Option<ProviderId>,
    cli_model: Option<&str>,
    config: Option<&Config>,
    env_lookup: impl Fn(&str) -> Option<String>,
) -> StatusReport {
    let paths = paths_status(&env_lookup);
    let (selected, provider_error) = selected_provider(cli_provider, config, &env_lookup);

    let providers = PROVIDER_ORDER
        .iter()
        .map(|&id| {
            let is_selected = id == selected;
            // The --model flag applies ONLY to the selected provider; others
            // resolve from env/default so they aren't mislabeled `flag`.
            let model_flag = if is_selected { cli_model } else { None };
            let (model, msrc) = resolve_model_with_source(id, model_flag, &env_lookup);

            let (key_source, endpoint, endpoint_source, zero_egress) = if id == ProviderId::Ollama {
                let (ep, src) = ollama_endpoint(config, &env_lookup);
                let zero = Some(!model.ends_with(":cloud"));
                (None, Some(ep), Some(src), zero)
            } else {
                (Some(key_source(id, config, &env_lookup)), None, None, None)
            };

            ProviderStatus {
                name: id,
                selected: is_selected,
                activated: is_activated(id, config, &env_lookup),
                key_source,
                endpoint,
                endpoint_source,
                model,
                model_source: model_source_label(msrc),
                zero_egress,
            }
        })
        .collect();

    StatusReport {
        v: SCHEMA_VERSION,
        version: crate::cli::VERSION,
        paths,
        providers,
        provider_error,
    }
}

/// Resolve the config dir source, path, and existence. Handles the no-config-dir
/// case gracefully (`config_file_path: None`, `exists: false`).
fn paths_status(env_lookup: &impl Fn(&str) -> Option<String>) -> PathsStatus {
    let from_env = env_lookup("GCM_CONFIG")
        .map(|v| v.trim().to_string())
        .is_some_and(|v| !v.is_empty());
    let config_dir_source = if from_env {
        "env var GCM_CONFIG".to_string()
    } else {
        "default dir".to_string()
    };
    let path = config::config_path();
    let config_file_exists = path.as_ref().is_some_and(|p| p.exists());
    PathsStatus {
        config_dir_source,
        config_file_path: path,
        config_file_exists,
    }
}

/// The effective selected provider and an optional error note. Precedence
/// `--provider` flag > `GCM_PROVIDER` env > `config.default` > built-in `Groq`.
/// An unknown non-blank `GCM_PROVIDER` is reported (not fatal); the displayed
/// selection falls back to `config.default` (if any) else `Groq`.
fn selected_provider(
    cli_provider: Option<ProviderId>,
    config: Option<&Config>,
    env_lookup: &impl Fn(&str) -> Option<String>,
) -> (ProviderId, Option<String>) {
    if let Some(id) = cli_provider {
        return (id, None);
    }
    if let Some(raw) = env_lookup("GCM_PROVIDER")
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    {
        return match ProviderId::parse(&raw) {
            Some(id) => (id, None),
            None => {
                let fallback = config.map_or(ProviderId::Groq, |c| c.default);
                (
                    fallback,
                    Some(format!(
                        "unknown provider '{raw}' in GCM_PROVIDER (valid: groq, google, openai, \
                         anthropic, ollama); showing {} as selected",
                        fallback.as_str()
                    )),
                )
            }
        };
    }
    if let Some(c) = config {
        return (c.default, None);
    }
    (ProviderId::Groq, None)
}

/// Whether a provider is "activated". Cloud: listed in config OR its key env var
/// is set & non-blank. Ollama: listed in config OR `OLLAMA_HOST` /
/// `GCM_OLLAMA_BASE_URL` is set & non-blank (never "active by default").
fn is_activated(
    id: ProviderId,
    config: Option<&Config>,
    env_lookup: &impl Fn(&str) -> Option<String>,
) -> bool {
    if config.is_some_and(|c| c.providers.iter().any(|p| p.id == id)) {
        return true;
    }
    match id {
        ProviderId::Ollama => {
            env_nonblank(env_lookup, "GCM_OLLAMA_BASE_URL")
                || env_nonblank(env_lookup, "OLLAMA_HOST")
        }
        _ => id
            .key_env_var()
            .is_some_and(|var| env_nonblank(env_lookup, var)),
    }
}

/// Key source for a cloud provider, applying env > inline-config precedence (the
/// effective runtime precedence, since the env bridge only fills an unset var).
fn key_source(
    id: ProviderId,
    config: Option<&Config>,
    env_lookup: &impl Fn(&str) -> Option<String>,
) -> String {
    if let Some(var) = id.key_env_var() {
        if env_nonblank(env_lookup, var) {
            return format!("env var {var}");
        }
    }
    let inline = config.and_then(|c| c.providers.iter().find(|p| p.id == id));
    if inline.is_some_and(|pc| pc.key.is_some()) {
        return "config file".to_string();
    }
    "not set".to_string()
}

/// Resolve the Ollama endpoint and its source without calling `apply_to_env`.
fn ollama_endpoint(
    config: Option<&Config>,
    env_lookup: &impl Fn(&str) -> Option<String>,
) -> (String, String) {
    if let Some(v) = env_value(env_lookup, "GCM_OLLAMA_BASE_URL") {
        return (v, "env var GCM_OLLAMA_BASE_URL".to_string());
    }
    if let Some(v) = env_value(env_lookup, "OLLAMA_HOST") {
        return (
            ollama::normalize_host(&v),
            "env var OLLAMA_HOST".to_string(),
        );
    }
    if let Some(ep) = config
        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Ollama))
        .and_then(|pc| pc.endpoint.as_deref())
        .map(str::trim)
        .filter(|e| !e.is_empty())
    {
        return (ep.to_string(), "config file".to_string());
    }
    (ollama::DEFAULT_BASE_URL.to_string(), "default".to_string())
}

fn model_source_label(src: ModelSource) -> String {
    match src {
        ModelSource::Flag => "flag".to_string(),
        ModelSource::Env(var) => format!("env var {var}"),
        ModelSource::Default => "default".to_string(),
    }
}

/// True when an env var is present and non-blank (trimmed).
fn env_nonblank(env_lookup: &impl Fn(&str) -> Option<String>, name: &str) -> bool {
    env_value(env_lookup, name).is_some()
}

/// The trimmed, non-empty value of an env var, else `None`.
fn env_value(env_lookup: &impl Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    env_lookup(name)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Render the default human view to stdout (Version / Paths / Providers).
fn print_human(report: &StatusReport) {
    println!("gcm {}", report.version);

    println!("\nPaths:");
    println!("  config dir source: {}", report.paths.config_dir_source);
    match &report.paths.config_file_path {
        Some(p) => println!(
            "  config file:       {} ({})",
            p.display(),
            if report.paths.config_file_exists {
                "exists"
            } else {
                "no config file"
            }
        ),
        None => println!("  config file:       (no OS config dir available)"),
    }

    if let Some(err) = &report.provider_error {
        println!("\nWarning: {err}");
    }

    println!("\nProviders:");
    for p in &report.providers {
        let mut tags = Vec::new();
        if p.selected {
            tags.push("selected");
        }
        tags.push(if p.activated {
            "activated"
        } else {
            "not activated"
        });
        println!("  {} [{}]", p.name.as_str(), tags.join(", "));

        if let Some(ks) = &p.key_source {
            println!("    key:   {ks}");
        }
        if let Some(ep) = &p.endpoint {
            let src = p.endpoint_source.as_deref().unwrap_or("unknown");
            print!("    endpoint: {ep} ({src})");
            match p.zero_egress {
                Some(false) => println!("  [NOT zero-egress: :cloud model]"),
                _ => println!(),
            }
        }
        println!("    model: {} ({})", p.model, p.model_source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;

    fn cfg(default: ProviderId, providers: Vec<ProviderConfig>) -> Config {
        Config {
            version: 1,
            default,
            providers,
        }
    }

    fn pc(id: ProviderId, key: Option<&str>, endpoint: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            id,
            key: key.map(String::from),
            endpoint: endpoint.map(String::from),
        }
    }

    /// An env_lookup backed by a slice of (name, value) pairs.
    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn key_source_env_wins_over_config() {
        // GROQ_API_KEY set AND inline config key present -> env wins (runtime precedence)
        let c = cfg(
            ProviderId::Groq,
            vec![pc(ProviderId::Groq, Some("sk-inline"), None)],
        );
        assert_eq!(
            key_source(
                ProviderId::Groq,
                Some(&c),
                &env(&[("GROQ_API_KEY", "sk-env")])
            ),
            "env var GROQ_API_KEY"
        );
        // only inline config key -> config file
        assert_eq!(
            key_source(ProviderId::Groq, Some(&c), &env(&[])),
            "config file"
        );
        // neither -> not set
        assert_eq!(
            key_source(ProviderId::Openai, Some(&c), &env(&[])),
            "not set"
        );
    }

    #[test]
    fn key_source_blank_env_is_not_set() {
        // a blank GROQ_API_KEY="" must not count as a key source
        assert_eq!(
            key_source(ProviderId::Groq, None, &env(&[("GROQ_API_KEY", "   ")])),
            "not set"
        );
    }

    #[test]
    fn activation_rules() {
        // cloud: activated by env key (non-blank) or config membership
        assert!(is_activated(
            ProviderId::Groq,
            None,
            &env(&[("GROQ_API_KEY", "sk")])
        ));
        assert!(!is_activated(
            ProviderId::Groq,
            None,
            &env(&[("GROQ_API_KEY", "")])
        ));
        let c = cfg(ProviderId::Openai, vec![pc(ProviderId::Openai, None, None)]);
        assert!(is_activated(ProviderId::Openai, Some(&c), &env(&[])));

        // Ollama: NOT active by default on a clean machine...
        assert!(!is_activated(ProviderId::Ollama, None, &env(&[])));
        // ...active via OLLAMA_HOST...
        assert!(is_activated(
            ProviderId::Ollama,
            None,
            &env(&[("OLLAMA_HOST", "remote:11434")])
        ));
        // ...or config membership
        let oc = cfg(ProviderId::Ollama, vec![pc(ProviderId::Ollama, None, None)]);
        assert!(is_activated(ProviderId::Ollama, Some(&oc), &env(&[])));
    }

    #[test]
    fn selected_provider_precedence() {
        // flag wins over everything
        let c = cfg(ProviderId::Openai, vec![pc(ProviderId::Openai, None, None)]);
        assert_eq!(
            selected_provider(
                Some(ProviderId::Anthropic),
                Some(&c),
                &env(&[("GCM_PROVIDER", "ollama")])
            )
            .0,
            ProviderId::Anthropic
        );
        // GCM_PROVIDER over config.default
        assert_eq!(
            selected_provider(None, Some(&c), &env(&[("GCM_PROVIDER", "ollama")])).0,
            ProviderId::Ollama
        );
        // config.default when no flag/env (the fix: pick_provider_id alone would miss this)
        assert_eq!(
            selected_provider(None, Some(&c), &env(&[])).0,
            ProviderId::Openai
        );
        // built-in Groq when nothing
        assert_eq!(selected_provider(None, None, &env(&[])).0, ProviderId::Groq);
    }

    #[test]
    fn selected_provider_invalid_env_is_reported_not_fatal() {
        let (id, err) = selected_provider(None, None, &env(&[("GCM_PROVIDER", "bogus")]));
        assert_eq!(id, ProviderId::Groq);
        let err = err.expect("invalid provider reported");
        assert!(err.contains("bogus"), "{err}");
    }

    #[test]
    fn ollama_endpoint_precedence_chain() {
        // GCM_OLLAMA_BASE_URL wins
        assert_eq!(
            ollama_endpoint(None, &env(&[("GCM_OLLAMA_BASE_URL", "http://a:1")])),
            (
                "http://a:1".to_string(),
                "env var GCM_OLLAMA_BASE_URL".to_string()
            )
        );
        // OLLAMA_HOST normalized
        assert_eq!(
            ollama_endpoint(None, &env(&[("OLLAMA_HOST", "remote:8080")])),
            (
                "http://remote:8080".to_string(),
                "env var OLLAMA_HOST".to_string()
            )
        );
        // config endpoint
        let c = cfg(
            ProviderId::Ollama,
            vec![pc(ProviderId::Ollama, None, Some("http://cfg:2"))],
        );
        assert_eq!(
            ollama_endpoint(Some(&c), &env(&[])),
            ("http://cfg:2".to_string(), "config file".to_string())
        );
        // default
        assert_eq!(
            ollama_endpoint(None, &env(&[])),
            (ollama::DEFAULT_BASE_URL.to_string(), "default".to_string())
        );
    }

    #[test]
    fn report_masks_secrets_and_orders_providers() {
        let c = cfg(
            ProviderId::Groq,
            vec![pc(ProviderId::Groq, Some("sk-INLINE-SECRET"), None)],
        );
        let report = build_report(
            None,
            None,
            Some(&c),
            env(&[("OPENAI_API_KEY", "sk-ENV-SECRET")]),
        );
        // canonical order
        let names: Vec<&str> = report.providers.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["groq", "google", "openai", "anthropic", "ollama"]);
        // no raw secret anywhere in the serialized JSON
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("sk-INLINE-SECRET"), "{json}");
        assert!(!json.contains("sk-ENV-SECRET"), "{json}");
        // groq selected (config.default), openai key from env, groq key from config
        let groq = report
            .providers
            .iter()
            .find(|p| p.name == ProviderId::Groq)
            .unwrap();
        assert!(groq.selected);
        assert_eq!(groq.key_source.as_deref(), Some("config file"));
        let openai = report
            .providers
            .iter()
            .find(|p| p.name == ProviderId::Openai)
            .unwrap();
        assert_eq!(openai.key_source.as_deref(), Some("env var OPENAI_API_KEY"));
    }

    #[test]
    fn model_flag_scoped_to_selected_provider() {
        // --provider openai --model foo: only openai reports flag; others env/default
        let report = build_report(Some(ProviderId::Openai), Some("foo"), None, env(&[]));
        let openai = report
            .providers
            .iter()
            .find(|p| p.name == ProviderId::Openai)
            .unwrap();
        assert_eq!(openai.model, "foo");
        assert_eq!(openai.model_source, "flag");
        let groq = report
            .providers
            .iter()
            .find(|p| p.name == ProviderId::Groq)
            .unwrap();
        assert_eq!(groq.model_source, "default");
        assert_ne!(groq.model, "foo");
    }

    #[test]
    fn ollama_zero_egress_flag() {
        // local model -> zero_egress true
        let report = build_report(
            None,
            None,
            None,
            env(&[("GCM_OLLAMA_MODEL", "gemma4:e4b-mlx")]),
        );
        let ollama = report
            .providers
            .iter()
            .find(|p| p.name == ProviderId::Ollama)
            .unwrap();
        assert_eq!(ollama.zero_egress, Some(true));
        // :cloud model -> zero_egress false
        let report = build_report(
            None,
            None,
            None,
            env(&[("GCM_OLLAMA_MODEL", "deepseek-v4-flash:cloud")]),
        );
        let ollama = report
            .providers
            .iter()
            .find(|p| p.name == ProviderId::Ollama)
            .unwrap();
        assert_eq!(ollama.zero_egress, Some(false));
    }
}
