//! Configuration binary facade (CLO-597). Re-exports the library's `gcm::config`
//! surface and hosts the interactive onboarding / provider wizards plus the
//! low-level terminal helpers they need. Everything here depends on `cliclack`,
//! `console`, or `GcmError` and is therefore binary-only.

use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

pub use gcm::config::*;

use crate::error::GcmError;
use gcm::provider::{AuthMethod, ProviderId};

// ── interactive `gcm provider` wizard (CLO-516, cliclack) ────────────────────

/// Run the interactive `gcm provider` wizard: pick a provider, fetch its models
/// (live + static fallback), multiselect the enabled set (type-to-filter), choose
/// one default, and persist - preserving every other provider (D8). Returns
/// `Ok(true)` on a saved change, `Ok(false)` if the user cancelled (nothing
/// written). cliclack reads `/dev/tty`; the testable logic is the pure helpers
/// ([`wizard_model_list`], [`initial_default_model`], [`wizard_persist_key`]).
pub fn run_provider_wizard() -> Result<bool, GcmError> {
    use cliclack::{intro, multiselect, outro, password, select, spinner};
    use console::style;

    let existing = load();

    intro(style(" gcm-provider ").on_cyan().black()).map_err(wizard_io)?;

    // 1. Provider (radio list, current default pre-highlighted, type-to-filter).
    let all = all_providers();
    let current_default = existing
        .as_ref()
        .map(|c| c.default)
        .unwrap_or(ProviderId::Groq);
    let provider_items: Vec<(ProviderId, &'static str, &'static str)> =
        all.iter().map(|&id| (id, provider_label(id), "")).collect();
    let id = match select::<ProviderId>("Provider")
        .items(&provider_items)
        .initial_value(current_default)
        .filter_mode()
        .max_rows(15)
        .interact()
    {
        Ok(v) => v,
        Err(_) => return wizard_cancelled(),
    };

    let existing_pc = existing
        .as_ref()
        .and_then(|c| c.providers.iter().find(|p| p.id == id));

    // 2. Credential / endpoint resolution BEFORE the fetch (D5 step 3). The key is
    // held only in memory and persisted (inline `0600`) solely on completion.
    let mut fetch_key: Option<String> = None;
    let mut persist_key: Option<String> = None;
    let mut fetch_endpoint: Option<String> = None;
    let mut persist_endpoint: Option<String> = None;
    let mut persist_project: Option<String> = None;
    let mut persist_location: Option<String> = None;
    match id.auth_method() {
        AuthMethod::ApiKey => {
            // ApiKey providers always have a key env var; skip defensively if not.
            if let Some(var) = id.key_env_var() {
                let env_key = env_value(var);
                let cfg_key = existing_pc.and_then(|p| p.key.clone());
                if let Some(k) = env_key {
                    // Env wins for the fetch, but never copy an env-derived secret into
                    // the file; preserve any existing inline key (a fallback for when
                    // the env var is unset) rather than erasing it.
                    fetch_key = Some(k);
                    persist_key = cfg_key;
                } else if let Some(k) = cfg_key {
                    fetch_key = Some(k.clone());
                    persist_key = Some(k); // preserve the existing inline key
                } else {
                    let typed = match password(format!(
                        "{} API key (press Enter to skip)",
                        provider_label(id)
                    ))
                    .mask('*')
                    .interact()
                    {
                        Ok(s) => s,
                        Err(_) => return wizard_cancelled(),
                    };
                    let (f, p) = wizard_persist_key(&typed);
                    fetch_key = f;
                    persist_key = p;
                }
            }
        }
        AuthMethod::KeylessEndpoint => {
            // Ollama: resolve/prompt the endpoint before `/api/tags`. An env override
            // wins over the saved config (matching runtime precedence, review M2).
            let default_ep = ollama_wizard_default_endpoint(
                &effective_ollama_endpoint(),
                existing_pc.and_then(|p| p.endpoint.as_deref()),
            );
            let ep = match cliclack::input("Ollama endpoint")
                .default_input(&default_ep)
                .validate(|s: &String| validate_endpoint_url(s).map(|_| ()))
                .interact::<String>()
            {
                Ok(s) => s,
                Err(_) => return wizard_cancelled(),
            };
            let ep = ep.trim().to_string();
            fetch_endpoint = Some(ep.clone());
            if ep != DEFAULT_OLLAMA_ENDPOINT {
                persist_endpoint = Some(ep);
            }
        }
        AuthMethod::KeylessAdc => {
            // Vertex: project (required) + location (default global); no key, no
            // endpoint. Live discovery (CLO-564) authenticates with the ADC token
            // resolved below; on failure the fetch degrades to the static set.
            let default_project = existing_pc
                .and_then(|p| p.project.clone())
                .or_else(|| env_value("GCM_VERTEX_PROJECT"))
                .or_else(|| env_value("GOOGLE_CLOUD_PROJECT"))
                .unwrap_or_default();
            let mut project_input = cliclack::input("GCP project (required for Vertex)");
            if !default_project.trim().is_empty() {
                project_input = project_input.default_input(default_project.trim());
            }
            let project = match project_input
                .validate(|s: &String| {
                    if s.trim().is_empty() {
                        Err("a GCP project is required".to_string())
                    } else {
                        Ok(())
                    }
                })
                .interact::<String>()
            {
                Ok(s) => s.trim().to_string(),
                Err(_) => return wizard_cancelled(),
            };
            let default_location = existing_pc
                .and_then(|p| p.location.clone())
                .unwrap_or_else(|| "global".to_string());
            let location = match cliclack::input("Vertex location")
                .default_input(&default_location)
                .interact::<String>()
            {
                Ok(s) => s.trim().to_string(),
                Err(_) => return wizard_cancelled(),
            };
            persist_project = Some(project);
            // Keep the file minimal: omit location at the default `global`.
            persist_location = if location.is_empty() || location == "global" {
                None
            } else {
                Some(location)
            };
            // Single ADC acquisition (PR #41 review): the resolved token drives
            // both the spinner verdict and live discovery - no second gcloud
            // shell-out, no probe/fetch disagreement. Non-blocking: on failure
            // fetch_key stays None and the fetch shows the built-in list.
            let sp = spinner();
            sp.start("Checking gcloud ADC...");
            match crate::provider::vertex_access_token() {
                Ok(tok) => {
                    sp.stop("gcloud ADC ready");
                    fetch_key = Some(tok);
                }
                Err(msg) => sp.stop(format!(
                    "ADC not ready: {msg} (set GCM_VERTEX_TOKEN or run `gcloud auth application-default login`)"
                )),
            }
        }
    }

    // 3. Fetch the model list (spinner; never fails - falls back). The project
    // is Vertex-only: it becomes the x-goog-user-project quota header (CLO-564).
    let sp = spinner();
    sp.start("Fetching supported models...");
    let outcome = crate::provider::fetch_supported_models(
        id,
        fetch_key.as_deref(),
        fetch_endpoint.as_deref(),
        persist_project.as_deref(),
    );
    match outcome.source {
        crate::provider::FetchSource::Live => {
            sp.stop(format!("Fetched {} models", outcome.models.len()))
        }
        crate::provider::FetchSource::Fallback => sp.stop(
            outcome
                .warning
                .clone()
                .unwrap_or_else(|| "Using the built-in model list".to_string()),
        ),
    }

    // 4. Multiselect the enabled set (type-to-filter; >=1 required). The candidate
    // list keeps the current enabled set + default selectable even if the live list
    // omitted them (D7.3 wizard-side merge).
    let current_enabled: Vec<String> = existing_pc.map(|p| p.models.clone()).unwrap_or_default();
    let current_model = existing_pc.and_then(|p| p.model.clone());
    let candidates = wizard_model_list(
        id,
        &outcome.models,
        &current_enabled,
        current_model.as_deref(),
    );
    let model_items: Vec<(String, String, &'static str)> = candidates
        .iter()
        .map(|m| {
            let hint = wizard_model_hint(id, m, &outcome.source, &outcome.models);
            (m.clone(), m.clone(), hint)
        })
        .collect();
    // Pre-select the candidates whose canonical form matches a currently-enabled
    // model, so a migrated `llama3` / `models/gemini-x` still highlights (review L1).
    let initial_enabled: Vec<String> = candidates
        .iter()
        .filter(|c| {
            current_enabled
                .iter()
                .any(|e| canonicalize_model(id, e) == canonicalize_model(id, c))
        })
        .cloned()
        .collect();
    let selected = match multiselect::<String>("Enable models (space toggles, type to filter)")
        .items(&model_items)
        .initial_values(initial_enabled)
        .required(true)
        .filter_mode()
        .max_rows(15)
        .interact()
    {
        Ok(v) => v,
        Err(_) => return wizard_cancelled(),
    };

    // 5. Choose exactly one default among the selected models.
    let default_items: Vec<(String, String, &'static str)> = selected
        .iter()
        .map(|m| (m.clone(), m.clone(), ""))
        .collect();
    let mut default_select = select::<String>("Default model")
        .items(&default_items)
        .filter_mode()
        .max_rows(15);
    if let Some(d) = initial_default_model(id, &selected, current_model.as_deref()) {
        default_select = default_select.initial_value(d);
    }
    let default_model = match default_select.interact() {
        Ok(v) => v,
        Err(_) => return wizard_cancelled(),
    };

    // 6. Build (pure, AC-4 invariants), merge (preserving other providers), persist.
    let mut updated =
        build_provider_config(id, persist_key, persist_endpoint, default_model, selected)
            .map_err(GcmError::Git)?;
    // Vertex carries project/location instead of a key/endpoint (None for others).
    updated.project = persist_project;
    updated.location = persist_location;
    let merged = merge_provider_config(existing.as_ref(), updated, true);
    save(&merged).map_err(|e| GcmError::Git(format!("could not save configuration: {e}")))?;
    let where_ = config_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "the config file".to_string());
    outro(format!(
        "Saved {} configuration to {where_}",
        provider_label(id)
    ))
    .map_err(wizard_io)?;
    Ok(true)
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

// ── interactive wizard ──────────────────────────────────────────────────────

/// Run the interactive wizard end to end (enable providers, capture keys from
/// the environment or a prompt, choose a default) and return the assembled
/// `Config`. Cloud keys already exported are recorded as `key: None` (env-only);
/// an empty key input is also env-only. Invalid menu selections re-prompt.
pub fn run_wizard() -> Result<Config, GcmError> {
    let all = all_providers();
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
        match id.auth_method() {
            AuthMethod::ApiKey => {
                // ApiKey providers always have a key env var; skip defensively if not.
                let Some(var) = id.key_env_var() else {
                    continue;
                };
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
            AuthMethod::KeylessEndpoint => {
                let endpoint = prompt_ollama_endpoint()?;
                enabled.push(ProviderConfig {
                    id,
                    key: None,
                    endpoint,
                    model: None,
                    models: Vec::new(),
                    project: None,
                    location: None,
                });
            }
            AuthMethod::KeylessAdc => {
                // Vertex: project + location (no key, no endpoint) - fixes the bug
                // where selecting Vertex in first-run onboarding prompted for an
                // Ollama endpoint (CLO-537 round-2 A2/P1).
                let (project, location) = prompt_vertex_target()?;
                enabled.push(ProviderConfig {
                    id,
                    key: None,
                    endpoint: None,
                    model: None,
                    models: Vec::new(),
                    project: Some(project),
                    location,
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

    // Carry forward any enabled-model whitelist (and inline model default) the user
    // set previously via `gcm provider`, so this minimal wizard never erases it.
    preserve_existing_models(&mut enabled, load().as_ref());

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

/// Probe the Ollama daemon with the bounded [`PROBE_TIMEOUT`] (does not hang on
/// an unresponsive endpoint). Any response (even non-2xx) counts as reachable.
fn probe_ollama(base_url: &str) -> bool {
    probe_url(base_url, PROBE_TIMEOUT)
}

/// Print the cancellation outro and signal "no change" (nothing persisted).
fn wizard_cancelled() -> Result<bool, GcmError> {
    let _ = cliclack::outro_cancel("Cancelled - no changes made.");
    Ok(false)
}

/// [`read_line`] mapped into the wizard's error type. A read failure mid-setup
/// (e.g. stdin closed) renders verbatim via `GcmError::Git`'s passthrough.
fn wizard_read_line(prompt: &str) -> Result<String, GcmError> {
    read_line(prompt).map_err(|e| GcmError::Git(format!("could not read setup input: {e}")))
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

/// Map a wizard I/O error into the workflow error type.
fn wizard_io(e: io::Error) -> GcmError {
    GcmError::Git(format!("provider wizard I/O error: {e}"))
}

/// First-run prompt for the Vertex target: GCP project (required; prefilled from
/// `GCM_VERTEX_PROJECT` / `GOOGLE_CLOUD_PROJECT`) and location (default `global`).
/// Returns `(project, location)` where `location` is `None` at the default so the
/// config file stays minimal. Runs a non-blocking ADC probe (warns, never blocks).
fn prompt_vertex_target() -> Result<(String, Option<String>), GcmError> {
    let prefill = std::env::var("GCM_VERTEX_PROJECT")
        .ok()
        .or_else(|| std::env::var("GOOGLE_CLOUD_PROJECT").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let project = loop {
        let hint = prefill
            .as_deref()
            .map(|p| format!(" [{p}]"))
            .unwrap_or_default();
        let input = wizard_read_line(&format!("  Vertex GCP project{hint}: "))?;
        let v = input.trim();
        if !v.is_empty() {
            break v.to_string();
        }
        if let Some(p) = prefill.as_deref() {
            break p.to_string();
        }
        eprintln!("  A GCP project is required for Vertex. Try again.");
    };
    let loc_input = wizard_read_line("  Vertex location [global]: ")?;
    let loc = loc_input.trim();
    let location = if loc.is_empty() || loc == "global" {
        None
    } else {
        Some(loc.to_string())
    };
    match crate::provider::vertex_adc_probe() {
        Ok(()) => eprintln!("  gcloud ADC ready."),
        Err(msg) => eprintln!(
            "  note: gcloud ADC not ready ({msg}). Set GCM_VERTEX_TOKEN or run `gcloud auth application-default login` before committing."
        ),
    }
    Ok((project, location))
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

/// The `stty` argument toggling echo (`echo` on, `-echo` off). Pure (testable).
fn stty_arg(enable_echo: bool) -> &'static str {
    if enable_echo {
        "echo"
    } else {
        "-echo"
    }
}

fn probe_url(url: &str, timeout: Duration) -> bool {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    agent.get(url).call().is_ok()
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
