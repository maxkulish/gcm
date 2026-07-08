# Design: CLO-537 — Add Vertex AI provider (keyless ADC) selectable in `gcm provider`

**Status:** Draft
**Linear:** [CLO-537](https://linear.app/cloud-ai/issue/CLO-537/add-vertex-ai-provider-keyless-adc-selectable-in-gcm-provider)
**Branch:** `feat/clo-537-vertex-provider` (proposed)
**Date:** 2026-07-07
**Related:** CLO-489 (Provider trait + Gemini), CLO-516 (`gcm provider` wizard), CLO-531/534 (`gcm resolve` + resolve-schema fix)
**External reference:** bot-reviewer `docs/guides/vertex-local-dev.md`, `docs/guides/vertex-gemini-setup.md`

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
│   └── models.rs     — fetch_supported_models: Vertex arm → static Gemini set
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

Wired into `select()` (`ProviderId::Vertex => Box::new(vertex::Vertex::new(model))`) and added to the `pick_provider_id` "valid names" error string.

### 4.2 `vertex.rs` — auth and request

Auth is resolved **lazily** at call time (mirroring `gemini.rs::api_key()`), so `--dry-run` and cache resolution never need a token:

```
access_token():
  1. GCM_VERTEX_TOKEN (trimmed, non-empty)                       → use it
  2. else run `gcloud auth application-default print-access-token`
       - trim stdout; non-empty → use it
       - non-zero exit / not found → typed ProviderError with an
         actionable message:
           "gcloud not found: install the Google Cloud SDK" | 
           "run: gcloud auth application-default login"
           (surface an invalid_grant/reauth hint from stderr)
```

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

## 5. Testing

- **`vertex.rs` unit:** `request()` URL for `global` (bare `aiplatform`) vs a regional location (`{loc}-aiplatform`); `Authorization: Bearer` header; `GCM_VERTEX_BASE_URL` override; `cache_model_id()` prefix. Token precedence (`GCM_VERTEX_TOKEN` beats gcloud); missing project → typed `Config` error; token-acquisition failure → actionable typed error.
- **Shared-payload parity:** assert Vertex uses the same `build_plan_payload`/`build_resolve_payload` output as Gemini (extractor already covered by `gemini.rs` tests).
- **Config:** `project`/`location` serialize round-trip; `skip_serializing_if` omits them when `None`; a v2 file (no fields) still loads; `apply_to_env` sets the vertex env vars only when unset.
- **Wizard:** pure resolution helpers (project prefill from `GOOGLE_CLOUD_PROJECT`, location default `global`).
- **Acceptance:** `gcm status --provider vertex` with `GCM_VERTEX_PROJECT` set; an end-to-end `gcm --provider vertex` against a local mock `generateContent` server via `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN` (no gcloud needed in CI).
- **Live (HITL):** one manual `generateContent` 200 against the maintainer's GCP project via real gcloud ADC.

## 6. Open items for spec review

- **Env alias set (D3):** confirmed reasonable — primary `GCM_VERTEX_*`, aliases `GOOGLE_CLOUD_*` / `GCP_*`. Flag any alias to drop.
- **Static model list (D4):** MVP. If a live Vertex model list is wanted, it is an additive follow-up (`fetch_supported_models` Vertex arm calling `publishers/google/models` with an ADC token).
- **Shared-payload extraction:** promoting `gemini.rs` fns to `pub(super)` vs. moving them into a `google_common.rs` module. Proposed: `pub(super)` (smaller diff); revisit if a third Google-shaped backend appears.
