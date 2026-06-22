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
use std::process::{Command, Stdio};
use std::time::Duration;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::error::GcmError;
use crate::provider::ProviderId;

/// On-disk config format version (mirrors `cache::CacheFile.version`); a mismatch
/// on read is treated as "no usable config" so a future schema can evolve.
const CONFIG_FORMAT_VERSION: u32 = 1;
/// Config file name within the config dir (or the `GCM_CONFIG` override dir).
const CONFIG_FILE_NAME: &str = "config.toml";
/// Default Ollama endpoint (mirrors `provider::ollama`'s default base URL).
const DEFAULT_OLLAMA_ENDPOINT: &str = "http://localhost:11434";
/// Connection timeout for the wizard's Ollama daemon probe (ADR-001 Decision 8):
/// short enough that an unresponsive endpoint never hangs the wizard.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Persisted configuration, written as TOML to `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub version: u32,
    /// Provider used when neither `--provider` nor `GCM_PROVIDER` is set.
    pub default: ProviderId,
    /// Every provider the user enabled during onboarding.
    pub providers: Vec<ProviderConfig>,
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
}

/// Why a present config file is not usable; drives the stderr warning in [`load`].
#[derive(Debug)]
enum LoadIssue {
    Malformed(String),
    WrongVersion,
    DefaultNotEnabled,
}

// ── path resolution ────────────────────────────────────────────────────────

/// `$GCM_CONFIG/config.toml` if the override is set (tests / relocation, per
/// ADR-001 Decision 4), else the OS config dir via `directories` (mirrors
/// `cache::cache_dir`). `None` if no config dir can be determined.
pub fn config_path() -> Option<PathBuf> {
    config_path_from(
        std::env::var_os("GCM_CONFIG").as_deref(),
        ProjectDirs::from("", "", "gcm").map(|d| d.config_dir().to_path_buf()),
    )
}

/// Pure path resolution (the body of [`config_path`], so the override precedence
/// is unit-testable without touching process env or the real config dir).
fn config_path_from(gcm_config: Option<&OsStr>, fallback_dir: Option<PathBuf>) -> Option<PathBuf> {
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
fn load_from(path: &Path) -> Option<Config> {
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
fn parse_config(data: &str) -> Result<Config, LoadIssue> {
    let cfg: Config = toml::from_str(data).map_err(|e| LoadIssue::Malformed(e.to_string()))?;
    if cfg.version != CONFIG_FORMAT_VERSION {
        return Err(LoadIssue::WrongVersion);
    }
    if !cfg.providers.iter().any(|p| p.id == cfg.default) {
        return Err(LoadIssue::DefaultNotEnabled);
    }
    Ok(cfg)
}

/// `Some(reason)` when the file's permissions are wider than user-only on Unix
/// (any group/other bit set); `None` when `0600`-equivalent or off-Unix.
#[cfg(unix)]
fn insecure_permissions(path: &Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path).ok()?.permissions().mode();
    if mode & 0o077 != 0 {
        Some(format!("mode {:o}, expected 600", mode & 0o777))
    } else {
        None
    }
}

#[cfg(not(unix))]
fn insecure_permissions(_path: &Path) -> Option<String> {
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
fn save_to(path: &Path, config: &Config) -> io::Result<()> {
    let text = toml::to_string_pretty(config).map_err(io::Error::other)?;
    write_atomic(path, text.as_bytes())
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
fn should_onboard(
    cli_provider: Option<ProviderId>,
    gcm_provider: Option<&str>,
    any_cloud_key: bool,
) -> bool {
    cli_provider.is_none() && gcm_provider.is_none_or(|s| s.trim().is_empty()) && !any_cloud_key
}

/// Whether any cloud provider's key env var is set and non-blank.
fn any_cloud_key_set() -> bool {
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
fn env_plan(config: &Config, is_set: impl Fn(&str) -> bool) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for pc in &config.providers {
        match pc.id.key_env_var() {
            Some(var) => {
                if let Some(key) = pc.key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
                    if !is_set(var) {
                        out.push((var, key.to_string()));
                    }
                }
            }
            None => {
                // Ollama: set the base URL only when neither gcm's own var nor
                // the Ollama-native OLLAMA_HOST is already set.
                if let Some(ep) = pc.endpoint.as_deref().filter(|e| !e.trim().is_empty()) {
                    if !is_set("GCM_OLLAMA_BASE_URL") && !is_set("OLLAMA_HOST") {
                        out.push(("GCM_OLLAMA_BASE_URL", ep.to_string()));
                    }
                }
            }
        }
    }
    if !is_set("GCM_PROVIDER") {
        out.push(("GCM_PROVIDER", provider_token(config.default)));
    }
    out
}

// ── interactive wizard ──────────────────────────────────────────────────────

/// Run the interactive wizard end to end (enable providers, capture keys from
/// the environment or a prompt, choose a default) and return the assembled
/// `Config`. Cloud keys already exported are recorded as `key: None` (env-only);
/// an empty key input is also env-only. Invalid menu selections re-prompt.
pub fn run_wizard() -> Result<Config, GcmError> {
    let all = cloud_then_ollama();
    eprintln!("gcm first-run setup");
    eprintln!(
        "Pick the provider(s) you want to use. You can re-run this anytime with `gcm config`.\n"
    );

    // 1. Choose which providers to enable (re-prompt until at least one valid).
    let selected = loop {
        for (i, id) in all.iter().enumerate() {
            eprintln!("  {}. {}", i + 1, provider_label(*id));
        }
        let input = wizard_read_line("Enable which providers? (comma-separated numbers): ")?;
        match parse_selection(&input, all.len()) {
            Ok(idxs) => break idxs,
            Err(msg) => eprintln!("  {msg}. Try again.\n"),
        }
    };

    // 2. Capture each enabled provider's key (env or prompt) or Ollama endpoint.
    let mut enabled: Vec<ProviderConfig> = Vec::new();
    for idx in selected {
        let id = all[idx];
        match id.key_env_var() {
            Some(var) => {
                if env_nonblank(var) {
                    eprintln!(
                        "  {} key found in {var} - using the environment variable.",
                        provider_label(id)
                    );
                    enabled.push(cloud_provider_config(id, true, None));
                } else {
                    let typed = read_secret(&format!(
                        "  Enter the {} API key for {} (or press Enter to set {var} yourself later): ",
                        var,
                        provider_label(id)
                    ))
                    .map_err(|e| GcmError::Git(format!("could not read key input: {e}")))?;
                    enabled.push(cloud_provider_config(id, false, Some(&typed)));
                }
            }
            None => {
                let endpoint = prompt_ollama_endpoint()?;
                enabled.push(ProviderConfig {
                    id,
                    key: None,
                    endpoint,
                });
            }
        }
    }

    // 3. Choose the default from the enabled set (re-prompt until valid).
    let default = loop {
        eprintln!("\nWhich provider should be the default?");
        for (i, pc) in enabled.iter().enumerate() {
            eprintln!("  {}. {}", i + 1, provider_label(pc.id));
        }
        let input = wizard_read_line("Default provider (number): ")?;
        match parse_one(&input, enabled.len()) {
            Some(i) => break enabled[i].id,
            None => eprintln!("  Please enter a number from the list."),
        }
    };

    build_config(&enabled, default).map_err(|msg| {
        // Unreachable: `default` is chosen from `enabled`. Surfaced defensively.
        eprintln!("gcm: {msg}");
        GcmError::OnboardingRequired
    })
}

/// Prompt for the Ollama endpoint (default offered), validate it, probe the
/// daemon, and return `Some(endpoint)` when non-default (so the file stays
/// minimal) or `None` for the default.
fn prompt_ollama_endpoint() -> Result<Option<String>, GcmError> {
    // Seed the default + probe from the effective runtime endpoint so an
    // existing OLLAMA_HOST / GCM_OLLAMA_BASE_URL is honored (not ignored).
    let effective = effective_ollama_endpoint();
    let url = loop {
        let input = wizard_read_line(&format!("  Ollama endpoint [{effective}]: "))?;
        let raw = input.trim();
        if raw.is_empty() {
            break effective.clone();
        }
        match validate_endpoint_url(raw) {
            Ok(u) => break u,
            Err(msg) => eprintln!("  {msg}"),
        }
    };
    if probe_ollama(&url) {
        eprintln!("  Ollama is reachable at {url}.");
    } else {
        eprintln!(
            "  Warning: could not reach Ollama at {url} within {}s. Start it with `ollama serve` \
             (or set OLLAMA_HOST). Saving the choice anyway.",
            PROBE_TIMEOUT.as_secs()
        );
    }
    Ok(if url == DEFAULT_OLLAMA_ENDPOINT {
        None
    } else {
        Some(url)
    })
}

/// Assemble a validated `Config` from collected answers (pure; no I/O). Errors
/// if `default` is not among `enabled`.
fn build_config(enabled: &[ProviderConfig], default: ProviderId) -> Result<Config, String> {
    if !enabled.iter().any(|p| p.id == default) {
        return Err(format!(
            "default provider {} is not among the enabled providers",
            provider_token(default)
        ));
    }
    Ok(Config {
        version: CONFIG_FORMAT_VERSION,
        default,
        providers: enabled.to_vec(),
    })
}

/// Build the `ProviderConfig` for a cloud provider: `key: None` (env-only) when
/// the key env var is already set or the typed input is empty/whitespace;
/// otherwise the typed key inline.
fn cloud_provider_config(id: ProviderId, env_present: bool, typed: Option<&str>) -> ProviderConfig {
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
    s.push_str("  export GCM_PROVIDER=groq   # or google, openai, anthropic, ollama\n\n");
    s.push_str("Option B - write a 0600 config file (see ADR-001 Decision 4 for the path):\n\n");
    s.push_str(sample_toml_template());
    s
}

/// A minimal, copy-pasteable `config.toml` template for the non-TTY path.
fn sample_toml_template() -> &'static str {
    "version = 1\n\
     default = \"groq\"\n\
     \n\
     [[providers]]\n\
     id = \"groq\"\n\
     # key = \"<inline-secret>\"   # omit to read GROQ_API_KEY from the environment\n\
     \n\
     [[providers]]\n\
     id = \"ollama\"\n\
     endpoint = \"http://localhost:11434\"\n"
}

// ── secret entry (echo-suppressed) ──────────────────────────────────────────

/// RAII guard that disables terminal echo on creation and restores it on drop -
/// covering the normal return path and an unwinding panic (mirroring `ui`'s
/// shell-out idiom). Best-effort: if `stty` is unavailable the guard is a no-op.
/// A hard kill that bypasses destructors (a default `SIGINT`/`SIGTERM`, or a
/// panic under `panic = "abort"`) can still leave echo off; recover with
/// `stty echo` or `reset`. gcm installs no signal handler (lean-deps; out of
/// scope for v1).
struct EchoGuard;

impl EchoGuard {
    fn new() -> Self {
        let _ = set_echo(false);
        EchoGuard
    }
}

impl Drop for EchoGuard {
    fn drop(&mut self) {
        let _ = set_echo(true);
    }
}

/// The `stty` argument toggling echo (`echo` on, `-echo` off). Pure (testable).
fn stty_arg(enable_echo: bool) -> &'static str {
    if enable_echo {
        "echo"
    } else {
        "-echo"
    }
}

/// Toggle terminal echo via `stty`, shelling out to `sh` exactly as
/// `ui::edit_in_editor` does (sh is present on the supported platforms).
fn set_echo(on: bool) -> io::Result<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("stty {}", stty_arg(on)))
        .stdin(Stdio::inherit())
        // stty only needs the controlling terminal; suppress its own output so a
        // non-TTY context (e.g. tests) does not leak "stty: stdin isn't a terminal".
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("stty failed"))
    }
}

/// Read one line from stdin with terminal echo disabled (best-effort). Echo is
/// restored via the RAII guard (see [`EchoGuard`] for the SIGINT caveat); a
/// trailing newline is printed (the user's Enter was not echoed). End-of-input
/// is an error; an empty/whitespace-only line returns `String::new()`, which the
/// wizard interprets as "use the env var, do not store inline".
fn read_secret(prompt: &str) -> io::Result<String> {
    eprint!("{prompt}");
    io::stderr().flush().ok();
    let (line, n) = {
        let _guard = EchoGuard::new();
        let mut buf = String::new();
        let n = io::stdin().read_line(&mut buf)?;
        (buf, n)
        // guard drops here, restoring echo before the newline below
    };
    eprintln!();
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "reached end of input during setup",
        ));
    }
    Ok(line.trim().to_string())
}

// ── Ollama probe ────────────────────────────────────────────────────────────

/// The effective Ollama base URL the backend would use, so the wizard seeds its
/// default + probe from it instead of always assuming `localhost`. Precedence
/// `GCM_OLLAMA_BASE_URL` > `OLLAMA_HOST` (normalized) > default - mirrors
/// `provider::ollama`'s resolution.
fn effective_ollama_endpoint() -> String {
    if let Some(u) = env_value("GCM_OLLAMA_BASE_URL") {
        return u;
    }
    if let Some(h) = env_value("OLLAMA_HOST") {
        return normalize_ollama_host(&h);
    }
    DEFAULT_OLLAMA_ENDPOINT.to_string()
}

/// Normalize an `OLLAMA_HOST` value into a base URL: a value with no scheme gets
/// `http://` (and the default `:11434` port if none); a value with a scheme is
/// taken as-is. Mirrors `provider::ollama::normalize_host`.
fn normalize_ollama_host(host: &str) -> String {
    let h = host.trim();
    if h.contains("://") {
        return h.to_string();
    }
    let has_port = h
        .rsplit_once(':')
        .is_some_and(|(_, p)| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    if has_port {
        format!("http://{h}")
    } else {
        format!("http://{h}:11434")
    }
}

/// Probe the Ollama daemon with the bounded [`PROBE_TIMEOUT`] (does not hang on
/// an unresponsive endpoint). Any response (even non-2xx) counts as reachable.
fn probe_ollama(base_url: &str) -> bool {
    probe_url(base_url, PROBE_TIMEOUT)
}

fn probe_url(url: &str, timeout: Duration) -> bool {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    agent.get(url).call().is_ok()
}

/// Validate an Ollama endpoint URL (no `url` dependency): must be `http(s)://`
/// with a non-empty host (the authority before any `:port` or `/path`). Returns
/// the trimmed URL on success.
fn validate_endpoint_url(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    let rest = s
        .strip_prefix("http://")
        .or_else(|| s.strip_prefix("https://"));
    let invalid = || {
        Err(format!(
            "'{raw}' is not a valid http(s) URL (expected e.g. {DEFAULT_OLLAMA_ENDPOINT})"
        ))
    };
    let Some(rest) = rest else { return invalid() };
    // the host is everything up to the first ':' (port) or '/' (path); it must
    // be non-empty, so `http://:1234` and `http:///x` are rejected.
    let host = rest.split([':', '/']).next().unwrap_or("");
    if host.is_empty() {
        return invalid();
    }
    Ok(s.to_string())
}

// ── small shared helpers ────────────────────────────────────────────────────

/// The five v1 providers, cloud first then Ollama (the wizard's menu order).
fn cloud_then_ollama() -> [ProviderId; 5] {
    [
        ProviderId::Groq,
        ProviderId::Google,
        ProviderId::Openai,
        ProviderId::Anthropic,
        ProviderId::Ollama,
    ]
}

/// The four cloud (key-bearing) providers.
fn cloud_providers() -> [ProviderId; 4] {
    [
        ProviderId::Groq,
        ProviderId::Google,
        ProviderId::Openai,
        ProviderId::Anthropic,
    ]
}

/// Human label for a provider in wizard prompts.
fn provider_label(id: ProviderId) -> &'static str {
    match id {
        ProviderId::Groq => "Groq",
        ProviderId::Google => "Google (Gemini)",
        ProviderId::Openai => "OpenAI",
        ProviderId::Anthropic => "Anthropic",
        ProviderId::Ollama => "Ollama (local, no key)",
    }
}

/// The lowercase token for a provider (the value written to TOML / `GCM_PROVIDER`).
fn provider_token(id: ProviderId) -> String {
    serde_json::to_value(id)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "groq".to_string())
}

/// Read a non-empty, trimmed env var as a bool "is set".
fn env_nonblank(name: &str) -> bool {
    env_value(name).is_some()
}

/// Read a non-empty, trimmed env var value, else `None`.
fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Print a prompt to stderr and read one raw line from stdin. End-of-input (a
/// closed/empty stdin) is an error, not an empty line - otherwise a re-prompt
/// loop on EOF would spin forever (the "never hang on a closed stdin" rule).
fn read_line(prompt: &str) -> io::Result<String> {
    eprint!("{prompt}");
    io::stderr().flush().ok();
    let mut s = String::new();
    let n = io::stdin().read_line(&mut s)?;
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "reached end of input during setup",
        ));
    }
    Ok(s)
}

/// [`read_line`] mapped into the wizard's error type. A read failure mid-setup
/// (e.g. stdin closed) renders verbatim via `GcmError::Git`'s passthrough.
fn wizard_read_line(prompt: &str) -> Result<String, GcmError> {
    read_line(prompt).map_err(|e| GcmError::Git(format!("could not read setup input: {e}")))
}

/// Parse a comma/space-separated list of 1-based indices into deduped 0-based
/// indices, in input order. Errors on a non-number, an out-of-range value, or an
/// empty selection.
fn parse_selection(input: &str, max: usize) -> Result<Vec<usize>, String> {
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
fn parse_one(input: &str, max: usize) -> Option<usize> {
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
fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
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
fn open_private(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_private(path: &Path) -> io::Result<fs::File> {
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
        }
    }

    #[test]
    fn config_round_trips_toml() {
        let cfg = Config {
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
            version: 1,
            default: ProviderId::Ollama,
            providers: vec![pc(ProviderId::Ollama, None, Some("http://host:1234"))],
        };
        // OLLAMA_HOST present -> do not set GCM_OLLAMA_BASE_URL (Ollama-native wins)
        let plan = env_plan(&cfg, |name| name == "OLLAMA_HOST");
        assert!(!plan.iter().any(|(v, _)| *v == "GCM_OLLAMA_BASE_URL"));
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
        assert!(out.contains("version = 1"), "{out}");
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
    fn ollama_endpoint_validates_url_format() {
        assert!(validate_endpoint_url("not-a-url").is_err());
        assert!(validate_endpoint_url("ftp://x").is_err());
        assert!(validate_endpoint_url("http://").is_err());
        // empty host (port/path only) is rejected
        assert!(validate_endpoint_url("http://:1234").is_err());
        assert!(validate_endpoint_url("http:///path").is_err());
        assert_eq!(
            validate_endpoint_url("http://localhost:11434").unwrap(),
            "http://localhost:11434"
        );
        assert_eq!(
            validate_endpoint_url("  https://h.example:8080  ").unwrap(),
            "https://h.example:8080"
        );
        // host with a path is fine
        assert_eq!(
            validate_endpoint_url("http://host/api").unwrap(),
            "http://host/api"
        );
    }

    #[test]
    fn normalize_ollama_host_matches_backend() {
        assert_eq!(normalize_ollama_host("localhost"), "http://localhost:11434");
        assert_eq!(
            normalize_ollama_host("127.0.0.1:8080"),
            "http://127.0.0.1:8080"
        );
        assert_eq!(
            normalize_ollama_host("https://remote.example"),
            "https://remote.example"
        );
    }

    #[test]
    fn save_to_overwrites_without_duplicating_providers() {
        // reconfigure idempotency: a second save replaces the file cleanly, no
        // duplicate [[providers]] tables, and load reflects the new config.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        let first = Config {
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Groq,
            providers: vec![pc(ProviderId::Groq, Some("k1"), None)],
        };
        save_to(&path, &first).unwrap();
        let second = Config {
            version: CONFIG_FORMAT_VERSION,
            default: ProviderId::Openai,
            providers: vec![pc(ProviderId::Openai, Some("k2"), None)],
        };
        save_to(&path, &second).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(
            text.matches("[[providers]]").count(),
            1,
            "no duplicate provider tables: {text}"
        );
        assert_eq!(load_from(&path).unwrap(), second);
    }

    #[test]
    fn ollama_probe_respects_timeout() {
        // The probe uses a bounded 3s timeout...
        assert_eq!(PROBE_TIMEOUT, Duration::from_secs(3));
        // ...and does not hang on an unreachable endpoint (connection refused
        // returns promptly as `false`, well under the timeout).
        assert!(!probe_url("http://127.0.0.1:1", PROBE_TIMEOUT));
    }

    #[test]
    fn read_secret_restores_echo_on_drop() {
        // stty arg mapping is the unit under test; the guard restores via Drop.
        assert_eq!(stty_arg(false), "-echo");
        assert_eq!(stty_arg(true), "echo");
        // Constructing and dropping the guard must not panic even with no TTY
        // (set_echo fails harmlessly and is ignored).
        {
            let _g = EchoGuard::new();
        }
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
}
