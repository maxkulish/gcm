# CLO-537 Implementation Plan: Add Vertex AI provider (keyless ADC)

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-537
**Design Document**: docs/designs/clo-537-vertex-provider.md (Finalized 2026-07-08)
**Architecture Reference**: docs/adrs/001-foundational-architecture-decisions.md
**Created**: 2026-07-08
**Overall Progress**: 0% (0/97 tasks completed; 25 parent tasks across 8 phases)

---

## Architecture Context

Vertex is a thin backend over the existing Gemini `generateContent` payloads: only the endpoint URL and auth (keyless ADC token vs API key) differ. It plugs into the enum-dispatch `Provider` layer (CLO-489), so `gcm` and `gcm resolve` both get it for free. The one cross-cutting change is that Vertex is the **second** keyless provider, which retires `key_env_var().is_none()` as an "is-Ollama" proxy in favour of an explicit `auth_method()` classifier. All line references below were validated against `src/` during design review.

Implementation order follows design §7: the enum + classifier first (unblocks exhaustive matches), then the backend, then config/wizard/status/models, then tests, then live HITL.

---

## Tasks

### Phase 1: ProviderId::Vertex + auth_method() classifier

- [ ] Task 1: Add the `Vertex` enum variant and its methods (`src/provider/mod.rs`)
  - [ ] Add `Vertex` to `enum ProviderId` (:332) with `#[value(alias = "google-vertex")]` + `#[serde(alias = "google-vertex")]` (N5)
  - [ ] `key_env_var()` (:346) -> `ProviderId::Vertex => None`
  - [ ] `default_model()` (:357) -> `"gemini-3.1-flash-lite"`
  - [ ] `model_env_vars()` (:372) -> `&["GCM_VERTEX_MODEL"]`
  - [ ] `as_str()` (:390) -> `"vertex"`
  - [ ] Add `Vertex => Box::new(vertex::Vertex::new(model))` arm to `select()` (:410)
  - [ ] Add `vertex` to the `pick_provider_id` valid-names error string (:454)
  - [ ] Add `provider_label()` arm (config.rs:1162) -> `Vertex => "Google (Vertex AI)"` (compile-required, no wildcard)
  - [ ] **Compile-gate:** the new variant forces arms in `provider_label` and the 5 `models.rs` fns (Task 18) too; land ALL of them in the SAME step so `cargo build` is green after Phase 1 (else the tree does not compile between Phase 1 and Phase 6)
- [ ] Task 2: Add the `auth_method()` classifier (design §4.6)
  - [ ] Define `enum AuthMethod { ApiKey, KeylessEndpoint, KeylessAdc }`
  - [ ] `ProviderId::auth_method()`: Ollama -> KeylessEndpoint, Vertex -> KeylessAdc, `_` -> ApiKey
  - [ ] Unit test: `auth_method()` returns the expected variant for every provider

### Phase 2: vertex.rs backend

- [ ] Task 3: Create `src/provider/vertex.rs` skeleton + module wiring
  - [ ] Register `mod vertex;` in `src/provider/mod.rs`
  - [ ] `Vertex::new(model)` struct + `Provider` trait impl signatures
- [ ] Task 4: Promote shared Gemini payload builders to `pub(super)` (`src/provider/gemini.rs`)
  - [ ] `build_plan_payload`, `build_message_payload`, `build_resolve_payload`, `extract_text` -> `pub(super)` (no logic change)
  - [ ] Confirm existing `gemini.rs` tests still pass (extractor coverage unchanged)
- [ ] Task 5: `access_token()` with bounded timeout (design §4.2, A3/P8)
  - [ ] Order: `GCM_VERTEX_TOKEN` (trimmed, non-empty) -> else `gcloud auth application-default print-access-token`
  - [ ] Bound the gcloud subprocess (~10s) with the `mpsc::channel` + `thread::spawn` + `rx.recv_timeout` pattern from `src/resolve/remote/publish.rs:116` (std `Command` has no built-in timeout); timeout -> typed error, not a hang
  - [ ] Distinguish spawn `io::ErrorKind::NotFound` (or `which::which("gcloud")`, host.rs:305 idiom) -> "gcloud not found: install the Google Cloud SDK"
  - [ ] Non-zero exit / timeout -> "run: gcloud auth application-default login" (surface invalid_grant/reauth hint from stderr)
- [ ] Task 6: `project()` / `location()` resolution + validation (design §4.2/§4.3, D3/D1/P2)
  - [ ] project: `GCM_VERTEX_PROJECT` -> `GOOGLE_CLOUD_PROJECT` -> `GCP_PROJECT`; missing -> typed `Config` error
  - [ ] location: `GCM_VERTEX_LOCATION` -> `GOOGLE_CLOUD_LOCATION` -> `GCP_REGION`; default `global`
  - [ ] Validate `location` strictly (host label): `^(global|[a-z][a-z0-9-]*)$`
  - [ ] Validate `project` permissively (path segment): allow legacy domain-scoped ids (`.`/`:`); reject URL-structural chars
- [ ] Task 7: `request()` URL + auth header (design §4.2)
  - [ ] base: `GCM_VERTEX_BASE_URL` (test seam) | `https://aiplatform.googleapis.com` (global) | `https://{location}-aiplatform.googleapis.com` (regional)
  - [ ] endpoint: `{base}/v1/projects/{project}/locations/{location}/publishers/google/models/{model}:generateContent`
  - [ ] header `Authorization: Bearer {token}`; body = shared gemini payload
  - [ ] Pass `auth_env_var: None` to the HTTP layer (so 401/403 -> `Http(status)`, not `Auth{env_var}`)
- [ ] Task 8: Vertex error mapping (design §4.2, N1/P6)
  - [ ] Intercept `ErrorKind::Http(401)` in vertex.rs -> "run: gcloud auth application-default login"
  - [ ] Intercept `ErrorKind::Http(403)` -> distinguish IAM-denied vs "enable the Vertex AI API on project {project}"
  - [ ] No change to shared `http.rs::classify_status`
- [ ] Task 9: trait glue + cache id
  - [ ] `generate_plan` / `generate_message` / `resolve_hunks` call shared `extract_text` + `parse_*` (three-line shape)
  - [ ] `cache_model_id()` -> `"vertex:{model}"`; `diff_budget()` -> `DiffBudget::standard()`

### Phase 3: Config + apply_to_env + call-site rewrite

- [ ] Task 10: `ProviderConfig` fields (`src/config.rs`, design §4.3)
  - [ ] Add `project: Option<String>` + `location: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`
  - [ ] Confirm no config `version` bump needed (a v2 file parses unchanged)
- [ ] Task 11: `env_plan` Vertex arm (`src/config.rs:376`, A1)
  - [ ] Branch on `auth_method()`: `KeylessAdc` -> bridge `project`/`location` to `GCM_VERTEX_PROJECT`/`GCM_VERTEX_LOCATION` (only when unset), no endpoint
- [ ] Task 12: `commented_reference` Vertex arm (`src/config.rs:290`, A4)
  - [ ] `KeylessAdc` -> emit `project =` / `location =` comment lines, not `endpoint =`
- [ ] Task 13: Provider-registry cleanup in `src/config.rs` (round-2 review)
  - [ ] Expand `cloud_then_ollama()` (:1140) from `[ProviderId; 5]` to include `Vertex` (`[ProviderId; 6]`); rename to `all_providers()`. It is the iteration source of truth for `commented_reference` (:299), first-run `run_wizard` (:421), and `run_provider_wizard` (:673) — **without this, Vertex never appears in any wizard menu and Tasks 14/15 are dead code**
  - [ ] Leave `cloud_providers()` (:1151) at `[ProviderId; 4]` intentionally — it means "key-bearing" (drives `any_cloud_key_set`); keyless Vertex is correctly excluded (semantic note only, no change)
  - [ ] `canonicalize_model()` (:541) needs no arm — its `_ => m.to_string()` wildcard already handles Vertex's bare gemini model ids (no `models/` prefix, unlike AI Studio)
  - [ ] Grep-audit for any other `key_env_var().is_none()` / `== None` "is-Ollama" branches; route through `auth_method()`

### Phase 4: Wizards

- [ ] Task 14: Interactive `gcm provider` wizard third branch (`run_provider_wizard`, design §4.4)
  - [ ] `KeylessAdc` -> prompt project (required; prefill `GOOGLE_CLOUD_PROJECT`/config) + location (default `global`); skip key prompt
  - [ ] Non-blocking ADC probe (spinner): try `access_token()` -> ok "ADC ready" / err warn-and-continue
  - [ ] Model list = static Gemini set (D4); persist `ProviderConfig{ id: Vertex, project, location, model, models }`
- [ ] Task 15: First-run `run_wizard` third branch (`src/config.rs:420`, A2/P1)
  - [ ] Replace the `match id.key_env_var()` (:443) two-way branch with `auth_method()`; add the `KeylessAdc` project/location branch (no Ollama endpoint prompt for Vertex)

### Phase 5: gcm status

- [ ] Task 16: `ProviderStatus` new fields + rendering (`src/status.rs`, N2/P4)
  - [ ] Add `project`/`location`/`auth_source: Option<String>` to `ProviderStatus` (skip-serialize when None)
  - [ ] `build_report` (:161): add a Vertex branch (no bogus `key:` row); infer `auth_source` = `GCM_VERTEX_TOKEN` if set else `gcloud ADC` (no subprocess)
  - [ ] `print_provider_block` (:472): print project/location/auth-source for Vertex
  - [ ] `is_activated`: Vertex activates when a project resolves (env/config), mirroring Ollama's keyless rule
  - [ ] `PROVIDER_ORDER` (:31): `[ProviderId; 5]` -> `[ProviderId; 6]` (add Vertex)
  - [ ] `selected_provider` valid-names warning (:248): add `vertex` (N5)
- [ ] Task 17: CLI help text (`src/cli.rs:20`, P3)
  - [ ] Add `vertex` to the `--provider` / `GCM_PROVIDER` valid-names help string (the third hardcoded list)

### Phase 6: models.rs arms

- [ ] Task 18: Vertex short-circuit + compile-required arms (`src/provider/models.rs`, N3/P5)
  - [ ] `fetch_supported_models` (:37): short-circuit Vertex at the top -> static Gemini set (D4)
  - [ ] Add a `ProviderId::Vertex` arm to each of the 5 exhaustive `match id` fns: `fetch_live` (:105), `resolved_base_url_with` (:159), `parse_models` (:186), `static_fallback_models` (:264), `provider_name` (:298) — unreachable at runtime, reuse Google's values
  - [ ] Confirm `keep_chat_model` (:236) needs no arm (has `_ => true`)

### Phase 7: Testing & Validation

- [ ] Task 19: vertex.rs unit tests (design §5)
  - [ ] `request()` URL: global (bare `aiplatform`) vs regional (`{loc}-aiplatform`); `Authorization: Bearer`; `GCM_VERTEX_BASE_URL` override; `cache_model_id()` prefix
  - [ ] Token precedence (`GCM_VERTEX_TOKEN` beats gcloud); missing project -> typed `Config` error; token-acquisition failure -> actionable typed error; timeout -> typed error (slow fake gcloud)
  - [ ] Error mapping: mock 401 -> gcloud-reauth text (never "check <env_var>"); 403 -> IAM/API-not-enabled text
  - [ ] Input validation: malformed `location` rejected pre-request; legacy domain-scoped `project` accepted
- [ ] Task 20: shared-payload parity + config tests
  - [ ] Vertex uses the same `build_plan_payload`/`build_resolve_payload` output as Gemini
  - [ ] `project`/`location` round-trip; `skip_serializing_if` omits when None; v2 file (no fields) loads; `apply_to_env` sets vertex env vars only when unset
- [ ] Task 21: wizard + status tests
  - [ ] Wizard resolution helpers (project prefill from `GOOGLE_CLOUD_PROJECT`; location default `global`)
  - [ ] `gcm status --provider vertex`: project/location/auth-source, no key row; `--json` carries new fields; `PROVIDER_ORDER` includes Vertex
- [ ] Task 22: acceptance test (no gcloud in CI)
  - [ ] End-to-end `gcm --provider vertex` against a local mock `generateContent` server via `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN`
- [ ] Task 23: pre-flight gates
  - [ ] `cargo fmt --check` clean
  - [ ] `cargo clippy` clean (no new warnings)
  - [ ] `cargo test` green (unit + integration)
- [ ] Task 24: Live verification (HITL)
  - [ ] One manual `generateContent` 200 against the maintainer's GCP project via real gcloud ADC (`gcm --provider vertex` and `gcm status --provider vertex`)

### Phase 8: Finalization

- [ ] Task 25: Create PR
  - [ ] Verify commits follow `feat(CLO-537): ...` conventional format
  - [ ] Push branch `feat/clo-537-vertex`
  - [ ] `gh pr create` with a body covering the auth-method refactor + Vertex backend + acceptance criteria
  - [ ] Link PR to CLO-537; request review

---

## Module Structure

- `src/provider/mod.rs` — `ProviderId::Vertex` + methods; `auth_method()`; `select()` arm; valid-names list
- `src/provider/vertex.rs` — NEW: auth (ADC + timeout), project/location validation, `request()`, error mapping, trait impl
- `src/provider/gemini.rs` — payload builders + `extract_text` promoted to `pub(super)`
- `src/provider/models.rs` — Vertex short-circuit + 5 compile-required match arms
- `src/config.rs` — `ProviderConfig.project/location`; `env_plan` + `commented_reference` + both wizards' Vertex branches
- `src/status.rs` — `ProviderStatus` fields; Vertex render branch; `PROVIDER_ORDER`; valid-names warning
- `src/cli.rs` — `--provider` help text
- `src/provider/http.rs` — unchanged (Vertex uses `auth_env_var: None`)

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked (needs manual intervention)

**To update progress**: Edit this file and change checkboxes. The overall percentage is recalculated from completed tasks.

---

## Notes

- **Compile-gate (Phase 1):** adding the `Vertex` variant breaks every exhaustive `match id` with no wildcard. All such arms must land in the same step to keep `cargo build` green: `select()` + `provider_label()` (config.rs:1162) + the 5 `models.rs` fns (Task 18). `canonicalize_model()` (config.rs:541) and `keep_chat_model()` (models.rs:256) have `_` wildcards and need no arm.
- `cloud_then_ollama()` (the wizard/reference iteration source) must grow to include Vertex (Task 13) or the wizard branches are unreachable — a silent "invisible feature" bug, not a compile error.
- Reusing `build_resolve_payload` inherits the CLO-534 resolve-schema fix (no `additionalProperties`) for free.
- Secrets never reach stdout/JSON; `gcm status` stays no-subprocess/no-network (auth-source is inferred).
- Task 24 (live HITL) is the only step requiring the maintainer's GCP project + `gcloud auth application-default login`.
