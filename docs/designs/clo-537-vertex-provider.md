# Design: CLO-537 — Add Vertex AI provider (keyless ADC) selectable in `gcm provider`

**Status:** Finalized
**Finalized:** 2026-07-08
**Approved By:** Max Kulish (owner)
**Linear:** [CLO-537](https://linear.app/cloud-ai/issue/CLO-537/add-vertex-ai-provider-keyless-adc-selectable-in-gcm-provider)
**Branch:** `feat/clo-537-vertex-provider` (proposed)
**Date:** 2026-07-07
**Related:** CLO-489 (Provider trait + Gemini), CLO-516 (`gcm provider` wizard), CLO-531/534 (`gcm resolve` + resolve-schema fix)
**External reference:** bot-reviewer's `vertex-local-dev.md` / `vertex-gemini-setup.md` (external UX notes; **not committed in this repo** — do not treat as in-repo links, N6)

---

## 1. Problem

gcm reaches Google Gemini only through the **AI Studio** path: `GEMINI_API_KEY` in the `x-goog-api-key` header against `generativelanguage.googleapis.com` (`src/provider/gemini.rs`). That key is consumer-tier — a personal API key with no enterprise data guarantees. Users who work on GCP and have already run `gcloud auth application-default login` cannot point gcm at **Vertex AI**, which serves the *same* Gemini models under enterprise terms (prompts/responses not used for training), IAM, per-region quotas, and **keyless** auth.

The two platforms share the **identical** `generateContent` request/response body, response schema, and model IDs. The only differences are the **endpoint URL** and the **auth mechanism** (a short-lived OAuth token from Application Default Credentials instead of an API key). So Vertex support is a thin backend over the existing Gemini payloads, not a new integration.

## 2. Goals / Non-goals

### Goals

- **G1:** New `ProviderId::Vertex` (alias `google-vertex`), a first-class provider selectable via `--provider vertex`, `GCM_PROVIDER=vertex`, and the `gcm provider` wizard — distinct from the existing `Google` (AI Studio) provider.
- **G2:** `src/provider/vertex.rs` implementing the `Provider` trait by **reusing** `gemini.rs`'s payload builders and response extractor; only `request()` (URL + auth) differs.
- **G3:** Keyless auth via ADC: acquire the access token by shelling out to `gcloud auth application-default print-access-token`, with a `GCM_VERTEX_TOKEN` env escape hatch checked first.
- **G4:** Config carries the Vertex target: GCP **project** (required) + **location** (default `global`), with GCP-ecosystem env aliases so an existing `GOOGLE_CLOUD_PROJECT` just works.
- **G5:** `gcm provider` wizard grows a third credential branch for Vertex (project + location prompts, no key), and `gcm status` reports the Vertex target + auth source without leaking secrets.
- **G6:** Vertex works everywhere the trait is consumed — `gcm`, `gcm resolve` — for free, and inherits the CLO-534 resolve-schema fix by reusing the Gemini payloads.

### Non-goals

- **Workload Identity Federation / in-cluster auth.** gcm is a laptop CLI; its path is local ADC. WIF (bot-reviewer's EKS→GCP concern) is out of scope. `GOOGLE_APPLICATION_CREDENTIALS` pointing at a cred-config still flows transparently through gcloud, so no special handling is needed.
- **Native Rust ADC** (e.g. the `gcp_auth` crate). The token source is a single swappable function; a native path can replace the gcloud shell-out later without touching the rest.
- **Live Vertex model-list fetch** (`publishers/google/models`). MVP reuses the static Gemini model set for the wizard multiselect.
- **Vertex-hosted Anthropic** (`google-vertex-anthropic`, Claude-only).
- Regional data-residency guarantees beyond passing `location` through (Gemini 3.x is `global`-only here anyway).

## 3. Confirmed decisions (from brainstorming)

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | **Separate `ProviderId::Vertex`**, not a sub-mode of `Google`. | Matches gcm's 1-variant-per-backend, enum-dispatch architecture and opencode's picker ("Vertex" vs "Google"). Avoids polluting the Google `ProviderConfig` with a mode axis + fields valid in only one mode. |
| D2 | **Token via `gcloud` shell-out** (`GCM_VERTEX_TOKEN` escape hatch first). | Matches gcm's "optional external binary on PATH" pattern (git required; mergiraf/gh/glab optional). Zero new Rust deps (ADR-001 ethos). Covers local ADC and mounted cred-configs transparently. |
| D3 | **Honor GCP env aliases** `GOOGLE_CLOUD_PROJECT`/`GCP_PROJECT`, `GOOGLE_CLOUD_LOCATION`/`GCP_REGION`. | Users on GCP already export these; zero-config selection when they do. Primary gcm-namespaced vars still take precedence. |
| D4 | **MVP static model list** for the wizard (the Gemini fallback set). | The multiselect needs ≥1 model; a live Vertex list endpoint is deferred YAGNI. Same models as AI Studio. |

## 4. Architecture

### Files touched

```
src/
├── provider/
│   ├── mod.rs        — ProviderId::Vertex variant + methods; select() arm;
│   │                   pick_provider_id error list; promote gemini payloads to pub(super)
│   ├── gemini.rs     — make build_plan_payload / build_message_payload /
│   │                   build_resolve_payload / extract_text `pub(super)` (no logic change)
│   ├── vertex.rs     — NEW: Vertex provider (auth + request(); reuses gemini payloads)
│   └── models.rs     — fetch_supported_models: short-circuit Vertex at the top
│                       → static Gemini set (D4) governs RUNTIME. But 5 exhaustive
│                       `match id` fns still need a compile-required Vertex arm
│                       (fetch_live, resolved_base_url_with, parse_models,
│                       static_fallback_models, provider_name) — unreachable at
│                       runtime, reuse Google's values. keep_chat_model has a
│                       `_ => true` wildcard, so it is already covered (N3, corrected)
├── config.rs         — ProviderConfig.project/location fields; apply_to_env bridge;
│                       run_provider_wizard third branch; env alias resolution
└── status.rs         — Vertex row: project, location, auth source (no secret)
```

### 4.1 `ProviderId::Vertex`

New enum variant with the alias `google-vertex`:

| Method | Value |
|--------|-------|
| `as_str()` | `"vertex"` |
| `key_env_var()` | `None` (keyless, like Ollama) |
| `default_model()` | `"gemini-3.1-flash-lite"` |
| `model_env_vars()` | `["GCM_VERTEX_MODEL"]` |

Wired into `select()` — an exhaustive `match id` (mod.rs:410) — as `ProviderId::Vertex => Box::new(vertex::Vertex::new(model))`, and added to the `pick_provider_id` "valid names" error string (mod.rs:454). The `google-vertex` alias needs **both** a `#[value(alias = "google-vertex")]` (clap) and a `#[serde(alias = "google-vertex")]` (config) derive (N5). **Three** hardcoded provider-name lists must all learn `vertex` (grep-verified): `pick_provider_id` (mod.rs:454), the `GCM_PROVIDER` help text in **`cli.rs:20`** (round-2 finding — the design had missed this one), and `status.rs::selected_provider`'s unknown-provider warning (~line 248, see §4.5).

> **Do not use `key_env_var().is_none()` as an "is-Ollama" proxy.** Vertex is *also* keyless, so every site that currently treats "no key" as "Ollama" must instead branch on provider identity / auth method. This was the top review finding (A1/A2/N4); §4.6 gives the full auth-method taxonomy and the exhaustive call-site list.

### 4.2 `vertex.rs` — auth and request

Auth is resolved **lazily** at call time (mirroring `gemini.rs::api_key()`), so `--dry-run` and cache resolution never need a token:

```
access_token():
  1. GCM_VERTEX_TOKEN (trimmed, non-empty)                       → use it
  2. else run `gcloud auth application-default print-access-token`
       under a BOUNDED TIMEOUT (~10s wall clock) — A3: git is invoked
       without a timeout because it is local/instant, but a gcloud token
       refresh can block on the network; a timeout hit → typed error,
       never an indefinite hang
       - trim stdout; non-empty → use it
       - spawn err `io::ErrorKind::NotFound` (or a `which::which("gcloud")`
         pre-check, mirroring host.rs:305 for gh/glab) → distinct
         "gcloud not found: install the Google Cloud SDK"
       - non-zero exit / timeout → "run: gcloud auth application-default
         login" (surface an invalid_grant/reauth hint from stderr)
```

The `NotFound`-vs-nonzero-exit split (round-2 finding) is what lets the two messages be emitted correctly: git in `git.rs` maps every spawn error to one generic string because git is *required*, but gcloud is optional, so we must distinguish "not installed" (install hint) from "installed but ADC not initialized" (reauth hint).

`project()` / `location()` resolve from env with GCP aliases (config values are bridged into these env vars by `apply_to_env`, so `vertex.rs` reads only env):

| | Order (first non-empty wins) | Default |
|---|---|---|
| project | `GCM_VERTEX_PROJECT` → `GOOGLE_CLOUD_PROJECT` → `GCP_PROJECT` | — (missing → typed `Config` error) |
| location | `GCM_VERTEX_LOCATION` → `GOOGLE_CLOUD_LOCATION` → `GCP_REGION` | `global` |

`request()`:

```
base = GCM_VERTEX_BASE_URL (test seam)
     | "https://aiplatform.googleapis.com"          if location == "global"
     | "https://{location}-aiplatform.googleapis.com" otherwise
endpoint = {base}/v1/projects/{project}/locations/{location}
                 /publishers/google/models/{model}:generateContent
auth header = ("Authorization", "Bearer {token}")
payload = <gemini::build_*_payload>   (identical body)
```

`cache_model_id() → "vertex:{model}"` — distinct from `"google:{model}"` so a cached plan from one platform never satisfies the other (different endpoint + terms). `diff_budget()` = `DiffBudget::standard()` (same large-context Gemini).

`generate_plan` / `generate_message` / `resolve_hunks` bodies are the same three-line shape as `gemini.rs`, calling the shared `extract_text` and `parse_*`. Reusing `build_resolve_payload` means the CLO-534 OpenAPI-subset schema (no `additionalProperties`) applies to Vertex automatically.

**Vertex error mapping (N1).** `vertex.rs` sends an `Authorization: Bearer` token, so a 401/403 otherwise flows through `http.rs::classify_status` into `ErrorKind::Auth { env_var }` → *"check that `<env_var>` is valid"* — misleading, because the token came from gcloud, not an env var, and a Vertex `403` usually means IAM-denied or *"Vertex AI API not enabled"*, not a bad credential. Vertex must map auth failures to its own actionable text: `401`/expired → *"run: gcloud auth application-default login"*; `403` → distinguish IAM-permission vs. API-not-enabled (*"enable the Vertex AI API on project {project}"*).

**Implementation (code-validated, round-2):** `classify_status` maps `401|403` to `ErrorKind::Auth { env_var }` **only when `auth_env_var` is `Some`** (`http.rs:210`); a backend that passes `None` gets a generic `ErrorKind::Http(status)` — this is exactly how Ollama avoids naming a nonexistent key var. Vertex does the same: it sends the `Authorization: Bearer` token in the request header but passes **`auth_env_var: None`** to the HTTP layer, so a `401/403` surfaces as `ErrorKind::Http(401|403)`; `vertex.rs` then intercepts that and re-formats it with the Vertex-specific text above. This localizes the whole fix to `vertex.rs` and changes **no shared code** (cleaner than adding a Vertex arm to `classify_status`).

**Input validation (D1), scoped by URL position (round-2 correction).** `location` and `project` sit in **different** parts of the URL and carry different risk, so validate them differently:

- **`location` → host label** (`{loc}-aiplatform.googleapis.com`): higher risk (a bad value redirects the whole request + `Bearer` token), and the value space is tiny, so validate **strictly**: `^(global|[a-z][a-z0-9-]*)$`.
- **`project` → path segment** (`/projects/{project}/`): lower risk (can't change the host). The original strict `^[a-z][a-z0-9-]{4,28}[a-z0-9]$` **wrongly rejects legacy domain-scoped project IDs** like `example.com:my-project`, which contain `.` and `:`. Fix: either allow those characters (`^[a-z][a-z0-9.:-]{4,61}[a-z0-9]$`) or — simpler and safer against future formats — reject only URL-structural characters (`/`, whitespace, control chars) rather than enforcing a tight allowlist.

Reject with a typed `Config` error before the URL is built. (This is the one point the two reviewers diverged on: Gemini judged security "sound" considering only command injection, which is a non-issue since the gcloud argv is static; Claude flagged the URL-templating path. Applying the validation closes it either way.)

### 4.3 Config

Two optional fields added to `ProviderConfig` (flat, matching the existing `endpoint`/`model` pattern):

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub project: Option<String>,   // Vertex only
#[serde(default, skip_serializing_if = "Option::is_none")]
pub location: Option<String>,  // Vertex only, effective default "global"
```

Because both default to `None` and skip-serialize, **no config `version` bump is needed** — a v2 file parses unchanged. `apply_to_env` gains a Vertex arm that sets `GCM_VERTEX_PROJECT` / `GCM_VERTEX_LOCATION` from the config **only if unset** (preserving `flag > env > config > default`, exactly like the existing model/key bridge).

### 4.4 `gcm provider` wizard

The credential step is currently two-way (`key_env_var()` = `Some` → key prompt; `None` → Ollama endpoint prompt). Vertex is a **third** shape — keyless but not an endpoint. Add a branch:

```
if id == Vertex:
    project  = input("GCP project", default = GOOGLE_CLOUD_PROJECT | existing config)   [required]
    location = input("Vertex location", default = "global")
    // skip the API-key prompt entirely
    spinner("Checking gcloud ADC…"):
        try access_token()  → ok: "ADC ready"   err: warn (non-blocking), continue
    models = static Gemini set (D4)   // multiselect + default as usual
    persist ProviderConfig{ id: Vertex, project: Some, location: Some, model, models }
```

### 4.5 `gcm status`

Vertex row shows: selected model + source, **project** + its source, **location** + its source, and **auth source** = `GCM_VERTEX_TOKEN` (if set) else `gcloud ADC`. No token value, no masked suffix — never a secret on stdout/JSON.

The review found `status.rs` concretely under-specified (N2). Required, specific changes — `status` keeps its **no-subprocess / no-network** contract, so auth-source is *inferred*, never verified by calling gcloud:

- **`ProviderStatus` gains three optional fields:** `project: Option<String>`, `location: Option<String>`, `auth_source: Option<String>` (all `None` for non-Vertex providers; serialized in the `--json` `StatusReport`, so bump nothing else but keep `v:1` back-compatible via `#[serde(skip_serializing_if = "Option::is_none")]`).
- **`build_report` render branch:** today it is `if id == Ollama { endpoint row } else { key_source row }`; Vertex would fall into the `else` and print a bogus `key: not set` row. Add a **third** branch for Vertex that prints project/location/auth-source and **no key row**.
- **`auth_source` inference (no gcloud call):** `GCM_VERTEX_TOKEN` set → `"GCM_VERTEX_TOKEN"`; else → `"gcloud ADC"`. This reflects *which path would be used*, not that a token was actually obtained (obtaining one needs the forbidden subprocess).
- **`is_activated`:** the `_ => key_env_var().is_some_and(...)` fallback makes a keyless provider "activated" only via config membership. Add a Vertex rule so it activates when a project is resolvable (env/config), consistent with Ollama's keyless treatment.
- **`PROVIDER_ORDER: [ProviderId; 5]` → `[ProviderId; 6]`** (add `Vertex`).
- **Second valid-names list:** `status.rs::selected_provider` (~line 248) has its own hardcoded provider-name match separate from `pick_provider_id`; it must also accept `vertex` (N5).

### 4.6 Auth-method taxonomy & call-site coverage (top review finding: A1/A2/A4/N4)

gcm currently uses `key_env_var().is_none()` as a proxy for "this is Ollama" (the only prior keyless provider). Vertex is the **second** keyless provider, so that proxy is now wrong: it conflates "keyless-with-endpoint" (Ollama) with "keyless-with-ADC" (Vertex). Every branch that keys off key-presence to pick a credential UX must be rewritten to branch on **intent**.

**Chosen mechanism (N4):** add an explicit classifier on `ProviderId` rather than patching each call site's boolean:

```rust
enum AuthMethod { ApiKey, KeylessEndpoint, KeylessAdc }

impl ProviderId {
    fn auth_method(self) -> AuthMethod {
        match self {
            ProviderId::Ollama => AuthMethod::KeylessEndpoint,
            ProviderId::Vertex => AuthMethod::KeylessAdc,
            _                   => AuthMethod::ApiKey,   // key_env_var() == Some
        }
    }
}
```

Every site then matches on `auth_method()` (exhaustive → the compiler flags the next keyless backend), instead of on `key_env_var().is_none()`.

**Exhaustive call-site list** (the design previously named only `run_provider_wizard`; the review found three more):

| Site | File | Today | Needed for Vertex |
|------|------|-------|-------------------|
| `env_plan` | `config.rs` | `None` branch bridges an **Ollama endpoint** into env | `KeylessAdc` → bridge `project`/`location` to `GCM_VERTEX_PROJECT`/`GCM_VERTEX_LOCATION`, **no** endpoint (A1) |
| first-run `run_wizard` | `config.rs` (~:420) | `None` → prompt for Ollama endpoint | `KeylessAdc` → project + location prompts, no key, no endpoint (A2) |
| `run_provider_wizard` | `config.rs` | two-way (key vs endpoint) | third branch (project/location + ADC probe) — already in §4.4 |
| `commented_reference` | `config.rs` | emits `endpoint =` for keyless | `KeylessAdc` → emit `project =`/`location =` comments, **not** `endpoint =` (A4) |

Without the `env_plan` and first-run `run_wizard` fixes, a user who selects Vertex in first-run onboarding is prompted for an Ollama endpoint, and the config→env bridge never populates the Vertex target — the two highest-severity findings.

## 5. Testing

- **`vertex.rs` unit:** `request()` URL for `global` (bare `aiplatform`) vs a regional location (`{loc}-aiplatform`); `Authorization: Bearer` header; `GCM_VERTEX_BASE_URL` override; `cache_model_id()` prefix. Token precedence (`GCM_VERTEX_TOKEN` beats gcloud); missing project → typed `Config` error; token-acquisition failure → actionable typed error.
- **Shared-payload parity:** assert Vertex uses the same `build_plan_payload`/`build_resolve_payload` output as Gemini (extractor already covered by `gemini.rs` tests).
- **Config:** `project`/`location` serialize round-trip; `skip_serializing_if` omits them when `None`; a v2 file (no fields) still loads; `apply_to_env` sets the vertex env vars only when unset.
- **Wizard:** pure resolution helpers (project prefill from `GOOGLE_CLOUD_PROJECT`, location default `global`).
- **Auth-method coverage (§4.6):** `auth_method()` returns `KeylessAdc` for Vertex; `env_plan` bridges `project`/`location` (not an endpoint) for Vertex; `commented_reference` emits `project`/`location` (not `endpoint`) for Vertex; first-run + `run_provider_wizard` route Vertex to the project/location branch.
- **Error mapping (N1):** a mock `401`/expired → the gcloud-reauth message (not "check `<env_var>`"); `403` → the IAM/API-not-enabled message. Assert the env-var text never appears for Vertex auth failures.
- **Input validation (D1):** a malformed `location` (e.g. `us central1/../`) → typed `Config` error **before** any request (URL never built); a **legacy domain-scoped `project`** (`example.com:my-project`) is **accepted** (regression guard for the round-2 fix).
- **Timeout (A3):** a token-acquisition that exceeds the bound → typed error, not a hang (inject a slow fake `gcloud`/token fn).
- **Status (N2):** `gcm status --provider vertex` prints project/location/auth-source and **no** key row; `auth_source` = `GCM_VERTEX_TOKEN` when set else `gcloud ADC`; `--json` carries the new fields; `PROVIDER_ORDER` includes Vertex.
- **Acceptance:** `gcm status --provider vertex` with `GCM_VERTEX_PROJECT` set; an end-to-end `gcm --provider vertex` against a local mock `generateContent` server via `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN` (no gcloud needed in CI).
- **Live (HITL):** one manual `generateContent` 200 against the maintainer's GCP project via real gcloud ADC.

## 6. Review resolutions (AI review, 2026-07-08)

**Verdict: APPROVE_WITH_SUGGESTIONS** — Gemini 2.5 Pro + Claude (code-grounded); Ollama/Codex reviewer failed (environment, not a design signal). Reviews: `docs/reviews/clo-537-review-{gemini,claude-fallback,synthesis}.md`. The core architecture (thin Vertex backend over the exact Gemini payloads, keyless ADC via gcloud shell-out, lazy token, distinct `ProviderId::Vertex`) was verified sound, ADR-001-compliant, and a security improvement. All findings were integration-completeness / error-operational polish — applied into the design above so they ship as spec, not latent bugs:

| ID | Finding | Where applied |
|----|---------|---------------|
| A1/A2/A4/N4 | `key_env_var()==None` "is-Ollama" proxy breaks with a second keyless provider | §4.6 (new): `auth_method()` classifier + exhaustive call-site table (`env_plan`, first-run `run_wizard`, `run_provider_wizard`, `commented_reference`) |
| N1 | Misleading auth error (gcloud token vs env var; 403 = IAM/API-not-enabled) | §4.2 Vertex error mapping |
| N2 | `status.rs` under-specified | §4.5 concrete: new `ProviderStatus` fields, third render branch, `PROVIDER_ORDER` bump, `is_activated` rule, inferred auth-source |
| A3 | No timeout on gcloud shell-out | §4.2 bounded ~10s timeout |
| D1 | `location`/`project` templated into URL unvalidated | §4.2 input validation (regex + typed `Config` error) |
| N3 | `models.rs` 5 exhaustive `match` blocks | §4 files list: short-circuit Vertex at the top |
| N5 | Dual alias derives; second valid-names list in `status.rs` | §4.1 / §4.5 |
| N6 | Referenced `docs/guides/vertex-*.md` don't exist | header annotated (external, not in-repo) |

**Round-2 owner review (2026-07-08, code-validated against `src/`):** 8 points checked; all valid. Four confirmed items already applied above (run_wizard Ollama-bias → §4.6; status render → §4.5; `select()` arm → §4.1; the hardcoded-list cluster). Four sharpened the design:
- **cli.rs:20** is a **third** hardcoded provider-name list (help text) the design had missed → §4.1.
- **`models.rs` exhaustiveness corrected:** the top-level short-circuit governs runtime only; 5 `match id` fns still need compile-required Vertex arms, and `keep_chat_model`'s `_ => true` already covers it → §4 files list.
- **401/403 error mapping made concrete + cleaner:** pass `auth_env_var: None` (like Ollama) so it surfaces as `Http(status)`, intercept in `vertex.rs` — no shared-code change → §4.2.
- **Project-ID regex relaxed:** the strict form rejected legacy domain-scoped IDs (`example.com:my-project`); split validation by URL position (strict host label, permissive path segment) → §4.2.
- **gcloud not-found:** distinguish install-vs-reauth via `io::ErrorKind::NotFound` on spawn (or `which::which`, host.rs:305 idiom) → §4.2.

**Standing design decisions (unchanged, confirmed by review):**
- **Env alias set (D3):** primary `GCM_VERTEX_*`, aliases `GOOGLE_CLOUD_*` / `GCP_*` — kept.
- **Static model list (D4):** MVP; live `publishers/google/models` fetch is an additive follow-up.
- **Shared-payload reuse via `pub(super)`** (not a `google_common.rs` module yet): smaller diff; revisit if a third Google-shaped backend appears (N7).
- Cache cold-start on a `google`↔`vertex` switch is expected and correct (distinct `cache_model_id`), N6.

## 7. Implementation order

1. `ProviderId::Vertex` + `auth_method()` classifier (§4.1, §4.6) — unblocks every call site with an exhaustive match.
2. `vertex.rs`: `access_token()` (timeout), `project()`/`location()` (validation), `request()` (URL + Bearer), reuse gemini payloads, error mapping (§4.2).
3. `ProviderConfig.project`/`location` + `apply_to_env` bridge (§4.3); rewrite the four `auth_method()` call sites (§4.6).
4. `gcm provider` wizard third branch + first-run `run_wizard` (§4.4, §4.6).
5. `status.rs` fields + render branch + ordering (§4.5).
6. `models.rs` short-circuit (§4 files list).
7. Tests (§5); then live HITL `generateContent` 200 against the maintainer's GCP project.

Acceptance criteria are enumerated on the Linear issue (CLO-537) and are the finalize gate.
