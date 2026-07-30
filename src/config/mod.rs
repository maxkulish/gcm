//! Persistent provider configuration and the first-run onboarding wizard
//! (CLO-496). A per-user `config.toml` (ADR-001 Decision 4: TOML in the OS
//! config dir) records which providers are enabled, an optional inline key per
//! cloud provider (stored only when the user types a key not already in the
//! environment - the `0600` file is the defensive fallback FR-55 anticipates),
//! the Ollama endpoint, and the default provider.
//!
//! The module is a thin layer over the (unchanged) provider registry: [`load`]
//! reads the file and [`apply_to_env`] bridges it into the env vars the
//! providers already read lazily, so the documented precedence
//! (`flag > env > config > default`) is preserved by construction - a pre-set
//! env var is never overwritten. First-run detection ([`needs_onboarding`]) and
//! the interactive [`run_wizard`] handle the unconfigured case; a non-TTY first
//! run gets [`non_tty_instructions`] and a non-zero exit instead of a hang.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::provider::{AuthMethod, ProviderId};

/// On-disk config format version (mirrors `cache::CacheFile.version`). v2 (CLO-516)
/// added the per-provider `models` enabled-set whitelist. A v1 file is accepted and
/// migrated up on read (its `models` default empty = unrestricted); an unknown
/// version (0 or > current) is treated as "no usable config" so a future schema can
/// still evolve. A *newer* binary's v2 file read by an old v1-only binary is a
/// `WrongVersion` miss there (forward-compat: it re-onboards, never mis-enforces).
#[doc(hidden)]
pub const CONFIG_FORMAT_VERSION: u32 = 2;
/// Config file name within the config dir (or the `GCM_CONFIG` override dir).
#[doc(hidden)]
pub const CONFIG_FILE_NAME: &str = "config.toml";
/// Default Ollama endpoint (mirrors `provider::ollama`'s default base URL).
#[doc(hidden)]
pub const DEFAULT_OLLAMA_ENDPOINT: &str = "http://localhost:11434";
/// Connection timeout for the wizard's Ollama daemon probe (ADR-001 Decision 8):
/// short enough that an unresponsive endpoint never hangs the wizard.
#[doc(hidden)]
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Persisted configuration, written as TOML to `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub version: u32,
    /// Provider used when neither `--provider` nor `GCM_PROVIDER` is set.
    pub default: ProviderId,
    /// Every provider the user enabled during onboarding.
    pub providers: Vec<ProviderConfig>,
    /// Conflict-resolution settings for `gcm resolve` (CLO-531).
    #[serde(default)]
    pub conflict: ConflictConfig,
}

/// One enabled provider. `key == None` => read from the provider env var at run
/// time (env-only); `key == Some(_)` => inline secret in the 0600 file. Always
/// `None` for key-free Ollama, which uses `endpoint` instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    pub id: ProviderId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Override the provider's default model. Bridged into the provider layer's
    /// per-provider model env var (e.g. `GCM_OPENAI_MODEL`) when that var is not
    /// already set, so resolution stays `--model` flag > env var > this > default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Enabled-model whitelist (CLO-516). Empty = unrestricted (v1 migration and
    /// pre-`gcm provider` state); non-empty restricts runtime model resolution to
    /// this set (membership checked after per-provider canonicalization). `model`
    /// is the chosen default and is always a member when this is non-empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    /// Vertex only (CLO-537): the GCP project. Bridged into `GCM_VERTEX_PROJECT` by
    /// [`apply_to_env`] when that var is unset. `None`/skip-serialize for every other
    /// provider, so a pre-Vertex config file parses unchanged (no version bump).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Vertex only (CLO-537): the location/region (effective default `global`).
    /// Bridged into `GCM_VERTEX_LOCATION` by [`apply_to_env`] when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

/// Conflict-resolution settings for `gcm resolve` (CLO-531).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConflictConfig {
    /// LLM temperature for resolution (default 0.1).
    #[serde(default = "default_conflict_temperature")]
    pub temperature: f64,
    /// Optional validation command (e.g. `cargo check`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate_cmd: Option<String>,
    /// Glob patterns for paths that always require manual review.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sensitive_paths: Vec<String>,
    /// Auto-resolution policy: which hunk classes to auto-resolve.
    #[serde(default = "default_auto_policy")]
    pub auto_policy: AutoPolicy,
    /// Whether to use mergiraf if on PATH (default true).
    #[serde(default = "default_mergiraf")]
    pub mergiraf: bool,
    /// Maximum conflict rounds a single `gcm resolve` drives a rebase or
    /// cherry-pick sequence through (CLO-554, default 10). `1` reproduces the
    /// pre-loop behavior of stopping after one conflict. `0` is rejected.
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u32,
}

fn default_conflict_temperature() -> f64 {
    0.1
}

fn default_auto_policy() -> AutoPolicy {
    AutoPolicy::Trivial
}

fn default_mergiraf() -> bool {
    true
}

fn default_max_rounds() -> u32 {
    10
}

/// Shared wording for a zero round cap, used by the clap parser and the config
/// validator so both surfaces say the same thing.
pub const MAX_ROUNDS_ZERO: &str =
    "max_rounds must be at least 1 (1 resolves a single conflict stop, the pre-CLO-554 behavior)";

/// Must mirror the per-field serde defaults above. A derived `Default` does
/// not (bool -> false, f64 -> 0.0): the parent field's `#[serde(default)]`
/// routes through THIS impl whenever config.toml has no `[conflict]` section
/// (the common case), which used to silently disable mergiraf and zero the
/// resolve temperature.
impl Default for ConflictConfig {
    fn default() -> Self {
        ConflictConfig {
            temperature: default_conflict_temperature(),
            validate_cmd: None,
            sensitive_paths: Vec::new(),
            auto_policy: default_auto_policy(),
            mergiraf: default_mergiraf(),
            max_rounds: default_max_rounds(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum AutoPolicy {
    /// Auto-resolve only trivial hunks (identical, one-side-unchanged, one-side-empty).
    #[default]
    Trivial,
    /// Also auto-resolve moderate hunks (reserved for future heuristics).
    Moderate,
    /// Send everything to the LLM (no auto-resolution).
    Complex,
}

/// Why a present config file is not usable; drives the stderr warning in [`load`].
#[derive(Debug)]
#[doc(hidden)]
pub enum LoadIssue {
    Malformed(String),
    WrongVersion,
    DefaultNotEnabled,
}

// ── path resolution ────────────────────────────────────────────────────────

/// `$GCM_CONFIG/config.toml` if the override is set (tests / relocation, per
/// ADR-001 Decision 4), else the XDG config dir `~/.config/gcm` (mirrors
/// `cache::cache_dir`). `None` if no config dir can be determined.
pub fn config_path() -> Option<PathBuf> {
    config_path_from(std::env::var_os("GCM_CONFIG").as_deref(), config_dir())
}

/// The XDG config directory for gcm: `$XDG_CONFIG_HOME/gcm` if set (absolute),
/// else `~/.config/gcm`. `None` when no usable base exists (no `HOME`).
#[doc(hidden)]
pub fn config_dir() -> Option<PathBuf> {
    crate::paths::xdg_gcm_dir_from(
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        ".config",
    )
}

/// Pure path resolution (the body of [`config_path`], so the override precedence
/// is unit-testable without touching process env or the real config dir).
#[doc(hidden)]
pub fn config_path_from(
    gcm_config: Option<&OsStr>,
    fallback_dir: Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(dir) = gcm_config {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir).join(CONFIG_FILE_NAME));
        }
    }
    fallback_dir.map(|d| d.join(CONFIG_FILE_NAME))
}

// ── load ───────────────────────────────────────────────────────────────────

/// Load and parse the config, or `None` on absent / unreadable / unparseable /
/// wrong-version file (a miss, never an abort). A malformed parse, a wrong
/// version, or a `default` not among `providers` returns `None`; the caller
/// treats that as "needs onboarding". An absent file is a silent miss; a present
/// but unusable file warns to stderr pointing at the path. On Unix, a file with
/// group/other permission bits (not `0600`) also warns and returns `None`.
pub fn load() -> Option<Config> {
    load_from(&config_path()?)
}

/// Load from an explicit path (the body of [`load`]), so the file-level behavior
/// is unit-testable with a temp path and no `GCM_CONFIG` env mutation.
#[doc(hidden)]
pub fn load_from(path: &Path) -> Option<Config> {
    let data = fs::read_to_string(path).ok()?; // absent/unreadable -> silent miss

    if let Some(reason) = insecure_permissions(path) {
        eprintln!(
            "gcm: warning: config file {} has insecure permissions ({reason}); ignoring it. \
             Fix with `chmod 600 {}` or re-run `gcm config`.",
            path.display(),
            path.display()
        );
        return None;
    }

    match parse_config(&data) {
        Ok(cfg) => Some(cfg),
        Err(LoadIssue::Malformed(e)) => {
            eprintln!(
                "gcm: warning: config file {} is malformed ({e}); re-running first-run setup.",
                path.display()
            );
            None
        }
        Err(LoadIssue::WrongVersion) => None, // forward-compat: silent miss
        Err(LoadIssue::DefaultNotEnabled) => {
            eprintln!(
                "gcm: warning: config file {} sets a default provider that is not enabled; ignoring it.",
                path.display()
            );
            None
        }
    }
}

/// Parse + validate the on-disk text (pure: no I/O, no warnings), so the
/// malformed / wrong-version / default-not-enabled cases are unit-testable.
#[doc(hidden)]
pub fn parse_config(data: &str) -> Result<Config, LoadIssue> {
    let mut cfg: Config = toml::from_str(data).map_err(|e| LoadIssue::Malformed(e.to_string()))?;
    // Accept any known version (1..=current) and migrate up; reject unknown
    // (0 or newer-than-this-binary). The v1 -> v2 migration is purely additive:
    // `models` deserializes empty (= unrestricted), so nothing is rejected that a
    // v1 user relied on. Stamping the version means a re-save persists v2 (without
    // this, `render_config` would re-emit the old version and the bump would never
    // take effect).
    if cfg.version == 0 || cfg.version > CONFIG_FORMAT_VERSION {
        return Err(LoadIssue::WrongVersion);
    }
    cfg.version = CONFIG_FORMAT_VERSION;
    if !cfg.providers.iter().any(|p| p.id == cfg.default) {
        return Err(LoadIssue::DefaultNotEnabled);
    }
    Ok(cfg)
}

/// `Some(reason)` when the file's permissions are wider than user-only on Unix
/// (any group/other bit set); `None` when `0600`-equivalent or off-Unix.
#[cfg(unix)]
#[doc(hidden)]
pub fn insecure_permissions(path: &Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path).ok()?.permissions().mode();
    if mode & 0o077 != 0 {
        Some(format!("mode {:o}, expected 600", mode & 0o777))
    } else {
        None
    }
}

#[cfg(not(unix))]
#[doc(hidden)]
pub fn insecure_permissions(_path: &Path) -> Option<String> {
    None
}

// ── save ───────────────────────────────────────────────────────────────────

/// Persist atomically with `0600` permissions (mirrors `cache`'s write strategy:
/// a private temp file renamed over the target, so it is never world-readable).
/// The atomic rename means concurrent first-run processes are safe: first-to-
/// write wins, the second sees the config on its next [`load`].
pub fn save(config: &Config) -> io::Result<()> {
    save_to(&config_path().ok_or_else(no_config_dir)?, config)
}

/// Persist to an explicit path (the body of [`save`]), so the atomic `0600`
/// write is unit-testable with a temp path and no `GCM_CONFIG` env mutation.
#[doc(hidden)]
pub fn save_to(path: &Path, config: &Config) -> io::Result<()> {
    let text = render_config(config).map_err(io::Error::other)?;
    write_atomic(path, text.as_bytes())
}

/// The on-disk file body: the live config as TOML, followed by a fully-commented
/// reference block documenting every provider's overridable settings. Only the
/// live section is active TOML; the reference is all comments, so the file still
/// parses. Written on first-run onboarding (and `gcm config`) so the format is
/// discoverable without reading the docs.
#[doc(hidden)]
pub fn render_config(config: &Config) -> Result<String, toml::ser::Error> {
    // Force the serialized version to the current format regardless of the
    // in-memory value, so a config loaded as v1 (migrated up by `parse_config`)
    // is always persisted as the current version - belt-and-suspenders with the
    // migration's version stamp (CLO-516).
    let config = Config {
        version: CONFIG_FORMAT_VERSION,
        ..config.clone()
    };
    let mut s = toml::to_string_pretty(&config)?;
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s.push('\n');
    s.push_str(&commented_reference());
    Ok(s)
}

/// The commented reference block: each provider with its overridable knobs and
/// real default model, generated from the live provider tables so it never drifts
/// from the actual defaults / env-var names.
#[doc(hidden)]
pub fn commented_reference() -> String {
    let mut s = String::new();
    s.push_str("# ── Reference: all available settings ──────────────────────────────────────\n");
    s.push_str("# Copy an entry into the section above, uncomment, and edit. A provider entry\n");
    s.push_str("# supports: model (chosen default), models (enabled set), key (cloud),\n");
    s.push_str("# endpoint (Ollama only), project+location (Vertex only). Matching env vars\n");
    s.push_str("# override this file\n");
    s.push_str("# (e.g. GCM_OPENAI_MODEL=…, OPENAI_API_KEY=…). An empty/absent `models`\n");
    s.push_str("# means unrestricted; set it via `gcm provider` to restrict usage.\n");
    s.push_str("#\n");
    for id in all_providers() {
        let token = provider_token(id);
        let model = id.default_model();
        let model_var = id.model_env_vars()[0];
        s.push_str("# [[providers]]\n");
        s.push_str(&format!("# id = \"{token}\"\n"));
        s.push_str(&format!(
            "# model = \"{model}\"   # default; or set {model_var}\n"
        ));
        s.push_str(&format!(
            "# models = [\"{model}\"]   # enabled set (only these are usable); empty = any\n"
        ));
        match id.auth_method() {
            AuthMethod::ApiKey => {
                if let Some(key_var) = id.key_env_var() {
                    s.push_str(&format!(
                        "# key = \"…\"   # inline secret, or set {key_var}\n"
                    ));
                }
            }
            AuthMethod::KeylessEndpoint => {
                s.push_str(&format!(
                    "# endpoint = \"{DEFAULT_OLLAMA_ENDPOINT}\"   # or set GCM_OLLAMA_BASE_URL / OLLAMA_HOST\n"
                ));
            }
            AuthMethod::KeylessAdc => {
                s.push_str(
                    "# project = \"my-gcp-project\"   # required; or set GCM_VERTEX_PROJECT / GOOGLE_CLOUD_PROJECT\n",
                );
                s.push_str(
                    "# location = \"global\"   # or set GCM_VERTEX_LOCATION / GOOGLE_CLOUD_LOCATION\n",
                );
            }
        }
        s.push_str("#\n");
    }
    s
}

// ── first-run detection ─────────────────────────────────────────────────────

/// True iff onboarding should fire after [`load`] returned no usable config: no
/// `--provider`, no non-blank `GCM_PROVIDER`, and no cloud key env var set. An
/// env-configured user is never interrupted.
pub fn needs_onboarding(cli_provider: Option<ProviderId>) -> bool {
    should_onboard(
        cli_provider,
        std::env::var("GCM_PROVIDER").ok().as_deref(),
        any_cloud_key_set(),
    )
}

/// Pure onboarding decision (the body of [`needs_onboarding`]): no flag, no
/// non-blank `GCM_PROVIDER`, and no cloud key present. (Config-file presence is
/// handled upstream by [`load`] returning `Some`, which short-circuits this.)
#[doc(hidden)]
pub fn should_onboard(
    cli_provider: Option<ProviderId>,
    gcm_provider: Option<&str>,
    any_cloud_key: bool,
) -> bool {
    cli_provider.is_none() && gcm_provider.is_none_or(|s| s.trim().is_empty()) && !any_cloud_key
}

/// Whether any cloud provider's key env var is set and non-blank.
#[doc(hidden)]
pub fn any_cloud_key_set() -> bool {
    cloud_providers()
        .iter()
        .filter_map(|id| id.key_env_var())
        .any(env_nonblank)
}

// ── env bridge ──────────────────────────────────────────────────────────────

/// Bridge a loaded config into the (unchanged) provider layer by setting env
/// vars it has not already been given. Env always wins: a pre-set var is never
/// overwritten. Best-effort.
pub fn apply_to_env(config: &Config) {
    for (var, value) in env_plan(config, env_nonblank) {
        // edition 2021: `set_var` is safe, and hydration runs once at startup
        // before any provider call or thread spawn (design Assumptions).
        std::env::set_var(var, value);
    }
}

/// Pure planning for [`apply_to_env`]: given `is_set` (does this env var already
/// hold a non-blank value), return the `(var, value)` assignments to apply. Only
/// currently-unset vars are returned, so env precedence is preserved.
#[doc(hidden)]
pub fn env_plan(config: &Config, is_set: impl Fn(&str) -> bool) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for pc in &config.providers {
        match pc.id.auth_method() {
            AuthMethod::ApiKey => {
                if let Some(var) = pc.id.key_env_var() {
                    if let Some(key) = pc.key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
                        if !is_set(var) {
                            out.push((var, key.to_string()));
                        }
                    }
                }
            }
            AuthMethod::KeylessEndpoint => {
                // Ollama: set the base URL only when neither gcm's own var nor
                // the Ollama-native OLLAMA_HOST is already set.
                if let Some(ep) = pc.endpoint.as_deref().filter(|e| !e.trim().is_empty()) {
                    if !is_set("GCM_OLLAMA_BASE_URL") && !is_set("OLLAMA_HOST") {
                        out.push(("GCM_OLLAMA_BASE_URL", ep.to_string()));
                    }
                }
            }
            AuthMethod::KeylessAdc => {
                // Vertex: bridge project/location into the gcm-namespaced vars only
                // when unset (env still wins: flag > env > config > default).
                if let Some(p) = pc
                    .project
                    .as_deref()
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                {
                    if !is_set("GCM_VERTEX_PROJECT") {
                        out.push(("GCM_VERTEX_PROJECT", p.to_string()));
                    }
                }
                if let Some(l) = pc
                    .location
                    .as_deref()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                {
                    if !is_set("GCM_VERTEX_LOCATION") {
                        out.push(("GCM_VERTEX_LOCATION", l.to_string()));
                    }
                }
            }
        }
        // Bridge a config model into the provider's primary model env var, but
        // only when NONE of its model env vars is already set - any user-set var
        // (including an alias like GCM_GOOGLE_MODEL, which resolve_model honors)
        // must win, keeping precedence flag > env > config > default.
        if let Some(model) = pc.model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
            let vars = pc.id.model_env_vars();
            if !vars.iter().any(|v| is_set(v)) {
                out.push((vars[0], model.to_string()));
            }
        }
    }
    if !is_set("GCM_PROVIDER") {
        out.push(("GCM_PROVIDER", provider_token(config.default)));
    }
    out
}

// ── enabled-model whitelist + enforcement (CLO-516) ─────────────────────────

/// Canonicalize a model id for enabled-set comparison, per provider, so a value
/// that differs only by a provider alias is not falsely rejected: Gemini strips a
/// leading `models/` (its list endpoint returns prefixed names); Ollama treats a
/// tagless name as `:latest` (what `/api/tags` reports); all values are trimmed.
/// No general case-folding - model ids are case-sensitive.
#[doc(hidden)]
pub fn canonicalize_model(id: ProviderId, model: &str) -> String {
    let m = model.trim();
    match id {
        ProviderId::Google => m.strip_prefix("models/").unwrap_or(m).to_string(),
        ProviderId::Ollama if !m.contains(':') => format!("{m}:latest"),
        _ => m.to_string(),
    }
}

/// Enforce that `model` is in provider `id`'s enabled set. Returns `Ok` when the
/// provider has no entry, or an empty `models` (= unrestricted, the v1-migration /
/// pre-`gcm provider` state). A non-empty set rejects an out-of-set model with an
/// actionable message (compared after [`canonicalize_model`]).
#[doc(hidden)]
pub fn model_is_enabled(cfg: &Config, id: ProviderId, model: &str) -> Result<(), String> {
    let Some(pc) = cfg.providers.iter().find(|p| p.id == id) else {
        return Ok(());
    };
    if pc.models.is_empty() {
        return Ok(());
    }
    let want = canonicalize_model(id, model);
    if pc.models.iter().any(|m| canonicalize_model(id, m) == want) {
        Ok(())
    } else {
        Err(format!(
            "model '{model}' is not enabled for {}. Enabled: {}. \
             Run `gcm provider` to change the enabled models (or clear the list to allow any).",
            provider_token(id),
            pc.models.join(", ")
        ))
    }
}

/// Update exactly one provider in an existing config (add it if absent),
/// preserving every other provider verbatim; optionally make it the new default.
/// Pure (no I/O). The wizard (CLO-516) uses this so configuring one provider never
/// deletes the others' keys/endpoints/models. Always stamps the current version.
#[doc(hidden)]
pub fn merge_provider_config(
    existing: Option<&Config>,
    updated: ProviderConfig,
    make_default: bool,
) -> Config {
    let mut providers: Vec<ProviderConfig> =
        existing.map(|c| c.providers.clone()).unwrap_or_default();
    match providers.iter_mut().find(|p| p.id == updated.id) {
        Some(slot) => *slot = updated.clone(),
        None => providers.push(updated.clone()),
    }
    let default = if make_default {
        updated.id
    } else {
        existing.map(|c| c.default).unwrap_or(updated.id)
    };
    Config {
        conflict: ConflictConfig::default(),
        version: CONFIG_FORMAT_VERSION,
        default,
        providers,
    }
}

/// Carry forward each re-enabled provider's existing `models` whitelist (and inline
/// `model` default) from a prior config, so re-running the minimal onboarding wizard
/// (`gcm config` / `--reconfigure`) never erases a whitelist set by `gcm provider`.
/// Pure; only fills fields the wizard left empty.
#[doc(hidden)]
pub fn preserve_existing_models(enabled: &mut [ProviderConfig], existing: Option<&Config>) {
    let Some(prev) = existing else { return };
    for pc in enabled.iter_mut() {
        if let Some(prev_pc) = prev.providers.iter().find(|p| p.id == pc.id) {
            if pc.models.is_empty() {
                pc.models = prev_pc.models.clone();
            }
            if pc.model.is_none() {
                pc.model = prev_pc.model.clone();
            }
        }
    }
}

/// Assemble a validated `Config` from collected answers (pure; no I/O). Errors
/// if `default` is not among `enabled`.
#[doc(hidden)]
pub fn build_config(enabled: &[ProviderConfig], default: ProviderId) -> Result<Config, String> {
    if !enabled.iter().any(|p| p.id == default) {
        return Err(format!(
            "default provider {} is not among the enabled providers",
            provider_token(default)
        ));
    }
    Ok(Config {
        conflict: ConflictConfig::default(),
        version: CONFIG_FORMAT_VERSION,
        default,
        providers: enabled.to_vec(),
    })
}

/// Build the `ProviderConfig` for a cloud provider: `key: None` (env-only) when
/// the key env var is already set or the typed input is empty/whitespace;
/// otherwise the typed key inline.
#[doc(hidden)]
pub fn cloud_provider_config(
    id: ProviderId,
    env_present: bool,
    typed: Option<&str>,
) -> ProviderConfig {
    let key = if env_present {
        None
    } else {
        typed
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(String::from)
    };
    ProviderConfig {
        id,
        key,
        endpoint: None,
        model: None,
        models: Vec::new(),
        project: None,
        location: None,
    }
}

/// The multiselect candidate list (D7.3, wizard side): fetched ∪ current enabled ∪
/// current default, deduped, fetched first - so the user's existing selections and
/// default stay selectable even if the live list omitted them. Membership is by
/// canonical form (review L1), so a migrated `llama3` doesn't duplicate a fetched
/// `llama3:latest`. Pure.
#[doc(hidden)]
pub fn wizard_model_list(
    id: ProviderId,
    fetched: &[String],
    current_enabled: &[String],
    current_default: Option<&str>,
) -> Vec<String> {
    let mut out: Vec<String> = fetched.to_vec();
    let push_if_new = |m: &str, out: &mut Vec<String>| {
        let c = canonicalize_model(id, m);
        if !out.iter().any(|x| canonicalize_model(id, x) == c) {
            out.push(m.to_string());
        }
    };
    for m in current_enabled {
        push_if_new(m, &mut out);
    }
    if let Some(d) = current_default {
        push_if_new(d, &mut out);
    }
    out
}

/// Computes the hint for a model candidate in the wizard multiselect (AC5).
/// If the fetch succeeded (`Live`) but the candidate is not in the live list
/// (by canonical form), it gets a `not in live catalog` hint.
/// Otherwise, or if the source is `Fallback`, the hint is empty.
#[doc(hidden)]
pub fn wizard_model_hint(
    id: ProviderId,
    candidate: &str,
    source: &crate::provider::FetchSource,
    live_models: &[String],
) -> &'static str {
    match source {
        crate::provider::FetchSource::Fallback => "",
        crate::provider::FetchSource::Live => {
            let c = canonicalize_model(id, candidate);
            if live_models.iter().any(|m| canonicalize_model(id, m) == c) {
                ""
            } else {
                "not in live catalog"
            }
        }
    }
}

/// The pre-selected default model: the current default if it survived into
/// `selected` (canonical match, review L1), else the first selected (None only when
/// `selected` is empty). Returns the matching `selected` entry. Pure.
#[doc(hidden)]
pub fn initial_default_model(
    id: ProviderId,
    selected: &[String],
    current_default: Option<&str>,
) -> Option<String> {
    if let Some(d) = current_default {
        let c = canonicalize_model(id, d);
        if let Some(hit) = selected.iter().find(|m| canonicalize_model(id, m) == c) {
            return Some(hit.clone());
        }
    }
    selected.first().cloned()
}

/// The wizard's Ollama endpoint default, mirroring runtime precedence
/// (`GCM_OLLAMA_BASE_URL` > `OLLAMA_HOST` > saved config > default): a non-default
/// `effective` means an env override is present and wins over the saved config;
/// otherwise the saved config, else the default. Pure (review M2).
#[doc(hidden)]
pub fn ollama_wizard_default_endpoint(effective: &str, config_endpoint: Option<&str>) -> String {
    if effective != DEFAULT_OLLAMA_ENDPOINT {
        effective.to_string()
    } else {
        config_endpoint
            .map(str::to_string)
            .unwrap_or_else(|| effective.to_string())
    }
}

/// Assemble the wizard's `ProviderConfig` (pure), enforcing the AC-4 invariants so
/// they are unit-testable rather than only guaranteed by the cliclack flow: at
/// least one enabled model, and the default among them.
#[doc(hidden)]
pub fn build_provider_config(
    id: ProviderId,
    key: Option<String>,
    endpoint: Option<String>,
    default_model: String,
    models: Vec<String>,
) -> Result<ProviderConfig, String> {
    if models.is_empty() {
        return Err("at least one model must be enabled".to_string());
    }
    if !models.iter().any(|m| m == &default_model) {
        return Err(format!(
            "default model '{default_model}' is not among the enabled models"
        ));
    }
    Ok(ProviderConfig {
        id,
        key,
        endpoint,
        model: Some(default_model),
        models,
        project: None,
        location: None,
    })
}

/// Decide `(fetch_key, persist_key)` from a freshly-typed key: a blank entry is
/// "skip" (no key, nothing stored); a non-blank entry is used for the fetch and
/// stored inline. Pure (keeps the secret-handling rule unit-testable). Pure.
#[doc(hidden)]
pub fn wizard_persist_key(typed: &str) -> (Option<String>, Option<String>) {
    let t = typed.trim();
    if t.is_empty() {
        (None, None)
    } else {
        (Some(t.to_string()), Some(t.to_string()))
    }
}

// ── non-TTY guidance ────────────────────────────────────────────────────────

/// Render the non-TTY guidance: the `export` lines for each provider's key env
/// var plus a `config.toml` template, so an unattended first run can self-serve.
pub fn non_tty_instructions() -> String {
    let mut s = String::new();
    s.push_str(
        "gcm is not configured and there is no terminal available for interactive setup.\n\n",
    );
    s.push_str("Option A - export a provider key and select it, for example:\n");
    for id in cloud_providers() {
        if let Some(var) = id.key_env_var() {
            s.push_str(&format!("  export {var}=<your-key>\n"));
        }
    }
    s.push_str("  export GCM_PROVIDER=groq   # or google, vertex, openai, anthropic, ollama\n");
    s.push_str("  # vertex is keyless: set GCM_VERTEX_PROJECT + `gcloud auth application-default login` instead of a key\n\n");
    s.push_str("Option B - write a 0600 config file (see ADR-001 Decision 4 for the path):\n\n");
    s.push_str(sample_toml_template());
    s
}

/// A minimal, copy-pasteable `config.toml` template for the non-TTY path.
#[doc(hidden)]
pub fn sample_toml_template() -> &'static str {
    "version = 2\n\
     default = \"groq\"\n\
     \n\
     [[providers]]\n\
     id = \"groq\"\n\
     # key = \"<inline-secret>\"   # omit to read GROQ_API_KEY from the environment\n\
     # models = [\"openai/gpt-oss-120b\"]   # enabled set (only these usable); empty = any\n\
     \n\
     [[providers]]\n\
     id = \"ollama\"\n\
     endpoint = \"http://localhost:11434\"\n"
}

// ── small shared helpers ────────────────────────────────────────────────────

/// The five v1 providers, cloud first then Ollama (the wizard's menu order).
/// Every selectable provider, in wizard/reference display order (CLO-537 renamed this
/// from `cloud_then_ollama` and added Vertex; the old name implied a key-bearing/Ollama
/// dichotomy that no longer holds). This is the single source of truth iterated by the
/// reference template and both wizards - a provider absent here is invisible in the UI.
#[doc(hidden)]
pub fn all_providers() -> [ProviderId; 6] {
    [
        ProviderId::Groq,
        ProviderId::Google,
        ProviderId::Vertex,
        ProviderId::Openai,
        ProviderId::Anthropic,
        ProviderId::Ollama,
    ]
}

/// The four cloud (key-bearing) providers.
#[doc(hidden)]
pub fn cloud_providers() -> [ProviderId; 4] {
    [
        ProviderId::Groq,
        ProviderId::Google,
        ProviderId::Openai,
        ProviderId::Anthropic,
    ]
}

/// Human label for a provider in wizard prompts.
#[doc(hidden)]
pub fn provider_label(id: ProviderId) -> &'static str {
    match id {
        ProviderId::Groq => "Groq",
        ProviderId::Google => "Google (Gemini)",
        ProviderId::Openai => "OpenAI",
        ProviderId::Anthropic => "Anthropic",
        ProviderId::Ollama => "Ollama (local, no key)",
        ProviderId::Vertex => "Google (Vertex AI)",
    }
}

/// The lowercase token for a provider (the value written to TOML / `GCM_PROVIDER`).
#[doc(hidden)]
pub fn provider_token(id: ProviderId) -> String {
    serde_json::to_value(id)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "groq".to_string())
}

/// Read a non-empty, trimmed env var as a bool "is set".
#[doc(hidden)]
pub fn env_nonblank(name: &str) -> bool {
    env_value(name).is_some()
}

/// Read a non-empty, trimmed env var value, else `None`.
#[doc(hidden)]
pub fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Parse a comma/space-separated list of 1-based indices into deduped 0-based
/// indices, in input order. Errors on a non-number, an out-of-range value, or an
/// empty selection.
#[doc(hidden)]
pub fn parse_selection(input: &str, max: usize) -> Result<Vec<usize>, String> {
    let mut idxs: Vec<usize> = Vec::new();
    for tok in input
        .split([',', ' '])
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        let n: usize = tok
            .parse()
            .map_err(|_| format!("'{tok}' is not a number"))?;
        if n < 1 || n > max {
            return Err(format!("{n} is out of range 1..={max}"));
        }
        let i = n - 1;
        if !idxs.contains(&i) {
            idxs.push(i);
        }
    }
    if idxs.is_empty() {
        return Err("select at least one provider".to_string());
    }
    Ok(idxs)
}

/// Parse a single 1-based index in `1..=max` to a 0-based index, else `None`.
#[doc(hidden)]
pub fn parse_one(input: &str, max: usize) -> Option<usize> {
    let n: usize = input.trim().parse().ok()?;
    if n >= 1 && n <= max {
        Some(n - 1)
    } else {
        None
    }
}

// ── atomic private write (mirrors src/cache.rs) ─────────────────────────────

/// Atomic write with user-only permissions: a temp file created `0600` before
/// any content lands, then renamed over the target so it is never briefly
/// world-readable. Mirrors `cache::write_atomic`.
#[doc(hidden)]
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("config path has no parent"))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".config-{}.tmp", std::process::id()));
    {
        let mut f = open_private(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

#[cfg(unix)]
#[doc(hidden)]
pub fn open_private(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
#[doc(hidden)]
pub fn open_private(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
}

fn no_config_dir() -> io::Error {
    io::Error::other("no OS config directory available")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pc(id: ProviderId, key: Option<&str>, endpoint: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            id,
            key: key.map(String::from),
            endpoint: endpoint.map(String::from),
            model: None,
            models: Vec::new(),
            project: None,
            location: None,
        }
    }

    /// Like [`pc`] but with a `model` override, for the model-bridge tests.
    fn pcm(id: ProviderId, model: &str) -> ProviderConfig {
        ProviderConfig {
            id,
            key: None,
            endpoint: None,
            model: Some(model.to_string()),
            models: Vec::new(),
            project: None,
            location: None,
        }
    }

    /// Like [`pc`] but with an enabled-models whitelist, for the enforcement tests.
    fn pcw(id: ProviderId, default: Option<&str>, models: &[&str]) -> ProviderConfig {
        ProviderConfig {
            id,
            key: None,
            endpoint: None,
            model: default.map(String::from),
            models: models.iter().map(|s| s.to_string()).collect(),
            project: None,
            location: None,
        }
    }

    #[test]
    fn config_round_trips_toml() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Groq,
            providers: vec![
                pc(ProviderId::Groq, Some("sk-inline"), None),
                pc(ProviderId::Ollama, None, Some("http://localhost:11434")),
            ],
        };
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back = parse_config(&text).unwrap_or_else(|_| panic!("round-trips: {text}"));
        assert_eq!(back, cfg);
    }

    #[test]
    fn config_parses_array_of_tables() {
        let text = "version = 1\n\
                    default = \"groq\"\n\
                    \n\
                    [[providers]]\n\
                    id = \"groq\"\n\
                    key = \"sk-123\"\n\
                    \n\
                    [[providers]]\n\
                    id = \"ollama\"\n\
                    endpoint = \"http://localhost:11434\"\n";
        let cfg = parse_config(text).unwrap();
        assert_eq!(cfg.default, ProviderId::Groq);
        assert_eq!(cfg.providers.len(), 2);
        assert_eq!(cfg.providers[0].id, ProviderId::Groq);
        assert_eq!(cfg.providers[0].key.as_deref(), Some("sk-123"));
        assert_eq!(cfg.providers[1].id, ProviderId::Ollama);
        assert_eq!(
            cfg.providers[1].endpoint.as_deref(),
            Some("http://localhost:11434")
        );
    }

    #[test]
    fn missing_conflict_section_gets_documented_defaults() {
        // The `[conflict]` block is optional. Its absence must yield the
        // documented defaults (mergiraf on, temperature 0.1) - the derived
        // Default used to produce mergiraf=false / temperature=0.0 here.
        let text = "version = 2\n\
                    default = \"ollama\"\n\
                    \n\
                    [[providers]]\n\
                    id = \"ollama\"\n";
        let cfg = parse_config(text).unwrap();
        assert!(cfg.conflict.mergiraf, "mergiraf defaults to enabled");
        assert_eq!(cfg.conflict.temperature, 0.1);
        assert_eq!(cfg.conflict.auto_policy, AutoPolicy::Trivial);
        assert_eq!(cfg.conflict.max_rounds, 10, "round cap defaults to 10");
        // And the manual Default impl must agree with the serde defaults.
        assert_eq!(cfg.conflict, ConflictConfig::default());
    }

    #[test]
    fn partial_conflict_section_keeps_max_rounds_default() {
        // A `[conflict]` table written before CLO-554 has no max_rounds key;
        // the per-field serde default must fill it rather than zeroing it,
        // which would deadlock the loop driver on an impossible cap.
        let text = "version = 2\n\
                    default = \"ollama\"\n\
                    \n\
                    [[providers]]\n\
                    id = \"ollama\"\n\
                    \n\
                    [conflict]\n\
                    temperature = 0.3\n";
        let cfg = parse_config(text).unwrap();
        assert_eq!(cfg.conflict.temperature, 0.3);
        assert_eq!(cfg.conflict.max_rounds, 10);
    }

    #[test]
    fn explicit_max_rounds_is_honored() {
        let text = "version = 2\n\
                    default = \"ollama\"\n\
                    \n\
                    [[providers]]\n\
                    id = \"ollama\"\n\
                    \n\
                    [conflict]\n\
                    max_rounds = 3\n";
        let cfg = parse_config(text).unwrap();
        assert_eq!(cfg.conflict.max_rounds, 3);
    }

    #[test]
    fn key_none_is_env_some_is_inline() {
        // omitted key -> None (env-only); present key -> Some (inline secret)
        let text = "version = 1\n\
                    default = \"groq\"\n\
                    \n\
                    [[providers]]\n\
                    id = \"groq\"\n\
                    \n\
                    [[providers]]\n\
                    id = \"openai\"\n\
                    key = \"sk-x\"\n";
        let cfg = parse_config(text).unwrap();
        assert_eq!(cfg.providers[0].key, None);
        assert_eq!(cfg.providers[1].key.as_deref(), Some("sk-x"));
    }

    #[test]
    fn load_returns_none_on_malformed_toml() {
        assert!(matches!(
            parse_config("this is not = valid toml ["),
            Err(LoadIssue::Malformed(_))
        ));
    }

    #[test]
    fn load_returns_none_on_default_not_in_providers() {
        let text = "version = 1\n\
                    default = \"openai\"\n\
                    \n\
                    [[providers]]\n\
                    id = \"groq\"\n";
        assert!(matches!(
            parse_config(text),
            Err(LoadIssue::DefaultNotEnabled)
        ));
    }

    #[test]
    fn parse_config_rejects_wrong_version() {
        let text = "version = 0\n\
                    default = \"groq\"\n\
                    \n\
                    [[providers]]\n\
                    id = \"groq\"\n";
        assert!(matches!(parse_config(text), Err(LoadIssue::WrongVersion)));
    }

    #[test]
    fn config_path_honors_gcm_config_override() {
        let p = config_path_from(
            Some(OsStr::new("/tmp/hermetic-gcm")),
            Some(PathBuf::from("/home/u/.config/gcm")),
        )
        .unwrap();
        assert_eq!(p, PathBuf::from("/tmp/hermetic-gcm/config.toml"));
        // empty override falls through to the OS config dir
        let q = config_path_from(
            Some(OsStr::new("")),
            Some(PathBuf::from("/home/u/.config/gcm")),
        )
        .unwrap();
        assert_eq!(q, PathBuf::from("/home/u/.config/gcm/config.toml"));
        // no override, no dir -> None
        assert!(config_path_from(None, None).is_none());
    }

    #[test]
    fn needs_onboarding_matrix() {
        // no flag, no env hint -> onboard
        assert!(should_onboard(None, None, false));
        // a cloud key present -> not onboarding
        assert!(!should_onboard(None, None, true));
        // --provider set -> not onboarding
        assert!(!should_onboard(Some(ProviderId::Groq), None, false));
        // non-blank GCM_PROVIDER -> not onboarding
        assert!(!should_onboard(None, Some("ollama"), false));
        // blank/whitespace GCM_PROVIDER is treated as unset -> onboard
        assert!(should_onboard(None, Some("   "), false));
    }

    #[test]
    fn apply_to_env_does_not_override_existing() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: 1,
            default: ProviderId::Groq,
            providers: vec![pc(ProviderId::Groq, Some("sk-inline"), None)],
        };
        // GROQ_API_KEY already set -> not in the plan (env wins). GCM_PROVIDER
        // also pre-set -> not in the plan.
        let plan = env_plan(&cfg, |name| {
            name == "GROQ_API_KEY" || name == "GCM_PROVIDER"
        });
        assert!(plan.is_empty(), "nothing overridden, got {plan:?}");
    }

    #[test]
    fn apply_to_env_sets_inline_key_endpoint_and_default() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: 1,
            default: ProviderId::Groq,
            providers: vec![
                pc(ProviderId::Groq, Some("sk-inline"), None),
                pc(ProviderId::Ollama, None, Some("http://host:1234")),
            ],
        };
        // nothing set in the environment -> all three assignments planned
        let plan = env_plan(&cfg, |_| false);
        assert!(plan.contains(&("GROQ_API_KEY", "sk-inline".to_string())));
        assert!(plan.contains(&("GCM_OLLAMA_BASE_URL", "http://host:1234".to_string())));
        assert!(plan.contains(&("GCM_PROVIDER", "groq".to_string())));
    }

    #[test]
    fn apply_to_env_skips_ollama_url_when_ollama_host_set() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: 1,
            default: ProviderId::Ollama,
            providers: vec![pc(ProviderId::Ollama, None, Some("http://host:1234"))],
        };
        // OLLAMA_HOST present -> do not set GCM_OLLAMA_BASE_URL (Ollama-native wins)
        let plan = env_plan(&cfg, |name| name == "OLLAMA_HOST");
        assert!(!plan.iter().any(|(v, _)| *v == "GCM_OLLAMA_BASE_URL"));
    }

    #[test]
    fn env_plan_bridges_config_model_when_env_unset() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: 1,
            default: ProviderId::Openai,
            providers: vec![pcm(ProviderId::Openai, "gpt-x")],
        };
        let plan = env_plan(&cfg, |_| false);
        assert!(plan.contains(&("GCM_OPENAI_MODEL", "gpt-x".to_string())));
    }

    #[test]
    fn env_plan_yields_to_real_model_env_var() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: 1,
            default: ProviderId::Openai,
            providers: vec![pcm(ProviderId::Openai, "gpt-x")],
        };
        // GCM_OPENAI_MODEL already set -> config model is not bridged (env wins).
        let plan = env_plan(&cfg, |name| name == "GCM_OPENAI_MODEL");
        assert!(!plan.iter().any(|(v, _)| *v == "GCM_OPENAI_MODEL"));
    }

    #[test]
    fn env_plan_config_model_yields_to_google_alias_env() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: 1,
            default: ProviderId::Google,
            providers: vec![pcm(ProviderId::Google, "cfg-model")],
        };
        // Only the alias GCM_GOOGLE_MODEL is set (not the primary). The user's env
        // must win, so the config model is NOT bridged into GCM_GEMINI_MODEL -
        // otherwise resolve_model would read the primary first and override the
        // alias, violating env > config.
        let plan = env_plan(&cfg, |name| name == "GCM_GOOGLE_MODEL");
        assert!(
            !plan.iter().any(|(v, _)| *v == "GCM_GEMINI_MODEL"),
            "config model must not override the alias env var: {plan:?}"
        );
    }

    #[test]
    fn env_plan_bridges_google_model_to_primary_var() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: 1,
            default: ProviderId::Google,
            providers: vec![pcm(ProviderId::Google, "gemini-x")],
        };
        // Google's primary model var is GCM_GEMINI_MODEL (not the GOOGLE alias).
        let plan = env_plan(&cfg, |_| false);
        assert!(plan.contains(&("GCM_GEMINI_MODEL", "gemini-x".to_string())));
    }

    #[test]
    fn env_plan_bridges_vertex_project_and_location() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Vertex,
            providers: vec![ProviderConfig {
                id: ProviderId::Vertex,
                key: None,
                endpoint: None,
                model: None,
                models: Vec::new(),
                project: Some("my-proj".to_string()),
                location: Some("us-central1".to_string()),
            }],
        };
        // Nothing set -> both project and location bridge to the gcm-namespaced vars.
        let plan = env_plan(&cfg, |_| false);
        assert!(plan.contains(&("GCM_VERTEX_PROJECT", "my-proj".to_string())));
        assert!(plan.contains(&("GCM_VERTEX_LOCATION", "us-central1".to_string())));
        // A pre-set env var wins and is never overwritten (flag > env > config).
        let plan2 = env_plan(&cfg, |v| v == "GCM_VERTEX_PROJECT");
        assert!(!plan2.iter().any(|(k, _)| *k == "GCM_VERTEX_PROJECT"));
        assert!(plan2.contains(&("GCM_VERTEX_LOCATION", "us-central1".to_string())));
    }

    #[test]
    fn vertex_project_location_round_trip_and_skip_when_none() {
        // With values -> serialized and read back unchanged.
        let with = ProviderConfig {
            id: ProviderId::Vertex,
            key: None,
            endpoint: None,
            model: None,
            models: Vec::new(),
            project: Some("p".to_string()),
            location: Some("us-west1".to_string()),
        };
        let text = toml::to_string_pretty(&with).unwrap();
        assert!(text.contains("project = \"p\""), "{text}");
        assert!(text.contains("location = \"us-west1\""), "{text}");
        assert_eq!(toml::from_str::<ProviderConfig>(&text).unwrap(), with);
        // None -> both keys skip-serialize (a pre-Vertex file needs no version bump).
        let without = pc(ProviderId::Openai, None, None);
        let text2 = toml::to_string_pretty(&without).unwrap();
        assert!(!text2.contains("project"), "{text2}");
        assert!(!text2.contains("location"), "{text2}");
        // A pre-Vertex file (no project/location keys) still parses.
        let parsed: ProviderConfig = toml::from_str("id = \"openai\"\n").unwrap();
        assert_eq!(parsed.project, None);
        assert_eq!(parsed.location, None);
    }

    #[test]
    fn render_config_includes_live_values_and_commented_reference() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Openai,
            providers: vec![pc(ProviderId::Openai, None, None)],
        };
        let text = render_config(&cfg).expect("renders");
        // The live config still parses - the parser ignores the comment block.
        let back = parse_config(&text).expect("rendered config parses");
        assert_eq!(back.default, ProviderId::Openai);
        assert_eq!(back.providers.len(), 1);
        // The reference block documents the knobs + every provider + the env note.
        assert!(text.contains("Reference"), "{text}");
        assert!(text.contains("# model ="), "{text}");
        assert!(text.contains("# endpoint ="), "{text}");
        assert!(
            text.contains("gpt-5.6-terra"),
            "openai default in reference: {text}"
        );
        assert!(
            text.contains("GCM_OPENAI_MODEL"),
            "env override note: {text}"
        );
        assert!(text.contains("ollama"), "{text}");
    }

    #[test]
    fn config_round_trips_model_field() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Openai,
            providers: vec![pcm(ProviderId::Openai, "gpt-5.6-terra")],
        };
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back = parse_config(&text).unwrap();
        assert_eq!(back.providers[0].model.as_deref(), Some("gpt-5.6-terra"));
    }

    #[test]
    fn build_config_rejects_default_not_enabled() {
        let enabled = vec![pc(ProviderId::Groq, None, None)];
        assert!(build_config(&enabled, ProviderId::Openai).is_err());
        assert!(build_config(&enabled, ProviderId::Groq).is_ok());
    }

    #[test]
    fn build_config_records_env_when_key_already_set() {
        // env present -> key None (env-only), even if a key were typed
        let p = cloud_provider_config(ProviderId::Groq, true, Some("ignored"));
        assert_eq!(p.key, None);
        assert_eq!(p.id, ProviderId::Groq);
    }

    #[test]
    fn build_config_treats_empty_key_as_env_only() {
        assert_eq!(
            cloud_provider_config(ProviderId::Groq, false, Some("   ")).key,
            None
        );
        assert_eq!(
            cloud_provider_config(ProviderId::Openai, false, Some("sk-real")).key,
            Some("sk-real".to_string())
        );
    }

    #[test]
    fn non_tty_instructions_lists_each_enabled_provider() {
        let out = non_tty_instructions();
        // a TOML template...
        assert!(out.contains("version = 2"), "{out}");
        assert!(out.contains("[[providers]]"), "{out}");
        // ...and an export line per cloud provider key
        for var in [
            "GROQ_API_KEY",
            "GEMINI_API_KEY",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
        ] {
            assert!(
                out.contains(&format!("export {var}=")),
                "missing {var}: {out}"
            );
        }
    }

    #[test]
    fn save_to_overwrites_without_duplicating_providers() {
        // reconfigure idempotency: a second save replaces the file cleanly, no
        // duplicate [[providers]] tables, and load reflects the new config.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        let first = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Groq,
            providers: vec![pc(ProviderId::Groq, Some("k1"), None)],
        };
        save_to(&path, &first).unwrap();
        let second = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Openai,
            providers: vec![pc(ProviderId::Openai, Some("k2"), None)],
        };
        save_to(&path, &second).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        // Count only active table headers - the commented reference block also
        // contains `# [[providers]]` lines, which are documentation, not tables.
        let active_tables = text
            .lines()
            .filter(|l| l.trim_start() == "[[providers]]")
            .count();
        assert_eq!(active_tables, 1, "no duplicate provider tables: {text}");
        assert_eq!(load_from(&path).unwrap(), second);
    }

    #[cfg(unix)]
    #[test]
    fn load_warns_on_world_readable_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        fs::write(&path, "version = 1\n").unwrap();
        // 0600 -> secure (None)
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(insecure_permissions(&path).is_none());
        // 0644 -> insecure (group/other readable)
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(insecure_permissions(&path).is_some());
    }

    #[test]
    fn provider_token_is_lowercase_canonical() {
        assert_eq!(provider_token(ProviderId::Google), "google");
        assert_eq!(provider_token(ProviderId::Ollama), "ollama");
    }

    #[test]
    fn parse_selection_dedupes_and_validates() {
        assert_eq!(parse_selection("1, 3, 1", 5).unwrap(), vec![0, 2]);
        assert_eq!(parse_selection("2 4", 5).unwrap(), vec![1, 3]);
        assert!(parse_selection("", 5).is_err());
        assert!(parse_selection("9", 5).is_err());
        assert!(parse_selection("x", 5).is_err());
    }

    #[test]
    fn parse_one_in_range() {
        assert_eq!(parse_one(" 2 ", 3), Some(1));
        assert_eq!(parse_one("0", 3), None);
        assert_eq!(parse_one("4", 3), None);
        assert_eq!(parse_one("z", 3), None);
    }

    #[test]
    fn save_then_load_round_trips_to_disk() {
        // Exercises the atomic 0600 write + load file behavior hermetically via
        // an explicit temp path (no GCM_CONFIG env mutation, so no cross-test
        // env race).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);

        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Ollama,
            providers: vec![
                pc(ProviderId::Groq, Some("sk-inline"), None),
                pc(ProviderId::Ollama, None, Some("http://host:1234")),
            ],
        };
        save_to(&path, &cfg).unwrap();

        assert!(path.is_file(), "config written");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "config is 0600");
        }
        let back = load_from(&path).expect("loads back");
        assert_eq!(back, cfg);
    }

    #[test]
    fn load_from_absent_path_is_silent_miss() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_from(&dir.path().join("nope.toml")).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn load_from_rejects_world_readable_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        fs::write(
            &path,
            "version = 1\ndefault = \"groq\"\n\n[[providers]]\nid = \"groq\"\n",
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load_from(&path).is_none(), "0644 file is ignored");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(load_from(&path).is_some(), "0600 file loads");
    }

    // ── CLO-516: v2 migration, enabled-model enforcement, provider merge ──────

    #[test]
    fn migration_v1_config_loads_and_stamps_v2() {
        // A pre-CLO-516 v1 file must load without error and be migrated up: the
        // version is stamped to the current format and `models` defaults empty
        // (= unrestricted), so a v1 user's free-form model keeps working.
        let cfg = parse_config("version = 1\ndefault = \"groq\"\n\n[[providers]]\nid = \"groq\"\n")
            .expect("v1 migrates");
        assert_eq!(cfg.version, CONFIG_FORMAT_VERSION);
        assert!(cfg.providers[0].models.is_empty());
    }

    #[test]
    fn migration_rejects_unknown_versions() {
        // 0 and any version newer than this binary are a "no usable config" miss.
        assert!(matches!(
            parse_config("version = 0\ndefault = \"groq\"\n\n[[providers]]\nid = \"groq\"\n"),
            Err(LoadIssue::WrongVersion)
        ));
        assert!(matches!(
            parse_config("version = 3\ndefault = \"groq\"\n\n[[providers]]\nid = \"groq\"\n"),
            Err(LoadIssue::WrongVersion)
        ));
    }

    #[test]
    fn v2_config_round_trips_models() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Openai,
            providers: vec![pcw(
                ProviderId::Openai,
                Some("gpt-5.6-terra"),
                &["gpt-5.6-terra", "gpt-5.6-luna"],
            )],
        };
        let text = render_config(&cfg).unwrap();
        let back = parse_config(&text).unwrap();
        assert_eq!(
            back.providers[0].models,
            vec!["gpt-5.6-terra", "gpt-5.6-luna"]
        );
        assert_eq!(back.version, CONFIG_FORMAT_VERSION);
    }

    #[test]
    fn render_config_forces_current_version_from_v1() {
        // Even if an in-memory config still carries version 1, the serialized file
        // is the current format (closes the version-write trap).
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: 1,
            default: ProviderId::Groq,
            providers: vec![pc(ProviderId::Groq, None, None)],
        };
        let text = render_config(&cfg).unwrap();
        assert!(text.contains("version = 2"), "forces v2: {text}");
        assert!(!text.contains("version = 1"), "no stale v1: {text}");
    }

    #[test]
    fn model_is_enabled_empty_set_is_unrestricted() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Groq,
            providers: vec![pc(ProviderId::Groq, None, None)], // models empty
        };
        assert!(model_is_enabled(&cfg, ProviderId::Groq, "anything-goes").is_ok());
        // a provider with no config entry at all is also unrestricted
        assert!(model_is_enabled(&cfg, ProviderId::Openai, "whatever").is_ok());
    }

    #[test]
    fn model_is_enabled_non_empty_set_enforces_membership() {
        let cfg = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Openai,
            providers: vec![pcw(
                ProviderId::Openai,
                Some("gpt-5.6-terra"),
                &["gpt-5.6-terra", "gpt-5.6-luna"],
            )],
        };
        assert!(model_is_enabled(&cfg, ProviderId::Openai, "gpt-5.6-terra").is_ok());
        let err = model_is_enabled(&cfg, ProviderId::Openai, "dall-e-3").unwrap_err();
        assert!(err.contains("dall-e-3"), "names offender: {err}");
        assert!(err.contains("gpt-5.6-terra"), "lists set: {err}");
        assert!(err.contains("gcm provider"), "actionable: {err}");
    }

    #[test]
    fn model_is_enabled_canonicalizes_ollama_tag_and_gemini_prefix() {
        // Ollama: a tagless `--model` matches an enabled `:latest` entry.
        let ollama = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Ollama,
            providers: vec![pcw(
                ProviderId::Ollama,
                Some("llama3:latest"),
                &["llama3:latest"],
            )],
        };
        assert!(model_is_enabled(&ollama, ProviderId::Ollama, "llama3").is_ok());
        assert!(model_is_enabled(&ollama, ProviderId::Ollama, "llama3:latest").is_ok());
        // Gemini: the `models/`-prefixed list value matches the bare resolved id.
        let gem = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Google,
            providers: vec![pcw(
                ProviderId::Google,
                Some("gemini-x"),
                &["models/gemini-x"],
            )],
        };
        assert!(model_is_enabled(&gem, ProviderId::Google, "gemini-x").is_ok());
        assert!(model_is_enabled(&gem, ProviderId::Google, "models/gemini-x").is_ok());
    }

    #[test]
    fn canonicalize_model_rules() {
        assert_eq!(canonicalize_model(ProviderId::Google, "models/g"), "g");
        assert_eq!(canonicalize_model(ProviderId::Google, "g"), "g");
        assert_eq!(
            canonicalize_model(ProviderId::Ollama, "llama3"),
            "llama3:latest"
        );
        assert_eq!(
            canonicalize_model(ProviderId::Ollama, "llama3:8b"),
            "llama3:8b"
        );
        assert_eq!(canonicalize_model(ProviderId::Openai, "  gpt-x  "), "gpt-x");
    }

    #[test]
    fn merge_provider_config_preserves_others_and_sets_default() {
        let existing = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Groq,
            providers: vec![
                pcw(ProviderId::Groq, Some("g"), &["g"]),
                pc(ProviderId::Ollama, None, Some("http://h:1")),
            ],
        };
        let updated = pcw(ProviderId::Groq, Some("g2"), &["g2", "g3"]);
        let merged = merge_provider_config(Some(&existing), updated, true);
        assert_eq!(merged.version, CONFIG_FORMAT_VERSION);
        assert_eq!(merged.default, ProviderId::Groq);
        // Groq slot updated...
        let groq = merged
            .providers
            .iter()
            .find(|p| p.id == ProviderId::Groq)
            .unwrap();
        assert_eq!(groq.models, vec!["g2", "g3"]);
        // ...Ollama preserved verbatim.
        let ollama = merged
            .providers
            .iter()
            .find(|p| p.id == ProviderId::Ollama)
            .unwrap();
        assert_eq!(ollama.endpoint.as_deref(), Some("http://h:1"));
    }

    #[test]
    fn merge_provider_config_appends_absent_and_handles_no_existing() {
        // append when absent
        let existing = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Groq,
            providers: vec![pc(ProviderId::Groq, Some("k"), None)],
        };
        let merged = merge_provider_config(
            Some(&existing),
            pcw(ProviderId::Openai, Some("o"), &["o"]),
            false,
        );
        assert_eq!(merged.providers.len(), 2);
        assert_eq!(
            merged.default,
            ProviderId::Groq,
            "make_default=false keeps prior default"
        );
        // no existing config -> just the updated provider, it becomes default
        let fresh = merge_provider_config(None, pcw(ProviderId::Openai, Some("o"), &["o"]), true);
        assert_eq!(fresh.providers.len(), 1);
        assert_eq!(fresh.default, ProviderId::Openai);
    }

    #[test]
    fn preserve_existing_models_carries_forward_whitelist_and_default() {
        // a freshly-built (empty models) enabled set...
        let mut enabled = vec![
            pc(ProviderId::Openai, None, None),
            pc(ProviderId::Groq, None, None),
        ];
        // ...against a prior config where openai had a whitelist + default model.
        let prev = Config {
            conflict: ConflictConfig::default(),
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Openai,
            providers: vec![pcw(
                ProviderId::Openai,
                Some("gpt-5.6-terra"),
                &["gpt-5.6-terra", "gpt-5.6-luna"],
            )],
        };
        preserve_existing_models(&mut enabled, Some(&prev));
        let openai = enabled.iter().find(|p| p.id == ProviderId::Openai).unwrap();
        assert_eq!(
            openai.models,
            vec!["gpt-5.6-terra", "gpt-5.6-luna"],
            "whitelist preserved"
        );
        assert_eq!(
            openai.model.as_deref(),
            Some("gpt-5.6-terra"),
            "default preserved"
        );
        // groq had no prior entry -> unchanged (empty)
        let groq = enabled.iter().find(|p| p.id == ProviderId::Groq).unwrap();
        assert!(groq.models.is_empty());
    }

    // ── CLO-516: pure `gcm provider` wizard helpers ──────────────────────────

    #[test]
    fn wizard_model_list_unions_fetched_enabled_and_default() {
        let id = ProviderId::Openai;
        let fetched = vec!["a".to_string(), "b".to_string()];
        let enabled = vec!["b".to_string(), "c".to_string()]; // c not in fetched
                                                              // d is the current default, present in neither -> appended last
        let list = wizard_model_list(id, &fetched, &enabled, Some("d"));
        assert_eq!(
            list,
            vec!["a", "b", "c", "d"],
            "fetched first, then missing enabled, then default"
        );
        // no duplicates when the default is already present
        assert_eq!(
            wizard_model_list(id, &fetched, &[], Some("a")),
            vec!["a", "b"]
        );
    }

    #[test]
    fn wizard_model_list_dedupes_by_canonical_form() {
        // Ollama: a migrated tagless `llama3` must not duplicate a fetched
        // `llama3:latest` (review L1).
        let fetched = vec!["llama3:latest".to_string()];
        let enabled = vec!["llama3".to_string()];
        let list = wizard_model_list(ProviderId::Ollama, &fetched, &enabled, Some("llama3"));
        assert_eq!(
            list,
            vec!["llama3:latest"],
            "canonical dedupe keeps the fetched form"
        );
    }

    #[test]
    fn wizard_model_hint_flags_absent_live_models_by_canonical_form() {
        let live = vec!["llama3:latest".to_string(), "gemma".to_string()];

        // fallback source: always empty hint
        assert_eq!(
            wizard_model_hint(
                ProviderId::Ollama,
                "llama3",
                &crate::provider::FetchSource::Fallback,
                &live
            ),
            ""
        );

        // live source: present models get empty hint
        assert_eq!(
            wizard_model_hint(
                ProviderId::Ollama,
                "gemma",
                &crate::provider::FetchSource::Live,
                &live
            ),
            ""
        );

        // live source: absent models get warning
        assert_eq!(
            wizard_model_hint(
                ProviderId::Ollama,
                "absent",
                &crate::provider::FetchSource::Live,
                &live
            ),
            "not in live catalog"
        );

        // canonical matching: llama3 matches llama3:latest
        assert_eq!(
            wizard_model_hint(
                ProviderId::Ollama,
                "llama3",
                &crate::provider::FetchSource::Live,
                &live
            ),
            ""
        );
    }

    #[test]
    fn initial_default_model_prefers_current_then_first() {
        let id = ProviderId::Openai;
        let selected = vec!["x".to_string(), "y".to_string()];
        assert_eq!(
            initial_default_model(id, &selected, Some("y")).as_deref(),
            Some("y")
        );
        // current default no longer selected -> fall back to the first selected
        assert_eq!(
            initial_default_model(id, &selected, Some("z")).as_deref(),
            Some("x")
        );
        assert_eq!(
            initial_default_model(id, &selected, None).as_deref(),
            Some("x")
        );
        assert_eq!(initial_default_model(id, &[], Some("z")), None);
        // canonical match: a tagless current default selects the `:latest` entry
        let sel = vec!["llama3:latest".to_string()];
        assert_eq!(
            initial_default_model(ProviderId::Ollama, &sel, Some("llama3")).as_deref(),
            Some("llama3:latest")
        );
    }

    #[test]
    fn ollama_wizard_default_endpoint_env_beats_config() {
        // env override present (effective != default) -> wins over saved config
        assert_eq!(
            ollama_wizard_default_endpoint("http://env:1", Some("http://cfg:2")),
            "http://env:1"
        );
        // no env override -> saved config, else the default
        assert_eq!(
            ollama_wizard_default_endpoint(DEFAULT_OLLAMA_ENDPOINT, Some("http://cfg:2")),
            "http://cfg:2"
        );
        assert_eq!(
            ollama_wizard_default_endpoint(DEFAULT_OLLAMA_ENDPOINT, None),
            DEFAULT_OLLAMA_ENDPOINT
        );
    }

    #[test]
    fn build_provider_config_enforces_ac4_invariants() {
        // default must be among the enabled models, and >=1 enabled
        assert!(build_provider_config(
            ProviderId::Openai,
            Some("k".into()),
            None,
            "gpt-x".into(),
            vec![]
        )
        .is_err());
        assert!(build_provider_config(
            ProviderId::Openai,
            None,
            None,
            "gpt-x".into(),
            vec!["gpt-y".into()]
        )
        .is_err());
        let ok = build_provider_config(
            ProviderId::Openai,
            None,
            None,
            "gpt-x".into(),
            vec!["gpt-x".into(), "gpt-y".into()],
        )
        .unwrap();
        assert_eq!(ok.model.as_deref(), Some("gpt-x"));
        assert_eq!(ok.models, vec!["gpt-x", "gpt-y"]);
    }

    #[test]
    fn wizard_persist_key_blank_is_skip_else_inline() {
        assert_eq!(wizard_persist_key("   "), (None, None));
        let (fetch, persist) = wizard_persist_key("  sk-123 ");
        assert_eq!(fetch.as_deref(), Some("sk-123"));
        assert_eq!(persist.as_deref(), Some("sk-123"));
    }
}
