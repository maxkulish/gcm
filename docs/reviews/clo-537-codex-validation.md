Reading additional input from stdin...
OpenAI Codex v0.143.0
--------
workdir: /Users/mk/Code/gcm--feat-clo-537-vertex
model: gpt-5.4
provider: openai
approval: never
sandbox: read-only
reasoning effort: xhigh
reasoning summaries: none
session id: 019f45a8-b23d-7592-b146-55aa32495dbe
--------
user
You are a senior Rust code reviewer. Review all changes on this branch (CLO-537: add a Vertex AI provider to the gcm CLI) against the design document and implementation plan.

FILES TO READ:
1. Design: docs/designs/clo-537-vertex-provider.md
2. Plan: docs/plans/clo-537-vertex-provider.md
3. Run: git diff main...HEAD
4. Read the new/modified source: src/provider/vertex.rs, src/provider/mod.rs, src/provider/gemini.rs, src/provider/models.rs, src/config.rs, src/status.rs, src/cli.rs, tests/vertex.rs

CHECK FOR:
1. CORRECTNESS: Do the changes implement what the design specifies? Is the keyless ADC token flow correct? Is the 401/403 error-mapping (Bearer token in extra_headers with auth:None so classify_status yields Http(status)) actually correct given src/provider/http.rs?
2. COMPLETENESS: Are all acceptance criteria from the Linear issue / design covered? Any call site of the retired key_env_var()==None "is-Ollama" proxy missed?
3. REGRESSIONS: Could any change break existing providers (Groq/Google/OpenAI/Anthropic/Ollama), config parsing, the wizard, or gcm status?
4. CODE QUALITY: Clean interfaces, proper error handling, no dead code, no panics on the error path.
5. SECURITY: No hardcoded secrets; the ADC token never printed by gcm status; location/project validated before URL templating; safe subprocess handling (timeout, no shell injection).

OUTPUT FORMAT (markdown):
## Verdict: PASS | PASS_WITH_NOTES | FAIL
## Findings
[each with severity CRITICAL/HIGH/MEDIUM/LOW, file:line, and why]
## Missing Items
## Recommendations
codex
Reviewing the branch against the design, plan, and actual diff first. I’ll inspect the docs and the changed runtime paths together so the findings are grounded in code rather than the plan text alone.
exec
/bin/zsh -lc 'git status --short --branch' in /Users/mk/Code/gcm--feat-clo-537-vertex
exec
/bin/zsh -lc 'rg -n "CLO-537|vertex|Vertex|provider model env|Google model env" /Users/mk/.codex/memories/MEMORY.md' in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
## feat/clo-537-vertex
 M docs/status/clo-537-workflow.yaml
?? docs/reviews/clo-537-codex-validation.md
?? docs/reviews/clo-537-gemini-validation.md
?? docs/status/clo-537-vertex-provider.md

 succeeded in 0ms:
196:# Task Group: gcm Google model env-over-config precedence review
198:scope: Review findings for Google provider model resolution in `/Users/mk/Code/gcm`; use when touching `src/config.rs`, Google model env vars, or `gcm status` model reporting for Google providers.

exec
/bin/zsh -lc 'git diff --stat main...HEAD' in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
 docs/DEPENDENCIES.md                           |   6 +-
 docs/PROJECT.md                                |  13 +-
 docs/ROADMAP.md                                |   4 +-
 docs/designs/clo-537-vertex-provider.md        | 136 +++++++-
 docs/plans/clo-537-vertex-provider.md          | 174 ++++++++++
 docs/reviews/clo-537-review-claude-fallback.md |  88 +++++
 docs/reviews/clo-537-review-gemini.md          |  75 ++++
 docs/reviews/clo-537-review-ollama.md          |  18 +
 docs/reviews/clo-537-review-synthesis.md       |  64 ++++
 docs/status/clo-537-workflow.yaml              | 192 +++++++++++
 src/cli.rs                                     |   2 +-
 src/config.rs                                  | 341 +++++++++++++++---
 src/provider/gemini.rs                         |   8 +-
 src/provider/mod.rs                            |  39 ++-
 src/provider/models.rs                         |  22 +-
 src/provider/vertex.rs                         | 456 +++++++++++++++++++++++++
 src/status.rs                                  | 196 ++++++++++-
 tests/status.rs                                |   4 +-
 tests/vertex.rs                                | 297 ++++++++++++++++
 19 files changed, 2034 insertions(+), 101 deletions(-)

codex
I found one relevant prior note in memory: earlier `gcm` review work around Google model env precedence and `gcm status`, which is directly adjacent to this branch’s config/status surface. I’m reading the design, plan, and changed provider code next, then I’ll compare the implementation against the actual HTTP/auth path.
exec
/bin/zsh -lc "sed -n '1,260p' docs/plans/clo-537-vertex-provider.md" in /Users/mk/Code/gcm--feat-clo-537-vertex
exec
/bin/zsh -lc "sed -n '190,205p' /Users/mk/.codex/memories/MEMORY.md" in /Users/mk/Code/gcm--feat-clo-537-vertex
exec
/bin/zsh -lc "sed -n '1,260p' docs/designs/clo-537-vertex-provider.md" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
- Symptom: a design proposes a whitelist helper in `config.rs` and claims runtime enforcement is covered -> Cause: the real runtime path is env-driven after hydration, so the helper is unreachable from the actual selection seam -> Fix: wire enforcement at `main.rs` / provider-selection time with access to validated effective config state [Task 1]
- Symptom: a proposed config migration appears to accept v1 data safely -> Cause: `parse_config` rejects any version other than 1 and `render_config()` serializes the in-memory version field verbatim -> Fix: normalize migrated configs to version 2 in memory before save, or they will round-trip back to v1 [Task 1]
- Symptom: forward compatibility is described as harmless fallback -> Cause: older binaries currently treat unknown versions as no usable config, which can re-trigger onboarding and overwrite newer-format files -> Fix: review down-level overwrite behavior explicitly before accepting the migration plan [Task 1]
- Symptom: the design’s Google model-list or auth details look harmlessly approximate -> Cause: the doc drifted from the current Gemini backend shape -> Fix: verify endpoint and auth claims against `src/provider/gemini.rs` before approving provider-fetch behavior [Task 1]
- Symptom: helper-level tests pass but the interactive flow is still unproven -> Cause: cliclack behavior remains manual-only without a PTY harness -> Fix: keep UI-interaction claims scoped to manual verification and unit-test only the pure helper portions in-tree [Task 1]

# Task Group: gcm Google model env-over-config precedence review

scope: Review findings for Google provider model resolution in `/Users/mk/Code/gcm`; use when touching `src/config.rs`, Google model env vars, or `gcm status` model reporting for Google providers.
applies_to: cwd=/Users/mk/Code/gcm; reuse_rule=safe for similar Google provider/config precedence reviews in this checkout, but re-check exact env-var names and resolution order if `src/config.rs` changes.

## Task 1: Review Google model precedence, found alias-shadowing regression

### rollout_summary_files

- rollout_summaries/2026-06-26T21-15-03-Ri4t-review_google_model_precedence_bug.md (cwd=/Users/mk/Code/gcm, rollout_path=/Users/mk/.codex/sessions/2026/06/26/rollout-2026-06-26T23-15-03-019f05c9-4111-7aa0-9dea-72297340c4d5.jsonl, updated_at=2026-06-26T21:17:45+00:00, thread_id=019f05c9-4111-7aa0-9dea-72297340c4d5, outcome=fail)

 succeeded in 0ms:
# CLO-537 Implementation Plan: Add Vertex AI provider (keyless ADC)

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-537
**Design Document**: docs/designs/clo-537-vertex-provider.md (Finalized 2026-07-08)
**Architecture Reference**: docs/adrs/001-foundational-architecture-decisions.md
**Created**: 2026-07-08
**Overall Progress**: 94% (91/97 tasks completed). Remaining: live HITL verify (Task 24) + PR creation (Task 25, done in the PR phase). 25 parent tasks across 8 phases.

---

## Architecture Context

Vertex is a thin backend over the existing Gemini `generateContent` payloads: only the endpoint URL and auth (keyless ADC token vs API key) differ. It plugs into the enum-dispatch `Provider` layer (CLO-489), so `gcm` and `gcm resolve` both get it for free. The one cross-cutting change is that Vertex is the **second** keyless provider, which retires `key_env_var().is_none()` as an "is-Ollama" proxy in favour of an explicit `auth_method()` classifier. All line references below were validated against `src/` during design review.

Implementation order follows design §7: the enum + classifier first (unblocks exhaustive matches), then the backend, then config/wizard/status/models, then tests, then live HITL.

---

## Tasks

### Phase 1: ProviderId::Vertex + auth_method() classifier

- [x] Task 1: Add the `Vertex` enum variant and its methods (`src/provider/mod.rs`)
  - [x] Add `Vertex` to `enum ProviderId` (:332) with `#[value(alias = "google-vertex")]` + `#[serde(alias = "google-vertex")]` (N5)
  - [x] `key_env_var()` (:346) -> `ProviderId::Vertex => None`
  - [x] `default_model()` (:357) -> `"gemini-3.1-flash-lite"`
  - [x] `model_env_vars()` (:372) -> `&["GCM_VERTEX_MODEL"]`
  - [x] `as_str()` (:390) -> `"vertex"`
  - [x] Add `Vertex => Box::new(vertex::Vertex::new(model))` arm to `select()` (:410)
  - [x] Add `vertex` to the `pick_provider_id` valid-names error string (:454)
  - [x] Add `provider_label()` arm (config.rs:1162) -> `Vertex => "Google (Vertex AI)"` (compile-required, no wildcard)
  - [x] **Compile-gate:** the new variant forces arms in `provider_label` and the 5 `models.rs` fns (Task 18) too; land ALL of them in the SAME step so `cargo build` is green after Phase 1 (else the tree does not compile between Phase 1 and Phase 6)
- [x] Task 2: Add the `auth_method()` classifier (design §4.6)
  - [x] Define `enum AuthMethod { ApiKey, KeylessEndpoint, KeylessAdc }`
  - [x] `ProviderId::auth_method()`: Ollama -> KeylessEndpoint, Vertex -> KeylessAdc, `_` -> ApiKey
  - [x] Unit test: `auth_method()` returns the expected variant for every provider

### Phase 2: vertex.rs backend

- [x] Task 3: Create `src/provider/vertex.rs` skeleton + module wiring
  - [x] Register `mod vertex;` in `src/provider/mod.rs`
  - [x] `Vertex::new(model)` struct + `Provider` trait impl signatures
- [x] Task 4: Promote shared Gemini payload builders to `pub(super)` (`src/provider/gemini.rs`)
  - [x] `build_plan_payload`, `build_message_payload`, `build_resolve_payload`, `extract_text` -> `pub(super)` (no logic change)
  - [x] Confirm existing `gemini.rs` tests still pass (extractor coverage unchanged)
- [x] Task 5: `access_token()` with bounded timeout (design §4.2, A3/P8)
  - [x] Order: `GCM_VERTEX_TOKEN` (trimmed, non-empty) -> else `gcloud auth application-default print-access-token`
  - [x] Bound the gcloud subprocess (~10s) with the `mpsc::channel` + `thread::spawn` + `rx.recv_timeout` pattern from `src/resolve/remote/publish.rs:116` (std `Command` has no built-in timeout); timeout -> typed error, not a hang
  - [x] Distinguish spawn `io::ErrorKind::NotFound` (or `which::which("gcloud")`, host.rs:305 idiom) -> "gcloud not found: install the Google Cloud SDK"
  - [x] Non-zero exit / timeout -> "run: gcloud auth application-default login" (surface invalid_grant/reauth hint from stderr)
- [x] Task 6: `project()` / `location()` resolution + validation (design §4.2/§4.3, D3/D1/P2)
  - [x] project: `GCM_VERTEX_PROJECT` -> `GOOGLE_CLOUD_PROJECT` -> `GCP_PROJECT`; missing -> typed `Config` error
  - [x] location: `GCM_VERTEX_LOCATION` -> `GOOGLE_CLOUD_LOCATION` -> `GCP_REGION`; default `global`
  - [x] Validate `location` strictly (host label): `^(global|[a-z][a-z0-9-]*)$`
  - [x] Validate `project` permissively (path segment): allow legacy domain-scoped ids (`.`/`:`); reject URL-structural chars
- [x] Task 7: `request()` URL + auth header (design §4.2)
  - [x] base: `GCM_VERTEX_BASE_URL` (test seam) | `https://aiplatform.googleapis.com` (global) | `https://{location}-aiplatform.googleapis.com` (regional)
  - [x] endpoint: `{base}/v1/projects/{project}/locations/{location}/publishers/google/models/{model}:generateContent`
  - [x] header `Authorization: Bearer {token}`; body = shared gemini payload
  - [x] Pass `auth_env_var: None` to the HTTP layer (so 401/403 -> `Http(status)`, not `Auth{env_var}`)
- [x] Task 8: Vertex error mapping (design §4.2, N1/P6)
  - [x] Intercept `ErrorKind::Http(401)` in vertex.rs -> "run: gcloud auth application-default login"
  - [x] Intercept `ErrorKind::Http(403)` -> distinguish IAM-denied vs "enable the Vertex AI API on project {project}"
  - [x] No change to shared `http.rs::classify_status`
- [x] Task 9: trait glue + cache id
  - [x] `generate_plan` / `generate_message` / `resolve_hunks` call shared `extract_text` + `parse_*` (three-line shape)
  - [x] `cache_model_id()` -> `"vertex:{model}"`; `diff_budget()` -> `DiffBudget::standard()`

### Phase 3: Config + apply_to_env + call-site rewrite

- [x] Task 10: `ProviderConfig` fields (`src/config.rs`, design §4.3)
  - [x] Add `project: Option<String>` + `location: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`
  - [x] Confirm no config `version` bump needed (a v2 file parses unchanged)
- [x] Task 11: `env_plan` Vertex arm (`src/config.rs:376`, A1)
  - [x] Branch on `auth_method()`: `KeylessAdc` -> bridge `project`/`location` to `GCM_VERTEX_PROJECT`/`GCM_VERTEX_LOCATION` (only when unset), no endpoint
- [x] Task 12: `commented_reference` Vertex arm (`src/config.rs:290`, A4)
  - [x] `KeylessAdc` -> emit `project =` / `location =` comment lines, not `endpoint =`
- [x] Task 13: Provider-registry cleanup in `src/config.rs` (round-2 review)
  - [x] Expand `cloud_then_ollama()` (:1140) from `[ProviderId; 5]` to include `Vertex` (`[ProviderId; 6]`); rename to `all_providers()`. It is the iteration source of truth for `commented_reference` (:299), first-run `run_wizard` (:421), and `run_provider_wizard` (:673) — **without this, Vertex never appears in any wizard menu and Tasks 14/15 are dead code**
  - [x] Leave `cloud_providers()` (:1151) at `[ProviderId; 4]` intentionally — it means "key-bearing" (drives `any_cloud_key_set`); keyless Vertex is correctly excluded (semantic note only, no change)
  - [x] `canonicalize_model()` (:541) needs no arm — its `_ => m.to_string()` wildcard already handles Vertex's bare gemini model ids (no `models/` prefix, unlike AI Studio)
  - [x] Grep-audit for any other `key_env_var().is_none()` / `== None` "is-Ollama" branches; route through `auth_method()`

### Phase 4: Wizards

- [x] Task 14: Interactive `gcm provider` wizard third branch (`run_provider_wizard`, design §4.4)
  - [x] `KeylessAdc` -> prompt project (required; prefill `GOOGLE_CLOUD_PROJECT`/config) + location (default `global`); skip key prompt
  - [x] Non-blocking ADC probe (spinner): try `access_token()` -> ok "ADC ready" / err warn-and-continue
  - [x] Model list = static Gemini set (D4); persist `ProviderConfig{ id: Vertex, project, location, model, models }`
- [x] Task 15: First-run `run_wizard` third branch (`src/config.rs:420`, A2/P1)
  - [x] Replace the `match id.key_env_var()` (:443) two-way branch with `auth_method()`; add the `KeylessAdc` project/location branch (no Ollama endpoint prompt for Vertex)

### Phase 5: gcm status

- [x] Task 16: `ProviderStatus` new fields + rendering (`src/status.rs`, N2/P4)
  - [x] Add `project`/`location`/`auth_source: Option<String>` to `ProviderStatus` (skip-serialize when None)
  - [x] `build_report` (:161): add a Vertex branch (no bogus `key:` row); infer `auth_source` = `GCM_VERTEX_TOKEN` if set else `gcloud ADC` (no subprocess)
  - [x] `print_provider_block` (:472): print project/location/auth-source for Vertex
  - [x] `is_activated`: Vertex activates when a project resolves (env/config), mirroring Ollama's keyless rule
  - [x] `PROVIDER_ORDER` (:31): `[ProviderId; 5]` -> `[ProviderId; 6]` (add Vertex)
  - [x] `selected_provider` valid-names warning (:248): add `vertex` (N5)
- [x] Task 17: CLI help text (`src/cli.rs:20`, P3)
  - [x] Add `vertex` to the `--provider` / `GCM_PROVIDER` valid-names help string (the third hardcoded list)

### Phase 6: models.rs arms

- [x] Task 18: Vertex short-circuit + compile-required arms (`src/provider/models.rs`, N3/P5)
  - [x] `fetch_supported_models` (:37): short-circuit Vertex at the top -> static Gemini set (D4)
  - [x] Add a `ProviderId::Vertex` arm to each of the 5 exhaustive `match id` fns: `fetch_live` (:105), `resolved_base_url_with` (:159), `parse_models` (:186), `static_fallback_models` (:264), `provider_name` (:298) — unreachable at runtime, reuse Google's values
  - [x] Confirm `keep_chat_model` (:236) needs no arm (has `_ => true`)

### Phase 7: Testing & Validation

- [x] Task 19: vertex.rs unit tests (design §5)
  - [x] `request()` URL: global (bare `aiplatform`) vs regional (`{loc}-aiplatform`); `Authorization: Bearer`; `GCM_VERTEX_BASE_URL` override; `cache_model_id()` prefix
  - [x] Token precedence (`GCM_VERTEX_TOKEN` beats gcloud); missing project -> typed `Config` error; token-acquisition failure -> actionable typed error; timeout -> typed error (slow fake gcloud)
  - [x] Error mapping: mock 401 -> gcloud-reauth text (never "check <env_var>"); 403 -> IAM/API-not-enabled text
  - [x] Input validation: malformed `location` rejected pre-request; legacy domain-scoped `project` accepted
- [x] Task 20: shared-payload parity + config tests
  - [x] Vertex uses the same `build_plan_payload`/`build_resolve_payload` output as Gemini
  - [x] `project`/`location` round-trip; `skip_serializing_if` omits when None; v2 file (no fields) loads; `apply_to_env` sets vertex env vars only when unset
- [x] Task 21: wizard + status tests
  - [x] Wizard resolution helpers (project prefill from `GOOGLE_CLOUD_PROJECT`; location default `global`)
  - [x] `gcm status --provider vertex`: project/location/auth-source, no key row; `--json` carries new fields; `PROVIDER_ORDER` includes Vertex
- [x] Task 22: acceptance test (no gcloud in CI)
  - [x] End-to-end `gcm --provider vertex` against a local mock `generateContent` server via `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN`
- [x] Task 23: pre-flight gates
  - [x] `cargo fmt --check` clean
  - [x] `cargo clippy` clean (no new warnings)
  - [x] `cargo test` green (unit + integration)
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

 succeeded in 0ms:
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


exec
/bin/zsh -lc 'git diff --unified=80 main...HEAD -- src/provider/vertex.rs src/provider/mod.rs src/provider/gemini.rs src/provider/models.rs src/config.rs src/status.rs src/cli.rs tests/vertex.rs src/provider/http.rs tests/status.rs' in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
diff --git a/src/cli.rs b/src/cli.rs
index 7c9a117..906ca36 100644
--- a/src/cli.rs
+++ b/src/cli.rs
@@ -1,100 +1,100 @@
 use clap::Parser;
 
 use crate::config::AutoPolicy;
 use crate::privacy::SecretScanMode;
 use crate::provider::ProviderId;
 
 /// Build-stamped version: crate version plus the git short SHA from build.rs (AC-1).
 pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("GCM_GIT_SHA"));
 
 const EGRESS_DISCLOSURE: &str = "\
 gcm groups your working-tree changes into logical commits and commits the first group;\n\
 run it again to commit the next group. Grouping operates on whole files over the entire\n\
 working tree, so it overrides any manual hunk-level (git add -p) staging: group 1's files\n\
 are staged in full, later groups are left unstaged (their changes are never lost).\n\
 \n\
 MACHINE MODE: use --json to emit a stable JSON envelope on stdout (status: plan/noop/\n\
 committed/fallback/error). Combine with --plan-only for a non-destructive preview, or\n\
 --yes (alias --no-input) for unattended commits. All diagnostics go to stderr.\n\
 \n\
-PROVIDER: select with --provider (groq, google, openai, anthropic, ollama) or GCM_PROVIDER\n\
+PROVIDER: select with --provider (groq, google, vertex, openai, anthropic, ollama) or GCM_PROVIDER\n\
 (precedence flag > env > default groq); override the model with --model or the per-provider\n\
 env (GCM_GROQ_MODEL / GCM_GEMINI_MODEL / GCM_OPENAI_MODEL / GCM_ANTHROPIC_MODEL /\n\
 GCM_OLLAMA_MODEL). Keys: GROQ_API_KEY, GEMINI_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY.\n\
 Ollama is local and needs NO key - it talks to http://localhost:11434 (override with\n\
 OLLAMA_HOST / GCM_OLLAMA_BASE_URL).\n\
 \n\
 PRIVACY: gcm sends your working-tree diff and the content of untracked, non-gitignored\n\
 files to the configured LLM provider to generate the plan and commit messages.\n\
 Gitignored files (e.g. .env) are never sent. Repo-local .gcmignore/gcmignore patterns\n\
 exclude matching paths from analysis. Use --secret-scan=redact or abort to opt into\n\
 best-effort credential scanning before provider egress. With --provider=ollama and a\n\
 local model, nothing leaves the machine (zero-egress); an Ollama `:cloud` model routes\n\
 through Ollama Cloud and is NOT zero-egress. See the README for each provider's data policy.\n\
 \n\
 LOGGING: set GCM_LOG_LEVEL=off|error|warn|info|debug|trace (default off). The legacy\n\
 GCM_DEBUG=1 shortcut still enables debug-level output. Logs always go to stderr.\n\
 \n\
 RESILIENCE: transient provider failures (HTTP 429 rate limit, 5xx) are retried with\n\
 bounded exponential backoff; 400/auth errors fail fast. Set GCM_DEBUG=1 (or\n\
 GCM_LOG_LEVEL=debug) to print the typed error and retry attempts to stderr.";
 
 #[derive(Parser, Debug)]
 #[command(
     name = "gcm",
     version = VERSION,
     about = "Generate one signed conventional-commit from your working-tree changes via an LLM provider.",
     after_help = EGRESS_DISCLOSURE,
     after_long_help = EGRESS_DISCLOSURE
 )]
 pub struct Cli {
     /// Optional subcommand. With none, gcm runs the normal commit flow.
     #[command(subcommand)]
     pub command: Option<Commands>,
 
     /// Preview the grouping plan (or the single-commit message with --all) and
     /// exit without staging or committing.
     #[arg(long, global = true)]
     pub dry_run: bool,
 
     /// Emit a stable JSON envelope on stdout instead of human-oriented prose.
     /// All diagnostics are sent to stderr so stdout contains a single valid
     /// JSON object. Global so it is accepted after a subcommand too
     /// (e.g. `gcm status --json`).
     #[arg(long, global = true)]
     pub json: bool,
 
     /// Generate the plan (or single-commit preview with --all) and exit without
     /// staging, committing, or touching the cache.
     #[arg(long)]
     pub plan_only: bool,
 
     /// Skip grouping and commit all changes as a single commit.
     #[arg(long)]
     pub all: bool,
 
     /// Discard any cached grouping plan and re-analyze from scratch.
     #[arg(long)]
     pub reset: bool,
 
     /// Auto-confirm the commit without prompting (for non-interactive / agent / CI use).
     #[arg(long, global = true, visible_alias = "no-input")]
     pub yes: bool,
 
     ///LLM provider: groq (default), google (Gemini), openai, anthropic, or ollama (local,
     /// no key, zero-egress). Overrides GCM_PROVIDER (precedence: flag > env > default).
     #[arg(long, value_enum, global = true)]
     pub provider: Option<ProviderId>,
 
     /// Model id for the selected provider (e.g. gpt-5.4-mini).
     /// Overrides the per-provider model env var.
     #[arg(long, global = true)]
     pub model: Option<String>,
 
     /// Re-run the interactive provider setup wizard (updating keys/selections),
     /// then continue with the normal commit flow.
     #[arg(long)]
     pub reconfigure: bool,
 
     /// Optional pre-send secret scan: off (default), redact detected values, or abort
     /// before any provider request. Overrides GCM_SECRET_SCAN.
diff --git a/src/config.rs b/src/config.rs
index b52fec6..5fd2978 100644
--- a/src/config.rs
+++ b/src/config.rs
@@ -1,157 +1,166 @@
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
 
 use serde::{Deserialize, Serialize};
 
 use crate::error::GcmError;
-use crate::provider::ProviderId;
+use crate::provider::{AuthMethod, ProviderId};
 
 /// On-disk config format version (mirrors `cache::CacheFile.version`). v2 (CLO-516)
 /// added the per-provider `models` enabled-set whitelist. A v1 file is accepted and
 /// migrated up on read (its `models` default empty = unrestricted); an unknown
 /// version (0 or > current) is treated as "no usable config" so a future schema can
 /// still evolve. A *newer* binary's v2 file read by an old v1-only binary is a
 /// `WrongVersion` miss there (forward-compat: it re-onboards, never mis-enforces).
 const CONFIG_FORMAT_VERSION: u32 = 2;
 /// Config file name within the config dir (or the `GCM_CONFIG` override dir).
 const CONFIG_FILE_NAME: &str = "config.toml";
 /// Default Ollama endpoint (mirrors `provider::ollama`'s default base URL).
 const DEFAULT_OLLAMA_ENDPOINT: &str = "http://localhost:11434";
 /// Connection timeout for the wizard's Ollama daemon probe (ADR-001 Decision 8):
 /// short enough that an unresponsive endpoint never hangs the wizard.
 const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
 
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
+    /// Vertex only (CLO-537): the GCP project. Bridged into `GCM_VERTEX_PROJECT` by
+    /// [`apply_to_env`] when that var is unset. `None`/skip-serialize for every other
+    /// provider, so a pre-Vertex config file parses unchanged (no version bump).
+    #[serde(default, skip_serializing_if = "Option::is_none")]
+    pub project: Option<String>,
+    /// Vertex only (CLO-537): the location/region (effective default `global`).
+    /// Bridged into `GCM_VERTEX_LOCATION` by [`apply_to_env`] when unset.
+    #[serde(default, skip_serializing_if = "Option::is_none")]
+    pub location: Option<String>,
 }
 
 /// Conflict-resolution settings for `gcm resolve` (CLO-531).
 #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
 
 #[derive(Debug, Clone, Copy, Default, clap::ValueEnum, Serialize, Deserialize, PartialEq, Eq)]
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
 enum LoadIssue {
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
 fn config_dir() -> Option<PathBuf> {
     crate::paths::xdg_gcm_dir_from(
         std::env::var_os("XDG_CONFIG_HOME").as_deref(),
         std::env::var_os("HOME").as_deref(),
         ".config",
     )
 }
 
 /// Pure path resolution (the body of [`config_path`], so the override precedence
 /// is unit-testable without touching process env or the real config dir).
 fn config_path_from(gcm_config: Option<&OsStr>, fallback_dir: Option<PathBuf>) -> Option<PathBuf> {
     if let Some(dir) = gcm_config {
         if !dir.is_empty() {
             return Some(PathBuf::from(dir).join(CONFIG_FILE_NAME));
         }
@@ -215,802 +224,967 @@ fn parse_config(data: &str) -> Result<Config, LoadIssue> {
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
     let text = render_config(config).map_err(io::Error::other)?;
     write_atomic(path, text.as_bytes())
 }
 
 /// The on-disk file body: the live config as TOML, followed by a fully-commented
 /// reference block documenting every provider's overridable settings. Only the
 /// live section is active TOML; the reference is all comments, so the file still
 /// parses. Written on first-run onboarding (and `gcm config`) so the format is
 /// discoverable without reading the docs.
 fn render_config(config: &Config) -> Result<String, toml::ser::Error> {
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
 fn commented_reference() -> String {
     let mut s = String::new();
     s.push_str("# ── Reference: all available settings ──────────────────────────────────────\n");
     s.push_str("# Copy an entry into the section above, uncomment, and edit. A provider entry\n");
     s.push_str("# supports: model (chosen default), models (enabled set), key (cloud),\n");
-    s.push_str("# endpoint (Ollama only). Matching env vars override this file\n");
+    s.push_str("# endpoint (Ollama only), project+location (Vertex only). Matching env vars\n");
+    s.push_str("# override this file\n");
     s.push_str("# (e.g. GCM_OPENAI_MODEL=…, OPENAI_API_KEY=…). An empty/absent `models`\n");
     s.push_str("# means unrestricted; set it via `gcm provider` to restrict usage.\n");
     s.push_str("#\n");
-    for id in cloud_then_ollama() {
+    for id in all_providers() {
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
-        match id.key_env_var() {
-            Some(key_var) => {
-                s.push_str(&format!(
-                    "# key = \"…\"   # inline secret, or set {key_var}\n"
-                ));
+        match id.auth_method() {
+            AuthMethod::ApiKey => {
+                if let Some(key_var) = id.key_env_var() {
+                    s.push_str(&format!(
+                        "# key = \"…\"   # inline secret, or set {key_var}\n"
+                    ));
+                }
             }
-            None => {
+            AuthMethod::KeylessEndpoint => {
                 s.push_str(&format!(
                     "# endpoint = \"{DEFAULT_OLLAMA_ENDPOINT}\"   # or set GCM_OLLAMA_BASE_URL / OLLAMA_HOST\n"
                 ));
             }
+            AuthMethod::KeylessAdc => {
+                s.push_str(
+                    "# project = \"my-gcp-project\"   # required; or set GCM_VERTEX_PROJECT / GOOGLE_CLOUD_PROJECT\n",
+                );
+                s.push_str(
+                    "# location = \"global\"   # or set GCM_VERTEX_LOCATION / GOOGLE_CLOUD_LOCATION\n",
+                );
+            }
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
-        match pc.id.key_env_var() {
-            Some(var) => {
-                if let Some(key) = pc.key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
-                    if !is_set(var) {
-                        out.push((var, key.to_string()));
+        match pc.id.auth_method() {
+            AuthMethod::ApiKey => {
+                if let Some(var) = pc.id.key_env_var() {
+                    if let Some(key) = pc.key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
+                        if !is_set(var) {
+                            out.push((var, key.to_string()));
+                        }
                     }
                 }
             }
-            None => {
+            AuthMethod::KeylessEndpoint => {
                 // Ollama: set the base URL only when neither gcm's own var nor
                 // the Ollama-native OLLAMA_HOST is already set.
                 if let Some(ep) = pc.endpoint.as_deref().filter(|e| !e.trim().is_empty()) {
                     if !is_set("GCM_OLLAMA_BASE_URL") && !is_set("OLLAMA_HOST") {
                         out.push(("GCM_OLLAMA_BASE_URL", ep.to_string()));
                     }
                 }
             }
+            AuthMethod::KeylessAdc => {
+                // Vertex: bridge project/location into the gcm-namespaced vars only
+                // when unset (env still wins: flag > env > config > default).
+                if let Some(p) = pc
+                    .project
+                    .as_deref()
+                    .map(str::trim)
+                    .filter(|p| !p.is_empty())
+                {
+                    if !is_set("GCM_VERTEX_PROJECT") {
+                        out.push(("GCM_VERTEX_PROJECT", p.to_string()));
+                    }
+                }
+                if let Some(l) = pc
+                    .location
+                    .as_deref()
+                    .map(str::trim)
+                    .filter(|l| !l.is_empty())
+                {
+                    if !is_set("GCM_VERTEX_LOCATION") {
+                        out.push(("GCM_VERTEX_LOCATION", l.to_string()));
+                    }
+                }
+            }
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
 
 // ── interactive wizard ──────────────────────────────────────────────────────
 
 /// Run the interactive wizard end to end (enable providers, capture keys from
 /// the environment or a prompt, choose a default) and return the assembled
 /// `Config`. Cloud keys already exported are recorded as `key: None` (env-only);
 /// an empty key input is also env-only. Invalid menu selections re-prompt.
 pub fn run_wizard() -> Result<Config, GcmError> {
-    let all = cloud_then_ollama();
+    let all = all_providers();
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
-        match id.key_env_var() {
-            Some(var) => {
+        match id.auth_method() {
+            AuthMethod::ApiKey => {
+                // ApiKey providers always have a key env var; skip defensively if not.
+                let Some(var) = id.key_env_var() else {
+                    continue;
+                };
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
-            None => {
+            AuthMethod::KeylessEndpoint => {
                 let endpoint = prompt_ollama_endpoint()?;
                 enabled.push(ProviderConfig {
                     id,
                     key: None,
                     endpoint,
                     model: None,
                     models: Vec::new(),
+                    project: None,
+                    location: None,
+                });
+            }
+            AuthMethod::KeylessAdc => {
+                // Vertex: project + location (no key, no endpoint) - fixes the bug
+                // where selecting Vertex in first-run onboarding prompted for an
+                // Ollama endpoint (CLO-537 round-2 A2/P1).
+                let (project, location) = prompt_vertex_target()?;
+                enabled.push(ProviderConfig {
+                    id,
+                    key: None,
+                    endpoint: None,
+                    model: None,
+                    models: Vec::new(),
+                    project: Some(project),
+                    location,
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
 
+/// First-run prompt for the Vertex target: GCP project (required; prefilled from
+/// `GCM_VERTEX_PROJECT` / `GOOGLE_CLOUD_PROJECT`) and location (default `global`).
+/// Returns `(project, location)` where `location` is `None` at the default so the
+/// config file stays minimal. Runs a non-blocking ADC probe (warns, never blocks).
+fn prompt_vertex_target() -> Result<(String, Option<String>), GcmError> {
+    let prefill = std::env::var("GCM_VERTEX_PROJECT")
+        .ok()
+        .or_else(|| std::env::var("GOOGLE_CLOUD_PROJECT").ok())
+        .map(|s| s.trim().to_string())
+        .filter(|s| !s.is_empty());
+    let project = loop {
+        let hint = prefill
+            .as_deref()
+            .map(|p| format!(" [{p}]"))
+            .unwrap_or_default();
+        let input = wizard_read_line(&format!("  Vertex GCP project{hint}: "))?;
+        let v = input.trim();
+        if !v.is_empty() {
+            break v.to_string();
+        }
+        if let Some(p) = prefill.as_deref() {
+            break p.to_string();
+        }
+        eprintln!("  A GCP project is required for Vertex. Try again.");
+    };
+    let loc_input = wizard_read_line("  Vertex location [global]: ")?;
+    let loc = loc_input.trim();
+    let location = if loc.is_empty() || loc == "global" {
+        None
+    } else {
+        Some(loc.to_string())
+    };
+    match crate::provider::vertex_adc_probe() {
+        Ok(()) => eprintln!("  gcloud ADC ready."),
+        Err(msg) => eprintln!(
+            "  note: gcloud ADC not ready ({msg}). Set GCM_VERTEX_TOKEN or run `gcloud auth application-default login` before committing."
+        ),
+    }
+    Ok((project, location))
+}
+
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
 
 // ── enabled-model whitelist + enforcement (CLO-516) ─────────────────────────
 
 /// Canonicalize a model id for enabled-set comparison, per provider, so a value
 /// that differs only by a provider alias is not falsely rejected: Gemini strips a
 /// leading `models/` (its list endpoint returns prefixed names); Ollama treats a
 /// tagless name as `:latest` (what `/api/tags` reports); all values are trimmed.
 /// No general case-folding - model ids are case-sensitive.
 pub(crate) fn canonicalize_model(id: ProviderId, model: &str) -> String {
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
 pub(crate) fn model_is_enabled(cfg: &Config, id: ProviderId, model: &str) -> Result<(), String> {
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
 pub(crate) fn merge_provider_config(
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
 fn preserve_existing_models(enabled: &mut [ProviderConfig], existing: Option<&Config>) {
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
 fn build_config(enabled: &[ProviderConfig], default: ProviderId) -> Result<Config, String> {
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
         model: None,
         models: Vec::new(),
+        project: None,
+        location: None,
     }
 }
 
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
-    let all = cloud_then_ollama();
+    let all = all_providers();
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
-    match id.key_env_var() {
-        Some(var) => {
-            let env_key = env_value(var);
-            let cfg_key = existing_pc.and_then(|p| p.key.clone());
-            if let Some(k) = env_key {
-                // Env wins for the fetch, but never copy an env-derived secret into
-                // the file; preserve any existing inline key (a fallback for when the
-                // env var is unset) rather than erasing it.
-                fetch_key = Some(k);
-                persist_key = cfg_key;
-            } else if let Some(k) = cfg_key {
-                fetch_key = Some(k.clone());
-                persist_key = Some(k); // preserve the existing inline key
-            } else {
-                let typed = match password(format!(
-                    "{} API key (press Enter to skip)",
-                    provider_label(id)
-                ))
-                .mask('*')
-                .interact()
-                {
-                    Ok(s) => s,
-                    Err(_) => return wizard_cancelled(),
-                };
-                let (f, p) = wizard_persist_key(&typed);
-                fetch_key = f;
-                persist_key = p;
+    let mut persist_project: Option<String> = None;
+    let mut persist_location: Option<String> = None;
+    match id.auth_method() {
+        AuthMethod::ApiKey => {
+            // ApiKey providers always have a key env var; skip defensively if not.
+            if let Some(var) = id.key_env_var() {
+                let env_key = env_value(var);
+                let cfg_key = existing_pc.and_then(|p| p.key.clone());
+                if let Some(k) = env_key {
+                    // Env wins for the fetch, but never copy an env-derived secret into
+                    // the file; preserve any existing inline key (a fallback for when
+                    // the env var is unset) rather than erasing it.
+                    fetch_key = Some(k);
+                    persist_key = cfg_key;
+                } else if let Some(k) = cfg_key {
+                    fetch_key = Some(k.clone());
+                    persist_key = Some(k); // preserve the existing inline key
+                } else {
+                    let typed = match password(format!(
+                        "{} API key (press Enter to skip)",
+                        provider_label(id)
+                    ))
+                    .mask('*')
+                    .interact()
+                    {
+                        Ok(s) => s,
+                        Err(_) => return wizard_cancelled(),
+                    };
+                    let (f, p) = wizard_persist_key(&typed);
+                    fetch_key = f;
+                    persist_key = p;
+                }
             }
         }
-        None => {
+        AuthMethod::KeylessEndpoint => {
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
+        AuthMethod::KeylessAdc => {
+            // Vertex: project (required) + location (default global); no key, no
+            // endpoint. The model list comes from the static Gemini set (the fetch
+            // below short-circuits Vertex).
+            let default_project = existing_pc
+                .and_then(|p| p.project.clone())
+                .or_else(|| env_value("GCM_VERTEX_PROJECT"))
+                .or_else(|| env_value("GOOGLE_CLOUD_PROJECT"))
+                .unwrap_or_default();
+            let mut project_input = cliclack::input("GCP project (required for Vertex)");
+            if !default_project.trim().is_empty() {
+                project_input = project_input.default_input(default_project.trim());
+            }
+            let project = match project_input
+                .validate(|s: &String| {
+                    if s.trim().is_empty() {
+                        Err("a GCP project is required".to_string())
+                    } else {
+                        Ok(())
+                    }
+                })
+                .interact::<String>()
+            {
+                Ok(s) => s.trim().to_string(),
+                Err(_) => return wizard_cancelled(),
+            };
+            let default_location = existing_pc
+                .and_then(|p| p.location.clone())
+                .unwrap_or_else(|| "global".to_string());
+            let location = match cliclack::input("Vertex location")
+                .default_input(&default_location)
+                .interact::<String>()
+            {
+                Ok(s) => s.trim().to_string(),
+                Err(_) => return wizard_cancelled(),
+            };
+            persist_project = Some(project);
+            // Keep the file minimal: omit location at the default `global`.
+            persist_location = if location.is_empty() || location == "global" {
+                None
+            } else {
+                Some(location)
+            };
+            // Non-blocking ADC probe (warns; never blocks a keyless setup).
+            let sp = spinner();
+            sp.start("Checking gcloud ADC...");
+            match crate::provider::vertex_adc_probe() {
+                Ok(()) => sp.stop("gcloud ADC ready"),
+                Err(msg) => sp.stop(format!(
+                    "ADC not ready: {msg} (set GCM_VERTEX_TOKEN or run `gcloud auth application-default login`)"
+                )),
+            }
+        }
     }
 
     // 3. Fetch the model list (spinner; never fails - falls back).
     let sp = spinner();
     sp.start("Fetching supported models...");
     let outcome = crate::provider::fetch_supported_models(
         id,
         fetch_key.as_deref(),
         fetch_endpoint.as_deref(),
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
         .map(|m| (m.clone(), m.clone(), ""))
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
-    let updated = build_provider_config(id, persist_key, persist_endpoint, default_model, selected)
-        .map_err(GcmError::Git)?;
+    let mut updated =
+        build_provider_config(id, persist_key, persist_endpoint, default_model, selected)
+            .map_err(GcmError::Git)?;
+    // Vertex carries project/location instead of a key/endpoint (None for others).
+    updated.project = persist_project;
+    updated.location = persist_location;
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
 
 /// Print the cancellation outro and signal "no change" (nothing persisted).
 fn wizard_cancelled() -> Result<bool, GcmError> {
     let _ = cliclack::outro_cancel("Cancelled - no changes made.");
     Ok(false)
 }
 
 /// Map a wizard I/O error into the workflow error type.
 fn wizard_io(e: io::Error) -> GcmError {
     GcmError::Git(format!("provider wizard I/O error: {e}"))
 }
 
 /// The multiselect candidate list (D7.3, wizard side): fetched ∪ current enabled ∪
 /// current default, deduped, fetched first - so the user's existing selections and
 /// default stay selectable even if the live list omitted them. Membership is by
 /// canonical form (review L1), so a migrated `llama3` doesn't duplicate a fetched
 /// `llama3:latest`. Pure.
 fn wizard_model_list(
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
 
 /// The pre-selected default model: the current default if it survived into
 /// `selected` (canonical match, review L1), else the first selected (None only when
 /// `selected` is empty). Returns the matching `selected` entry. Pure.
 fn initial_default_model(
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
 fn ollama_wizard_default_endpoint(effective: &str, config_endpoint: Option<&str>) -> String {
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
 fn build_provider_config(
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
+        project: None,
+        location: None,
     })
 }
 
 /// Decide `(fetch_key, persist_key)` from a freshly-typed key: a blank entry is
 /// "skip" (no key, nothing stored); a non-blank entry is used for the fetch and
 /// stored inline. Pure (keeps the secret-handling rule unit-testable). Pure.
 fn wizard_persist_key(typed: &str) -> (Option<String>, Option<String>) {
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
     s.push_str("  export GCM_PROVIDER=groq   # or google, openai, anthropic, ollama\n\n");
     s.push_str("Option B - write a 0600 config file (see ADR-001 Decision 4 for the path):\n\n");
     s.push_str(sample_toml_template());
     s
 }
 
 /// A minimal, copy-pasteable `config.toml` template for the non-TTY path.
 fn sample_toml_template() -> &'static str {
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
@@ -1060,348 +1234,360 @@ fn read_secret(prompt: &str) -> io::Result<String> {
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
-fn cloud_then_ollama() -> [ProviderId; 5] {
+/// Every selectable provider, in wizard/reference display order (CLO-537 renamed this
+/// from `cloud_then_ollama` and added Vertex; the old name implied a key-bearing/Ollama
+/// dichotomy that no longer holds). This is the single source of truth iterated by the
+/// reference template and both wizards - a provider absent here is invisible in the UI.
+fn all_providers() -> [ProviderId; 6] {
     [
         ProviderId::Groq,
         ProviderId::Google,
+        ProviderId::Vertex,
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
+        ProviderId::Vertex => "Google (Vertex AI)",
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
             model: None,
             models: Vec::new(),
+            project: None,
+            location: None,
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
+            project: None,
+            location: None,
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
+            project: None,
+            location: None,
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
@@ -1476,160 +1662,213 @@ mod tests {
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
 
+    #[test]
+    fn env_plan_bridges_vertex_project_and_location() {
+        let cfg = Config {
+            conflict: ConflictConfig::default(),
+            version: CONFIG_FORMAT_VERSION,
+            default: ProviderId::Vertex,
+            providers: vec![ProviderConfig {
+                id: ProviderId::Vertex,
+                key: None,
+                endpoint: None,
+                model: None,
+                models: Vec::new(),
+                project: Some("my-proj".to_string()),
+                location: Some("us-central1".to_string()),
+            }],
+        };
+        // Nothing set -> both project and location bridge to the gcm-namespaced vars.
+        let plan = env_plan(&cfg, |_| false);
+        assert!(plan.contains(&("GCM_VERTEX_PROJECT", "my-proj".to_string())));
+        assert!(plan.contains(&("GCM_VERTEX_LOCATION", "us-central1".to_string())));
+        // A pre-set env var wins and is never overwritten (flag > env > config).
+        let plan2 = env_plan(&cfg, |v| v == "GCM_VERTEX_PROJECT");
+        assert!(!plan2.iter().any(|(k, _)| *k == "GCM_VERTEX_PROJECT"));
+        assert!(plan2.contains(&("GCM_VERTEX_LOCATION", "us-central1".to_string())));
+    }
+
+    #[test]
+    fn vertex_project_location_round_trip_and_skip_when_none() {
+        // With values -> serialized and read back unchanged.
+        let with = ProviderConfig {
+            id: ProviderId::Vertex,
+            key: None,
+            endpoint: None,
+            model: None,
+            models: Vec::new(),
+            project: Some("p".to_string()),
+            location: Some("us-west1".to_string()),
+        };
+        let text = toml::to_string_pretty(&with).unwrap();
+        assert!(text.contains("project = \"p\""), "{text}");
+        assert!(text.contains("location = \"us-west1\""), "{text}");
+        assert_eq!(toml::from_str::<ProviderConfig>(&text).unwrap(), with);
+        // None -> both keys skip-serialize (a pre-Vertex file needs no version bump).
+        let without = pc(ProviderId::Openai, None, None);
+        let text2 = toml::to_string_pretty(&without).unwrap();
+        assert!(!text2.contains("project"), "{text2}");
+        assert!(!text2.contains("location"), "{text2}");
+        // A pre-Vertex file (no project/location keys) still parses.
+        let parsed: ProviderConfig = toml::from_str("id = \"openai\"\n").unwrap();
+        assert_eq!(parsed.project, None);
+        assert_eq!(parsed.location, None);
+    }
+
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
             text.contains("gpt-5.4-mini"),
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
             providers: vec![pcm(ProviderId::Openai, "gpt-5.4-mini")],
         };
         let text = toml::to_string_pretty(&cfg).unwrap();
         let back = parse_config(&text).unwrap();
         assert_eq!(back.providers[0].model.as_deref(), Some("gpt-5.4-mini"));
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
diff --git a/src/provider/gemini.rs b/src/provider/gemini.rs
index 23275bf..f2d96d7 100644
--- a/src/provider/gemini.rs
+++ b/src/provider/gemini.rs
@@ -52,232 +52,232 @@ impl Gemini {
             .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
     }
 
     fn request<'a>(&self, key: &str, payload: &'a Value) -> HttpRequest<'a> {
         HttpRequest {
             provider: NAME,
             auth_env_var: API_KEY_ENV,
             endpoint: format!(
                 "{}/v1beta/models/{}:generateContent",
                 self.base_url().trim_end_matches('/'),
                 self.model
             ),
             auth: Some(("x-goog-api-key", key.to_string())),
             extra_headers: Vec::new(),
             payload,
         }
     }
 }
 
 impl Provider for Gemini {
     fn name(&self) -> &'static str {
         NAME
     }
 
     fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError> {
         let key = self.api_key()?;
         let payload = build_plan_payload(ctx);
         let raw = http::post_json(&self.request(&key, &payload))?;
         let json = extract_text(&raw)?;
         if json.is_empty() {
             return Err(empty());
         }
         crate::plan::parse_defensive(&json).map_err(|e| ProviderError {
             provider: NAME,
             kind: ErrorKind::Deserialize(e.to_string()),
         })
     }
 
     fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError> {
         let key = self.api_key()?;
         let payload = build_message_payload(&super::message_user_content(diff));
         let raw = http::post_json(&self.request(&key, &payload))?;
         let message = extract_text(&raw)?;
         if message.is_empty() {
             return Err(empty());
         }
         Ok(message)
     }
 
     fn resolve_hunks(
         &self,
         ctx: &super::ResolveContext,
     ) -> Result<Vec<super::Resolution>, ProviderError> {
         let key = self.api_key()?;
         let payload = build_resolve_payload(ctx);
         let raw = http::post_json(&self.request(&key, &payload))?;
         let json = extract_text(&raw)?;
         if json.is_empty() {
             return Err(empty());
         }
         super::parse_resolutions(NAME, &json, ctx.hunks.len())
     }
 
     fn cache_model_id(&self) -> String {
         format!("google:{}", self.model)
     }
 
     fn diff_budget(&self) -> DiffBudget {
         // gemini-3.1-flash-lite has a very large context; standard budget for v1.
         DiffBudget::standard()
     }
 }
 
 fn empty() -> ProviderError {
     ProviderError {
         provider: NAME,
         kind: ErrorKind::EmptyResponse,
     }
 }
 
-fn build_resolve_payload(ctx: &super::ResolveContext) -> Value {
+pub(super) fn build_resolve_payload(ctx: &super::ResolveContext) -> Value {
     json!({
         "systemInstruction": { "parts": [ { "text": super::RESOLVE_SYSTEM_PROMPT } ] },
         "contents": [ { "role": "user", "parts": [ { "text": super::resolve_user_content(ctx) } ] } ],
         "generationConfig": {
             "temperature": ctx.temperature,
             "responseMimeType": "application/json",
             "responseSchema": super::gemini_resolve_schema(),
             "thinkingConfig": { "thinkingLevel": "MINIMAL" }
         }
     })
 }
 
-fn build_plan_payload(ctx: &GroupingContext) -> Value {
+pub(super) fn build_plan_payload(ctx: &GroupingContext) -> Value {
     json!({
         "systemInstruction": { "parts": [ { "text": super::GROUPING_SYSTEM_PROMPT } ] },
         "contents": [ { "role": "user", "parts": [ { "text": super::grouping_user_content(ctx) } ] } ],
         "generationConfig": {
             "responseMimeType": "application/json",
             "responseSchema": crate::plan::gemini_schema(),
             "thinkingConfig": { "thinkingLevel": "MINIMAL" }
         }
     })
 }
 
-fn build_message_payload(user_content: &str) -> Value {
+pub(super) fn build_message_payload(user_content: &str) -> Value {
     json!({
         "systemInstruction": { "parts": [ { "text": super::SYSTEM_PROMPT } ] },
         "contents": [ { "role": "user", "parts": [ { "text": user_content } ] } ],
         "generationConfig": {
             "thinkingConfig": { "thinkingLevel": "MINIMAL" }
         }
     })
 }
 
 #[derive(Deserialize)]
 struct GeminiResponse {
     candidates: Option<Vec<Candidate>>,
     #[serde(rename = "promptFeedback")]
     prompt_feedback: Option<PromptFeedback>,
 }
 
 #[derive(Deserialize)]
 struct PromptFeedback {
     #[serde(rename = "blockReason")]
     block_reason: Option<String>,
 }
 
 #[derive(Deserialize)]
 struct Candidate {
     content: Option<Content>,
     #[serde(rename = "finishReason")]
     finish_reason: Option<String>,
 }
 
 #[derive(Deserialize)]
 struct Content {
     parts: Option<Vec<Part>>,
 }
 
 #[derive(Deserialize)]
 struct Part {
     text: Option<String>,
     thought: Option<bool>,
 }
 
 /// Extract the answer text from a Gemini response. Checks for prompt- and
 /// candidate-level blocks BEFORE reading content (a safety block returns 200 OK
 /// with no content), concatenates non-thought parts, and applies the `<think>`
 /// backstop. Returns `Ok("")` for a genuinely empty STOP response (the caller
 /// maps that to `EmptyResponse`); blocks/truncation are typed errors.
-fn extract_text(raw: &str) -> Result<String, ProviderError> {
+pub(super) fn extract_text(raw: &str) -> Result<String, ProviderError> {
     let err = |kind| ProviderError {
         provider: NAME,
         kind,
     };
     let resp: GeminiResponse =
         serde_json::from_str(raw).map_err(|e| err(ErrorKind::Deserialize(e.to_string())))?;
 
     if let Some(reason) = resp
         .prompt_feedback
         .as_ref()
         .and_then(|p| p.block_reason.as_deref())
         .map(str::trim)
         .filter(|r| !r.is_empty())
     {
         return Err(err(ErrorKind::BadRequest {
             detail: Some(format!("Gemini blocked the prompt (reason: {reason})")),
         }));
     }
 
     let Some(cand) = resp.candidates.as_ref().and_then(|c| c.first()) else {
         return Err(err(ErrorKind::EmptyResponse));
     };
 
     if let Some(fr) = cand.finish_reason.as_deref() {
         match fr {
             "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => {
                 return Err(err(ErrorKind::BadRequest {
                     detail: Some(format!("Gemini blocked the response (finishReason: {fr})")),
                 }));
             }
             "MAX_TOKENS" => {
                 return Err(err(ErrorKind::Deserialize(
                     "Gemini response truncated (finishReason: MAX_TOKENS); the diff may be too large"
                         .to_string(),
                 )));
             }
             _ => {}
         }
     }
 
     let text: String = cand
         .content
         .as_ref()
         .and_then(|c| c.parts.as_ref())
         .map(|parts| {
             parts
                 .iter()
                 .filter(|p| p.thought != Some(true))
                 .filter_map(|p| p.text.as_deref())
                 .collect::<String>()
         })
         .unwrap_or_default();
 
     Ok(super::strip_think(&text).trim().to_string())
 }
 
 #[cfg(test)]
 mod tests {
     use super::*;
 
     fn ctx() -> GroupingContext {
         GroupingContext {
             file_list: "a.rs".to_string(),
             status: " M a.rs".to_string(),
             stat: "1 file".to_string(),
             body: "diff --git a/a.rs b/a.rs".to_string(),
         }
     }
 
     #[test]
     fn plan_payload_has_response_schema_and_thinking() {
         let p = build_plan_payload(&ctx());
         let gc = &p["generationConfig"];
         assert_eq!(gc["responseMimeType"], json!("application/json"));
         assert_eq!(gc["thinkingConfig"]["thinkingLevel"], json!("MINIMAL"));
         // OpenAPI-subset schema (upper-case types)
         assert_eq!(gc["responseSchema"]["type"], json!("OBJECT"));
         assert_eq!(
             p["systemInstruction"]["parts"][0]["text"],
             json!(super::super::GROUPING_SYSTEM_PROMPT)
diff --git a/src/provider/mod.rs b/src/provider/mod.rs
index 4b67c30..bfeb653 100644
--- a/src/provider/mod.rs
+++ b/src/provider/mod.rs
@@ -1,98 +1,99 @@
 //! Provider abstraction (CLO-489, FR-11): one synchronous trait (ADR-001
 //! Decision 2 - blocking client, no async) that every LLM backend implements,
 //! plus a flag/env registry (FR-12, precedence flag > env > default) and a
 //! provider-agnostic error taxonomy generalized from CLO-488's `GroqError`.
 //!
 //! Backends: [`groq`] and [`openai`] share the OpenAI-compatible chat shape;
 //! [`gemini`] uses Google's divergent `generateContent`/`responseSchema` shape;
 //! [`ollama`] (CLO-495) is the local, key-free zero-egress backend - native
 //! `/api/chat` with a JSON-Schema `format`. Shared HTTP transport + retry/backoff
 //! (CLO-488) lives in [`http`].
 
 mod anthropic;
 mod gemini;
 mod groq;
 mod http;
 mod models;
 pub(crate) mod ollama;
 mod openai;
+mod vertex;
 
 pub(crate) use models::{fetch_supported_models, FetchSource};
 
 use std::fmt;
 use std::time::Duration;
 
 use clap::ValueEnum;
 use serde::{Deserialize, Serialize};
 use serde_json::json;
 
 use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
 use crate::plan::Plan;
 
 /// One LLM provider (FR-11). Synchronous (ADR-001 Decision 2). Both calls are
 /// required: the structured grouping plan and the single commit message (tracer,
 /// grouping fallback, and per-group message regeneration on an advanced cache hit).
 pub trait Provider {
     /// Stable display name for messages/debug (e.g. "Groq" / "Google" / "OpenAI").
     fn name(&self) -> &'static str;
     /// Structured grouping plan; defensively parsed into a typed [`Plan`].
     fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError>;
     /// A single conventional-commit message for the gathered diff.
     fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError>;
     /// Provider-qualified model id folded into the cache freshness fingerprint
     /// (FR-27); resolvable with **no** API key (e.g. "groq:openai/gpt-oss-120b").
     fn cache_model_id(&self) -> String;
     /// Per-provider diff budget (FR-13a), env-overridable.
     fn diff_budget(&self) -> DiffBudget;
     /// Resolve conflict hunks that could not be resolved deterministically.
     /// Sends base/ours/theirs at function granularity with a 3-way prompt.
     /// Returns the resolved replacement for each hunk, in input order.
     fn resolve_hunks(&self, ctx: &ResolveContext) -> Result<Vec<Resolution>, ProviderError>;
 }
 
 /// Typed, provider-agnostic failure taxonomy (FR-21). Carries the active provider
 /// name so [`fmt::Display`] is specific without a separate variant per provider;
 /// [`is_retryable`] decides which `kind`s are retried with bounded backoff (FR-22).
 #[derive(Debug)]
 pub struct ProviderError {
     pub provider: &'static str,
     pub kind: ErrorKind,
 }
 
 /// The failure category (generalized from CLO-488's `GroqError`). `MissingKey`
 /// and `Auth` carry the exact env var so the message names the right key (FR-18).
 #[derive(Debug)]
 pub enum ErrorKind {
     /// The provider's API key env var is unset/blank (fatal, never retried).
     MissingKey { env_var: &'static str },
     /// HTTP 429 rate limit (retryable); `retry_after` from a `Retry-After` header.
     RateLimit { retry_after: Option<Duration> },
     /// HTTP 401/403: the API key was rejected (fatal).
     Auth { status: u16, env_var: &'static str },
     /// HTTP 400 or a content block (e.g. Gemini safety): not retried.
     BadRequest { detail: Option<String> },
     /// HTTP 5xx incl. 504 Gateway Timeout (retryable).
     Server(u16),
     /// Any other unexpected non-2xx status (not retried).
     Http(u16),
     /// Client-side request timeout (not retried).
     Timeout,
     /// Connection/transport failure - DNS, refused, reset (not retried).
     Transport(String),
     /// A 2xx response carried no usable content (not retried).
     EmptyResponse,
     /// The response/plan could not be parsed (not retried).
     Deserialize(String),
     /// A configuration error (e.g. an unknown provider name); fatal, not retried.
     Config(String),
 }
 
 impl ProviderError {
     fn new(provider: &'static str, kind: ErrorKind) -> Self {
         ProviderError { provider, kind }
     }
 }
 
 impl fmt::Display for ProviderError {
     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
         let p = self.provider;
@@ -260,275 +261,311 @@ pub(super) fn gemini_resolve_schema() -> serde_json::Value {
         "required": ["resolutions"]
     })
 }
 
 /// Build the user content for a resolve call (CLO-531).
 pub(super) fn resolve_user_content(ctx: &ResolveContext) -> String {
     let mut s = format!(
         "File: {}\nStyle context:\n{}\n\nResolve these hunks:\n",
         ctx.path, ctx.style_context
     );
     for (i, h) in ctx.hunks.iter().enumerate() {
         s.push_str(&format!("\nHunk {}:\n", i));
         if let Some(base) = &h.base {
             s.push_str(&format!("BASE:\n{}\n", base));
         }
         s.push_str(&format!("OURS:\n{}\n", h.ours));
         s.push_str(&format!("THEIRS:\n{}\n", h.theirs));
     }
     s
 }
 
 /// Defensively parse a provider's resolution JSON. Returns resolutions in the
 /// order requested; missing indices are omitted, and out-of-range indices are
 /// dropped. The provider name is used for error attribution only.
 pub(super) fn parse_resolutions(
     provider: &'static str,
     raw: &str,
     expected_count: usize,
 ) -> Result<Vec<Resolution>, ProviderError> {
     let text = strip_think(raw).trim().to_string();
     if text.is_empty() {
         return Err(ProviderError {
             provider,
             kind: ErrorKind::EmptyResponse,
         });
     }
     #[derive(Deserialize)]
     struct ResolveBody {
         resolutions: Vec<ResolutionEntry>,
     }
     #[derive(Deserialize)]
     struct ResolutionEntry {
         hunk_index: usize,
         replacement: String,
     }
     let body: ResolveBody = serde_json::from_str(&text).map_err(|e| ProviderError {
         provider,
         kind: ErrorKind::Deserialize(format!("resolution parse error: {e}")),
     })?;
     let mut out: Vec<Resolution> = body
         .resolutions
         .into_iter()
         .filter(|r| r.hunk_index < expected_count)
         .map(|r| Resolution {
             hunk_index: r.hunk_index,
             replacement: r.replacement,
         })
         .collect();
     // Sort by index so callers can zip with input hunks.
     out.sort_by_key(|r| r.hunk_index);
     Ok(out)
 }
 
 // ---------------------------------------------------------------------------
 // Provider selection (FR-12) and model resolution (FR-14)
 // ---------------------------------------------------------------------------
 
 /// The selectable providers. `--provider` accepts the lower-case names; `google`
 /// also accepts the alias `gemini` (its API key is `GEMINI_API_KEY`).
 #[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
 #[value(rename_all = "lower")]
 #[serde(rename_all = "lowercase")]
 pub enum ProviderId {
     Groq,
     #[value(alias = "gemini")]
     #[serde(alias = "gemini")]
     Google,
     Openai,
     Anthropic,
     Ollama,
+    #[value(alias = "google-vertex")]
+    #[serde(alias = "google-vertex")]
+    Vertex,
+}
+
+/// How a provider authenticates - the axis that used to be inferred from
+/// `key_env_var().is_none()` (CLO-537). `KeylessEndpoint` = Ollama (local URL),
+/// `KeylessAdc` = Vertex (gcloud ADC token), `ApiKey` = every key-bearing cloud provider.
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum AuthMethod {
+    ApiKey,
+    KeylessEndpoint,
+    KeylessAdc,
 }
 
 impl ProviderId {
     /// The provider's API key env var, or `None` for key-free Ollama. Centralizes
     /// the per-backend key mapping so config onboarding (CLO-496) and the
     /// backends agree on one source of truth.
     pub fn key_env_var(self) -> Option<&'static str> {
         match self {
             ProviderId::Groq => Some("GROQ_API_KEY"),
             ProviderId::Google => Some("GEMINI_API_KEY"),
             ProviderId::Openai => Some("OPENAI_API_KEY"),
             ProviderId::Anthropic => Some("ANTHROPIC_API_KEY"),
             ProviderId::Ollama => None,
+            ProviderId::Vertex => None,
         }
     }
 
     /// Default model id (ADR-001 Decisions 5/7 + capability matrix).
     pub(crate) fn default_model(self) -> &'static str {
         match self {
             ProviderId::Groq => "openai/gpt-oss-120b",
             ProviderId::Google => "gemini-3.1-flash-lite",
             ProviderId::Openai => "gpt-5.4-mini",
             ProviderId::Anthropic => "claude-haiku-4-5",
             // Local, user-pulled model (FR-56; owner default). `:cloud` variants
             // (e.g. deepseek-v4-flash:cloud) work via --model but are NOT zero-egress.
             ProviderId::Ollama => "gemma4:e4b-mlx",
+            ProviderId::Vertex => "gemini-3.1-flash-lite",
         }
     }
 
     /// Per-provider model env vars, in precedence order (primary first). Google
     /// reads both `GCM_GEMINI_MODEL` (primary, matches `GEMINI_API_KEY`) and the
     /// `GCM_GOOGLE_MODEL` alias (round-2 review pt 4).
     pub(crate) fn model_env_vars(self) -> &'static [&'static str] {
         match self {
             ProviderId::Groq => &["GCM_GROQ_MODEL"],
             ProviderId::Google => &["GCM_GEMINI_MODEL", "GCM_GOOGLE_MODEL"],
             ProviderId::Openai => &["GCM_OPENAI_MODEL"],
             ProviderId::Anthropic => &["GCM_ANTHROPIC_MODEL"],
             ProviderId::Ollama => &["GCM_OLLAMA_MODEL"],
+            ProviderId::Vertex => &["GCM_VERTEX_MODEL"],
         }
     }
 
     /// Parse a provider name (env), case- and whitespace-insensitive, honoring
     /// the `gemini` alias.
     pub(crate) fn parse(s: &str) -> Option<Self> {
         <ProviderId as ValueEnum>::from_str(s.trim(), true).ok()
     }
 
     /// Canonical lowercase token (the `--provider` / `GCM_PROVIDER` value, e.g.
     /// `groq`, `google`). Stable identifier used in `gcm status` output (CLO-515).
     pub(crate) fn as_str(self) -> &'static str {
         match self {
             ProviderId::Groq => "groq",
             ProviderId::Google => "google",
             ProviderId::Openai => "openai",
             ProviderId::Anthropic => "anthropic",
             ProviderId::Ollama => "ollama",
+            ProviderId::Vertex => "vertex",
+        }
+    }
+
+    /// How this provider authenticates. Replaces `key_env_var().is_none()` as the
+    /// "is-Ollama" proxy now that Vertex is a second keyless provider (CLO-537): call
+    /// sites branch on intent, and the exhaustive match flags the next backend added.
+    pub(crate) fn auth_method(self) -> AuthMethod {
+        match self {
+            ProviderId::Ollama => AuthMethod::KeylessEndpoint,
+            ProviderId::Vertex => AuthMethod::KeylessAdc,
+            _ => AuthMethod::ApiKey,
         }
     }
 }
 
 /// Resolve and construct the selected provider (FR-12/FR-14). Pure w.r.t. the API
 /// key (keys are read lazily inside `generate_*`), so the cache path and
 /// `--dry-run` resolve a provider without a key.
 pub fn select(
     cli_provider: Option<ProviderId>,
     cli_model: Option<&str>,
 ) -> Result<Box<dyn Provider>, ProviderError> {
     let id = resolve_provider_id(cli_provider)?;
     let model = resolve_model(id, cli_model);
     Ok(match id {
         ProviderId::Groq => Box::new(groq::Groq::new(model)),
         ProviderId::Google => Box::new(gemini::Gemini::new(model)),
         ProviderId::Openai => Box::new(openai::OpenAi::new(model)),
         ProviderId::Anthropic => Box::new(anthropic::Anthropic::new(model)),
         ProviderId::Ollama => {
             // Privacy defense-in-depth (FR-56/FR-48): a cloud-tagged model is proxied
             // off-machine by the local daemon, so warn that it is NOT zero-egress.
             if ollama::is_cloud_model(&model) {
                 eprintln!(
                     "note: Ollama model '{model}' routes through Ollama Cloud; the diff is NOT zero-egress."
                 );
             }
             Box::new(ollama::Ollama::new(model))
         }
+        ProviderId::Vertex => Box::new(vertex::Vertex::new(model)),
     })
 }
 
+/// Non-blocking Vertex ADC readiness probe for the `gcm provider` wizard (CLO-537).
+/// `Ok(())` if an access token can be acquired now, else a short human reason. Never
+/// on the hot path.
+pub(crate) fn vertex_adc_probe() -> Result<(), String> {
+    vertex::probe_adc()
+}
+
 fn resolve_provider_id(cli: Option<ProviderId>) -> Result<ProviderId, ProviderError> {
     let env = std::env::var("GCM_PROVIDER").ok();
     pick_provider_id(cli, env.as_deref())
 }
 
 /// Precedence flag > env > default(groq). An empty/whitespace `GCM_PROVIDER` is
 /// treated as unset (round-2 review pt 4); a non-empty unknown name is a fatal
 /// config error listing the valid names.
 pub(crate) fn pick_provider_id(
     cli: Option<ProviderId>,
     env_raw: Option<&str>,
 ) -> Result<ProviderId, ProviderError> {
     if let Some(id) = cli {
         return Ok(id);
     }
     match env_raw {
         None => Ok(ProviderId::Groq),
         Some(raw) => {
             let t = raw.trim();
             if t.is_empty() {
                 return Ok(ProviderId::Groq);
             }
             ProviderId::parse(t).ok_or_else(|| {
                 ProviderError::new(
                     "gcm",
                     ErrorKind::Config(format!(
-                        "unknown provider '{t}'. Set --provider/GCM_PROVIDER to one of: groq, google, openai, anthropic, ollama."
+                        "unknown provider '{t}'. Set --provider/GCM_PROVIDER to one of: groq, google, vertex, openai, anthropic, ollama."
                     )),
                 )
             })
         }
     }
 }
 
 /// Where a resolved model value came from (CLO-515 source attribution). `Env`
 /// carries the winning env-var name, so Google's `GCM_GEMINI_MODEL` >
 /// `GCM_GOOGLE_MODEL` precedence is reportable.
 #[derive(Debug, Clone, Copy, PartialEq, Eq)]
 pub enum ModelSource {
     Flag,
     Env(&'static str),
     Default,
 }
 
 fn resolve_model(id: ProviderId, cli: Option<&str>) -> String {
     resolve_model_with_source(id, cli, |v| std::env::var(v).ok()).0
 }
 
 /// Resolve the effective model **and** its source for a provider (CLO-515).
 /// Same precedence as [`resolve_model`] (flag > per-provider env in order >
 /// default), with empty/whitespace flag and env values skipped. `env_lookup` is
 /// injected so `gcm status` can attribute without touching process env directly
 /// (and unit tests stay hermetic).
 pub fn resolve_model_with_source(
     id: ProviderId,
     cli: Option<&str>,
     env_lookup: impl Fn(&str) -> Option<String>,
 ) -> (String, ModelSource) {
     if let Some(m) = cli.map(str::trim).filter(|m| !m.is_empty()) {
         return (m.to_string(), ModelSource::Flag);
     }
     for &var in id.model_env_vars() {
         if let Some(m) = env_lookup(var)
             .as_deref()
             .map(str::trim)
             .filter(|m| !m.is_empty())
         {
             return (m.to_string(), ModelSource::Env(var));
         }
     }
     (id.default_model().to_string(), ModelSource::Default)
 }
 
 // ---------------------------------------------------------------------------
 // Shared OpenAI-compatible chat helpers (Groq + OpenAI) and the universal
 // `<think>` backstop (all providers).
 // ---------------------------------------------------------------------------
 
 /// Single-commit-message system prompt (shared by every provider).
 pub(super) const SYSTEM_PROMPT: &str = "\
 Analyze this git diff and generate a concise, conventional commit message.
 Use format: <type>(<scope>): <description>
 Types: feat, fix, docs, style, refactor, test, chore
 Keep the first line under 72 characters.
 Add a blank line and bullet points for details if there are multiple significant changes.
 Do NOT include any explanation - output ONLY the commit message.";
 
 /// Grouping-plan system prompt (shared by every provider). The structured-output
 /// schema (`format`/`response_format`) is sent alongside this prompt, but some
 /// providers do not enforce it - notably Ollama cloud (`:cloud`/`-cloud`)
 /// passthrough models, where `format` is a no-op (CLO-517). So the prompt itself
 /// restates the exact JSON shape and gives an example; the schema and this prompt
 /// must be kept in sync (see `plan::schema`).
 pub(super) const GROUPING_SYSTEM_PROMPT: &str = "\
 Analyze these git changes. Group related files into logical commits by semantic relevance.
 
 Output ONLY a single JSON object, no prose or markdown fences. The shape is EXACTLY:
 {
   \"groups\": [
     { \"files\": [\"path/one.rs\", \"path/two.rs\"], \"summary\": \"one-line description\", \"commit_message\": \"feat(scope): full conventional commit\" },
     { \"files\": [\"path/three.rs\"], \"summary\": \"one-line description\", \"commit_message\": null }
   ]
 }
 
 The top-level key MUST be \"groups\" (an array). Do NOT use \"commits\". Each group object MUST
 have exactly these keys: \"files\" (array of exact path strings), \"summary\" (string), and
 \"commit_message\" (string for groups[0], null for every other group).
diff --git a/src/provider/models.rs b/src/provider/models.rs
index fc11deb..b682384 100644
--- a/src/provider/models.rs
+++ b/src/provider/models.rs
@@ -1,383 +1,397 @@
 //! Model-list discovery for the interactive `gcm provider` wizard (CLO-516).
 //!
 //! Best-effort: each provider's live model endpoint is queried (short timeout, one
 //! light retry via [`super::http::get_json`]); on *any* failure - no key, transport
 //! error, non-2xx, unparseable body, or an empty result - it degrades to a static
 //! per-provider fallback list so the wizard spinner always resolves to a usable set.
 //! The raw list is post-processed (D7): non-chat models filtered out, static
 //! baselines (including the provider's `default_model`) merged in, then deduped.
 //!
 //! Centralized here (rather than spread across the five backends) deliberately:
 //! discovery is fallback-safe, so a base-URL drift only costs a fallback, not a
 //! runtime error. The hot commit path stays in the backends, untouched.
 
 use serde_json::Value;
 
 use super::http::{self, HttpGet};
 use super::ProviderId;
 
 /// Where a model list came from, so the wizard can message accurately.
 pub enum FetchSource {
     Live,
     Fallback,
 }
 
 /// The outcome of a model-list fetch: the (filtered, merged, deduped) models, the
 /// source, and an optional non-fatal warning to surface in the wizard.
 pub struct ModelFetchOutcome {
     pub models: Vec<String>,
     pub source: FetchSource,
     pub warning: Option<String>,
 }
 
 /// Fetch the provider's available models for the wizard. Never errors: returns a
 /// usable list in every case (live + baselines, or the static fallback).
 /// `key` is the resolved API key (None for Ollama, or a cloud provider with none
 /// yet); `endpoint` is an explicit base URL (Ollama, from the wizard).
 pub fn fetch_supported_models(
     id: ProviderId,
     key: Option<&str>,
     endpoint: Option<&str>,
 ) -> ModelFetchOutcome {
     let key = key.map(str::trim).filter(|k| !k.is_empty());
 
+    // Vertex (CLO-537): keyless ADC, no live models endpoint in the MVP (design D4),
+    // so return the static Gemini set directly. This short-circuit also keeps the
+    // exhaustive `match id` arms below unreachable for Vertex at runtime.
+    if id == ProviderId::Vertex {
+        return ModelFetchOutcome {
+            models: static_fallback_models(id),
+            source: FetchSource::Fallback,
+            warning: None,
+        };
+    }
+
     // No-key short-circuit (D7.2): a key-bearing provider with no key can't fetch,
     // so skip the network call and show the built-in list with an explicit note.
     if let Some(var) = id.key_env_var() {
         if key.is_none() {
             return ModelFetchOutcome {
                 models: static_fallback_models(id),
                 source: FetchSource::Fallback,
                 warning: Some(format!(
                     "no {var} set - showing the built-in model list; provide the key for the live catalog"
                 )),
             };
         }
     }
 
     match fetch_live(id, key, endpoint) {
         Ok(raw) => {
             let live: Vec<String> = raw.into_iter().filter(|m| keep_chat_model(id, m)).collect();
             let live_count = live.len();
             // Merge static baselines (incl. default_model) so known-good models are
             // always selectable even if the live list omits them (D7.3).
             let mut models = live;
             models.extend(static_fallback_models(id));
             let models = dedupe(models);
             if live_count == 0 {
                 ModelFetchOutcome {
                     models,
                     source: FetchSource::Fallback,
                     warning: Some(format!(
                         "{} returned no usable models; using the built-in list",
                         id.as_str()
                     )),
                 }
             } else {
                 ModelFetchOutcome {
                     models,
                     source: FetchSource::Live,
                     warning: None,
                 }
             }
         }
         Err(e) => ModelFetchOutcome {
             models: static_fallback_models(id),
             source: FetchSource::Fallback,
             warning: Some(format!(
                 "could not fetch {} models ({e}); using the built-in list",
                 id.as_str()
             )),
         },
     }
 }
 
 /// Query the live model-list endpoint and parse it into raw ids (unfiltered).
 fn fetch_live(
     id: ProviderId,
     key: Option<&str>,
     endpoint: Option<&str>,
 ) -> Result<Vec<String>, super::ProviderError> {
     let base = resolved_base_url(id, endpoint);
     let base = base.trim_end_matches('/');
     let name = provider_name(id);
     let env_var = id.key_env_var().unwrap_or("");
     let req = match id {
         ProviderId::Groq | ProviderId::Openai => HttpGet {
             provider: name,
             auth_env_var: env_var,
             endpoint: format!("{base}/models"),
             auth: key.map(|k| ("Authorization", format!("Bearer {k}"))),
             extra_headers: Vec::new(),
         },
         ProviderId::Anthropic => HttpGet {
             provider: name,
             auth_env_var: env_var,
             endpoint: format!("{base}/v1/models?limit=1000"),
             auth: key.map(|k| ("x-api-key", k.to_string())),
             extra_headers: vec![("anthropic-version", "2023-06-01".to_string())],
         },
-        ProviderId::Google => HttpGet {
+        // Vertex is short-circuited in fetch_supported_models; this arm only
+        // satisfies exhaustiveness and never runs.
+        ProviderId::Google | ProviderId::Vertex => HttpGet {
             provider: name,
             auth_env_var: env_var,
             endpoint: format!("{base}/v1beta/models?pageSize=1000"),
             auth: key.map(|k| ("x-goog-api-key", k.to_string())),
             extra_headers: Vec::new(),
         },
         ProviderId::Ollama => HttpGet {
             provider: name,
             auth_env_var: env_var,
             endpoint: format!("{base}/api/tags"),
             auth: None,
             extra_headers: Vec::new(),
         },
     };
     let raw = http::get_json(&req)?;
     Ok(parse_models(id, &raw))
 }
 
 /// Resolve the model-list base URL: an explicit `endpoint` (Ollama, from the
 /// wizard) wins, else the provider's `GCM_*_BASE_URL` override, else its default.
 /// Mirrors the backends' base URLs (the runtime source of truth); a drift only
 /// costs a fallback since fetch is best-effort.
 fn resolved_base_url(id: ProviderId, endpoint: Option<&str>) -> String {
     resolved_base_url_with(id, endpoint, |v| std::env::var(v).ok())
 }
 
 /// Body of [`resolved_base_url`] with the env lookup injected (hermetic tests).
 /// Env var precedence per provider mirrors the backends. Google reads both
 /// `GCM_GEMINI_BASE_URL` (primary) and the `GCM_GOOGLE_BASE_URL` alias, matching
 /// `gemini.rs` - otherwise an alias-based setup fetches from the wrong endpoint.
 fn resolved_base_url_with(
     id: ProviderId,
     endpoint: Option<&str>,
     lookup: impl Fn(&str) -> Option<String>,
 ) -> String {
     if let Some(e) = endpoint.map(str::trim).filter(|e| !e.is_empty()) {
         return e.to_string();
     }
     let (env_vars, default): (&[&str], &str) = match id {
         ProviderId::Groq => (&["GCM_GROQ_BASE_URL"], "https://api.groq.com/openai/v1"),
         ProviderId::Openai => (&["GCM_OPENAI_BASE_URL"], "https://api.openai.com/v1"),
         ProviderId::Anthropic => (&["GCM_ANTHROPIC_BASE_URL"], "https://api.anthropic.com"),
-        ProviderId::Google => (
+        ProviderId::Google | ProviderId::Vertex => (
             &["GCM_GEMINI_BASE_URL", "GCM_GOOGLE_BASE_URL"],
             "https://generativelanguage.googleapis.com",
         ),
         ProviderId::Ollama => (&["GCM_OLLAMA_BASE_URL"], "http://localhost:11434"),
     };
     env_vars
         .iter()
         .find_map(|var| {
             lookup(var)
                 .map(|v| v.trim().to_string())
                 .filter(|v| !v.is_empty())
         })
         .unwrap_or_else(|| default.to_string())
 }
 
 /// Parse a provider's model-list body into raw ids (tolerant of extra fields;
 /// returns empty on any shape mismatch). Gemini is filtered to `generateContent`
 /// models here (the authoritative capability signal) and de-prefixed.
 fn parse_models(id: ProviderId, body: &str) -> Vec<String> {
     let Ok(v) = serde_json::from_str::<Value>(body) else {
         return Vec::new();
     };
     match id {
         // OpenAI-compatible: { "data": [ { "id": "..." }, ... ] }
         ProviderId::Groq | ProviderId::Openai | ProviderId::Anthropic => v
             .get("data")
             .and_then(Value::as_array)
             .map(|arr| {
                 arr.iter()
                     .filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from))
                     .collect()
             })
             .unwrap_or_default(),
         // Gemini models.list: { "models": [ { "name": "models/x", "supportedGenerationMethods": [...] } ] }
-        ProviderId::Google => v
+        ProviderId::Google | ProviderId::Vertex => v
             .get("models")
             .and_then(Value::as_array)
             .map(|arr| {
                 arr.iter()
                     .filter(|m| {
                         m.get("supportedGenerationMethods")
                             .and_then(Value::as_array)
                             .is_some_and(|a| {
                                 a.iter().any(|x| x.as_str() == Some("generateContent"))
                             })
                     })
                     .filter_map(|m| {
                         m.get("name")
                             .and_then(Value::as_str)
                             .map(|n| n.strip_prefix("models/").unwrap_or(n).to_string())
                     })
                     .collect()
             })
             .unwrap_or_default(),
         // Ollama /api/tags: { "models": [ { "name": "llama3:latest" }, ... ] }
         ProviderId::Ollama => v
             .get("models")
             .and_then(Value::as_array)
             .map(|arr| {
                 arr.iter()
                     .filter_map(|m| m.get("name").and_then(Value::as_str).map(String::from))
                     .collect()
             })
             .unwrap_or_default(),
     }
 }
 
 /// Whether a model id is a chat/text-generation model gcm can use (D7.1). OpenAI
 /// and Groq `/models` return non-text families (whisper/tts/dall-e/embeddings);
 /// an exclude-list is safer than an include-list (new chat families aren't missed).
 /// Gemini is already filtered in [`parse_models`]; Anthropic/Ollama pass through.
 fn keep_chat_model(id: ProviderId, model: &str) -> bool {
     match id {
         ProviderId::Openai | ProviderId::Groq => {
             const EXCLUDE: &[&str] = &[
                 "whisper",
                 "tts",
                 "dall-e",
                 "dalle",
                 "embedding",
                 "embed",
                 "moderation",
                 "guard",
                 "babbage",
                 "davinci",
                 "audio",
                 "image",
                 "rerank",
             ];
             let m = model.to_ascii_lowercase();
             !EXCLUDE.iter().any(|bad| m.contains(bad))
         }
         _ => true,
     }
 }
 
 /// A static per-provider model catalog used when the live fetch is unavailable.
 /// Always includes the provider's `default_model` so the default is selectable
 /// offline. These are discovery hints, not the resolved model (no ADR violation).
 fn static_fallback_models(id: ProviderId) -> Vec<String> {
     let curated: &[&str] = match id {
         ProviderId::Groq => &[
             "openai/gpt-oss-120b",
             "openai/gpt-oss-20b",
             "llama-3.3-70b-versatile",
         ],
         ProviderId::Openai => &["gpt-5.4-mini", "gpt-5.4", "gpt-4o-mini"],
         ProviderId::Anthropic => &["claude-haiku-4-5", "claude-sonnet-4-6", "claude-opus-4-8"],
-        ProviderId::Google => &[
+        ProviderId::Google | ProviderId::Vertex => &[
             "gemini-3.1-flash-lite",
             "gemini-3.1-flash",
             "gemini-3.1-pro",
         ],
         ProviderId::Ollama => &[],
     };
     let mut out: Vec<String> = curated.iter().map(|s| s.to_string()).collect();
     let default = id.default_model().to_string();
     if !out.contains(&default) {
         out.insert(0, default);
     }
     out
 }
 
 /// Stable de-duplication preserving first occurrence (live entries stay first).
 fn dedupe(models: Vec<String>) -> Vec<String> {
     let mut seen = std::collections::HashSet::new();
     models
         .into_iter()
         .filter(|m| seen.insert(m.clone()))
         .collect()
 }
 
 /// Display name for error messages (mirrors each backend's `NAME`).
 fn provider_name(id: ProviderId) -> &'static str {
     match id {
         ProviderId::Groq => "Groq",
         ProviderId::Google => "Google",
         ProviderId::Openai => "OpenAI",
         ProviderId::Anthropic => "Anthropic",
         ProviderId::Ollama => "Ollama",
+        ProviderId::Vertex => "Vertex",
     }
 }
 
 #[cfg(test)]
 mod tests {
     use super::*;
 
     #[test]
     fn parse_openai_compatible_data_ids() {
         let body = r#"{"object":"list","data":[{"id":"gpt-5.4-mini"},{"id":"whisper-1"},{"id":"text-embedding-3-small"}]}"#;
         let ids = parse_models(ProviderId::Openai, body);
         assert_eq!(
             ids,
             vec!["gpt-5.4-mini", "whisper-1", "text-embedding-3-small"]
         );
     }
 
     #[test]
     fn parse_gemini_filters_generate_content_and_strips_prefix() {
         let body = r#"{"models":[
             {"name":"models/gemini-3.1-flash-lite","supportedGenerationMethods":["generateContent","countTokens"]},
             {"name":"models/text-embedding-004","supportedGenerationMethods":["embedContent"]}
         ]}"#;
         let ids = parse_models(ProviderId::Google, body);
         assert_eq!(
             ids,
             vec!["gemini-3.1-flash-lite"],
             "only generateContent, de-prefixed"
         );
     }
 
     #[test]
     fn parse_ollama_tags_names() {
         let body = r#"{"models":[{"name":"llama3:latest"},{"name":"gemma4:e4b-mlx"}]}"#;
         let ids = parse_models(ProviderId::Ollama, body);
         assert_eq!(ids, vec!["llama3:latest", "gemma4:e4b-mlx"]);
     }
 
     #[test]
     fn parse_malformed_body_is_empty() {
         assert!(parse_models(ProviderId::Openai, "not json [").is_empty());
         assert!(parse_models(ProviderId::Openai, "{}").is_empty());
     }
 
     #[test]
     fn keep_chat_model_excludes_non_text_for_openai_groq() {
         for bad in [
             "whisper-large-v3",
             "tts-1",
             "dall-e-3",
             "text-embedding-3-small",
             "omni-moderation-latest",
         ] {
             assert!(!keep_chat_model(ProviderId::Openai, bad), "{bad} excluded");
         }
         for good in [
             "gpt-5.4-mini",
             "gpt-4o",
             "openai/gpt-oss-120b",
             "llama-3.3-70b-versatile",
         ] {
             assert!(keep_chat_model(ProviderId::Groq, good), "{good} kept");
         }
         // Anthropic/Ollama pass through (no exclude-list)
         assert!(keep_chat_model(ProviderId::Anthropic, "claude-haiku-4-5"));
         assert!(keep_chat_model(ProviderId::Ollama, "anything:latest"));
     }
 
     #[test]
     fn fallback_always_contains_default_model() {
         for id in [
             ProviderId::Groq,
             ProviderId::Google,
             ProviderId::Openai,
             ProviderId::Anthropic,
             ProviderId::Ollama,
         ] {
             let fb = static_fallback_models(id);
             assert!(
                 fb.iter().any(|m| m == id.default_model()),
diff --git a/src/provider/vertex.rs b/src/provider/vertex.rs
new file mode 100644
index 0000000..ae916ed
--- /dev/null
+++ b/src/provider/vertex.rs
@@ -0,0 +1,456 @@
+//! Google Vertex AI backend (CLO-537). A thin backend over the *identical* Gemini
+//! `generateContent` payloads (reused from [`super::gemini`]): only the endpoint URL
+//! and auth differ. Auth is **keyless** - a short-lived OAuth token from Application
+//! Default Credentials, acquired by shelling out to `gcloud` (matching gcm's optional
+//! external-binary pattern; `GCM_VERTEX_TOKEN` escape hatch first). Because it reuses
+//! `gemini::build_resolve_payload`, it inherits the CLO-534 OpenAPI-subset resolve
+//! schema (no `additionalProperties`) for free.
+//!
+//! The token is resolved **lazily** at call time (like `gemini::api_key`), so cache
+//! resolution and `--dry-run` never spawn gcloud.
+
+use std::io;
+use std::process::{Command, Stdio};
+use std::sync::mpsc;
+use std::thread;
+use std::time::Duration;
+
+use serde_json::Value;
+
+use super::gemini;
+use super::http::{self, HttpRequest};
+use super::{ErrorKind, Provider, ProviderError};
+use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
+use crate::plan::Plan;
+
+const NAME: &str = "Vertex";
+const TOKEN_ENV: &str = "GCM_VERTEX_TOKEN";
+const PROJECT_ENV: &str = "GCM_VERTEX_PROJECT";
+const LOCATION_ENV: &str = "GCM_VERTEX_LOCATION";
+const BASE_URL_ENV: &str = "GCM_VERTEX_BASE_URL";
+const DEFAULT_LOCATION: &str = "global";
+
+/// Bound the gcloud token subprocess. git is invoked without a timeout because it is
+/// local/instant, but a gcloud ADC refresh can block on the network (design §4.2 A3).
+const GCLOUD_TIMEOUT: Duration = Duration::from_secs(10);
+
+pub struct Vertex {
+    model: String,
+}
+
+impl Vertex {
+    pub fn new(model: String) -> Self {
+        Vertex { model }
+    }
+
+    /// Acquire the ADC access token: `GCM_VERTEX_TOKEN` (trimmed, non-empty) wins,
+    /// else shell out to gcloud. Resolved lazily per call.
+    fn access_token(&self) -> Result<String, ProviderError> {
+        if let Some(tok) = env_nonblank(TOKEN_ENV) {
+            return Ok(tok);
+        }
+        gcloud_token()
+    }
+
+    /// GCP project: `GCM_VERTEX_PROJECT` > `GOOGLE_CLOUD_PROJECT` > `GCP_PROJECT`.
+    /// Required (no default); validated before it is templated into the URL path.
+    fn project(&self) -> Result<String, ProviderError> {
+        let p =
+            first_env(&[PROJECT_ENV, "GOOGLE_CLOUD_PROJECT", "GCP_PROJECT"]).ok_or_else(|| {
+                config_err(
+                    "Vertex project not set. Set GCM_VERTEX_PROJECT (or GOOGLE_CLOUD_PROJECT), \
+                 or run `gcm provider` to configure it."
+                        .to_string(),
+                )
+            })?;
+        validate_project(&p)?;
+        Ok(p)
+    }
+
+    /// Vertex location: `GCM_VERTEX_LOCATION` > `GOOGLE_CLOUD_LOCATION` > `GCP_REGION`,
+    /// default `global` (Gemini 3.x is global-only on Vertex). Validated strictly
+    /// because it is templated into the endpoint host.
+    fn location(&self) -> Result<String, ProviderError> {
+        let loc = first_env(&[LOCATION_ENV, "GOOGLE_CLOUD_LOCATION", "GCP_REGION"])
+            .unwrap_or_else(|| DEFAULT_LOCATION.to_string());
+        validate_location(&loc)?;
+        Ok(loc)
+    }
+
+    /// Endpoint base: `GCM_VERTEX_BASE_URL` (test seam) wins; else the global host for
+    /// `global`, or the regional `{location}-aiplatform` host otherwise.
+    fn base_url(&self, location: &str) -> String {
+        if let Some(u) = env_nonblank(BASE_URL_ENV) {
+            return u.trim_end_matches('/').to_string();
+        }
+        if location == DEFAULT_LOCATION {
+            "https://aiplatform.googleapis.com".to_string()
+        } else {
+            format!("https://{location}-aiplatform.googleapis.com")
+        }
+    }
+
+    fn request<'a>(
+        &self,
+        token: &str,
+        project: &str,
+        location: &str,
+        payload: &'a Value,
+    ) -> HttpRequest<'a> {
+        let base = self.base_url(location);
+        HttpRequest {
+            provider: NAME,
+            // The token rides in `extra_headers` and `auth` is None, so a 401/403 is
+            // classified as `Http(status)` (not `Auth{env_var}`); `map_auth_error`
+            // then rewrites it with a gcloud-specific hint (design §4.2 N1). `""` is
+            // the no-auth placeholder convention.
+            auth_env_var: "",
+            endpoint: format!(
+                "{base}/v1/projects/{project}/locations/{location}/publishers/google/models/{}:generateContent",
+                self.model
+            ),
+            auth: None,
+            extra_headers: vec![("Authorization", format!("Bearer {token}"))],
+            payload,
+        }
+    }
+
+    /// Resolve token + project + location once for a request (the common prologue of
+    /// all three trait methods).
+    fn target(&self) -> Result<(String, String, String), ProviderError> {
+        Ok((self.access_token()?, self.project()?, self.location()?))
+    }
+
+    /// Re-map a raw HTTP auth failure to Vertex-specific, actionable text. A Bearer
+    /// 401/403 is meaningless as "check <env_var>"; 403 on Vertex usually means IAM
+    /// denied or the API is not enabled, not a bad credential.
+    fn map_auth_error(&self, e: ProviderError) -> ProviderError {
+        match e.kind {
+            ErrorKind::Http(401) => config_err(
+                "Vertex rejected the credential (HTTP 401): the ADC token is invalid or \
+                 expired. Run: gcloud auth application-default login"
+                    .to_string(),
+            ),
+            ErrorKind::Http(403) => {
+                let project = self
+                    .project()
+                    .map(|p| p.to_string())
+                    .unwrap_or_else(|_| "<project>".to_string());
+                config_err(format!(
+                    "Vertex denied access (HTTP 403) on project '{project}'. Check IAM \
+                     (grant roles/aiplatform.user), and that the Vertex AI API is enabled: \
+                     gcloud services enable aiplatform.googleapis.com --project {project}"
+                ))
+            }
+            _ => e,
+        }
+    }
+}
+
+impl Provider for Vertex {
+    fn name(&self) -> &'static str {
+        NAME
+    }
+
+    fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError> {
+        let (token, project, location) = self.target()?;
+        let payload = gemini::build_plan_payload(ctx);
+        let raw = http::post_json(&self.request(&token, &project, &location, &payload))
+            .map_err(|e| self.map_auth_error(e))?;
+        let json = gemini::extract_text(&raw)?;
+        if json.is_empty() {
+            return Err(empty());
+        }
+        crate::plan::parse_defensive(&json).map_err(|e| ProviderError {
+            provider: NAME,
+            kind: ErrorKind::Deserialize(e.to_string()),
+        })
+    }
+
+    fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError> {
+        let (token, project, location) = self.target()?;
+        let payload = gemini::build_message_payload(&super::message_user_content(diff));
+        let raw = http::post_json(&self.request(&token, &project, &location, &payload))
+            .map_err(|e| self.map_auth_error(e))?;
+        let message = gemini::extract_text(&raw)?;
+        if message.is_empty() {
+            return Err(empty());
+        }
+        Ok(message)
+    }
+
+    fn resolve_hunks(
+        &self,
+        ctx: &super::ResolveContext,
+    ) -> Result<Vec<super::Resolution>, ProviderError> {
+        let (token, project, location) = self.target()?;
+        let payload = gemini::build_resolve_payload(ctx);
+        let raw = http::post_json(&self.request(&token, &project, &location, &payload))
+            .map_err(|e| self.map_auth_error(e))?;
+        let json = gemini::extract_text(&raw)?;
+        if json.is_empty() {
+            return Err(empty());
+        }
+        super::parse_resolutions(NAME, &json, ctx.hunks.len())
+    }
+
+    fn cache_model_id(&self) -> String {
+        // Distinct from "google:{model}" so a cached plan from AI Studio never
+        // satisfies a Vertex run (different endpoint + enterprise terms).
+        format!("vertex:{}", self.model)
+    }
+
+    fn diff_budget(&self) -> DiffBudget {
+        DiffBudget::standard()
+    }
+}
+
+fn empty() -> ProviderError {
+    ProviderError {
+        provider: NAME,
+        kind: ErrorKind::EmptyResponse,
+    }
+}
+
+fn config_err(msg: String) -> ProviderError {
+    ProviderError {
+        provider: NAME,
+        kind: ErrorKind::Config(msg),
+    }
+}
+
+/// A trimmed, non-blank env var value, or `None`.
+fn env_nonblank(var: &str) -> Option<String> {
+    std::env::var(var)
+        .ok()
+        .map(|s| s.trim().to_string())
+        .filter(|s| !s.is_empty())
+}
+
+/// The first non-blank value among `vars`, in order.
+fn first_env(vars: &[&str]) -> Option<String> {
+    vars.iter().find_map(|v| env_nonblank(v))
+}
+
+/// Validate a Vertex location (templated into the endpoint HOST, so strict): either
+/// `global` or a region like `us-central1` - a lowercase letter start, then
+/// lowercase-alphanumeric or `-`. Rejects anything that could malform the host.
+fn validate_location(loc: &str) -> Result<(), ProviderError> {
+    let ok = loc == DEFAULT_LOCATION
+        || (loc.chars().next().is_some_and(|c| c.is_ascii_lowercase())
+            && loc
+                .chars()
+                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
+    if ok {
+        Ok(())
+    } else {
+        Err(config_err(format!(
+            "invalid Vertex location '{loc}': expected 'global' or a region like 'us-central1'"
+        )))
+    }
+}
+
+/// Validate a GCP project id (templated into the URL PATH segment, so lenient - only
+/// reject characters that would break URL structure). This deliberately accepts legacy
+/// domain-scoped ids like `example.com:my-project` (which contain `.` and `:`).
+fn validate_project(project: &str) -> Result<(), ProviderError> {
+    let bad = project.is_empty()
+        || project
+            .chars()
+            .any(|c| c == '/' || c == '?' || c == '#' || c.is_whitespace() || c.is_control());
+    if bad {
+        Err(config_err(format!(
+            "invalid Vertex project '{project}': contains characters not allowed in a GCP project id"
+        )))
+    } else {
+        Ok(())
+    }
+}
+
+/// Shell out to `gcloud auth application-default print-access-token` under a bounded
+/// timeout. Distinguishes "gcloud not installed" (io::ErrorKind::NotFound) from
+/// "installed but ADC not initialized" so the two hints are correct (design §4.2 P8).
+fn gcloud_token() -> Result<String, ProviderError> {
+    let mut cmd = Command::new("gcloud");
+    cmd.args(["auth", "application-default", "print-access-token"])
+        .stdout(Stdio::piped())
+        .stderr(Stdio::piped());
+
+    let child = match cmd.spawn() {
+        Ok(c) => c,
+        Err(e) if e.kind() == io::ErrorKind::NotFound => {
+            return Err(config_err(
+                "gcloud not found: install the Google Cloud SDK (https://cloud.google.com/sdk), \
+                 or set GCM_VERTEX_TOKEN with a valid access token."
+                    .to_string(),
+            ));
+        }
+        Err(e) => return Err(config_err(format!("failed to run gcloud: {e}"))),
+    };
+
+    // Bounded wait: a background thread drains stdout/stderr (avoids the pipe-fill
+    // deadlock) and sends the output; the main thread gives up after GCLOUD_TIMEOUT.
+    let (tx, rx) = mpsc::channel();
+    let pid = child.id();
+    thread::spawn(move || {
+        let _ = tx.send(child.wait_with_output());
+    });
+
+    match rx.recv_timeout(GCLOUD_TIMEOUT) {
+        Ok(Ok(out)) if out.status.success() => {
+            let tok = String::from_utf8_lossy(&out.stdout).trim().to_string();
+            if tok.is_empty() {
+                Err(config_err(
+                    "gcloud returned an empty access token. Run: gcloud auth application-default login"
+                        .to_string(),
+                ))
+            } else {
+                Ok(tok)
+            }
+        }
+        Ok(Ok(out)) => {
+            let stderr = String::from_utf8_lossy(&out.stderr);
+            Err(config_err(reauth_hint(&stderr)))
+        }
+        Ok(Err(e)) => Err(config_err(format!("failed to wait on gcloud: {e}"))),
+        Err(_) => {
+            // Best-effort reap of the timed-out child.
+            let _ = Command::new("kill").arg(pid.to_string()).status();
+            Err(config_err(format!(
+                "gcloud token request timed out after {GCLOUD_TIMEOUT:?}. Check your network, \
+                 or run: gcloud auth application-default login"
+            )))
+        }
+    }
+}
+
+/// Turn gcloud stderr into an actionable message, surfacing a reauth hint when the
+/// failure looks like an expired/invalid grant.
+fn reauth_hint(stderr: &str) -> String {
+    let s = stderr.trim();
+    let lower = s.to_lowercase();
+    if lower.contains("invalid_grant")
+        || lower.contains("reauth")
+        || lower.contains("could not automatically determine credentials")
+        || lower.contains("application default credentials")
+    {
+        return format!(
+            "gcloud could not provide an access token (run: gcloud auth application-default login). Details: {s}"
+        );
+    }
+    format!("gcloud failed to print an access token: {s}")
+}
+
+/// Wizard readiness probe (CLO-537): try to acquire an ADC token now, bounded by the
+/// same timeout as the hot path. Returns `Ok(())` or a short human-readable reason.
+/// Not used on the commit path (the wizard calls it once, non-blocking).
+pub(super) fn probe_adc() -> Result<(), String> {
+    Vertex::new(String::new())
+        .access_token()
+        .map(|_| ())
+        .map_err(|e| e.to_string())
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn request_url_global_uses_bare_aiplatform_host() {
+        let v = Vertex::new("gemini-3.1-flash-lite".to_string());
+        let payload = serde_json::json!({});
+        let req = v.request("tok", "my-proj", "global", &payload);
+        assert_eq!(
+            req.endpoint,
+            "https://aiplatform.googleapis.com/v1/projects/my-proj/locations/global/publishers/google/models/gemini-3.1-flash-lite:generateContent"
+        );
+    }
+
+    #[test]
+    fn request_url_regional_uses_prefixed_host() {
+        let v = Vertex::new("gemini-3.1-flash-lite".to_string());
+        let payload = serde_json::json!({});
+        let req = v.request("tok", "my-proj", "us-central1", &payload);
+        assert!(req
+            .endpoint
+            .starts_with("https://us-central1-aiplatform.googleapis.com/v1/projects/my-proj/locations/us-central1/"));
+    }
+
+    #[test]
+    fn request_sends_bearer_via_extra_headers_and_no_auth() {
+        // auth: None keeps classify_status from emitting Auth{env_var}; the token is
+        // still sent (extra_headers), and map_auth_error handles 401/403.
+        let v = Vertex::new("m".to_string());
+        let payload = serde_json::json!({});
+        let req = v.request("secret-token", "p", "global", &payload);
+        assert!(req.auth.is_none());
+        assert_eq!(
+            req.extra_headers,
+            vec![("Authorization", "Bearer secret-token".to_string())]
+        );
+    }
+
+    #[test]
+    fn cache_model_id_is_vertex_qualified() {
+        let v = Vertex::new("gemini-3.1-flash-lite".to_string());
+        assert_eq!(v.cache_model_id(), "vertex:gemini-3.1-flash-lite");
+    }
+
+    #[test]
+    fn base_url_test_seam_overrides_host() {
+        let v = Vertex::new("m".to_string());
+        // With no env override, global -> bare host.
+        assert_eq!(v.base_url("global"), "https://aiplatform.googleapis.com");
+    }
+
+    #[test]
+    fn validate_location_accepts_global_and_regions() {
+        assert!(validate_location("global").is_ok());
+        assert!(validate_location("us-central1").is_ok());
+        assert!(validate_location("europe-west4").is_ok());
+    }
+
+    #[test]
+    fn validate_location_rejects_malformed() {
+        assert!(validate_location("US-CENTRAL1").is_err()); // uppercase
+        assert!(validate_location("us central1").is_err()); // space
+        assert!(validate_location("../evil").is_err());
+        assert!(validate_location("-leading").is_err()); // must start with a letter
+    }
+
+    #[test]
+    fn validate_project_accepts_domain_scoped() {
+        // Legacy domain-scoped ids must be accepted (round-2 review P2).
+        assert!(validate_project("my-project-123").is_ok());
+        assert!(validate_project("example.com:my-project").is_ok());
+    }
+
+    #[test]
+    fn validate_project_rejects_url_structural_chars() {
+        assert!(validate_project("").is_err());
+        assert!(validate_project("a/b").is_err());
+        assert!(validate_project("a b").is_err());
+        assert!(validate_project("a?b").is_err());
+    }
+
+    #[test]
+    fn map_auth_error_rewrites_401_403_to_actionable_text() {
+        let v = Vertex::new("m".to_string());
+        let e401 = v.map_auth_error(ProviderError {
+            provider: NAME,
+            kind: ErrorKind::Http(401),
+        });
+        assert!(e401.to_string().contains("application-default login"));
+        let e403 = v.map_auth_error(ProviderError {
+            provider: NAME,
+            kind: ErrorKind::Http(403),
+        });
+        assert!(e403.to_string().contains("aiplatform.googleapis.com"));
+        // Non-auth errors pass through unchanged.
+        let other = v.map_auth_error(ProviderError {
+            provider: NAME,
+            kind: ErrorKind::Http(500),
+        });
+        assert!(matches!(other.kind, ErrorKind::Http(500)));
+    }
+}
diff --git a/src/status.rs b/src/status.rs
index dfae0b1..626c59f 100644
--- a/src/status.rs
+++ b/src/status.rs
@@ -1,362 +1,471 @@
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
-use crate::provider::{ollama, resolve_model_with_source, ModelSource, ProviderId};
+use crate::provider::{ollama, resolve_model_with_source, AuthMethod, ModelSource, ProviderId};
 
-/// Canonical provider order for output (matches the wizard's `cloud_then_ollama`).
-const PROVIDER_ORDER: [ProviderId; 5] = [
+/// Canonical provider order for output (matches the wizard's `all_providers`).
+const PROVIDER_ORDER: [ProviderId; 6] = [
     ProviderId::Groq,
     ProviderId::Google,
+    ProviderId::Vertex,
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
     /// Resolved config directory (parent of the file), or `None` if no OS config
     /// dir is available.
     #[serde(skip_serializing_if = "Option::is_none")]
     pub config_dir: Option<PathBuf>,
     /// Resolved `config.toml` path, or `None` if no OS config dir is available.
     #[serde(skip_serializing_if = "Option::is_none")]
     pub config_file_path: Option<PathBuf>,
     /// Whether the config file exists on disk.
     pub config_file_exists: bool,
     /// Whether the config file was parsed and is usable (`config::load` returned
     /// `Some`). `false` while `config_file_exists` is `true` means the file is
     /// present but malformed / wrong-version / insecure (a stderr warning was
     /// emitted); the report falls back to env-derived state.
     pub config_file_loaded: bool,
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
     /// For Ollama only: false when the model routes off-machine (a cloud-tagged
     /// `:cloud` / `-cloud` model); see [`ollama::is_cloud_model`].
     #[serde(skip_serializing_if = "Option::is_none")]
     pub zero_egress: Option<bool>,
+    /// Vertex only (CLO-537): GCP project as `value (source)`; `None` otherwise.
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub project: Option<String>,
+    /// Vertex only: location as `value (source)` (default `global`); `None` otherwise.
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub location: Option<String>,
+    /// Vertex only: inferred auth source (`GCM_VERTEX_TOKEN` or `gcloud ADC`). Never a
+    /// secret, and never verified by a gcloud call (status stays no-subprocess).
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub auth_source: Option<String>,
 }
 
 /// Entry point for the `status` subcommand. Pure introspection: loads the config
 /// and reads the environment, builds the report, prints it (JSON or human), and
 /// returns exit code 0 (misconfiguration is reported as fields, not a failure).
 /// A non-zero exit is reserved for a catastrophic internal error - per AC-9, a
 /// JSON serialization failure (infallible for these owned types in practice) is
 /// the one such case. Dispatched at the top of `run()` before any repo/provider/
 /// LLM work.
 pub fn run_status_subcommand(args: &Cli) -> i32 {
     let config = config::load();
     let report = build_report(
         args.provider,
         args.model.as_deref(),
         config.as_ref(),
         |var| std::env::var(var).ok(),
     );
 
     if args.json {
         match serde_json::to_string(&report) {
             Ok(json) => println!("{json}"),
             Err(e) => {
                 // AC-9: catastrophic internal error -> stderr + non-zero exit.
                 eprintln!("gcm: error: could not serialize status report: {e}");
                 return 1;
             }
         }
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
     let paths = paths_status(&env_lookup, config::config_path(), config.is_some());
     let (selected, provider_error) = selected_provider(cli_provider, config, &env_lookup);
 
     let providers = PROVIDER_ORDER
         .iter()
         .map(|&id| {
             let is_selected = id == selected;
             // The --model flag applies ONLY to the selected provider; others
             // resolve from env/default so they aren't mislabeled `flag`.
             let model_flag = if is_selected { cli_model } else { None };
             let (model, msrc) = resolve_model_with_source(id, model_flag, &env_lookup);
             // If neither a flag nor an env var produced the model, a model set in
             // the config file is what the next run will actually use (bridged via
             // apply_to_env), so attribute it to the config rather than `default`.
             let (model, model_source) = match msrc {
                 ModelSource::Default => match config_model(config, id) {
                     Some(m) => (m, "config file".to_string()),
                     None => (model, model_source_label(msrc)),
                 },
                 _ => (model, model_source_label(msrc)),
             };
 
-            let (key_source, endpoint, endpoint_source, zero_egress) = if id == ProviderId::Ollama {
-                let (ep, src) = ollama_endpoint(config, &env_lookup);
-                let zero = Some(!ollama::is_cloud_model(&model));
-                (None, Some(ep), Some(src), zero)
-            } else {
-                (Some(key_source(id, config, &env_lookup)), None, None, None)
+            let (
+                key_source,
+                endpoint,
+                endpoint_source,
+                zero_egress,
+                project,
+                location,
+                auth_source,
+            ) = match id.auth_method() {
+                AuthMethod::KeylessEndpoint => {
+                    let (ep, src) = ollama_endpoint(config, &env_lookup);
+                    let zero = Some(!ollama::is_cloud_model(&model));
+                    (None, Some(ep), Some(src), zero, None, None, None)
+                }
+                AuthMethod::KeylessAdc => (
+                    None,
+                    None,
+                    None,
+                    None,
+                    vertex_project(config, &env_lookup),
+                    Some(vertex_location(config, &env_lookup)),
+                    Some(vertex_auth_source(&env_lookup)),
+                ),
+                AuthMethod::ApiKey => (
+                    Some(key_source(id, config, &env_lookup)),
+                    None,
+                    None,
+                    None,
+                    None,
+                    None,
+                    None,
+                ),
             };
 
             ProviderStatus {
                 name: id,
                 selected: is_selected,
                 activated: is_activated(id, config, &env_lookup),
                 key_source,
                 endpoint,
                 endpoint_source,
                 model,
                 model_source,
                 zero_egress,
+                project,
+                location,
+                auth_source,
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
 
 /// Resolve the config dir source, dir, path, existence, and whether a present
 /// file actually loaded. Handles the no-config-dir case gracefully (all `None` /
 /// `false`). `config_path` is injected (the resolved `config::config_path()`) so
 /// the no-OS-config-dir branch is unit-testable; `config_loaded` is whether
 /// `config::load` returned `Some`, so a present-but-unusable file is
 /// distinguishable from an absent one.
 fn paths_status(
     env_lookup: &impl Fn(&str) -> Option<String>,
     config_path: Option<PathBuf>,
     config_loaded: bool,
 ) -> PathsStatus {
     let from_env = env_lookup("GCM_CONFIG")
         .map(|v| v.trim().to_string())
         .is_some_and(|v| !v.is_empty());
     let config_dir_source = if from_env {
         "env var GCM_CONFIG".to_string()
     } else {
         "default dir".to_string()
     };
     let path = config_path;
     let config_dir = path.as_ref().and_then(|p| p.parent().map(PathBuf::from));
     let config_file_exists = path.as_ref().is_some_and(|p| p.exists());
     PathsStatus {
         config_dir_source,
         config_dir,
         config_file_path: path,
         config_file_exists,
         config_file_loaded: config_loaded,
     }
 }
 
 /// The effective selected provider and an optional error note. Precedence
 /// `--provider` flag > `GCM_PROVIDER` env > `config.default` > built-in `Groq`.
 /// An unknown non-blank `GCM_PROVIDER` is reported (not fatal): at runtime it
 /// would be a fatal config error regardless of `config.default` (the env bridge
 /// never overwrites a set `GCM_PROVIDER`), so the display falls back to `Groq`,
 /// not `config.default`, to avoid implying a selection the runtime would reject.
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
             None => (
                 ProviderId::Groq,
                 Some(format!(
-                    "unknown provider '{raw}' in GCM_PROVIDER (valid: groq, google, openai, \
-                     anthropic, ollama); it would be a fatal error on a normal run - showing \
-                     groq as the display fallback"
+                    "unknown provider '{raw}' in GCM_PROVIDER (valid: groq, google, vertex, \
+                     openai, anthropic, ollama); it would be a fatal error on a normal run - \
+                     showing groq as the display fallback"
                 )),
             ),
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
+        // Vertex (keyless): activated when a project resolves from the env (a
+        // config-listed Vertex already returned true above).
+        ProviderId::Vertex => {
+            env_nonblank(env_lookup, "GCM_VERTEX_PROJECT")
+                || env_nonblank(env_lookup, "GOOGLE_CLOUD_PROJECT")
+                || env_nonblank(env_lookup, "GCP_PROJECT")
+        }
         _ => id
             .key_env_var()
             .is_some_and(|var| env_nonblank(env_lookup, var)),
     }
 }
 
+/// Vertex GCP project as a `value (source)` display string (CLO-537): env precedence
+/// `GCM_VERTEX_PROJECT` > `GOOGLE_CLOUD_PROJECT` > `GCP_PROJECT` > inline config;
+/// `None` when unset.
+fn vertex_project(
+    config: Option<&Config>,
+    env_lookup: &impl Fn(&str) -> Option<String>,
+) -> Option<String> {
+    for (var, label) in [
+        ("GCM_VERTEX_PROJECT", "env var GCM_VERTEX_PROJECT"),
+        ("GOOGLE_CLOUD_PROJECT", "env var GOOGLE_CLOUD_PROJECT"),
+        ("GCP_PROJECT", "env var GCP_PROJECT"),
+    ] {
+        if let Some(v) = env_value(env_lookup, var) {
+            return Some(format!("{v} ({label})"));
+        }
+    }
+    config
+        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Vertex))
+        .and_then(|pc| pc.project.as_deref())
+        .map(str::trim)
+        .filter(|p| !p.is_empty())
+        .map(|p| format!("{p} (config file)"))
+}
+
+/// Vertex location as a `value (source)` display string; env precedence
+/// `GCM_VERTEX_LOCATION` > `GOOGLE_CLOUD_LOCATION` > `GCP_REGION` > inline config,
+/// else the effective default `global`.
+fn vertex_location(
+    config: Option<&Config>,
+    env_lookup: &impl Fn(&str) -> Option<String>,
+) -> String {
+    for (var, label) in [
+        ("GCM_VERTEX_LOCATION", "env var GCM_VERTEX_LOCATION"),
+        ("GOOGLE_CLOUD_LOCATION", "env var GOOGLE_CLOUD_LOCATION"),
+        ("GCP_REGION", "env var GCP_REGION"),
+    ] {
+        if let Some(v) = env_value(env_lookup, var) {
+            return format!("{v} ({label})");
+        }
+    }
+    if let Some(loc) = config
+        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Vertex))
+        .and_then(|pc| pc.location.as_deref())
+        .map(str::trim)
+        .filter(|l| !l.is_empty())
+    {
+        return format!("{loc} (config file)");
+    }
+    "global (default)".to_string()
+}
+
+/// Vertex auth source, inferred WITHOUT a gcloud call (status stays no-subprocess):
+/// `GCM_VERTEX_TOKEN` if that env var is set, else `gcloud ADC`. Reflects which path
+/// would be used, not that a token was actually obtained. Never a secret.
+fn vertex_auth_source(env_lookup: &impl Fn(&str) -> Option<String>) -> String {
+    if env_nonblank(env_lookup, "GCM_VERTEX_TOKEN") {
+        "GCM_VERTEX_TOKEN".to_string()
+    } else {
+        "gcloud ADC".to_string()
+    }
+}
+
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
     // A blank inline key is treated as "not set" to match the runtime: `env_plan`
     // trims and filters empty inline keys before bridging them.
     let inline_nonblank = config
         .and_then(|c| c.providers.iter().find(|p| p.id == id))
         .and_then(|pc| pc.key.as_deref())
         .map(str::trim)
         .is_some_and(|k| !k.is_empty());
     if inline_nonblank {
         return "config file".to_string();
     }
     "not set".to_string()
 }
 
 /// A provider's non-blank `model` from the loaded config, if any. Blank/whitespace
 /// is treated as unset, matching the `env_plan` bridge that trims before applying.
 fn config_model(config: Option<&Config>, id: ProviderId) -> Option<String> {
     config
         .and_then(|c| c.providers.iter().find(|p| p.id == id))
         .and_then(|pc| pc.model.as_deref())
         .map(str::trim)
         .filter(|m| !m.is_empty())
         .map(String::from)
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
@@ -402,188 +511,199 @@ fn print_human(report: &StatusReport) {
     } else if let Some(p) = report.providers.iter().find(|p| p.selected) {
         let tag = locality_tag(p)
             .map(|t| format!(" [{t}]"))
             .unwrap_or_default();
         // Truthful runtime caveat. Only a cloud provider missing its key
         // necessarily errors; Ollama is key-free and falls back to the local
         // daemon, so an unconfigured Ollama selection can still run.
         let note = match (p.activated, p.endpoint.as_deref()) {
             (true, _) => String::new(),
             (false, Some(ep)) => {
                 format!(" (not configured - will try the local Ollama daemon at {ep})")
             }
             (false, None) => {
                 " (NOT activated - no API key; gcm would error on a real run)".to_string()
             }
         };
         println!(
             "  {} -> {} ({}){tag}{note}",
             p.name.as_str(),
             p.model,
             p.model_source
         );
     } else {
         println!("  (none)");
     }
 
     // GCM_PROVIDER parse failures stay between the headline and the sections.
     if let Some(err) = &report.provider_error {
         println!("\nWarning: {err}");
     }
 
     print_provider_section(report, "Activated:", true);
     print_provider_section(report, "Not activated:", false);
 }
 
 /// The neutral `cloud` / `local` descriptor for an Ollama model, or `None` for a
 /// cloud provider (the distinction is only meaningful for Ollama). Driven by the
 /// already-computed `zero_egress` field; never recomputed here.
 fn locality_tag(p: &ProviderStatus) -> Option<&'static str> {
     match p.zero_egress {
         Some(true) => Some("local"),
         Some(false) => Some("cloud"),
         None => None,
     }
 }
 
 /// Print one provider section. Members are those matching `activated`; the
 /// selected provider leads (marked `>`), the rest keep canonical order - a stable
 /// sort on `!selected` floats the selection without disturbing the others. An
 /// empty section prints `(none)`.
 fn print_provider_section(report: &StatusReport, heading: &str, activated: bool) {
     println!("\n{heading}");
     let mut members: Vec<&ProviderStatus> = report
         .providers
         .iter()
         .filter(|p| p.activated == activated)
         .collect();
     members.sort_by_key(|p| !p.selected);
     if members.is_empty() {
         println!("  (none)");
         return;
     }
     for p in members {
         print_provider_block(p);
     }
 }
 
 /// Print a single de-noised provider block: the name line (marked `>` when
 /// selected, two spaces otherwise) and the same indented detail lines as before.
 /// Ollama appends a `[cloud]` / `[local]` tag to its model line.
 fn print_provider_block(p: &ProviderStatus) {
     let marker = if p.selected { ">" } else { " " };
     println!("{marker} {}", p.name.as_str());
     if let Some(ks) = &p.key_source {
         println!("    key:   {ks}");
     }
     if let Some(ep) = &p.endpoint {
         let src = p.endpoint_source.as_deref().unwrap_or("unknown");
         println!("    endpoint: {ep} ({src})");
     }
+    if let Some(proj) = &p.project {
+        println!("    project: {proj}");
+    }
+    if let Some(loc) = &p.location {
+        println!("    location: {loc}");
+    }
+    if let Some(auth) = &p.auth_source {
+        println!("    auth:  {auth}");
+    }
     match locality_tag(p) {
         // `model:` padded to align its value under the Ollama `endpoint:` line.
         Some(tag) => println!("    model:    {} ({}) [{tag}]", p.model, p.model_source),
         None => println!("    model: {} ({})", p.model, p.model_source),
     }
 }
 
 #[cfg(test)]
 mod tests {
     use super::*;
     use crate::config::{ConflictConfig, ProviderConfig};
 
     fn cfg(default: ProviderId, providers: Vec<ProviderConfig>) -> Config {
         Config {
             version: 1,
             default,
             providers,
             conflict: ConflictConfig::default(),
         }
     }
 
     fn pc(id: ProviderId, key: Option<&str>, endpoint: Option<&str>) -> ProviderConfig {
         ProviderConfig {
             id,
             key: key.map(String::from),
             endpoint: endpoint.map(String::from),
             model: None,
             models: Vec::new(),
+            project: None,
+            location: None,
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
     fn key_source_blank_inline_key_is_not_set() {
         // a blank inline key in config is treated as "not set" (matches env_plan)
         let c = cfg(
             ProviderId::Groq,
             vec![pc(ProviderId::Groq, Some("   "), None)],
         );
         assert_eq!(key_source(ProviderId::Groq, Some(&c), &env(&[])), "not set");
     }
 
     #[test]
     fn paths_status_handles_no_config_dir() {
         // AC-10: config_path() == None (no OS config dir) reported gracefully.
         let p = paths_status(&env(&[]), None, false);
         assert!(p.config_dir.is_none());
         assert!(p.config_file_path.is_none());
         assert!(!p.config_file_exists);
         assert!(!p.config_file_loaded);
         assert_eq!(p.config_dir_source, "default dir");
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
@@ -622,215 +742,265 @@ mod tests {
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
     fn selected_provider_invalid_env_falls_back_to_groq_even_with_config_default() {
         // Even with config.default = openai, an invalid GCM_PROVIDER displays Groq
         // (runtime would fatally error, not silently use config.default).
         let c = cfg(ProviderId::Openai, vec![pc(ProviderId::Openai, None, None)]);
         let (id, err) = selected_provider(None, Some(&c), &env(&[("GCM_PROVIDER", "bogus")]));
         assert_eq!(id, ProviderId::Groq);
         assert!(err.expect("reported").contains("bogus"));
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
-        assert_eq!(names, ["groq", "google", "openai", "anthropic", "ollama"]);
+        assert_eq!(
+            names,
+            ["groq", "google", "vertex", "openai", "anthropic", "ollama"]
+        );
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
 
+    #[test]
+    fn vertex_status_shows_project_location_auth_and_no_key() {
+        // Vertex reports project/location/auth-source and NO key row (N2/P4).
+        let report = build_report(
+            Some(ProviderId::Vertex),
+            None,
+            None,
+            env(&[
+                ("GCM_VERTEX_PROJECT", "my-proj"),
+                ("GCM_VERTEX_LOCATION", "us-central1"),
+            ]),
+        );
+        let v = report
+            .providers
+            .iter()
+            .find(|p| p.name == ProviderId::Vertex)
+            .unwrap();
+        assert!(v.selected);
+        assert!(v.key_source.is_none(), "no key row for Vertex: {v:?}");
+        assert!(v.project.as_deref().unwrap().contains("my-proj"), "{v:?}");
+        assert!(
+            v.location.as_deref().unwrap().contains("us-central1"),
+            "{v:?}"
+        );
+        assert_eq!(v.auth_source.as_deref(), Some("gcloud ADC"));
+
+        // GCM_VERTEX_TOKEN set -> auth source reflects the token env var; location
+        // falls back to the global default when unset.
+        let report2 = build_report(
+            Some(ProviderId::Vertex),
+            None,
+            None,
+            env(&[("GCM_VERTEX_TOKEN", "t"), ("GCM_VERTEX_PROJECT", "p")]),
+        );
+        let v2 = report2
+            .providers
+            .iter()
+            .find(|p| p.name == ProviderId::Vertex)
+            .unwrap();
+        assert_eq!(v2.auth_source.as_deref(), Some("GCM_VERTEX_TOKEN"));
+        assert!(v2.location.as_deref().unwrap().contains("global"), "{v2:?}");
+    }
+
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
     fn config_model_attributed_as_config_file() {
         // A model set in config (no flag, no env) is what the next run will use
         // (bridged via apply_to_env), so status attributes it to the config file.
         let config = cfg(
             ProviderId::Openai,
             vec![ProviderConfig {
                 id: ProviderId::Openai,
                 key: None,
                 endpoint: None,
                 model: Some("gpt-config".to_string()),
                 models: Vec::new(),
+                project: None,
+                location: None,
             }],
         );
         let report = build_report(None, None, Some(&config), env(&[]));
         let openai = report
             .providers
             .iter()
             .find(|p| p.name == ProviderId::Openai)
             .unwrap();
         assert_eq!(openai.model, "gpt-config");
         assert_eq!(openai.model_source, "config file");
     }
 
     #[test]
     fn real_env_model_wins_over_config_model() {
         let config = cfg(
             ProviderId::Openai,
             vec![ProviderConfig {
                 id: ProviderId::Openai,
                 key: None,
                 endpoint: None,
                 model: Some("gpt-config".to_string()),
                 models: Vec::new(),
+                project: None,
+                location: None,
             }],
         );
         let report = build_report(
             None,
             None,
             Some(&config),
             env(&[("GCM_OPENAI_MODEL", "gpt-env")]),
         );
         let openai = report
             .providers
             .iter()
             .find(|p| p.name == ProviderId::Openai)
             .unwrap();
         assert_eq!(openai.model, "gpt-env");
         assert_eq!(openai.model_source, "env var GCM_OPENAI_MODEL");
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
         // -cloud model (Ollama Cloud naming) -> also off-machine, zero_egress false
         let report = build_report(
             None,
             None,
             None,
             env(&[("GCM_OLLAMA_MODEL", "nemotron-3-nano:30b-cloud")]),
         );
         let ollama = report
             .providers
             .iter()
             .find(|p| p.name == ProviderId::Ollama)
             .unwrap();
         assert_eq!(ollama.zero_egress, Some(false));
     }
 }
diff --git a/tests/status.rs b/tests/status.rs
index 308dfc4..f39824a 100644
--- a/tests/status.rs
+++ b/tests/status.rs
@@ -92,308 +92,308 @@ fn status_env_key_and_model_attribution() {
     assert!(out.status.success());
     let stdout = stdout_of(&out);
     assert!(stdout.contains("key:   env var GROQ_API_KEY"), "{stdout}");
     assert!(
         stdout.contains("model: m-x (env var GCM_GROQ_MODEL)"),
         "{stdout}"
     );
 }
 
 #[test]
 fn status_never_prints_raw_secret() {
     let cfg = tempfile::tempdir().unwrap();
     // human mode
     let out = run_status(cfg.path(), &["status"], &[("GROQ_API_KEY", "sk-secret123")]);
     assert!(
         !stdout_of(&out).contains("sk-secret123"),
         "human leaked secret"
     );
     // json mode
     let out = run_status(
         cfg.path(),
         &["status", "--json"],
         &[("GROQ_API_KEY", "sk-secret123")],
     );
     assert!(
         !stdout_of(&out).contains("sk-secret123"),
         "json leaked secret"
     );
 }
 
 #[test]
 fn status_mixed_inline_and_env_key_attribution() {
     let cfg = tempfile::tempdir().unwrap();
     // groq has an inline key; openai relies on the env var
     write_config(
         cfg.path(),
         "version = 1\n\
          default = \"groq\"\n\
          \n\
          [[providers]]\n\
          id = \"groq\"\n\
          key = \"sk-inline-xyz\"\n\
          \n\
          [[providers]]\n\
          id = \"openai\"\n",
     );
     let out = run_status(
         cfg.path(),
         &["status", "--json"],
         &[("OPENAI_API_KEY", "sk-env-openai")],
     );
     assert!(out.status.success());
     let stdout = stdout_of(&out);
     assert!(
         !stdout.contains("sk-inline-xyz"),
         "inline key leaked: {stdout}"
     );
     assert!(
         !stdout.contains("sk-env-openai"),
         "env key leaked: {stdout}"
     );
     let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
     let providers = json["providers"].as_array().unwrap();
     let groq = providers.iter().find(|p| p["name"] == "groq").unwrap();
     let openai = providers.iter().find(|p| p["name"] == "openai").unwrap();
     assert_eq!(groq["key_source"], "config file");
     assert_eq!(openai["key_source"], "env var OPENAI_API_KEY");
     // config.default = groq -> groq is the selected provider
     assert_eq!(groq["selected"], true);
 }
 
 #[test]
 fn status_json_valid_both_flag_positions() {
     let cfg = tempfile::tempdir().unwrap();
     for args in [["status", "--json"], ["--json", "status"]] {
         let out = run_status(cfg.path(), &args, &[]);
         assert!(out.status.success(), "exit 0 for {args:?}");
         let json: serde_json::Value = serde_json::from_slice(&out.stdout)
             .unwrap_or_else(|e| panic!("invalid JSON for {args:?}: {e}"));
         assert_eq!(json["v"], 1);
-        assert_eq!(json["providers"].as_array().unwrap().len(), 5);
+        assert_eq!(json["providers"].as_array().unwrap().len(), 6);
     }
 }
 
 #[test]
 fn status_ollama_endpoint_source() {
     let cfg = tempfile::tempdir().unwrap();
     let out = run_status(cfg.path(), &["status"], &[("OLLAMA_HOST", "remote:8080")]);
     assert!(out.status.success());
     let stdout = stdout_of(&out);
     assert!(
         stdout.contains("endpoint: http://remote:8080 (env var OLLAMA_HOST)"),
         "{stdout}"
     );
 }
 
 #[test]
 fn status_model_flag_scoped_to_selected_provider() {
     let cfg = tempfile::tempdir().unwrap();
     // --provider openai --model custom: only openai reports flag
     let out = run_status(
         cfg.path(),
         &["--provider", "openai", "--model", "custom-model", "status"],
         &[],
     );
     assert!(out.status.success());
     let stdout = stdout_of(&out);
     // headline names the selected provider; its block carries the `>` marker
     assert!(stdout.contains("Selected (gcm will use this):"), "{stdout}");
     assert!(stdout.contains("> openai"), "{stdout}");
     assert!(stdout.contains("model: custom-model (flag)"), "{stdout}");
     // de-noised: the old bracket tags are gone
     assert!(!stdout.contains("[selected"), "{stdout}");
     assert!(!stdout.contains("[activated]"), "{stdout}");
     // other providers keep their defaults
     assert!(stdout.contains("claude-haiku-4-5 (default)"), "{stdout}");
 }
 
 #[test]
 fn status_human_layout_groups_and_cloud_tag() {
     let cfg = tempfile::tempdir().unwrap();
     // ollama is config.default (selected) with a -cloud model; groq is activated via key.
     write_config(
         cfg.path(),
         "version = 1\n\
          default = \"ollama\"\n\
          \n\
          [[providers]]\n\
          id = \"ollama\"\n\
          model = \"nemotron-3-nano:30b-cloud\"\n",
     );
     let out = run_status(cfg.path(), &["status"], &[("GROQ_API_KEY", "sk-x")]);
     assert!(out.status.success());
     let stdout = stdout_of(&out);
     // headline: selected provider + model + neutral cloud tag (no warning framing)
     assert!(
         stdout.contains("ollama -> nemotron-3-nano:30b-cloud (config file) [cloud]"),
         "{stdout}"
     );
     assert!(!stdout.contains("not zero-egress"), "{stdout}");
     // both grouped sections present; selected ollama leads Activated, marked
     assert!(stdout.contains("Activated:"), "{stdout}");
     assert!(stdout.contains("Not activated:"), "{stdout}");
     assert!(stdout.contains("> ollama"), "{stdout}");
     // anthropic has no key -> Not activated section
     assert!(stdout.contains("anthropic"), "{stdout}");
 }
 
 #[test]
 fn status_invalid_gcm_provider_headline_does_not_claim_use() {
     // GCM_PROVIDER=bogus is fatal at runtime; groq is only a display fallback.
     // Even with GROQ_API_KEY set, the headline must NOT assert groq will be used.
     let cfg = tempfile::tempdir().unwrap();
     let out = run_status(
         cfg.path(),
         &["status"],
         &[("GCM_PROVIDER", "bogus"), ("GROQ_API_KEY", "sk-x")],
     );
     assert!(
         out.status.success(),
         "invalid provider is not fatal for status"
     );
     let stdout = stdout_of(&out);
     assert!(stdout.contains("Selected (gcm will use this):"), "{stdout}");
     // headline reports no usable selection and points to the warning...
     assert!(stdout.contains("(none -"), "{stdout}");
     assert!(stdout.contains("the next run would fail"), "{stdout}");
     // ...and never claims the groq fallback as what the next run uses
     // (the `->` arrow appears only in the headline, never in the section blocks).
     assert!(!stdout.contains("groq ->"), "{stdout}");
     // the explanatory warning still prints
     assert!(
         stdout.contains("Warning:") && stdout.contains("bogus"),
         "{stdout}"
     );
 }
 
 #[test]
 fn status_ollama_selected_unconfigured_does_not_claim_error() {
     // --provider ollama on a clean machine: not "activated" (no config entry, no
     // endpoint env), but Ollama is key-free and defaults to the local daemon, so a
     // real run can succeed. The headline must NOT claim it would error.
     let cfg = tempfile::tempdir().unwrap();
     let out = run_status(cfg.path(), &["--provider", "ollama", "status"], &[]);
     assert!(out.status.success());
     let stdout = stdout_of(&out);
     assert!(stdout.contains("Selected (gcm will use this):"), "{stdout}");
     // ollama is named as the selection...
     assert!(stdout.contains("ollama ->"), "{stdout}");
     // ...with the truthful local-daemon note, not an error claim
     assert!(
         stdout.contains("will try the local Ollama daemon"),
         "{stdout}"
     );
     assert!(!stdout.contains("would error on a real run"), "{stdout}");
 }
 
 #[test]
 fn status_invalid_gcm_provider_reported_exit_zero() {
     let cfg = tempfile::tempdir().unwrap();
     let out = run_status(
         cfg.path(),
         &["status", "--json"],
         &[("GCM_PROVIDER", "bogus")],
     );
     assert!(out.status.success(), "invalid provider is not fatal");
     let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
     let err = json["provider_error"].as_str().expect("provider_error set");
     assert!(err.contains("bogus"), "{err}");
     // falls back to groq as the displayed selection
     let groq = json["providers"]
         .as_array()
         .unwrap()
         .iter()
         .find(|p| p["name"] == "groq")
         .unwrap();
     assert_eq!(groq["selected"], true);
 }
 
 #[test]
 fn status_malformed_config_falls_back_to_env_state() {
     let cfg = tempfile::tempdir().unwrap();
     write_config(cfg.path(), "this is not = valid toml [");
     let out = run_status(cfg.path(), &["status", "--json"], &[]);
     assert!(out.status.success(), "malformed config is not fatal");
     let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
     assert_eq!(json["v"], 1);
-    assert_eq!(json["providers"].as_array().unwrap().len(), 5);
+    assert_eq!(json["providers"].as_array().unwrap().len(), 6);
     // machine-readable: the file exists but is not usable (distinguishable from absent)
     assert_eq!(json["paths"]["config_file_exists"], true);
     assert_eq!(json["paths"]["config_file_loaded"], false);
 }
 
 #[test]
 fn status_reports_config_dir_and_loaded_state() {
     // AC-2: resolved config dir is present; a good config loads.
     let cfg = tempfile::tempdir().unwrap();
     write_config(
         cfg.path(),
         "version = 1\ndefault = \"groq\"\n\n[[providers]]\nid = \"groq\"\n",
     );
     let out = run_status(cfg.path(), &["status", "--json"], &[]);
     assert!(out.status.success());
     let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
     assert_eq!(json["paths"]["config_dir"], cfg.path().to_str().unwrap());
     assert_eq!(json["paths"]["config_dir_source"], "env var GCM_CONFIG");
     assert_eq!(json["paths"]["config_file_loaded"], true);
 }
 
 #[test]
 fn status_help_lists_subcommand() {
     // AC-8: the subcommand is discoverable in help output.
     let cfg = tempfile::tempdir().unwrap();
     let out = run_status(cfg.path(), &["--help"], &[]);
     assert!(out.status.success());
     assert!(stdout_of(&out).contains("status"), "status in --help");
     // `gcm status --help` also works (subcommand help)
     let out = run_status(cfg.path(), &["status", "--help"], &[]);
     assert!(out.status.success());
 }
 
 #[test]
 fn status_invalid_gcm_provider_with_config_default_still_shows_groq() {
     // config.default = openai, but GCM_PROVIDER=bogus -> groq displayed + error.
     let cfg = tempfile::tempdir().unwrap();
     write_config(
         cfg.path(),
         "version = 1\ndefault = \"openai\"\n\n[[providers]]\nid = \"openai\"\n",
     );
     let out = run_status(
         cfg.path(),
         &["status", "--json"],
         &[("GCM_PROVIDER", "bogus")],
     );
     assert!(out.status.success());
     let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
     assert!(json["provider_error"].as_str().unwrap().contains("bogus"));
     let providers = json["providers"].as_array().unwrap();
     let groq = providers.iter().find(|p| p["name"] == "groq").unwrap();
     let openai = providers.iter().find(|p| p["name"] == "openai").unwrap();
     assert_eq!(groq["selected"], true, "groq is the display fallback");
     assert_eq!(
         openai["selected"], false,
         "config.default is NOT shown selected"
     );
 }
 
 #[test]
 fn status_config_default_drives_selection_without_env() {
     let cfg = tempfile::tempdir().unwrap();
     write_config(
         cfg.path(),
         "version = 1\n\
          default = \"openai\"\n\
          \n\
          [[providers]]\n\
          id = \"openai\"\n",
     );
     // no --provider, no GCM_PROVIDER -> config.default (openai) is selected
     let out = run_status(cfg.path(), &["status", "--json"], &[]);
     assert!(out.status.success());
     let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
     let openai = json["providers"]
         .as_array()
         .unwrap()
         .iter()
         .find(|p| p["name"] == "openai")
         .unwrap();
diff --git a/tests/vertex.rs b/tests/vertex.rs
new file mode 100644
index 0000000..7361780
--- /dev/null
+++ b/tests/vertex.rs
@@ -0,0 +1,297 @@
+//! End-to-end acceptance test for the Vertex AI provider (CLO-537).
+//!
+//! Drives the built `gcm` binary against a local mock `generateContent` server via
+//! `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN`, so the full Vertex HTTP path is
+//! exercised hermetically (no gcloud, no network): `request()` builds the Vertex URL,
+//! sends the Bearer token via `extra_headers` with `auth: None`, `post_json` performs
+//! the round-trip, and `gemini::extract_text` parses the Gemini-shaped response. The
+//! live variant (a real 200 against a GCP project) is the HITL step, out of scope here.
+
+use std::fs;
+use std::io::{Read, Write};
+use std::net::TcpListener;
+use std::path::Path;
+use std::process::{Command, Output, Stdio};
+use std::thread;
+
+/// Env vars scrubbed before each run so a developer's real Vertex/GCP config can't
+/// leak into the hermetic test.
+const SCRUB_ENV: &[&str] = &[
+    "GROQ_API_KEY",
+    "GEMINI_API_KEY",
+    "OPENAI_API_KEY",
+    "ANTHROPIC_API_KEY",
+    "GCM_PROVIDER",
+    "GCM_VERTEX_PROJECT",
+    "GCM_VERTEX_LOCATION",
+    "GCM_VERTEX_TOKEN",
+    "GCM_VERTEX_BASE_URL",
+    "GOOGLE_CLOUD_PROJECT",
+    "GCP_PROJECT",
+    "GOOGLE_CLOUD_LOCATION",
+    "GCP_REGION",
+];
+
+fn git_init(dir: &Path) {
+    for args in [
+        vec!["init", "-q"],
+        vec!["config", "user.email", "t@example.com"],
+        vec!["config", "user.name", "Test"],
+    ] {
+        assert!(
+            Command::new("git")
+                .args(&args)
+                .current_dir(dir)
+                .status()
+                .expect("run git")
+                .success(),
+            "git {args:?} failed"
+        );
+    }
+}
+
+fn write_config(dir: &Path, body: &str) {
+    let path = dir.join("config.toml");
+    fs::write(&path, body).unwrap();
+    #[cfg(unix)]
+    {
+        use std::os::unix::fs::PermissionsExt;
+        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
+    }
+}
+
+fn run_gcm(repo: &Path, config_dir: &Path, extra_env: &[(&str, &str)], args: &[&str]) -> Output {
+    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
+    cmd.current_dir(repo)
+        .args(args)
+        .env("GCM_CONFIG", config_dir)
+        .env_remove("GCM_DEBUG")
+        .stdin(Stdio::null());
+    for var in SCRUB_ENV {
+        cmd.env_remove(var);
+    }
+    for (k, v) in extra_env {
+        cmd.env(k, v);
+    }
+    cmd.output().expect("run gcm")
+}
+
+/// A Gemini-shaped `generateContent` response whose single text part carries the
+/// resolve JSON (`{"resolutions":[{hunk_index, replacement}]}`), matching what
+/// `gemini::extract_text` (reused by Vertex) expects.
+fn mock_vertex_resolve_response(replacement: &str) -> String {
+    let inner = serde_json::json!({
+        "resolutions": [{ "hunk_index": 0, "replacement": replacement }]
+    })
+    .to_string();
+    serde_json::json!({
+        "candidates": [{
+            "content": { "parts": [{ "text": inner }] },
+            "finishReason": "STOP"
+        }]
+    })
+    .to_string()
+}
+
+/// Serve `body` once on a random port, capturing the request line/headers so the test
+/// can assert the Vertex URL + Bearer header were actually sent. Returns
+/// `(base_url, handle)`; `handle.join()` yields the captured request bytes.
+fn mock_server(body: String) -> (String, thread::JoinHandle<String>) {
+    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
+    let port = listener.local_addr().unwrap().port();
+    let handle = thread::spawn(move || {
+        listener.set_nonblocking(true).ok();
+        let start = std::time::Instant::now();
+        loop {
+            match listener.accept() {
+                Ok((mut stream, _)) => {
+                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
+                    let mut buf = [0u8; 8192];
+                    let n = stream.read(&mut buf).unwrap_or(0);
+                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
+                    let response = format!(
+                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
+                        body.len(),
+                        body
+                    );
+                    let _ = stream.write_all(response.as_bytes());
+                    return req;
+                }
+                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
+                    if start.elapsed() > std::time::Duration::from_secs(10) {
+                        return String::new();
+                    }
+                    thread::sleep(std::time::Duration::from_millis(50));
+                }
+                Err(_) => return String::new(),
+            }
+        }
+    });
+    (format!("http://127.0.0.1:{port}"), handle)
+}
+
+fn git(repo: &Path, args: &[&str]) {
+    assert!(
+        Command::new("git")
+            .args(args)
+            .current_dir(repo)
+            .output()
+            .expect("run git")
+            .status
+            .success(),
+        "git {args:?} failed"
+    );
+}
+
+/// A real 3-way conflict on `f.txt` (base/feature/mainline all differ), so the
+/// resolver classifies it Complex and calls the provider.
+fn create_conflict(repo: &Path) {
+    fs::write(repo.join("f.txt"), "base\n").unwrap();
+    git(repo, &["add", "-A"]);
+    git(repo, &["commit", "-q", "-m", "base"]);
+    let base = String::from_utf8_lossy(
+        &Command::new("git")
+            .args(["branch", "--show-current"])
+            .current_dir(repo)
+            .output()
+            .unwrap()
+            .stdout,
+    )
+    .trim()
+    .to_string();
+    git(repo, &["switch", "-q", "-c", "feature"]);
+    fs::write(repo.join("f.txt"), "feature\n").unwrap();
+    git(repo, &["commit", "-qam", "feature"]);
+    git(repo, &["switch", "-q", &base]);
+    fs::write(repo.join("f.txt"), "mainline\n").unwrap();
+    git(repo, &["commit", "-qam", "mainline"]);
+    // Expected to fail (leaves conflict markers) - that's the state resolve acts on.
+    let _ = Command::new("git")
+        .args(["merge", "feature"])
+        .current_dir(repo)
+        .output();
+}
+
+const VERTEX_CONFIG: &str = r#"version = 2
+default = "vertex"
+
+[[providers]]
+id = "vertex"
+project = "test-proj"
+"#;
+
+#[test]
+fn resolve_via_vertex_hits_mock_generatecontent_with_bearer_and_resolves() {
+    let dir = tempfile::tempdir().unwrap();
+    let repo = dir.path();
+    git_init(repo);
+    create_conflict(repo);
+
+    let (url, server) = mock_server(mock_vertex_resolve_response("RESOLVED_BY_VERTEX\n"));
+
+    let cfg_dir = tempfile::tempdir().unwrap();
+    write_config(cfg_dir.path(), VERTEX_CONFIG);
+
+    let out = run_gcm(
+        repo,
+        cfg_dir.path(),
+        &[
+            ("GCM_VERTEX_BASE_URL", &url),
+            ("GCM_VERTEX_TOKEN", "fake-adc-token"),
+        ],
+        &["resolve", "--yes", "--provider", "vertex"],
+    );
+    let request = server.join().unwrap();
+
+    assert!(
+        out.status.success(),
+        "gcm resolve --provider vertex failed.\nstdout: {}\nstderr: {}",
+        String::from_utf8_lossy(&out.stdout),
+        String::from_utf8_lossy(&out.stderr)
+    );
+
+    // The Vertex request actually reached the server with the right URL shape + auth.
+    assert!(
+        request.contains("/v1/projects/test-proj/locations/global/publishers/google/models/"),
+        "request line missing Vertex URL path: {request}"
+    );
+    assert!(
+        request.contains(":generateContent"),
+        "request missing :generateContent: {request}"
+    );
+    assert!(
+        request
+            .to_lowercase()
+            .contains("authorization: bearer fake-adc-token"),
+        "request missing Bearer token header: {request}"
+    );
+
+    // The provider's resolution was applied: markers gone, replacement present.
+    let after = fs::read_to_string(repo.join("f.txt")).unwrap();
+    assert!(
+        !after.contains("<<<<<<<") && !after.contains(">>>>>>>"),
+        "conflict markers should be gone after resolve: {after:?}"
+    );
+    assert!(
+        after.contains("RESOLVED_BY_VERTEX"),
+        "resolved content should come from the mock Vertex response: {after:?}"
+    );
+}
+
+/// Resolve a tool's absolute path from the current PATH (via `command -v`).
+#[cfg(unix)]
+fn tool_path(name: &str) -> String {
+    let out = Command::new("sh")
+        .args(["-c", &format!("command -v {name}")])
+        .output()
+        .expect("run sh");
+    String::from_utf8_lossy(&out.stdout).trim().to_string()
+}
+
+#[cfg(unix)]
+#[test]
+fn vertex_missing_gcloud_and_token_is_actionable_not_a_panic() {
+    // AC-3: no GCM_VERTEX_TOKEN and gcloud absent -> an actionable, typed error (no
+    // panic, no "check <env_var>"). Build a clean bin dir that has git + sh symlinked
+    // (so gcm's own git work still runs) but NOT gcloud, then point PATH there.
+    use std::os::unix::fs::symlink;
+
+    let git = tool_path("git");
+    let sh = tool_path("sh");
+    if git.is_empty() || sh.is_empty() {
+        return; // environment without a resolvable git/sh; skip (never on CI).
+    }
+
+    let dir = tempfile::tempdir().unwrap();
+    let repo = dir.path();
+    git_init(repo);
+    create_conflict(repo);
+
+    let bin = tempfile::tempdir().unwrap();
+    symlink(&git, bin.path().join("git")).unwrap();
+    symlink(&sh, bin.path().join("sh")).unwrap();
+
+    let cfg_dir = tempfile::tempdir().unwrap();
+    write_config(cfg_dir.path(), VERTEX_CONFIG);
+
+    let out = run_gcm(
+        repo,
+        cfg_dir.path(),
+        &[("PATH", &bin.path().to_string_lossy())],
+        &["resolve", "--yes", "--provider", "vertex"],
+    );
+    let combined = format!(
+        "{}{}",
+        String::from_utf8_lossy(&out.stdout),
+        String::from_utf8_lossy(&out.stderr)
+    );
+    assert!(
+        !out.status.success(),
+        "expected failure without a token: {combined}"
+    );
+    assert!(!combined.contains("panicked"), "must not panic: {combined}");
+    assert!(
+        combined.contains("gcloud") || combined.contains("GCM_VERTEX_TOKEN"),
+        "error should mention gcloud/ADC or the token env var: {combined}"
+    );
+}

codex
I have the design and plan. Now I’m checking the implemented seams that usually drift in this repo: provider selection, config-to-env bridging, status rendering, and the HTTP auth/error path for keyless providers.
exec
/bin/zsh -lc 'wc -l src/provider/vertex.rs src/provider/mod.rs src/provider/gemini.rs src/provider/models.rs src/config.rs src/status.rs src/cli.rs src/provider/http.rs tests/vertex.rs' in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
     456 src/provider/vertex.rs
     965 src/provider/mod.rs
     401 src/provider/gemini.rs
     456 src/provider/models.rs
    2408 src/config.rs
    1006 src/status.rs
     271 src/cli.rs
     518 src/provider/http.rs
     297 tests/vertex.rs
    6778 total

exec
/bin/zsh -lc "rg -n \"key_env_var\\(|auth_method\\(|Vertex|google-vertex|GCM_VERTEX|GOOGLE_CLOUD_PROJECT|GCP_PROJECT|GOOGLE_CLOUD_LOCATION|GCP_REGION|classify_status|auth_env_var|Http\\(|401|403|gcloud|print-access-token|timeout|command -v|all_providers|cloud_providers|PROVIDER_ORDER|selected_provider|provider_label|commented_reference|run_provider_wizard|run_wizard|env_plan|apply_to_env|fetch_supported_models|fetch_live|resolved_base_url_with|parse_models|static_fallback_models|provider_name\" src/provider/vertex.rs src/provider/mod.rs src/provider/gemini.rs src/provider/models.rs src/config.rs src/status.rs src/cli.rs src/provider/http.rs tests/vertex.rs tests/status.rs" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
tests/status.rs:189:fn status_model_flag_scoped_to_selected_provider() {
tests/vertex.rs:1://! End-to-end acceptance test for the Vertex AI provider (CLO-537).
tests/vertex.rs:4://! `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN`, so the full Vertex HTTP path is
tests/vertex.rs:5://! exercised hermetically (no gcloud, no network): `request()` builds the Vertex URL,
tests/vertex.rs:17:/// Env vars scrubbed before each run so a developer's real Vertex/GCP config can't
tests/vertex.rs:25:    "GCM_VERTEX_PROJECT",
tests/vertex.rs:26:    "GCM_VERTEX_LOCATION",
tests/vertex.rs:27:    "GCM_VERTEX_TOKEN",
tests/vertex.rs:28:    "GCM_VERTEX_BASE_URL",
tests/vertex.rs:29:    "GOOGLE_CLOUD_PROJECT",
tests/vertex.rs:30:    "GCP_PROJECT",
tests/vertex.rs:31:    "GOOGLE_CLOUD_LOCATION",
tests/vertex.rs:32:    "GCP_REGION",
tests/vertex.rs:81:/// `gemini::extract_text` (reused by Vertex) expects.
tests/vertex.rs:97:/// can assert the Vertex URL + Bearer header were actually sent. Returns
tests/vertex.rs:108:                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
tests/vertex.rs:199:            ("GCM_VERTEX_BASE_URL", &url),
tests/vertex.rs:200:            ("GCM_VERTEX_TOKEN", "fake-adc-token"),
tests/vertex.rs:213:    // The Vertex request actually reached the server with the right URL shape + auth.
tests/vertex.rs:216:        "request line missing Vertex URL path: {request}"
tests/vertex.rs:237:        "resolved content should come from the mock Vertex response: {after:?}"
tests/vertex.rs:241:/// Resolve a tool's absolute path from the current PATH (via `command -v`).
tests/vertex.rs:245:        .args(["-c", &format!("command -v {name}")])
tests/vertex.rs:253:fn vertex_missing_gcloud_and_token_is_actionable_not_a_panic() {
tests/vertex.rs:254:    // AC-3: no GCM_VERTEX_TOKEN and gcloud absent -> an actionable, typed error (no
tests/vertex.rs:256:    // (so gcm's own git work still runs) but NOT gcloud, then point PATH there.
tests/vertex.rs:294:        combined.contains("gcloud") || combined.contains("GCM_VERTEX_TOKEN"),
tests/vertex.rs:295:        "error should mention gcloud/ADC or the token env var: {combined}"
src/provider/models.rs:3://! Best-effort: each provider's live model endpoint is queried (short timeout, one
src/provider/models.rs:37:pub fn fetch_supported_models(
src/provider/models.rs:44:    // Vertex (CLO-537): keyless ADC, no live models endpoint in the MVP (design D4),
src/provider/models.rs:46:    // exhaustive `match id` arms below unreachable for Vertex at runtime.
src/provider/models.rs:47:    if id == ProviderId::Vertex {
src/provider/models.rs:49:            models: static_fallback_models(id),
src/provider/models.rs:57:    if let Some(var) = id.key_env_var() {
src/provider/models.rs:60:                models: static_fallback_models(id),
src/provider/models.rs:69:    match fetch_live(id, key, endpoint) {
src/provider/models.rs:76:            models.extend(static_fallback_models(id));
src/provider/models.rs:96:            models: static_fallback_models(id),
src/provider/models.rs:107:fn fetch_live(
src/provider/models.rs:114:    let name = provider_name(id);
src/provider/models.rs:115:    let env_var = id.key_env_var().unwrap_or("");
src/provider/models.rs:119:            auth_env_var: env_var,
src/provider/models.rs:126:            auth_env_var: env_var,
src/provider/models.rs:131:        // Vertex is short-circuited in fetch_supported_models; this arm only
src/provider/models.rs:133:        ProviderId::Google | ProviderId::Vertex => HttpGet {
src/provider/models.rs:135:            auth_env_var: env_var,
src/provider/models.rs:142:            auth_env_var: env_var,
src/provider/models.rs:149:    Ok(parse_models(id, &raw))
src/provider/models.rs:157:    resolved_base_url_with(id, endpoint, |v| std::env::var(v).ok())
src/provider/models.rs:164:fn resolved_base_url_with(
src/provider/models.rs:176:        ProviderId::Google | ProviderId::Vertex => (
src/provider/models.rs:195:fn parse_models(id: ProviderId, body: &str) -> Vec<String> {
src/provider/models.rs:211:        ProviderId::Google | ProviderId::Vertex => v
src/provider/models.rs:247:/// Gemini is already filtered in [`parse_models`]; Anthropic/Ollama pass through.
src/provider/models.rs:276:fn static_fallback_models(id: ProviderId) -> Vec<String> {
src/provider/models.rs:285:        ProviderId::Google | ProviderId::Vertex => &[
src/provider/models.rs:310:fn provider_name(id: ProviderId) -> &'static str {
src/provider/models.rs:317:        ProviderId::Vertex => "Vertex",
src/provider/models.rs:328:        let ids = parse_models(ProviderId::Openai, body);
src/provider/models.rs:341:        let ids = parse_models(ProviderId::Google, body);
src/provider/models.rs:352:        let ids = parse_models(ProviderId::Ollama, body);
src/provider/models.rs:358:        assert!(parse_models(ProviderId::Openai, "not json [").is_empty());
src/provider/models.rs:359:        assert!(parse_models(ProviderId::Openai, "{}").is_empty());
src/provider/models.rs:395:            let fb = static_fallback_models(id);
src/provider/models.rs:405:            static_fallback_models(ProviderId::Ollama),
src/provider/models.rs:414:        let out = fetch_supported_models(ProviderId::Openai, None, None);
src/provider/models.rs:435:            resolved_base_url_with(ProviderId::Ollama, Some("http://h:1"), |_| None),
src/provider/models.rs:439:        let g = resolved_base_url_with(ProviderId::Google, None, |v| match v {
src/provider/models.rs:446:        let a = resolved_base_url_with(ProviderId::Google, None, |v| {
src/provider/models.rs:452:            resolved_base_url_with(ProviderId::Google, None, |_| None),
src/provider/gemini.rs:58:            auth_env_var: API_KEY_ENV,
src/provider/mod.rs:21:pub(crate) use models::{fetch_supported_models, FetchSource};
src/provider/mod.rs:71:    /// HTTP 401/403: the API key was rejected (fatal).
src/provider/mod.rs:78:    Http(u16),
src/provider/mod.rs:79:    /// Client-side request timeout (not retried).
src/provider/mod.rs:125:            ErrorKind::Http(code) => write!(f, "{p} API returned HTTP {code}"),
src/provider/mod.rs:341:    #[value(alias = "google-vertex")]
src/provider/mod.rs:342:    #[serde(alias = "google-vertex")]
src/provider/mod.rs:343:    Vertex,
src/provider/mod.rs:347:/// `key_env_var().is_none()` (CLO-537). `KeylessEndpoint` = Ollama (local URL),
src/provider/mod.rs:348:/// `KeylessAdc` = Vertex (gcloud ADC token), `ApiKey` = every key-bearing cloud provider.
src/provider/mod.rs:360:    pub fn key_env_var(self) -> Option<&'static str> {
src/provider/mod.rs:367:            ProviderId::Vertex => None,
src/provider/mod.rs:381:            ProviderId::Vertex => "gemini-3.1-flash-lite",
src/provider/mod.rs:395:            ProviderId::Vertex => &["GCM_VERTEX_MODEL"],
src/provider/mod.rs:414:            ProviderId::Vertex => "vertex",
src/provider/mod.rs:418:    /// How this provider authenticates. Replaces `key_env_var().is_none()` as the
src/provider/mod.rs:419:    /// "is-Ollama" proxy now that Vertex is a second keyless provider (CLO-537): call
src/provider/mod.rs:421:    pub(crate) fn auth_method(self) -> AuthMethod {
src/provider/mod.rs:424:            ProviderId::Vertex => AuthMethod::KeylessAdc,
src/provider/mod.rs:454:        ProviderId::Vertex => Box::new(vertex::Vertex::new(model)),
src/provider/mod.rs:458:/// Non-blocking Vertex ADC readiness probe for the `gcm provider` wizard (CLO-537).
src/provider/mod.rs:673:        assert_eq!(ProviderId::Groq.key_env_var(), Some("GROQ_API_KEY"));
src/provider/mod.rs:674:        assert_eq!(ProviderId::Google.key_env_var(), Some("GEMINI_API_KEY"));
src/provider/mod.rs:675:        assert_eq!(ProviderId::Openai.key_env_var(), Some("OPENAI_API_KEY"));
src/provider/mod.rs:677:            ProviderId::Anthropic.key_env_var(),
src/provider/mod.rs:680:        assert_eq!(ProviderId::Ollama.key_env_var(), None);
src/provider/mod.rs:834:                status: 401,
src/provider/mod.rs:840:        assert!(auth.to_string().contains("401"));
src/provider/mod.rs:867:                status: 401,
src/provider/mod.rs:875:            ErrorKind::Http(418),
src/provider/vertex.rs:1://! Google Vertex AI backend (CLO-537). A thin backend over the *identical* Gemini
src/provider/vertex.rs:4://! Default Credentials, acquired by shelling out to `gcloud` (matching gcm's optional
src/provider/vertex.rs:5://! external-binary pattern; `GCM_VERTEX_TOKEN` escape hatch first). Because it reuses
src/provider/vertex.rs:10://! resolution and `--dry-run` never spawn gcloud.
src/provider/vertex.rs:26:const NAME: &str = "Vertex";
src/provider/vertex.rs:27:const TOKEN_ENV: &str = "GCM_VERTEX_TOKEN";
src/provider/vertex.rs:28:const PROJECT_ENV: &str = "GCM_VERTEX_PROJECT";
src/provider/vertex.rs:29:const LOCATION_ENV: &str = "GCM_VERTEX_LOCATION";
src/provider/vertex.rs:30:const BASE_URL_ENV: &str = "GCM_VERTEX_BASE_URL";
src/provider/vertex.rs:33:/// Bound the gcloud token subprocess. git is invoked without a timeout because it is
src/provider/vertex.rs:34:/// local/instant, but a gcloud ADC refresh can block on the network (design §4.2 A3).
src/provider/vertex.rs:37:pub struct Vertex {
src/provider/vertex.rs:41:impl Vertex {
src/provider/vertex.rs:43:        Vertex { model }
src/provider/vertex.rs:46:    /// Acquire the ADC access token: `GCM_VERTEX_TOKEN` (trimmed, non-empty) wins,
src/provider/vertex.rs:47:    /// else shell out to gcloud. Resolved lazily per call.
src/provider/vertex.rs:52:        gcloud_token()
src/provider/vertex.rs:55:    /// GCP project: `GCM_VERTEX_PROJECT` > `GOOGLE_CLOUD_PROJECT` > `GCP_PROJECT`.
src/provider/vertex.rs:59:            first_env(&[PROJECT_ENV, "GOOGLE_CLOUD_PROJECT", "GCP_PROJECT"]).ok_or_else(|| {
src/provider/vertex.rs:61:                    "Vertex project not set. Set GCM_VERTEX_PROJECT (or GOOGLE_CLOUD_PROJECT), \
src/provider/vertex.rs:70:    /// Vertex location: `GCM_VERTEX_LOCATION` > `GOOGLE_CLOUD_LOCATION` > `GCP_REGION`,
src/provider/vertex.rs:71:    /// default `global` (Gemini 3.x is global-only on Vertex). Validated strictly
src/provider/vertex.rs:74:        let loc = first_env(&[LOCATION_ENV, "GOOGLE_CLOUD_LOCATION", "GCP_REGION"])
src/provider/vertex.rs:80:    /// Endpoint base: `GCM_VERTEX_BASE_URL` (test seam) wins; else the global host for
src/provider/vertex.rs:103:            // The token rides in `extra_headers` and `auth` is None, so a 401/403 is
src/provider/vertex.rs:104:            // classified as `Http(status)` (not `Auth{env_var}`); `map_auth_error`
src/provider/vertex.rs:105:            // then rewrites it with a gcloud-specific hint (design §4.2 N1). `""` is
src/provider/vertex.rs:107:            auth_env_var: "",
src/provider/vertex.rs:124:    /// Re-map a raw HTTP auth failure to Vertex-specific, actionable text. A Bearer
src/provider/vertex.rs:125:    /// 401/403 is meaningless as "check <env_var>"; 403 on Vertex usually means IAM
src/provider/vertex.rs:129:            ErrorKind::Http(401) => config_err(
src/provider/vertex.rs:130:                "Vertex rejected the credential (HTTP 401): the ADC token is invalid or \
src/provider/vertex.rs:131:                 expired. Run: gcloud auth application-default login"
src/provider/vertex.rs:134:            ErrorKind::Http(403) => {
src/provider/vertex.rs:140:                    "Vertex denied access (HTTP 403) on project '{project}'. Check IAM \
src/provider/vertex.rs:141:                     (grant roles/aiplatform.user), and that the Vertex AI API is enabled: \
src/provider/vertex.rs:142:                     gcloud services enable aiplatform.googleapis.com --project {project}"
src/provider/vertex.rs:150:impl Provider for Vertex {
src/provider/vertex.rs:199:        // satisfies a Vertex run (different endpoint + enterprise terms).
src/provider/vertex.rs:235:/// Validate a Vertex location (templated into the endpoint HOST, so strict): either
src/provider/vertex.rs:248:            "invalid Vertex location '{loc}': expected 'global' or a region like 'us-central1'"
src/provider/vertex.rs:263:            "invalid Vertex project '{project}': contains characters not allowed in a GCP project id"
src/provider/vertex.rs:270:/// Shell out to `gcloud auth application-default print-access-token` under a bounded
src/provider/vertex.rs:271:/// timeout. Distinguishes "gcloud not installed" (io::ErrorKind::NotFound) from
src/provider/vertex.rs:273:fn gcloud_token() -> Result<String, ProviderError> {
src/provider/vertex.rs:274:    let mut cmd = Command::new("gcloud");
src/provider/vertex.rs:275:    cmd.args(["auth", "application-default", "print-access-token"])
src/provider/vertex.rs:283:                "gcloud not found: install the Google Cloud SDK (https://cloud.google.com/sdk), \
src/provider/vertex.rs:284:                 or set GCM_VERTEX_TOKEN with a valid access token."
src/provider/vertex.rs:288:        Err(e) => return Err(config_err(format!("failed to run gcloud: {e}"))),
src/provider/vertex.rs:299:    match rx.recv_timeout(GCLOUD_TIMEOUT) {
src/provider/vertex.rs:304:                    "gcloud returned an empty access token. Run: gcloud auth application-default login"
src/provider/vertex.rs:315:        Ok(Err(e)) => Err(config_err(format!("failed to wait on gcloud: {e}"))),
src/provider/vertex.rs:320:                "gcloud token request timed out after {GCLOUD_TIMEOUT:?}. Check your network, \
src/provider/vertex.rs:321:                 or run: gcloud auth application-default login"
src/provider/vertex.rs:327:/// Turn gcloud stderr into an actionable message, surfacing a reauth hint when the
src/provider/vertex.rs:338:            "gcloud could not provide an access token (run: gcloud auth application-default login). Details: {s}"
src/provider/vertex.rs:341:    format!("gcloud failed to print an access token: {s}")
src/provider/vertex.rs:345:/// same timeout as the hot path. Returns `Ok(())` or a short human-readable reason.
src/provider/vertex.rs:348:    Vertex::new(String::new())
src/provider/vertex.rs:360:        let v = Vertex::new("gemini-3.1-flash-lite".to_string());
src/provider/vertex.rs:371:        let v = Vertex::new("gemini-3.1-flash-lite".to_string());
src/provider/vertex.rs:381:        // auth: None keeps classify_status from emitting Auth{env_var}; the token is
src/provider/vertex.rs:382:        // still sent (extra_headers), and map_auth_error handles 401/403.
src/provider/vertex.rs:383:        let v = Vertex::new("m".to_string());
src/provider/vertex.rs:395:        let v = Vertex::new("gemini-3.1-flash-lite".to_string());
src/provider/vertex.rs:401:        let v = Vertex::new("m".to_string());
src/provider/vertex.rs:437:    fn map_auth_error_rewrites_401_403_to_actionable_text() {
src/provider/vertex.rs:438:        let v = Vertex::new("m".to_string());
src/provider/vertex.rs:439:        let e401 = v.map_auth_error(ProviderError {
src/provider/vertex.rs:441:            kind: ErrorKind::Http(401),
src/provider/vertex.rs:443:        assert!(e401.to_string().contains("application-default login"));
src/provider/vertex.rs:444:        let e403 = v.map_auth_error(ProviderError {
src/provider/vertex.rs:446:            kind: ErrorKind::Http(403),
src/provider/vertex.rs:448:        assert!(e403.to_string().contains("aiplatform.googleapis.com"));
src/provider/vertex.rs:452:            kind: ErrorKind::Http(500),
src/provider/vertex.rs:454:        assert!(matches!(other.kind, ErrorKind::Http(500)));
src/provider/http.rs:13:/// Default client timeout. Bumped 30 -> 60s (CLO-489 round-2 review pt 2):
src/provider/http.rs:15:/// 30s global timeout reliably killed them. Override: `GCM_HTTP_TIMEOUT_SECS`.
src/provider/http.rs:25:/// Short timeout for the interactive model-list fetch (CLO-516): the `gcm provider`
src/provider/http.rs:27:/// to the static list. Deliberately separate from the 60s generation timeout.
src/provider/http.rs:30:fn timeout_secs() -> u64 {
src/provider/http.rs:42:    /// API-key env var, surfaced in an `Auth` (401/403) error message (FR-18).
src/provider/http.rs:44:    pub auth_env_var: &'static str,
src/provider/http.rs:64:    /// API-key env var, surfaced in an `Auth` (401/403) error; `""` for no-auth.
src/provider/http.rs:65:    pub auth_env_var: &'static str,
src/provider/http.rs:71:/// GET a JSON body for model-list discovery. Short timeout + a single light retry
src/provider/http.rs:90:        .timeout_global(Some(MODEL_FETCH_TIMEOUT))
src/provider/http.rs:123:    let kind = classify_status(
src/provider/http.rs:127:        req.auth.as_ref().map(|_| req.auth_env_var),
src/provider/http.rs:143:        .timeout_global(Some(Duration::from_secs(timeout_secs())))
src/provider/http.rs:187:    let kind = classify_status(
src/provider/http.rs:191:        req.auth.as_ref().map(|_| req.auth_env_var),
src/provider/http.rs:199:fn classify_status(
src/provider/http.rs:203:    auth_env_var: Option<&'static str>,
src/provider/http.rs:207:        // 401/403 mean "bad key" only for a backend that sends one; a no-auth
src/provider/http.rs:208:        // backend (`auth_env_var: None`, e.g. Ollama behind a proxy) treats them
src/provider/http.rs:210:        401 | 403 => match auth_env_var {
src/provider/http.rs:212:            None => ErrorKind::Http(status),
src/provider/http.rs:216:        _ => ErrorKind::Http(status),
src/provider/http.rs:324:        ureq::Error::StatusCode(code) => ErrorKind::Http(code),
src/provider/http.rs:352:    fn classify_status_maps_codes() {
src/provider/http.rs:354:            classify_status(400, None, None, Some("K")),
src/provider/http.rs:358:            classify_status(401, None, None, Some("K")),
src/provider/http.rs:359:            ErrorKind::Auth { status: 401, .. }
src/provider/http.rs:362:            classify_status(403, None, None, Some("K")),
src/provider/http.rs:363:            ErrorKind::Auth { status: 403, .. }
src/provider/http.rs:366:            classify_status(429, None, None, Some("K")),
src/provider/http.rs:370:            classify_status(500, None, None, Some("K")),
src/provider/http.rs:375:            classify_status(504, None, None, Some("K")),
src/provider/http.rs:379:            classify_status(418, None, None, Some("K")),
src/provider/http.rs:380:            ErrorKind::Http(418)
src/provider/http.rs:385:    fn classify_status_no_auth_401_403_is_http_not_auth() {
src/provider/http.rs:386:        // CLO-495: a no-auth backend (Ollama, auth_env_var None) must not
src/provider/http.rs:388:        // 401/403 - it degrades to a generic HTTP error.
src/provider/http.rs:390:            classify_status(401, None, None, None),
src/provider/http.rs:391:            ErrorKind::Http(401)
src/provider/http.rs:394:            classify_status(403, None, None, None),
src/provider/http.rs:395:            ErrorKind::Http(403)
src/provider/http.rs:399:            classify_status(400, None, None, None),
src/status.rs:9://! **without** calling [`crate::config::apply_to_env`] (which would copy inline
src/status.rs:19://! mirroring the `config_path_from` / `env_plan(is_set)` style elsewhere.
src/status.rs:30:/// Canonical provider order for output (matches the wizard's `all_providers`).
src/status.rs:31:const PROVIDER_ORDER: [ProviderId; 6] = [
src/status.rs:34:    ProviderId::Vertex,
src/status.rs:99:    /// Vertex only (CLO-537): GCP project as `value (source)`; `None` otherwise.
src/status.rs:102:    /// Vertex only: location as `value (source)` (default `global`); `None` otherwise.
src/status.rs:105:    /// Vertex only: inferred auth source (`GCM_VERTEX_TOKEN` or `gcloud ADC`). Never a
src/status.rs:106:    /// secret, and never verified by a gcloud call (status stays no-subprocess).
src/status.rs:151:    let (selected, provider_error) = selected_provider(cli_provider, config, &env_lookup);
src/status.rs:153:    let providers = PROVIDER_ORDER
src/status.rs:163:            // apply_to_env), so attribute it to the config rather than `default`.
src/status.rs:180:            ) = match id.auth_method() {
src/status.rs:269:fn selected_provider(
src/status.rs:317:        // Vertex (keyless): activated when a project resolves from the env (a
src/status.rs:318:        // config-listed Vertex already returned true above).
src/status.rs:319:        ProviderId::Vertex => {
src/status.rs:320:            env_nonblank(env_lookup, "GCM_VERTEX_PROJECT")
src/status.rs:321:                || env_nonblank(env_lookup, "GOOGLE_CLOUD_PROJECT")
src/status.rs:322:                || env_nonblank(env_lookup, "GCP_PROJECT")
src/status.rs:325:            .key_env_var()
src/status.rs:330:/// Vertex GCP project as a `value (source)` display string (CLO-537): env precedence
src/status.rs:331:/// `GCM_VERTEX_PROJECT` > `GOOGLE_CLOUD_PROJECT` > `GCP_PROJECT` > inline config;
src/status.rs:338:        ("GCM_VERTEX_PROJECT", "env var GCM_VERTEX_PROJECT"),
src/status.rs:339:        ("GOOGLE_CLOUD_PROJECT", "env var GOOGLE_CLOUD_PROJECT"),
src/status.rs:340:        ("GCP_PROJECT", "env var GCP_PROJECT"),
src/status.rs:347:        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Vertex))
src/status.rs:354:/// Vertex location as a `value (source)` display string; env precedence
src/status.rs:355:/// `GCM_VERTEX_LOCATION` > `GOOGLE_CLOUD_LOCATION` > `GCP_REGION` > inline config,
src/status.rs:362:        ("GCM_VERTEX_LOCATION", "env var GCM_VERTEX_LOCATION"),
src/status.rs:363:        ("GOOGLE_CLOUD_LOCATION", "env var GOOGLE_CLOUD_LOCATION"),
src/status.rs:364:        ("GCP_REGION", "env var GCP_REGION"),
src/status.rs:371:        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Vertex))
src/status.rs:381:/// Vertex auth source, inferred WITHOUT a gcloud call (status stays no-subprocess):
src/status.rs:382:/// `GCM_VERTEX_TOKEN` if that env var is set, else `gcloud ADC`. Reflects which path
src/status.rs:385:    if env_nonblank(env_lookup, "GCM_VERTEX_TOKEN") {
src/status.rs:386:        "GCM_VERTEX_TOKEN".to_string()
src/status.rs:388:        "gcloud ADC".to_string()
src/status.rs:399:    if let Some(var) = id.key_env_var() {
src/status.rs:404:    // A blank inline key is treated as "not set" to match the runtime: `env_plan`
src/status.rs:418:/// is treated as unset, matching the `env_plan` bridge that trims before applying.
src/status.rs:428:/// Resolve the Ollama endpoint and its source without calling `apply_to_env`.
src/status.rs:681:        // a blank inline key in config is treated as "not set" (matches env_plan)
src/status.rs:730:    fn selected_provider_precedence() {
src/status.rs:734:            selected_provider(
src/status.rs:744:            selected_provider(None, Some(&c), &env(&[("GCM_PROVIDER", "ollama")])).0,
src/status.rs:749:            selected_provider(None, Some(&c), &env(&[])).0,
src/status.rs:753:        assert_eq!(selected_provider(None, None, &env(&[])).0, ProviderId::Groq);
src/status.rs:757:    fn selected_provider_invalid_env_is_reported_not_fatal() {
src/status.rs:758:        let (id, err) = selected_provider(None, None, &env(&[("GCM_PROVIDER", "bogus")]));
src/status.rs:765:    fn selected_provider_invalid_env_falls_back_to_groq_even_with_config_default() {
src/status.rs:769:        let (id, err) = selected_provider(None, Some(&c), &env(&[("GCM_PROVIDER", "bogus")]));
src/status.rs:848:        // Vertex reports project/location/auth-source and NO key row (N2/P4).
src/status.rs:850:            Some(ProviderId::Vertex),
src/status.rs:854:                ("GCM_VERTEX_PROJECT", "my-proj"),
src/status.rs:855:                ("GCM_VERTEX_LOCATION", "us-central1"),
src/status.rs:861:            .find(|p| p.name == ProviderId::Vertex)
src/status.rs:864:        assert!(v.key_source.is_none(), "no key row for Vertex: {v:?}");
src/status.rs:870:        assert_eq!(v.auth_source.as_deref(), Some("gcloud ADC"));
src/status.rs:872:        // GCM_VERTEX_TOKEN set -> auth source reflects the token env var; location
src/status.rs:875:            Some(ProviderId::Vertex),
src/status.rs:878:            env(&[("GCM_VERTEX_TOKEN", "t"), ("GCM_VERTEX_PROJECT", "p")]),
src/status.rs:883:            .find(|p| p.name == ProviderId::Vertex)
src/status.rs:885:        assert_eq!(v2.auth_source.as_deref(), Some("GCM_VERTEX_TOKEN"));
src/status.rs:890:    fn model_flag_scoped_to_selected_provider() {
src/status.rs:912:        // (bridged via apply_to_env), so status attributes it to the config file.
src/config.rs:9://! reads the file and [`apply_to_env`] bridges it into the env vars the
src/config.rs:13://! the interactive [`run_wizard`] handle the unconfigured case; a non-TTY first
src/config.rs:39:/// Connection timeout for the wizard's Ollama daemon probe (ADR-001 Decision 8):
src/config.rs:78:    /// Vertex only (CLO-537): the GCP project. Bridged into `GCM_VERTEX_PROJECT` by
src/config.rs:79:    /// [`apply_to_env`] when that var is unset. `None`/skip-serialize for every other
src/config.rs:80:    /// provider, so a pre-Vertex config file parses unchanged (no version bump).
src/config.rs:83:    /// Vertex only (CLO-537): the location/region (effective default `global`).
src/config.rs:84:    /// Bridged into `GCM_VERTEX_LOCATION` by [`apply_to_env`] when unset.
src/config.rs:292:    s.push_str(&commented_reference());
src/config.rs:299:fn commented_reference() -> String {
src/config.rs:304:    s.push_str("# endpoint (Ollama only), project+location (Vertex only). Matching env vars\n");
src/config.rs:309:    for id in all_providers() {
src/config.rs:321:        match id.auth_method() {
src/config.rs:323:                if let Some(key_var) = id.key_env_var() {
src/config.rs:336:                    "# project = \"my-gcp-project\"   # required; or set GCM_VERTEX_PROJECT / GOOGLE_CLOUD_PROJECT\n",
src/config.rs:339:                    "# location = \"global\"   # or set GCM_VERTEX_LOCATION / GOOGLE_CLOUD_LOCATION\n",
src/config.rs:374:    cloud_providers()
src/config.rs:376:        .filter_map(|id| id.key_env_var())
src/config.rs:385:pub fn apply_to_env(config: &Config) {
src/config.rs:386:    for (var, value) in env_plan(config, env_nonblank) {
src/config.rs:393:/// Pure planning for [`apply_to_env`]: given `is_set` (does this env var already
src/config.rs:396:fn env_plan(config: &Config, is_set: impl Fn(&str) -> bool) -> Vec<(&'static str, String)> {
src/config.rs:399:        match pc.id.auth_method() {
src/config.rs:401:                if let Some(var) = pc.id.key_env_var() {
src/config.rs:419:                // Vertex: bridge project/location into the gcm-namespaced vars only
src/config.rs:427:                    if !is_set("GCM_VERTEX_PROJECT") {
src/config.rs:428:                        out.push(("GCM_VERTEX_PROJECT", p.to_string()));
src/config.rs:437:                    if !is_set("GCM_VERTEX_LOCATION") {
src/config.rs:438:                        out.push(("GCM_VERTEX_LOCATION", l.to_string()));
src/config.rs:466:pub fn run_wizard() -> Result<Config, GcmError> {
src/config.rs:467:    let all = all_providers();
src/config.rs:476:            eprintln!("  {}. {}", i + 1, provider_label(*id));
src/config.rs:489:        match id.auth_method() {
src/config.rs:492:                let Some(var) = id.key_env_var() else {
src/config.rs:498:                        provider_label(id)
src/config.rs:505:                        provider_label(id)
src/config.rs:524:                // Vertex: project + location (no key, no endpoint) - fixes the bug
src/config.rs:525:                // where selecting Vertex in first-run onboarding prompted for an
src/config.rs:545:            eprintln!("  {}. {}", i + 1, provider_label(pc.id));
src/config.rs:565:/// First-run prompt for the Vertex target: GCP project (required; prefilled from
src/config.rs:566:/// `GCM_VERTEX_PROJECT` / `GOOGLE_CLOUD_PROJECT`) and location (default `global`).
src/config.rs:570:    let prefill = std::env::var("GCM_VERTEX_PROJECT")
src/config.rs:572:        .or_else(|| std::env::var("GOOGLE_CLOUD_PROJECT").ok())
src/config.rs:580:        let input = wizard_read_line(&format!("  Vertex GCP project{hint}: "))?;
src/config.rs:588:        eprintln!("  A GCP project is required for Vertex. Try again.");
src/config.rs:590:    let loc_input = wizard_read_line("  Vertex location [global]: ")?;
src/config.rs:598:        Ok(()) => eprintln!("  gcloud ADC ready."),
src/config.rs:600:            "  note: gcloud ADC not ready ({msg}). Set GCM_VERTEX_TOKEN or run `gcloud auth application-default login` before committing."
src/config.rs:774:pub fn run_provider_wizard() -> Result<bool, GcmError> {
src/config.rs:783:    let all = all_providers();
src/config.rs:789:        all.iter().map(|&id| (id, provider_label(id), "")).collect();
src/config.rs:813:    match id.auth_method() {
src/config.rs:816:            if let Some(var) = id.key_env_var() {
src/config.rs:831:                        provider_label(id)
src/config.rs:867:            // Vertex: project (required) + location (default global); no key, no
src/config.rs:869:            // below short-circuits Vertex).
src/config.rs:872:                .or_else(|| env_value("GCM_VERTEX_PROJECT"))
src/config.rs:873:                .or_else(|| env_value("GOOGLE_CLOUD_PROJECT"))
src/config.rs:875:            let mut project_input = cliclack::input("GCP project (required for Vertex)");
src/config.rs:895:            let location = match cliclack::input("Vertex location")
src/config.rs:911:            sp.start("Checking gcloud ADC...");
src/config.rs:913:                Ok(()) => sp.stop("gcloud ADC ready"),
src/config.rs:915:                    "ADC not ready: {msg} (set GCM_VERTEX_TOKEN or run `gcloud auth application-default login`)"
src/config.rs:924:    let outcome = crate::provider::fetch_supported_models(
src/config.rs:1000:    // Vertex carries project/location instead of a key/endpoint (None for others).
src/config.rs:1010:        provider_label(id)
src/config.rs:1136:    for id in cloud_providers() {
src/config.rs:1137:        if let Some(var) = id.key_env_var() {
src/config.rs:1279:fn probe_url(url: &str, timeout: Duration) -> bool {
src/config.rs:1281:        .timeout_global(Some(timeout))
src/config.rs:1315:/// from `cloud_then_ollama` and added Vertex; the old name implied a key-bearing/Ollama
src/config.rs:1318:fn all_providers() -> [ProviderId; 6] {
src/config.rs:1322:        ProviderId::Vertex,
src/config.rs:1330:fn cloud_providers() -> [ProviderId; 4] {
src/config.rs:1340:fn provider_label(id: ProviderId) -> &'static str {
src/config.rs:1347:        ProviderId::Vertex => "Google (Vertex AI)",
src/config.rs:1639:    fn apply_to_env_does_not_override_existing() {
src/config.rs:1648:        let plan = env_plan(&cfg, |name| {
src/config.rs:1655:    fn apply_to_env_sets_inline_key_endpoint_and_default() {
src/config.rs:1666:        let plan = env_plan(&cfg, |_| false);
src/config.rs:1673:    fn apply_to_env_skips_ollama_url_when_ollama_host_set() {
src/config.rs:1681:        let plan = env_plan(&cfg, |name| name == "OLLAMA_HOST");
src/config.rs:1686:    fn env_plan_bridges_config_model_when_env_unset() {
src/config.rs:1693:        let plan = env_plan(&cfg, |_| false);
src/config.rs:1698:    fn env_plan_yields_to_real_model_env_var() {
src/config.rs:1706:        let plan = env_plan(&cfg, |name| name == "GCM_OPENAI_MODEL");
src/config.rs:1711:    fn env_plan_config_model_yields_to_google_alias_env() {
src/config.rs:1722:        let plan = env_plan(&cfg, |name| name == "GCM_GOOGLE_MODEL");
src/config.rs:1730:    fn env_plan_bridges_google_model_to_primary_var() {
src/config.rs:1738:        let plan = env_plan(&cfg, |_| false);
src/config.rs:1743:    fn env_plan_bridges_vertex_project_and_location() {
src/config.rs:1747:            default: ProviderId::Vertex,
src/config.rs:1749:                id: ProviderId::Vertex,
src/config.rs:1759:        let plan = env_plan(&cfg, |_| false);
src/config.rs:1760:        assert!(plan.contains(&("GCM_VERTEX_PROJECT", "my-proj".to_string())));
src/config.rs:1761:        assert!(plan.contains(&("GCM_VERTEX_LOCATION", "us-central1".to_string())));
src/config.rs:1763:        let plan2 = env_plan(&cfg, |v| v == "GCM_VERTEX_PROJECT");
src/config.rs:1764:        assert!(!plan2.iter().any(|(k, _)| *k == "GCM_VERTEX_PROJECT"));
src/config.rs:1765:        assert!(plan2.contains(&("GCM_VERTEX_LOCATION", "us-central1".to_string())));
src/config.rs:1772:            id: ProviderId::Vertex,
src/config.rs:1784:        // None -> both keys skip-serialize (a pre-Vertex file needs no version bump).
src/config.rs:1789:        // A pre-Vertex file (no project/location keys) still parses.
src/config.rs:1796:    fn render_config_includes_live_values_and_commented_reference() {
src/config.rs:1952:    fn ollama_probe_respects_timeout() {
src/config.rs:1953:        // The probe uses a bounded 3s timeout...
src/config.rs:1956:        // returns promptly as `false`, well under the timeout).

codex
I’m doing an explicit grep for the old `key_env_var()==None` style proxy and for any remaining hardcoded provider lists. That’s the highest-risk completeness seam here because Vertex is only the second keyless provider.
exec
/bin/zsh -lc 'rg -n "groq, google|openai, anthropic, ollama|vertex|valid names|valid provider|GCM_PROVIDER" src tests docs/designs/clo-537-vertex-provider.md docs/plans/clo-537-vertex-provider.md' in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
docs/plans/clo-537-vertex-provider.md:4:**Design Document**: docs/designs/clo-537-vertex-provider.md (Finalized 2026-07-08)
docs/plans/clo-537-vertex-provider.md:24:  - [x] Add `Vertex` to `enum ProviderId` (:332) with `#[value(alias = "google-vertex")]` + `#[serde(alias = "google-vertex")]` (N5)
docs/plans/clo-537-vertex-provider.md:28:  - [x] `as_str()` (:390) -> `"vertex"`
docs/plans/clo-537-vertex-provider.md:29:  - [x] Add `Vertex => Box::new(vertex::Vertex::new(model))` arm to `select()` (:410)
docs/plans/clo-537-vertex-provider.md:30:  - [x] Add `vertex` to the `pick_provider_id` valid-names error string (:454)
docs/plans/clo-537-vertex-provider.md:38:### Phase 2: vertex.rs backend
docs/plans/clo-537-vertex-provider.md:40:- [x] Task 3: Create `src/provider/vertex.rs` skeleton + module wiring
docs/plans/clo-537-vertex-provider.md:41:  - [x] Register `mod vertex;` in `src/provider/mod.rs`
docs/plans/clo-537-vertex-provider.md:62:  - [x] Intercept `ErrorKind::Http(401)` in vertex.rs -> "run: gcloud auth application-default login"
docs/plans/clo-537-vertex-provider.md:67:  - [x] `cache_model_id()` -> `"vertex:{model}"`; `diff_budget()` -> `DiffBudget::standard()`
docs/plans/clo-537-vertex-provider.md:101:  - [x] `selected_provider` valid-names warning (:248): add `vertex` (N5)
docs/plans/clo-537-vertex-provider.md:103:  - [x] Add `vertex` to the `--provider` / `GCM_PROVIDER` valid-names help string (the third hardcoded list)
docs/plans/clo-537-vertex-provider.md:114:- [x] Task 19: vertex.rs unit tests (design §5)
docs/plans/clo-537-vertex-provider.md:121:  - [x] `project`/`location` round-trip; `skip_serializing_if` omits when None; v2 file (no fields) loads; `apply_to_env` sets vertex env vars only when unset
docs/plans/clo-537-vertex-provider.md:124:  - [x] `gcm status --provider vertex`: project/location/auth-source, no key row; `--json` carries new fields; `PROVIDER_ORDER` includes Vertex
docs/plans/clo-537-vertex-provider.md:126:  - [x] End-to-end `gcm --provider vertex` against a local mock `generateContent` server via `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN`
docs/plans/clo-537-vertex-provider.md:132:  - [ ] One manual `generateContent` 200 against the maintainer's GCP project via real gcloud ADC (`gcm --provider vertex` and `gcm status --provider vertex`)
docs/plans/clo-537-vertex-provider.md:138:  - [ ] Push branch `feat/clo-537-vertex`
docs/plans/clo-537-vertex-provider.md:147:- `src/provider/vertex.rs` — NEW: auth (ADC + timeout), project/location validation, `request()`, error mapping, trait impl
docs/designs/clo-537-vertex-provider.md:6:**Linear:** [CLO-537](https://linear.app/cloud-ai/issue/CLO-537/add-vertex-ai-provider-keyless-adc-selectable-in-gcm-provider)
docs/designs/clo-537-vertex-provider.md:7:**Branch:** `feat/clo-537-vertex-provider` (proposed)
docs/designs/clo-537-vertex-provider.md:10:**External reference:** bot-reviewer's `vertex-local-dev.md` / `vertex-gemini-setup.md` (external UX notes; **not committed in this repo** — do not treat as in-repo links, N6)
docs/designs/clo-537-vertex-provider.md:24:- **G1:** New `ProviderId::Vertex` (alias `google-vertex`), a first-class provider selectable via `--provider vertex`, `GCM_PROVIDER=vertex`, and the `gcm provider` wizard — distinct from the existing `Google` (AI Studio) provider.
docs/designs/clo-537-vertex-provider.md:25:- **G2:** `src/provider/vertex.rs` implementing the `Provider` trait by **reusing** `gemini.rs`'s payload builders and response extractor; only `request()` (URL + auth) differs.
docs/designs/clo-537-vertex-provider.md:36:- **Vertex-hosted Anthropic** (`google-vertex-anthropic`, Claude-only).
docs/designs/clo-537-vertex-provider.md:59:│   ├── vertex.rs     — NEW: Vertex provider (auth + request(); reuses gemini payloads)
docs/designs/clo-537-vertex-provider.md:74:New enum variant with the alias `google-vertex`:
docs/designs/clo-537-vertex-provider.md:78:| `as_str()` | `"vertex"` |
docs/designs/clo-537-vertex-provider.md:83:Wired into `select()` — an exhaustive `match id` (mod.rs:410) — as `ProviderId::Vertex => Box::new(vertex::Vertex::new(model))`, and added to the `pick_provider_id` "valid names" error string (mod.rs:454). The `google-vertex` alias needs **both** a `#[value(alias = "google-vertex")]` (clap) and a `#[serde(alias = "google-vertex")]` (config) derive (N5). **Three** hardcoded provider-name lists must all learn `vertex` (grep-verified): `pick_provider_id` (mod.rs:454), the `GCM_PROVIDER` help text in **`cli.rs:20`** (round-2 finding — the design had missed this one), and `status.rs::selected_provider`'s unknown-provider warning (~line 248, see §4.5).
docs/designs/clo-537-vertex-provider.md:87:### 4.2 `vertex.rs` — auth and request
docs/designs/clo-537-vertex-provider.md:109:`project()` / `location()` resolve from env with GCP aliases (config values are bridged into these env vars by `apply_to_env`, so `vertex.rs` reads only env):
docs/designs/clo-537-vertex-provider.md:128:`cache_model_id() → "vertex:{model}"` — distinct from `"google:{model}"` so a cached plan from one platform never satisfies the other (different endpoint + terms). `diff_budget()` = `DiffBudget::standard()` (same large-context Gemini).
docs/designs/clo-537-vertex-provider.md:132:**Vertex error mapping (N1).** `vertex.rs` sends an `Authorization: Bearer` token, so a 401/403 otherwise flows through `http.rs::classify_status` into `ErrorKind::Auth { env_var }` → *"check that `<env_var>` is valid"* — misleading, because the token came from gcloud, not an env var, and a Vertex `403` usually means IAM-denied or *"Vertex AI API not enabled"*, not a bad credential. Vertex must map auth failures to its own actionable text: `401`/expired → *"run: gcloud auth application-default login"*; `403` → distinguish IAM-permission vs. API-not-enabled (*"enable the Vertex AI API on project {project}"*).
docs/designs/clo-537-vertex-provider.md:134:**Implementation (code-validated, round-2):** `classify_status` maps `401|403` to `ErrorKind::Auth { env_var }` **only when `auth_env_var` is `Some`** (`http.rs:210`); a backend that passes `None` gets a generic `ErrorKind::Http(status)` — this is exactly how Ollama avoids naming a nonexistent key var. Vertex does the same: it sends the `Authorization: Bearer` token in the request header but passes **`auth_env_var: None`** to the HTTP layer, so a `401/403` surfaces as `ErrorKind::Http(401|403)`; `vertex.rs` then intercepts that and re-formats it with the Vertex-specific text above. This localizes the whole fix to `vertex.rs` and changes **no shared code** (cleaner than adding a Vertex arm to `classify_status`).
docs/designs/clo-537-vertex-provider.md:182:- **Second valid-names list:** `status.rs::selected_provider` (~line 248) has its own hardcoded provider-name match separate from `pick_provider_id`; it must also accept `vertex` (N5).
docs/designs/clo-537-vertex-provider.md:219:- **`vertex.rs` unit:** `request()` URL for `global` (bare `aiplatform`) vs a regional location (`{loc}-aiplatform`); `Authorization: Bearer` header; `GCM_VERTEX_BASE_URL` override; `cache_model_id()` prefix. Token precedence (`GCM_VERTEX_TOKEN` beats gcloud); missing project → typed `Config` error; token-acquisition failure → actionable typed error.
docs/designs/clo-537-vertex-provider.md:221:- **Config:** `project`/`location` serialize round-trip; `skip_serializing_if` omits them when `None`; a v2 file (no fields) still loads; `apply_to_env` sets the vertex env vars only when unset.
docs/designs/clo-537-vertex-provider.md:227:- **Status (N2):** `gcm status --provider vertex` prints project/location/auth-source and **no** key row; `auth_source` = `GCM_VERTEX_TOKEN` when set else `gcloud ADC`; `--json` carries the new fields; `PROVIDER_ORDER` includes Vertex.
docs/designs/clo-537-vertex-provider.md:228:- **Acceptance:** `gcm status --provider vertex` with `GCM_VERTEX_PROJECT` set; an end-to-end `gcm --provider vertex` against a local mock `generateContent` server via `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN` (no gcloud needed in CI).
docs/designs/clo-537-vertex-provider.md:244:| N6 | Referenced `docs/guides/vertex-*.md` don't exist | header annotated (external, not in-repo) |
docs/designs/clo-537-vertex-provider.md:249:- **401/403 error mapping made concrete + cleaner:** pass `auth_env_var: None` (like Ollama) so it surfaces as `Http(status)`, intercept in `vertex.rs` — no shared-code change → §4.2.
docs/designs/clo-537-vertex-provider.md:257:- Cache cold-start on a `google`↔`vertex` switch is expected and correct (distinct `cache_model_id`), N6.
docs/designs/clo-537-vertex-provider.md:262:2. `vertex.rs`: `access_token()` (timeout), `project()`/`location()` (validation), `request()` (URL + Bearer), reuse gemini payloads, error mapping (§4.2).
tests/resolve_remote.rs:18:    "GCM_PROVIDER",
tests/resolve_remote.rs:1313:            ("GCM_PROVIDER", "ollama"),
tests/status.rs:18:    "GCM_PROVIDER",
tests/status.rs:242:    // GCM_PROVIDER=bogus is fatal at runtime; groq is only a display fallback.
tests/status.rs:248:        &[("GCM_PROVIDER", "bogus"), ("GROQ_API_KEY", "sk-x")],
tests/status.rs:252:        "invalid provider is not fatal for status"
tests/status.rs:295:        &[("GCM_PROVIDER", "bogus")],
tests/status.rs:297:    assert!(out.status.success(), "invalid provider is not fatal");
tests/status.rs:355:    // config.default = openai, but GCM_PROVIDER=bogus -> groq displayed + error.
tests/status.rs:364:        &[("GCM_PROVIDER", "bogus")],
tests/status.rs:390:    // no --provider, no GCM_PROVIDER -> config.default (openai) is selected
tests/provider.rs:21:    "GCM_PROVIDER",
src/cli.rs:20:PROVIDER: select with --provider (groq, google, vertex, openai, anthropic, ollama) or GCM_PROVIDER\n\
src/cli.rs:85:    /// no key, zero-egress). Overrides GCM_PROVIDER (precedence: flag > env > default).
tests/onboarding.rs:17:    "GCM_PROVIDER",
tests/onboarding.rs:130:    cmd.env_remove("GCM_PROVIDER");
tests/onboarding.rs:223:    // (transport error), proving GCM_PROVIDER + GCM_OLLAMA_BASE_URL were applied.
tests/onboarding.rs:265:        "selected provider is Ollama (proves GCM_PROVIDER hydrated): {stdout}"
tests/resolve_integration.rs:19:    "GCM_PROVIDER",
tests/vertex.rs:24:    "GCM_PROVIDER",
tests/vertex.rs:82:fn mock_vertex_resolve_response(replacement: &str) -> String {
tests/vertex.rs:176:default = "vertex"
tests/vertex.rs:179:id = "vertex"
tests/vertex.rs:184:fn resolve_via_vertex_hits_mock_generatecontent_with_bearer_and_resolves() {
tests/vertex.rs:190:    let (url, server) = mock_server(mock_vertex_resolve_response("RESOLVED_BY_VERTEX\n"));
tests/vertex.rs:202:        &["resolve", "--yes", "--provider", "vertex"],
tests/vertex.rs:208:        "gcm resolve --provider vertex failed.\nstdout: {}\nstderr: {}",
tests/vertex.rs:253:fn vertex_missing_gcloud_and_token_is_actionable_not_a_panic() {
tests/vertex.rs:281:        &["resolve", "--yes", "--provider", "vertex"],
src/main.rs:256:/// terminal. An env-configured user (key/`GCM_PROVIDER`/`--provider`) is left
src/main.rs:298:    let env = std::env::var("GCM_PROVIDER").ok();
src/config.rs:48:    /// Provider used when neither `--provider` nor `GCM_PROVIDER` is set.
src/config.rs:351:/// `--provider`, no non-blank `GCM_PROVIDER`, and no cloud key env var set. An
src/config.rs:356:        std::env::var("GCM_PROVIDER").ok().as_deref(),
src/config.rs:362:/// non-blank `GCM_PROVIDER`, and no cloud key present. (Config-file presence is
src/config.rs:454:    if !is_set("GCM_PROVIDER") {
src/config.rs:455:        out.push(("GCM_PROVIDER", provider_token(config.default)));
src/config.rs:527:                let (project, location) = prompt_vertex_target()?;
src/config.rs:569:fn prompt_vertex_target() -> Result<(String, Option<String>), GcmError> {
src/config.rs:597:    match crate::provider::vertex_adc_probe() {
src/config.rs:912:            match crate::provider::vertex_adc_probe() {
src/config.rs:1141:    s.push_str("  export GCM_PROVIDER=groq   # or google, openai, anthropic, ollama\n\n");
src/config.rs:1351:/// The lowercase token for a provider (the value written to TOML / `GCM_PROVIDER`).
src/config.rs:1632:        // non-blank GCM_PROVIDER -> not onboarding
src/config.rs:1634:        // blank/whitespace GCM_PROVIDER is treated as unset -> onboard
src/config.rs:1646:        // GROQ_API_KEY already set -> not in the plan (env wins). GCM_PROVIDER
src/config.rs:1649:            name == "GROQ_API_KEY" || name == "GCM_PROVIDER"
src/config.rs:1669:        assert!(plan.contains(&("GCM_PROVIDER", "groq".to_string())));
src/config.rs:1743:    fn env_plan_bridges_vertex_project_and_location() {
src/config.rs:1769:    fn vertex_project_location_round_trip_and_skip_when_none() {
src/status.rs:13://!   * **selected provider**: `--provider` flag > `GCM_PROVIDER` > `config.default` > Groq
src/status.rs:49:    /// Set only when `GCM_PROVIDER` holds an unknown value (reported, not fatal).
src/status.rs:191:                    vertex_project(config, &env_lookup),
src/status.rs:192:                    Some(vertex_location(config, &env_lookup)),
src/status.rs:193:                    Some(vertex_auth_source(&env_lookup)),
src/status.rs:264:/// `--provider` flag > `GCM_PROVIDER` env > `config.default` > built-in `Groq`.
src/status.rs:265:/// An unknown non-blank `GCM_PROVIDER` is reported (not fatal): at runtime it
src/status.rs:267:/// never overwrites a set `GCM_PROVIDER`), so the display falls back to `Groq`,
src/status.rs:277:    if let Some(raw) = env_lookup("GCM_PROVIDER")
src/status.rs:288:                    "unknown provider '{raw}' in GCM_PROVIDER (valid: groq, google, vertex, \
src/status.rs:289:                     openai, anthropic, ollama); it would be a fatal error on a normal run - \
src/status.rs:333:fn vertex_project(
src/status.rs:357:fn vertex_location(
src/status.rs:384:fn vertex_auth_source(env_lookup: &impl Fn(&str) -> Option<String>) -> String {
src/status.rs:507:        // An invalid GCM_PROVIDER is fatal at runtime: the next run errors out
src/status.rs:510:        println!("  (none - GCM_PROVIDER is invalid; the next run would fail - see Warning below)");
src/status.rs:537:    // GCM_PROVIDER parse failures stay between the headline and the sections.
src/status.rs:737:                &env(&[("GCM_PROVIDER", "ollama")])
src/status.rs:742:        // GCM_PROVIDER over config.default
src/status.rs:744:            selected_provider(None, Some(&c), &env(&[("GCM_PROVIDER", "ollama")])).0,
src/status.rs:758:        let (id, err) = selected_provider(None, None, &env(&[("GCM_PROVIDER", "bogus")]));
src/status.rs:760:        let err = err.expect("invalid provider reported");
src/status.rs:766:        // Even with config.default = openai, an invalid GCM_PROVIDER displays Groq
src/status.rs:769:        let (id, err) = selected_provider(None, Some(&c), &env(&[("GCM_PROVIDER", "bogus")]));
src/status.rs:824:            ["groq", "google", "vertex", "openai", "anthropic", "ollama"]
src/status.rs:847:    fn vertex_status_shows_project_location_auth_and_no_key() {
src/error.rs:97:                 GROQ_API_KEY) and set GCM_PROVIDER, then retry."
src/provider/mod.rs:19:mod vertex;
src/provider/mod.rs:341:    #[value(alias = "google-vertex")]
src/provider/mod.rs:342:    #[serde(alias = "google-vertex")]
src/provider/mod.rs:405:    /// Canonical lowercase token (the `--provider` / `GCM_PROVIDER` value, e.g.
src/provider/mod.rs:414:            ProviderId::Vertex => "vertex",
src/provider/mod.rs:454:        ProviderId::Vertex => Box::new(vertex::Vertex::new(model)),
src/provider/mod.rs:461:pub(crate) fn vertex_adc_probe() -> Result<(), String> {
src/provider/mod.rs:462:    vertex::probe_adc()
src/provider/mod.rs:466:    let env = std::env::var("GCM_PROVIDER").ok();
src/provider/mod.rs:470:/// Precedence flag > env > default(groq). An empty/whitespace `GCM_PROVIDER` is
src/provider/mod.rs:472:/// config error listing the valid names.
src/provider/mod.rs:491:                        "unknown provider '{t}'. Set --provider/GCM_PROVIDER to one of: groq, google, vertex, openai, anthropic, ollama."
src/provider/vertex.rs:200:        format!("vertex:{}", self.model)
src/provider/vertex.rs:394:    fn cache_model_id_is_vertex_qualified() {
src/provider/vertex.rs:396:        assert_eq!(v.cache_model_id(), "vertex:gemini-3.1-flash-lite");

exec
/bin/zsh -lc "rg -n \"key_env_var\\(\\)\\s*==\\s*None|key_env_var\\(\\)\\.is_none\\(|key_env_var\\(\\)\\s*"'!='"\\s*None|match\\s+.*key_env_var\\(|key_env_var\\(\\)\" src tests" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
src/status.rs:325:            .key_env_var()
src/status.rs:399:    if let Some(var) = id.key_env_var() {
src/provider/mod.rs:347:/// `key_env_var().is_none()` (CLO-537). `KeylessEndpoint` = Ollama (local URL),
src/provider/mod.rs:418:    /// How this provider authenticates. Replaces `key_env_var().is_none()` as the
src/provider/mod.rs:673:        assert_eq!(ProviderId::Groq.key_env_var(), Some("GROQ_API_KEY"));
src/provider/mod.rs:674:        assert_eq!(ProviderId::Google.key_env_var(), Some("GEMINI_API_KEY"));
src/provider/mod.rs:675:        assert_eq!(ProviderId::Openai.key_env_var(), Some("OPENAI_API_KEY"));
src/provider/mod.rs:677:            ProviderId::Anthropic.key_env_var(),
src/provider/mod.rs:680:        assert_eq!(ProviderId::Ollama.key_env_var(), None);
src/provider/models.rs:57:    if let Some(var) = id.key_env_var() {
src/provider/models.rs:115:    let env_var = id.key_env_var().unwrap_or("");
src/config.rs:323:                if let Some(key_var) = id.key_env_var() {
src/config.rs:376:        .filter_map(|id| id.key_env_var())
src/config.rs:401:                if let Some(var) = pc.id.key_env_var() {
src/config.rs:492:                let Some(var) = id.key_env_var() else {
src/config.rs:816:            if let Some(var) = id.key_env_var() {
src/config.rs:1137:        if let Some(var) = id.key_env_var() {

exec
/bin/zsh -lc "rg -n \"ProviderId::Vertex|google-vertex|auth_method\\(\" src tests" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
src/status.rs:34:    ProviderId::Vertex,
src/status.rs:180:            ) = match id.auth_method() {
src/status.rs:319:        ProviderId::Vertex => {
src/status.rs:347:        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Vertex))
src/status.rs:371:        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Vertex))
src/status.rs:850:            Some(ProviderId::Vertex),
src/status.rs:861:            .find(|p| p.name == ProviderId::Vertex)
src/status.rs:875:            Some(ProviderId::Vertex),
src/status.rs:883:            .find(|p| p.name == ProviderId::Vertex)
src/provider/mod.rs:341:    #[value(alias = "google-vertex")]
src/provider/mod.rs:342:    #[serde(alias = "google-vertex")]
src/provider/mod.rs:367:            ProviderId::Vertex => None,
src/provider/mod.rs:381:            ProviderId::Vertex => "gemini-3.1-flash-lite",
src/provider/mod.rs:395:            ProviderId::Vertex => &["GCM_VERTEX_MODEL"],
src/provider/mod.rs:414:            ProviderId::Vertex => "vertex",
src/provider/mod.rs:421:    pub(crate) fn auth_method(self) -> AuthMethod {
src/provider/mod.rs:424:            ProviderId::Vertex => AuthMethod::KeylessAdc,
src/provider/mod.rs:454:        ProviderId::Vertex => Box::new(vertex::Vertex::new(model)),
src/provider/models.rs:47:    if id == ProviderId::Vertex {
src/provider/models.rs:133:        ProviderId::Google | ProviderId::Vertex => HttpGet {
src/provider/models.rs:176:        ProviderId::Google | ProviderId::Vertex => (
src/provider/models.rs:211:        ProviderId::Google | ProviderId::Vertex => v
src/provider/models.rs:285:        ProviderId::Google | ProviderId::Vertex => &[
src/provider/models.rs:317:        ProviderId::Vertex => "Vertex",
src/config.rs:321:        match id.auth_method() {
src/config.rs:399:        match pc.id.auth_method() {
src/config.rs:489:        match id.auth_method() {
src/config.rs:813:    match id.auth_method() {
src/config.rs:1322:        ProviderId::Vertex,
src/config.rs:1347:        ProviderId::Vertex => "Google (Vertex AI)",
src/config.rs:1747:            default: ProviderId::Vertex,
src/config.rs:1749:                id: ProviderId::Vertex,
src/config.rs:1772:            id: ProviderId::Vertex,

exec
/bin/zsh -lc "nl -ba src/provider/mod.rs | sed -n '330,520p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
exec
/bin/zsh -lc "nl -ba src/provider/http.rs | sed -n '35,230p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
    35	
    36	/// One provider HTTP request (CLO-489 round-2 review pt 5): `auth` is an optional
    37	/// `(header_name, header_value)` pair passed straight to `ureq` - Groq/OpenAI send
    38	/// `Some(("Authorization", "Bearer <key>"))`, Gemini `Some(("x-goog-api-key", key))`,
    39	/// and the local Ollama provider (CLO-495) sends `None` (no key, no auth header).
    40	pub(super) struct HttpRequest<'a> {
    41	    pub provider: &'static str,
    42	    /// API-key env var, surfaced in an `Auth` (401/403) error message (FR-18).
    43	    /// Meaningful only when `auth` is `Some`; a no-auth backend passes `""`.
    44	    pub auth_env_var: &'static str,
    45	    pub endpoint: String,
    46	    pub auth: Option<(&'static str, String)>,
    47	    /// Additional headers beyond auth + Content-Type (e.g. Anthropic's
    48	    /// `anthropic-version`). Sent after the auth header, before `.send()`.
    49	    pub extra_headers: Vec<(&'static str, String)>,
    50	    pub payload: &'a Value,
    51	}
    52	
    53	/// POST a JSON payload and return the raw 2xx body, retrying transient failures
    54	/// (429/5xx) with bounded backoff (FR-22). Response parsing is the caller's
    55	/// concern and is not retried.
    56	pub(super) fn post_json(req: &HttpRequest) -> Result<String, ProviderError> {
    57	    let cfg = RetryConfig::from_env();
    58	    retry_with(&cfg, std::thread::sleep, || send_once(req))
    59	}
    60	
    61	/// A model-list discovery GET (CLO-516): like [`HttpRequest`] but no payload.
    62	pub(super) struct HttpGet {
    63	    pub provider: &'static str,
    64	    /// API-key env var, surfaced in an `Auth` (401/403) error; `""` for no-auth.
    65	    pub auth_env_var: &'static str,
    66	    pub endpoint: String,
    67	    pub auth: Option<(&'static str, String)>,
    68	    pub extra_headers: Vec<(&'static str, String)>,
    69	}
    70	
    71	/// GET a JSON body for model-list discovery. Short timeout + a single light retry
    72	/// on transient failures so the wizard spinner can't hang; the caller falls back
    73	/// to a static list on any `Err`.
    74	pub(super) fn get_json(req: &HttpGet) -> Result<String, ProviderError> {
    75	    let cfg = RetryConfig {
    76	        max_retries: 1,
    77	        base: Duration::from_millis(200),
    78	        max: Duration::from_secs(2),
    79	    };
    80	    retry_with(&cfg, std::thread::sleep, || get_once(req))
    81	}
    82	
    83	/// One GET attempt (mirrors [`send_once`] but with no request body and the short
    84	/// [`MODEL_FETCH_TIMEOUT`]). Non-2xx is classified into a typed [`ErrorKind`].
    85	fn get_once(req: &HttpGet) -> Result<String, ProviderError> {
    86	    let provider = req.provider;
    87	    let wrap = |kind| ProviderError { provider, kind };
    88	
    89	    let config = ureq::Agent::config_builder()
    90	        .timeout_global(Some(MODEL_FETCH_TIMEOUT))
    91	        .http_status_as_error(false)
    92	        .build();
    93	    let agent = ureq::Agent::new_with_config(config);
    94	    let mut builder = agent.get(&req.endpoint);
    95	    if let Some((name, value)) = req.auth.as_ref() {
    96	        builder = builder.header(*name, value.as_str());
    97	    }
    98	    for (name, value) in &req.extra_headers {
    99	        builder = builder.header(*name, value.as_str());
   100	    }
   101	    let mut response = builder.call().map_err(|e| wrap(map_ureq_error(e)))?;
   102	
   103	    let status = response.status().as_u16();
   104	    if (200..300).contains(&status) {
   105	        return response
   106	            .body_mut()
   107	            .read_to_string()
   108	            .map_err(|e| wrap(ErrorKind::Transport(e.to_string())));
   109	    }
   110	    let retry_after = parse_retry_after(
   111	        response
   112	            .headers()
   113	            .get("retry-after")
   114	            .and_then(|v| v.to_str().ok()),
   115	    );
   116	    let mut buf = Vec::new();
   117	    let _ = response
   118	        .body_mut()
   119	        .as_reader()
   120	        .take(MAX_ERROR_BODY_BYTES)
   121	        .read_to_end(&mut buf);
   122	    let err_body = String::from_utf8_lossy(&buf);
   123	    let kind = classify_status(
   124	        status,
   125	        retry_after,
   126	        bad_request_detail(&err_body),
   127	        req.auth.as_ref().map(|_| req.auth_env_var),
   128	    );
   129	    crate::debug_log!("{provider} model-list response error: {kind:?}");
   130	    Err(wrap(kind))
   131	}
   132	
   133	/// One HTTP attempt. Non-2xx responses are inspected (status + `Retry-After` +
   134	/// a capped error body) and classified into a typed [`ErrorKind`] (FR-21);
   135	/// pre-response transport failures map via [`map_ureq_error`].
   136	fn send_once(req: &HttpRequest) -> Result<String, ProviderError> {
   137	    let provider = req.provider;
   138	    let wrap = |kind| ProviderError { provider, kind };
   139	
   140	    let body = serde_json::to_string(req.payload)
   141	        .map_err(|e| wrap(ErrorKind::Deserialize(e.to_string())))?;
   142	    let config = ureq::Agent::config_builder()
   143	        .timeout_global(Some(Duration::from_secs(timeout_secs())))
   144	        .http_status_as_error(false)
   145	        .build();
   146	    let agent = ureq::Agent::new_with_config(config);
   147	    let mut builder = agent
   148	        .post(&req.endpoint)
   149	        .header("Content-Type", "application/json");
   150	    // No-auth backends (Ollama) send no auth header; everyone else sends one.
   151	    if let Some((name, value)) = req.auth.as_ref() {
   152	        builder = builder.header(*name, value.as_str());
   153	    }
   154	    // Additional provider headers (e.g. Anthropic's `anthropic-version`).
   155	    for (name, value) in &req.extra_headers {
   156	        builder = builder.header(*name, value.as_str());
   157	    }
   158	    let mut response = builder
   159	        .send(body.as_str())
   160	        .map_err(|e| wrap(map_ureq_error(e)))?;
   161	
   162	    let status = response.status().as_u16();
   163	    if (200..300).contains(&status) {
   164	        return response
   165	            .body_mut()
   166	            .read_to_string()
   167	            .map_err(|e| wrap(ErrorKind::Transport(e.to_string())));
   168	    }
   169	    // Non-2xx: capture the case-insensitive Retry-After hint + a size-capped
   170	    // error body (std `Take` so a hit cap truncates cleanly), then classify.
   171	    let retry_after = parse_retry_after(
   172	        response
   173	            .headers()
   174	            .get("retry-after")
   175	            .and_then(|v| v.to_str().ok()),
   176	    );
   177	    let mut buf = Vec::new();
   178	    let _ = response
   179	        .body_mut()
   180	        .as_reader()
   181	        .take(MAX_ERROR_BODY_BYTES)
   182	        .read_to_end(&mut buf);
   183	    let err_body = String::from_utf8_lossy(&buf);
   184	    // Only surface the auth env var when the request actually carried auth - a
   185	    // no-auth backend (Ollama, `auth: None`) must never synthesize an `Auth`
   186	    // error naming an empty placeholder env var (CLO-495 validation).
   187	    let kind = classify_status(
   188	        status,
   189	        retry_after,
   190	        bad_request_detail(&err_body),
   191	        req.auth.as_ref().map(|_| req.auth_env_var),
   192	    );
   193	    crate::debug_log!("{provider} response error: {kind:?}");
   194	    Err(wrap(kind))
   195	}
   196	
   197	/// Classify a non-2xx HTTP status into a typed [`ErrorKind`] (pure; unit-tested).
   198	/// 504 (Gateway Timeout) is a `Server` error, NOT the client-side `Timeout`.
   199	fn classify_status(
   200	    status: u16,
   201	    retry_after: Option<Duration>,
   202	    detail: Option<String>,
   203	    auth_env_var: Option<&'static str>,
   204	) -> ErrorKind {
   205	    match status {
   206	        400 => ErrorKind::BadRequest { detail },
   207	        // 401/403 mean "bad key" only for a backend that sends one; a no-auth
   208	        // backend (`auth_env_var: None`, e.g. Ollama behind a proxy) treats them
   209	        // as a generic HTTP error rather than naming a nonexistent key var.
   210	        401 | 403 => match auth_env_var {
   211	            Some(env_var) => ErrorKind::Auth { status, env_var },
   212	            None => ErrorKind::Http(status),
   213	        },
   214	        429 => ErrorKind::RateLimit { retry_after },
   215	        500..=599 => ErrorKind::Server(status),
   216	        _ => ErrorKind::Http(status),
   217	    }
   218	}
   219	
   220	/// Parse a `Retry-After` header value (integer seconds only; HTTP-date or
   221	/// unparseable/empty -> `None`).
   222	fn parse_retry_after(value: Option<&str>) -> Option<Duration> {
   223	    value
   224	        .map(str::trim)
   225	        .filter(|s| !s.is_empty())
   226	        .and_then(|s| s.parse::<u64>().ok())
   227	        .map(Duration::from_secs)
   228	}
   229	
   230	/// Pull an actionable detail from a 400/blocked body: JSON `error.message` when

 succeeded in 0ms:
   330	#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
   331	#[value(rename_all = "lower")]
   332	#[serde(rename_all = "lowercase")]
   333	pub enum ProviderId {
   334	    Groq,
   335	    #[value(alias = "gemini")]
   336	    #[serde(alias = "gemini")]
   337	    Google,
   338	    Openai,
   339	    Anthropic,
   340	    Ollama,
   341	    #[value(alias = "google-vertex")]
   342	    #[serde(alias = "google-vertex")]
   343	    Vertex,
   344	}
   345	
   346	/// How a provider authenticates - the axis that used to be inferred from
   347	/// `key_env_var().is_none()` (CLO-537). `KeylessEndpoint` = Ollama (local URL),
   348	/// `KeylessAdc` = Vertex (gcloud ADC token), `ApiKey` = every key-bearing cloud provider.
   349	#[derive(Debug, Clone, Copy, PartialEq, Eq)]
   350	pub enum AuthMethod {
   351	    ApiKey,
   352	    KeylessEndpoint,
   353	    KeylessAdc,
   354	}
   355	
   356	impl ProviderId {
   357	    /// The provider's API key env var, or `None` for key-free Ollama. Centralizes
   358	    /// the per-backend key mapping so config onboarding (CLO-496) and the
   359	    /// backends agree on one source of truth.
   360	    pub fn key_env_var(self) -> Option<&'static str> {
   361	        match self {
   362	            ProviderId::Groq => Some("GROQ_API_KEY"),
   363	            ProviderId::Google => Some("GEMINI_API_KEY"),
   364	            ProviderId::Openai => Some("OPENAI_API_KEY"),
   365	            ProviderId::Anthropic => Some("ANTHROPIC_API_KEY"),
   366	            ProviderId::Ollama => None,
   367	            ProviderId::Vertex => None,
   368	        }
   369	    }
   370	
   371	    /// Default model id (ADR-001 Decisions 5/7 + capability matrix).
   372	    pub(crate) fn default_model(self) -> &'static str {
   373	        match self {
   374	            ProviderId::Groq => "openai/gpt-oss-120b",
   375	            ProviderId::Google => "gemini-3.1-flash-lite",
   376	            ProviderId::Openai => "gpt-5.4-mini",
   377	            ProviderId::Anthropic => "claude-haiku-4-5",
   378	            // Local, user-pulled model (FR-56; owner default). `:cloud` variants
   379	            // (e.g. deepseek-v4-flash:cloud) work via --model but are NOT zero-egress.
   380	            ProviderId::Ollama => "gemma4:e4b-mlx",
   381	            ProviderId::Vertex => "gemini-3.1-flash-lite",
   382	        }
   383	    }
   384	
   385	    /// Per-provider model env vars, in precedence order (primary first). Google
   386	    /// reads both `GCM_GEMINI_MODEL` (primary, matches `GEMINI_API_KEY`) and the
   387	    /// `GCM_GOOGLE_MODEL` alias (round-2 review pt 4).
   388	    pub(crate) fn model_env_vars(self) -> &'static [&'static str] {
   389	        match self {
   390	            ProviderId::Groq => &["GCM_GROQ_MODEL"],
   391	            ProviderId::Google => &["GCM_GEMINI_MODEL", "GCM_GOOGLE_MODEL"],
   392	            ProviderId::Openai => &["GCM_OPENAI_MODEL"],
   393	            ProviderId::Anthropic => &["GCM_ANTHROPIC_MODEL"],
   394	            ProviderId::Ollama => &["GCM_OLLAMA_MODEL"],
   395	            ProviderId::Vertex => &["GCM_VERTEX_MODEL"],
   396	        }
   397	    }
   398	
   399	    /// Parse a provider name (env), case- and whitespace-insensitive, honoring
   400	    /// the `gemini` alias.
   401	    pub(crate) fn parse(s: &str) -> Option<Self> {
   402	        <ProviderId as ValueEnum>::from_str(s.trim(), true).ok()
   403	    }
   404	
   405	    /// Canonical lowercase token (the `--provider` / `GCM_PROVIDER` value, e.g.
   406	    /// `groq`, `google`). Stable identifier used in `gcm status` output (CLO-515).
   407	    pub(crate) fn as_str(self) -> &'static str {
   408	        match self {
   409	            ProviderId::Groq => "groq",
   410	            ProviderId::Google => "google",
   411	            ProviderId::Openai => "openai",
   412	            ProviderId::Anthropic => "anthropic",
   413	            ProviderId::Ollama => "ollama",
   414	            ProviderId::Vertex => "vertex",
   415	        }
   416	    }
   417	
   418	    /// How this provider authenticates. Replaces `key_env_var().is_none()` as the
   419	    /// "is-Ollama" proxy now that Vertex is a second keyless provider (CLO-537): call
   420	    /// sites branch on intent, and the exhaustive match flags the next backend added.
   421	    pub(crate) fn auth_method(self) -> AuthMethod {
   422	        match self {
   423	            ProviderId::Ollama => AuthMethod::KeylessEndpoint,
   424	            ProviderId::Vertex => AuthMethod::KeylessAdc,
   425	            _ => AuthMethod::ApiKey,
   426	        }
   427	    }
   428	}
   429	
   430	/// Resolve and construct the selected provider (FR-12/FR-14). Pure w.r.t. the API
   431	/// key (keys are read lazily inside `generate_*`), so the cache path and
   432	/// `--dry-run` resolve a provider without a key.
   433	pub fn select(
   434	    cli_provider: Option<ProviderId>,
   435	    cli_model: Option<&str>,
   436	) -> Result<Box<dyn Provider>, ProviderError> {
   437	    let id = resolve_provider_id(cli_provider)?;
   438	    let model = resolve_model(id, cli_model);
   439	    Ok(match id {
   440	        ProviderId::Groq => Box::new(groq::Groq::new(model)),
   441	        ProviderId::Google => Box::new(gemini::Gemini::new(model)),
   442	        ProviderId::Openai => Box::new(openai::OpenAi::new(model)),
   443	        ProviderId::Anthropic => Box::new(anthropic::Anthropic::new(model)),
   444	        ProviderId::Ollama => {
   445	            // Privacy defense-in-depth (FR-56/FR-48): a cloud-tagged model is proxied
   446	            // off-machine by the local daemon, so warn that it is NOT zero-egress.
   447	            if ollama::is_cloud_model(&model) {
   448	                eprintln!(
   449	                    "note: Ollama model '{model}' routes through Ollama Cloud; the diff is NOT zero-egress."
   450	                );
   451	            }
   452	            Box::new(ollama::Ollama::new(model))
   453	        }
   454	        ProviderId::Vertex => Box::new(vertex::Vertex::new(model)),
   455	    })
   456	}
   457	
   458	/// Non-blocking Vertex ADC readiness probe for the `gcm provider` wizard (CLO-537).
   459	/// `Ok(())` if an access token can be acquired now, else a short human reason. Never
   460	/// on the hot path.
   461	pub(crate) fn vertex_adc_probe() -> Result<(), String> {
   462	    vertex::probe_adc()
   463	}
   464	
   465	fn resolve_provider_id(cli: Option<ProviderId>) -> Result<ProviderId, ProviderError> {
   466	    let env = std::env::var("GCM_PROVIDER").ok();
   467	    pick_provider_id(cli, env.as_deref())
   468	}
   469	
   470	/// Precedence flag > env > default(groq). An empty/whitespace `GCM_PROVIDER` is
   471	/// treated as unset (round-2 review pt 4); a non-empty unknown name is a fatal
   472	/// config error listing the valid names.
   473	pub(crate) fn pick_provider_id(
   474	    cli: Option<ProviderId>,
   475	    env_raw: Option<&str>,
   476	) -> Result<ProviderId, ProviderError> {
   477	    if let Some(id) = cli {
   478	        return Ok(id);
   479	    }
   480	    match env_raw {
   481	        None => Ok(ProviderId::Groq),
   482	        Some(raw) => {
   483	            let t = raw.trim();
   484	            if t.is_empty() {
   485	                return Ok(ProviderId::Groq);
   486	            }
   487	            ProviderId::parse(t).ok_or_else(|| {
   488	                ProviderError::new(
   489	                    "gcm",
   490	                    ErrorKind::Config(format!(
   491	                        "unknown provider '{t}'. Set --provider/GCM_PROVIDER to one of: groq, google, vertex, openai, anthropic, ollama."
   492	                    )),
   493	                )
   494	            })
   495	        }
   496	    }
   497	}
   498	
   499	/// Where a resolved model value came from (CLO-515 source attribution). `Env`
   500	/// carries the winning env-var name, so Google's `GCM_GEMINI_MODEL` >
   501	/// `GCM_GOOGLE_MODEL` precedence is reportable.
   502	#[derive(Debug, Clone, Copy, PartialEq, Eq)]
   503	pub enum ModelSource {
   504	    Flag,
   505	    Env(&'static str),
   506	    Default,
   507	}
   508	
   509	fn resolve_model(id: ProviderId, cli: Option<&str>) -> String {
   510	    resolve_model_with_source(id, cli, |v| std::env::var(v).ok()).0
   511	}
   512	
   513	/// Resolve the effective model **and** its source for a provider (CLO-515).
   514	/// Same precedence as [`resolve_model`] (flag > per-provider env in order >
   515	/// default), with empty/whitespace flag and env values skipped. `env_lookup` is
   516	/// injected so `gcm status` can attribute without touching process env directly
   517	/// (and unit tests stay hermetic).
   518	pub fn resolve_model_with_source(
   519	    id: ProviderId,
   520	    cli: Option<&str>,

exec
/bin/zsh -lc "nl -ba src/cli.rs | sed -n '1,130p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
     1	use clap::Parser;
     2	
     3	use crate::config::AutoPolicy;
     4	use crate::privacy::SecretScanMode;
     5	use crate::provider::ProviderId;
     6	
     7	/// Build-stamped version: crate version plus the git short SHA from build.rs (AC-1).
     8	pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("GCM_GIT_SHA"));
     9	
    10	const EGRESS_DISCLOSURE: &str = "\
    11	gcm groups your working-tree changes into logical commits and commits the first group;\n\
    12	run it again to commit the next group. Grouping operates on whole files over the entire\n\
    13	working tree, so it overrides any manual hunk-level (git add -p) staging: group 1's files\n\
    14	are staged in full, later groups are left unstaged (their changes are never lost).\n\
    15	\n\
    16	MACHINE MODE: use --json to emit a stable JSON envelope on stdout (status: plan/noop/\n\
    17	committed/fallback/error). Combine with --plan-only for a non-destructive preview, or\n\
    18	--yes (alias --no-input) for unattended commits. All diagnostics go to stderr.\n\
    19	\n\
    20	PROVIDER: select with --provider (groq, google, vertex, openai, anthropic, ollama) or GCM_PROVIDER\n\
    21	(precedence flag > env > default groq); override the model with --model or the per-provider\n\
    22	env (GCM_GROQ_MODEL / GCM_GEMINI_MODEL / GCM_OPENAI_MODEL / GCM_ANTHROPIC_MODEL /\n\
    23	GCM_OLLAMA_MODEL). Keys: GROQ_API_KEY, GEMINI_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY.\n\
    24	Ollama is local and needs NO key - it talks to http://localhost:11434 (override with\n\
    25	OLLAMA_HOST / GCM_OLLAMA_BASE_URL).\n\
    26	\n\
    27	PRIVACY: gcm sends your working-tree diff and the content of untracked, non-gitignored\n\
    28	files to the configured LLM provider to generate the plan and commit messages.\n\
    29	Gitignored files (e.g. .env) are never sent. Repo-local .gcmignore/gcmignore patterns\n\
    30	exclude matching paths from analysis. Use --secret-scan=redact or abort to opt into\n\
    31	best-effort credential scanning before provider egress. With --provider=ollama and a\n\
    32	local model, nothing leaves the machine (zero-egress); an Ollama `:cloud` model routes\n\
    33	through Ollama Cloud and is NOT zero-egress. See the README for each provider's data policy.\n\
    34	\n\
    35	LOGGING: set GCM_LOG_LEVEL=off|error|warn|info|debug|trace (default off). The legacy\n\
    36	GCM_DEBUG=1 shortcut still enables debug-level output. Logs always go to stderr.\n\
    37	\n\
    38	RESILIENCE: transient provider failures (HTTP 429 rate limit, 5xx) are retried with\n\
    39	bounded exponential backoff; 400/auth errors fail fast. Set GCM_DEBUG=1 (or\n\
    40	GCM_LOG_LEVEL=debug) to print the typed error and retry attempts to stderr.";
    41	
    42	#[derive(Parser, Debug)]
    43	#[command(
    44	    name = "gcm",
    45	    version = VERSION,
    46	    about = "Generate one signed conventional-commit from your working-tree changes via an LLM provider.",
    47	    after_help = EGRESS_DISCLOSURE,
    48	    after_long_help = EGRESS_DISCLOSURE
    49	)]
    50	pub struct Cli {
    51	    /// Optional subcommand. With none, gcm runs the normal commit flow.
    52	    #[command(subcommand)]
    53	    pub command: Option<Commands>,
    54	
    55	    /// Preview the grouping plan (or the single-commit message with --all) and
    56	    /// exit without staging or committing.
    57	    #[arg(long, global = true)]
    58	    pub dry_run: bool,
    59	
    60	    /// Emit a stable JSON envelope on stdout instead of human-oriented prose.
    61	    /// All diagnostics are sent to stderr so stdout contains a single valid
    62	    /// JSON object. Global so it is accepted after a subcommand too
    63	    /// (e.g. `gcm status --json`).
    64	    #[arg(long, global = true)]
    65	    pub json: bool,
    66	
    67	    /// Generate the plan (or single-commit preview with --all) and exit without
    68	    /// staging, committing, or touching the cache.
    69	    #[arg(long)]
    70	    pub plan_only: bool,
    71	
    72	    /// Skip grouping and commit all changes as a single commit.
    73	    #[arg(long)]
    74	    pub all: bool,
    75	
    76	    /// Discard any cached grouping plan and re-analyze from scratch.
    77	    #[arg(long)]
    78	    pub reset: bool,
    79	
    80	    /// Auto-confirm the commit without prompting (for non-interactive / agent / CI use).
    81	    #[arg(long, global = true, visible_alias = "no-input")]
    82	    pub yes: bool,
    83	
    84	    ///LLM provider: groq (default), google (Gemini), openai, anthropic, or ollama (local,
    85	    /// no key, zero-egress). Overrides GCM_PROVIDER (precedence: flag > env > default).
    86	    #[arg(long, value_enum, global = true)]
    87	    pub provider: Option<ProviderId>,
    88	
    89	    /// Model id for the selected provider (e.g. gpt-5.4-mini).
    90	    /// Overrides the per-provider model env var.
    91	    #[arg(long, global = true)]
    92	    pub model: Option<String>,
    93	
    94	    /// Re-run the interactive provider setup wizard (updating keys/selections),
    95	    /// then continue with the normal commit flow.
    96	    #[arg(long)]
    97	    pub reconfigure: bool,
    98	
    99	    /// Optional pre-send secret scan: off (default), redact detected values, or abort
   100	    /// before any provider request. Overrides GCM_SECRET_SCAN.
   101	    #[arg(long, value_enum, global = true)]
   102	    pub secret_scan: Option<SecretScanMode>,
   103	}
   104	
   105	/// Top-level subcommands. `gcm` with no subcommand runs the commit flow.
   106	#[derive(clap::Subcommand, Debug)]
   107	pub enum Commands {
   108	    /// Run the interactive provider setup wizard and exit.
   109	    Config,
   110	    /// Print active providers, models, paths, and config sources, then exit.
   111	    Status,
   112	    /// Interactively pick a provider, fetch and enable its models, choose a default.
   113	    Provider,
   114	    /// Resolve in-progress merge/rebase/cherry-pick conflicts using the LLM provider.
   115	    Resolve {
   116	        /// Conflict resolution temperature (overrides [conflict].temperature).
   117	        #[arg(long)]
   118	        conflict_temperature: Option<f64>,
   119	
   120	        /// Validation command for resolved files (overrides [conflict].validate_cmd).
   121	        #[arg(long)]
   122	        conflict_validate_cmd: Option<String>,
   123	
   124	        /// Auto-resolution policy (overrides [conflict].auto_policy).
   125	        #[arg(long)]
   126	        conflict_auto_policy: Option<AutoPolicy>,
   127	
   128	        /// Glob patterns for paths that require manual review.
   129	        #[arg(long, value_delimiter = ',')]
   130	        conflict_sensitive_paths: Option<Vec<String>>,

exec
/bin/zsh -lc "nl -ba src/provider/models.rs | sed -n '1,340p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
     1	//! Model-list discovery for the interactive `gcm provider` wizard (CLO-516).
     2	//!
     3	//! Best-effort: each provider's live model endpoint is queried (short timeout, one
     4	//! light retry via [`super::http::get_json`]); on *any* failure - no key, transport
     5	//! error, non-2xx, unparseable body, or an empty result - it degrades to a static
     6	//! per-provider fallback list so the wizard spinner always resolves to a usable set.
     7	//! The raw list is post-processed (D7): non-chat models filtered out, static
     8	//! baselines (including the provider's `default_model`) merged in, then deduped.
     9	//!
    10	//! Centralized here (rather than spread across the five backends) deliberately:
    11	//! discovery is fallback-safe, so a base-URL drift only costs a fallback, not a
    12	//! runtime error. The hot commit path stays in the backends, untouched.
    13	
    14	use serde_json::Value;
    15	
    16	use super::http::{self, HttpGet};
    17	use super::ProviderId;
    18	
    19	/// Where a model list came from, so the wizard can message accurately.
    20	pub enum FetchSource {
    21	    Live,
    22	    Fallback,
    23	}
    24	
    25	/// The outcome of a model-list fetch: the (filtered, merged, deduped) models, the
    26	/// source, and an optional non-fatal warning to surface in the wizard.
    27	pub struct ModelFetchOutcome {
    28	    pub models: Vec<String>,
    29	    pub source: FetchSource,
    30	    pub warning: Option<String>,
    31	}
    32	
    33	/// Fetch the provider's available models for the wizard. Never errors: returns a
    34	/// usable list in every case (live + baselines, or the static fallback).
    35	/// `key` is the resolved API key (None for Ollama, or a cloud provider with none
    36	/// yet); `endpoint` is an explicit base URL (Ollama, from the wizard).
    37	pub fn fetch_supported_models(
    38	    id: ProviderId,
    39	    key: Option<&str>,
    40	    endpoint: Option<&str>,
    41	) -> ModelFetchOutcome {
    42	    let key = key.map(str::trim).filter(|k| !k.is_empty());
    43	
    44	    // Vertex (CLO-537): keyless ADC, no live models endpoint in the MVP (design D4),
    45	    // so return the static Gemini set directly. This short-circuit also keeps the
    46	    // exhaustive `match id` arms below unreachable for Vertex at runtime.
    47	    if id == ProviderId::Vertex {
    48	        return ModelFetchOutcome {
    49	            models: static_fallback_models(id),
    50	            source: FetchSource::Fallback,
    51	            warning: None,
    52	        };
    53	    }
    54	
    55	    // No-key short-circuit (D7.2): a key-bearing provider with no key can't fetch,
    56	    // so skip the network call and show the built-in list with an explicit note.
    57	    if let Some(var) = id.key_env_var() {
    58	        if key.is_none() {
    59	            return ModelFetchOutcome {
    60	                models: static_fallback_models(id),
    61	                source: FetchSource::Fallback,
    62	                warning: Some(format!(
    63	                    "no {var} set - showing the built-in model list; provide the key for the live catalog"
    64	                )),
    65	            };
    66	        }
    67	    }
    68	
    69	    match fetch_live(id, key, endpoint) {
    70	        Ok(raw) => {
    71	            let live: Vec<String> = raw.into_iter().filter(|m| keep_chat_model(id, m)).collect();
    72	            let live_count = live.len();
    73	            // Merge static baselines (incl. default_model) so known-good models are
    74	            // always selectable even if the live list omits them (D7.3).
    75	            let mut models = live;
    76	            models.extend(static_fallback_models(id));
    77	            let models = dedupe(models);
    78	            if live_count == 0 {
    79	                ModelFetchOutcome {
    80	                    models,
    81	                    source: FetchSource::Fallback,
    82	                    warning: Some(format!(
    83	                        "{} returned no usable models; using the built-in list",
    84	                        id.as_str()
    85	                    )),
    86	                }
    87	            } else {
    88	                ModelFetchOutcome {
    89	                    models,
    90	                    source: FetchSource::Live,
    91	                    warning: None,
    92	                }
    93	            }
    94	        }
    95	        Err(e) => ModelFetchOutcome {
    96	            models: static_fallback_models(id),
    97	            source: FetchSource::Fallback,
    98	            warning: Some(format!(
    99	                "could not fetch {} models ({e}); using the built-in list",
   100	                id.as_str()
   101	            )),
   102	        },
   103	    }
   104	}
   105	
   106	/// Query the live model-list endpoint and parse it into raw ids (unfiltered).
   107	fn fetch_live(
   108	    id: ProviderId,
   109	    key: Option<&str>,
   110	    endpoint: Option<&str>,
   111	) -> Result<Vec<String>, super::ProviderError> {
   112	    let base = resolved_base_url(id, endpoint);
   113	    let base = base.trim_end_matches('/');
   114	    let name = provider_name(id);
   115	    let env_var = id.key_env_var().unwrap_or("");
   116	    let req = match id {
   117	        ProviderId::Groq | ProviderId::Openai => HttpGet {
   118	            provider: name,
   119	            auth_env_var: env_var,
   120	            endpoint: format!("{base}/models"),
   121	            auth: key.map(|k| ("Authorization", format!("Bearer {k}"))),
   122	            extra_headers: Vec::new(),
   123	        },
   124	        ProviderId::Anthropic => HttpGet {
   125	            provider: name,
   126	            auth_env_var: env_var,
   127	            endpoint: format!("{base}/v1/models?limit=1000"),
   128	            auth: key.map(|k| ("x-api-key", k.to_string())),
   129	            extra_headers: vec![("anthropic-version", "2023-06-01".to_string())],
   130	        },
   131	        // Vertex is short-circuited in fetch_supported_models; this arm only
   132	        // satisfies exhaustiveness and never runs.
   133	        ProviderId::Google | ProviderId::Vertex => HttpGet {
   134	            provider: name,
   135	            auth_env_var: env_var,
   136	            endpoint: format!("{base}/v1beta/models?pageSize=1000"),
   137	            auth: key.map(|k| ("x-goog-api-key", k.to_string())),
   138	            extra_headers: Vec::new(),
   139	        },
   140	        ProviderId::Ollama => HttpGet {
   141	            provider: name,
   142	            auth_env_var: env_var,
   143	            endpoint: format!("{base}/api/tags"),
   144	            auth: None,
   145	            extra_headers: Vec::new(),
   146	        },
   147	    };
   148	    let raw = http::get_json(&req)?;
   149	    Ok(parse_models(id, &raw))
   150	}
   151	
   152	/// Resolve the model-list base URL: an explicit `endpoint` (Ollama, from the
   153	/// wizard) wins, else the provider's `GCM_*_BASE_URL` override, else its default.
   154	/// Mirrors the backends' base URLs (the runtime source of truth); a drift only
   155	/// costs a fallback since fetch is best-effort.
   156	fn resolved_base_url(id: ProviderId, endpoint: Option<&str>) -> String {
   157	    resolved_base_url_with(id, endpoint, |v| std::env::var(v).ok())
   158	}
   159	
   160	/// Body of [`resolved_base_url`] with the env lookup injected (hermetic tests).
   161	/// Env var precedence per provider mirrors the backends. Google reads both
   162	/// `GCM_GEMINI_BASE_URL` (primary) and the `GCM_GOOGLE_BASE_URL` alias, matching
   163	/// `gemini.rs` - otherwise an alias-based setup fetches from the wrong endpoint.
   164	fn resolved_base_url_with(
   165	    id: ProviderId,
   166	    endpoint: Option<&str>,
   167	    lookup: impl Fn(&str) -> Option<String>,
   168	) -> String {
   169	    if let Some(e) = endpoint.map(str::trim).filter(|e| !e.is_empty()) {
   170	        return e.to_string();
   171	    }
   172	    let (env_vars, default): (&[&str], &str) = match id {
   173	        ProviderId::Groq => (&["GCM_GROQ_BASE_URL"], "https://api.groq.com/openai/v1"),
   174	        ProviderId::Openai => (&["GCM_OPENAI_BASE_URL"], "https://api.openai.com/v1"),
   175	        ProviderId::Anthropic => (&["GCM_ANTHROPIC_BASE_URL"], "https://api.anthropic.com"),
   176	        ProviderId::Google | ProviderId::Vertex => (
   177	            &["GCM_GEMINI_BASE_URL", "GCM_GOOGLE_BASE_URL"],
   178	            "https://generativelanguage.googleapis.com",
   179	        ),
   180	        ProviderId::Ollama => (&["GCM_OLLAMA_BASE_URL"], "http://localhost:11434"),
   181	    };
   182	    env_vars
   183	        .iter()
   184	        .find_map(|var| {
   185	            lookup(var)
   186	                .map(|v| v.trim().to_string())
   187	                .filter(|v| !v.is_empty())
   188	        })
   189	        .unwrap_or_else(|| default.to_string())
   190	}
   191	
   192	/// Parse a provider's model-list body into raw ids (tolerant of extra fields;
   193	/// returns empty on any shape mismatch). Gemini is filtered to `generateContent`
   194	/// models here (the authoritative capability signal) and de-prefixed.
   195	fn parse_models(id: ProviderId, body: &str) -> Vec<String> {
   196	    let Ok(v) = serde_json::from_str::<Value>(body) else {
   197	        return Vec::new();
   198	    };
   199	    match id {
   200	        // OpenAI-compatible: { "data": [ { "id": "..." }, ... ] }
   201	        ProviderId::Groq | ProviderId::Openai | ProviderId::Anthropic => v
   202	            .get("data")
   203	            .and_then(Value::as_array)
   204	            .map(|arr| {
   205	                arr.iter()
   206	                    .filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from))
   207	                    .collect()
   208	            })
   209	            .unwrap_or_default(),
   210	        // Gemini models.list: { "models": [ { "name": "models/x", "supportedGenerationMethods": [...] } ] }
   211	        ProviderId::Google | ProviderId::Vertex => v
   212	            .get("models")
   213	            .and_then(Value::as_array)
   214	            .map(|arr| {
   215	                arr.iter()
   216	                    .filter(|m| {
   217	                        m.get("supportedGenerationMethods")
   218	                            .and_then(Value::as_array)
   219	                            .is_some_and(|a| {
   220	                                a.iter().any(|x| x.as_str() == Some("generateContent"))
   221	                            })
   222	                    })
   223	                    .filter_map(|m| {
   224	                        m.get("name")
   225	                            .and_then(Value::as_str)
   226	                            .map(|n| n.strip_prefix("models/").unwrap_or(n).to_string())
   227	                    })
   228	                    .collect()
   229	            })
   230	            .unwrap_or_default(),
   231	        // Ollama /api/tags: { "models": [ { "name": "llama3:latest" }, ... ] }
   232	        ProviderId::Ollama => v
   233	            .get("models")
   234	            .and_then(Value::as_array)
   235	            .map(|arr| {
   236	                arr.iter()
   237	                    .filter_map(|m| m.get("name").and_then(Value::as_str).map(String::from))
   238	                    .collect()
   239	            })
   240	            .unwrap_or_default(),
   241	    }
   242	}
   243	
   244	/// Whether a model id is a chat/text-generation model gcm can use (D7.1). OpenAI
   245	/// and Groq `/models` return non-text families (whisper/tts/dall-e/embeddings);
   246	/// an exclude-list is safer than an include-list (new chat families aren't missed).
   247	/// Gemini is already filtered in [`parse_models`]; Anthropic/Ollama pass through.
   248	fn keep_chat_model(id: ProviderId, model: &str) -> bool {
   249	    match id {
   250	        ProviderId::Openai | ProviderId::Groq => {
   251	            const EXCLUDE: &[&str] = &[
   252	                "whisper",
   253	                "tts",
   254	                "dall-e",
   255	                "dalle",
   256	                "embedding",
   257	                "embed",
   258	                "moderation",
   259	                "guard",
   260	                "babbage",
   261	                "davinci",
   262	                "audio",
   263	                "image",
   264	                "rerank",
   265	            ];
   266	            let m = model.to_ascii_lowercase();
   267	            !EXCLUDE.iter().any(|bad| m.contains(bad))
   268	        }
   269	        _ => true,
   270	    }
   271	}
   272	
   273	/// A static per-provider model catalog used when the live fetch is unavailable.
   274	/// Always includes the provider's `default_model` so the default is selectable
   275	/// offline. These are discovery hints, not the resolved model (no ADR violation).
   276	fn static_fallback_models(id: ProviderId) -> Vec<String> {
   277	    let curated: &[&str] = match id {
   278	        ProviderId::Groq => &[
   279	            "openai/gpt-oss-120b",
   280	            "openai/gpt-oss-20b",
   281	            "llama-3.3-70b-versatile",
   282	        ],
   283	        ProviderId::Openai => &["gpt-5.4-mini", "gpt-5.4", "gpt-4o-mini"],
   284	        ProviderId::Anthropic => &["claude-haiku-4-5", "claude-sonnet-4-6", "claude-opus-4-8"],
   285	        ProviderId::Google | ProviderId::Vertex => &[
   286	            "gemini-3.1-flash-lite",
   287	            "gemini-3.1-flash",
   288	            "gemini-3.1-pro",
   289	        ],
   290	        ProviderId::Ollama => &[],
   291	    };
   292	    let mut out: Vec<String> = curated.iter().map(|s| s.to_string()).collect();
   293	    let default = id.default_model().to_string();
   294	    if !out.contains(&default) {
   295	        out.insert(0, default);
   296	    }
   297	    out
   298	}
   299	
   300	/// Stable de-duplication preserving first occurrence (live entries stay first).
   301	fn dedupe(models: Vec<String>) -> Vec<String> {
   302	    let mut seen = std::collections::HashSet::new();
   303	    models
   304	        .into_iter()
   305	        .filter(|m| seen.insert(m.clone()))
   306	        .collect()
   307	}
   308	
   309	/// Display name for error messages (mirrors each backend's `NAME`).
   310	fn provider_name(id: ProviderId) -> &'static str {
   311	    match id {
   312	        ProviderId::Groq => "Groq",
   313	        ProviderId::Google => "Google",
   314	        ProviderId::Openai => "OpenAI",
   315	        ProviderId::Anthropic => "Anthropic",
   316	        ProviderId::Ollama => "Ollama",
   317	        ProviderId::Vertex => "Vertex",
   318	    }
   319	}
   320	
   321	#[cfg(test)]
   322	mod tests {
   323	    use super::*;
   324	
   325	    #[test]
   326	    fn parse_openai_compatible_data_ids() {
   327	        let body = r#"{"object":"list","data":[{"id":"gpt-5.4-mini"},{"id":"whisper-1"},{"id":"text-embedding-3-small"}]}"#;
   328	        let ids = parse_models(ProviderId::Openai, body);
   329	        assert_eq!(
   330	            ids,
   331	            vec!["gpt-5.4-mini", "whisper-1", "text-embedding-3-small"]
   332	        );
   333	    }
   334	
   335	    #[test]
   336	    fn parse_gemini_filters_generate_content_and_strips_prefix() {
   337	        let body = r#"{"models":[
   338	            {"name":"models/gemini-3.1-flash-lite","supportedGenerationMethods":["generateContent","countTokens"]},
   339	            {"name":"models/text-embedding-004","supportedGenerationMethods":["embedContent"]}
   340	        ]}"#;

exec
/bin/zsh -lc "nl -ba src/provider/vertex.rs | sed -n '1,360p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
     1	//! Google Vertex AI backend (CLO-537). A thin backend over the *identical* Gemini
     2	//! `generateContent` payloads (reused from [`super::gemini`]): only the endpoint URL
     3	//! and auth differ. Auth is **keyless** - a short-lived OAuth token from Application
     4	//! Default Credentials, acquired by shelling out to `gcloud` (matching gcm's optional
     5	//! external-binary pattern; `GCM_VERTEX_TOKEN` escape hatch first). Because it reuses
     6	//! `gemini::build_resolve_payload`, it inherits the CLO-534 OpenAPI-subset resolve
     7	//! schema (no `additionalProperties`) for free.
     8	//!
     9	//! The token is resolved **lazily** at call time (like `gemini::api_key`), so cache
    10	//! resolution and `--dry-run` never spawn gcloud.
    11	
    12	use std::io;
    13	use std::process::{Command, Stdio};
    14	use std::sync::mpsc;
    15	use std::thread;
    16	use std::time::Duration;
    17	
    18	use serde_json::Value;
    19	
    20	use super::gemini;
    21	use super::http::{self, HttpRequest};
    22	use super::{ErrorKind, Provider, ProviderError};
    23	use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
    24	use crate::plan::Plan;
    25	
    26	const NAME: &str = "Vertex";
    27	const TOKEN_ENV: &str = "GCM_VERTEX_TOKEN";
    28	const PROJECT_ENV: &str = "GCM_VERTEX_PROJECT";
    29	const LOCATION_ENV: &str = "GCM_VERTEX_LOCATION";
    30	const BASE_URL_ENV: &str = "GCM_VERTEX_BASE_URL";
    31	const DEFAULT_LOCATION: &str = "global";
    32	
    33	/// Bound the gcloud token subprocess. git is invoked without a timeout because it is
    34	/// local/instant, but a gcloud ADC refresh can block on the network (design §4.2 A3).
    35	const GCLOUD_TIMEOUT: Duration = Duration::from_secs(10);
    36	
    37	pub struct Vertex {
    38	    model: String,
    39	}
    40	
    41	impl Vertex {
    42	    pub fn new(model: String) -> Self {
    43	        Vertex { model }
    44	    }
    45	
    46	    /// Acquire the ADC access token: `GCM_VERTEX_TOKEN` (trimmed, non-empty) wins,
    47	    /// else shell out to gcloud. Resolved lazily per call.
    48	    fn access_token(&self) -> Result<String, ProviderError> {
    49	        if let Some(tok) = env_nonblank(TOKEN_ENV) {
    50	            return Ok(tok);
    51	        }
    52	        gcloud_token()
    53	    }
    54	
    55	    /// GCP project: `GCM_VERTEX_PROJECT` > `GOOGLE_CLOUD_PROJECT` > `GCP_PROJECT`.
    56	    /// Required (no default); validated before it is templated into the URL path.
    57	    fn project(&self) -> Result<String, ProviderError> {
    58	        let p =
    59	            first_env(&[PROJECT_ENV, "GOOGLE_CLOUD_PROJECT", "GCP_PROJECT"]).ok_or_else(|| {
    60	                config_err(
    61	                    "Vertex project not set. Set GCM_VERTEX_PROJECT (or GOOGLE_CLOUD_PROJECT), \
    62	                 or run `gcm provider` to configure it."
    63	                        .to_string(),
    64	                )
    65	            })?;
    66	        validate_project(&p)?;
    67	        Ok(p)
    68	    }
    69	
    70	    /// Vertex location: `GCM_VERTEX_LOCATION` > `GOOGLE_CLOUD_LOCATION` > `GCP_REGION`,
    71	    /// default `global` (Gemini 3.x is global-only on Vertex). Validated strictly
    72	    /// because it is templated into the endpoint host.
    73	    fn location(&self) -> Result<String, ProviderError> {
    74	        let loc = first_env(&[LOCATION_ENV, "GOOGLE_CLOUD_LOCATION", "GCP_REGION"])
    75	            .unwrap_or_else(|| DEFAULT_LOCATION.to_string());
    76	        validate_location(&loc)?;
    77	        Ok(loc)
    78	    }
    79	
    80	    /// Endpoint base: `GCM_VERTEX_BASE_URL` (test seam) wins; else the global host for
    81	    /// `global`, or the regional `{location}-aiplatform` host otherwise.
    82	    fn base_url(&self, location: &str) -> String {
    83	        if let Some(u) = env_nonblank(BASE_URL_ENV) {
    84	            return u.trim_end_matches('/').to_string();
    85	        }
    86	        if location == DEFAULT_LOCATION {
    87	            "https://aiplatform.googleapis.com".to_string()
    88	        } else {
    89	            format!("https://{location}-aiplatform.googleapis.com")
    90	        }
    91	    }
    92	
    93	    fn request<'a>(
    94	        &self,
    95	        token: &str,
    96	        project: &str,
    97	        location: &str,
    98	        payload: &'a Value,
    99	    ) -> HttpRequest<'a> {
   100	        let base = self.base_url(location);
   101	        HttpRequest {
   102	            provider: NAME,
   103	            // The token rides in `extra_headers` and `auth` is None, so a 401/403 is
   104	            // classified as `Http(status)` (not `Auth{env_var}`); `map_auth_error`
   105	            // then rewrites it with a gcloud-specific hint (design §4.2 N1). `""` is
   106	            // the no-auth placeholder convention.
   107	            auth_env_var: "",
   108	            endpoint: format!(
   109	                "{base}/v1/projects/{project}/locations/{location}/publishers/google/models/{}:generateContent",
   110	                self.model
   111	            ),
   112	            auth: None,
   113	            extra_headers: vec![("Authorization", format!("Bearer {token}"))],
   114	            payload,
   115	        }
   116	    }
   117	
   118	    /// Resolve token + project + location once for a request (the common prologue of
   119	    /// all three trait methods).
   120	    fn target(&self) -> Result<(String, String, String), ProviderError> {
   121	        Ok((self.access_token()?, self.project()?, self.location()?))
   122	    }
   123	
   124	    /// Re-map a raw HTTP auth failure to Vertex-specific, actionable text. A Bearer
   125	    /// 401/403 is meaningless as "check <env_var>"; 403 on Vertex usually means IAM
   126	    /// denied or the API is not enabled, not a bad credential.
   127	    fn map_auth_error(&self, e: ProviderError) -> ProviderError {
   128	        match e.kind {
   129	            ErrorKind::Http(401) => config_err(
   130	                "Vertex rejected the credential (HTTP 401): the ADC token is invalid or \
   131	                 expired. Run: gcloud auth application-default login"
   132	                    .to_string(),
   133	            ),
   134	            ErrorKind::Http(403) => {
   135	                let project = self
   136	                    .project()
   137	                    .map(|p| p.to_string())
   138	                    .unwrap_or_else(|_| "<project>".to_string());
   139	                config_err(format!(
   140	                    "Vertex denied access (HTTP 403) on project '{project}'. Check IAM \
   141	                     (grant roles/aiplatform.user), and that the Vertex AI API is enabled: \
   142	                     gcloud services enable aiplatform.googleapis.com --project {project}"
   143	                ))
   144	            }
   145	            _ => e,
   146	        }
   147	    }
   148	}
   149	
   150	impl Provider for Vertex {
   151	    fn name(&self) -> &'static str {
   152	        NAME
   153	    }
   154	
   155	    fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError> {
   156	        let (token, project, location) = self.target()?;
   157	        let payload = gemini::build_plan_payload(ctx);
   158	        let raw = http::post_json(&self.request(&token, &project, &location, &payload))
   159	            .map_err(|e| self.map_auth_error(e))?;
   160	        let json = gemini::extract_text(&raw)?;
   161	        if json.is_empty() {
   162	            return Err(empty());
   163	        }
   164	        crate::plan::parse_defensive(&json).map_err(|e| ProviderError {
   165	            provider: NAME,
   166	            kind: ErrorKind::Deserialize(e.to_string()),
   167	        })
   168	    }
   169	
   170	    fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError> {
   171	        let (token, project, location) = self.target()?;
   172	        let payload = gemini::build_message_payload(&super::message_user_content(diff));
   173	        let raw = http::post_json(&self.request(&token, &project, &location, &payload))
   174	            .map_err(|e| self.map_auth_error(e))?;
   175	        let message = gemini::extract_text(&raw)?;
   176	        if message.is_empty() {
   177	            return Err(empty());
   178	        }
   179	        Ok(message)
   180	    }
   181	
   182	    fn resolve_hunks(
   183	        &self,
   184	        ctx: &super::ResolveContext,
   185	    ) -> Result<Vec<super::Resolution>, ProviderError> {
   186	        let (token, project, location) = self.target()?;
   187	        let payload = gemini::build_resolve_payload(ctx);
   188	        let raw = http::post_json(&self.request(&token, &project, &location, &payload))
   189	            .map_err(|e| self.map_auth_error(e))?;
   190	        let json = gemini::extract_text(&raw)?;
   191	        if json.is_empty() {
   192	            return Err(empty());
   193	        }
   194	        super::parse_resolutions(NAME, &json, ctx.hunks.len())
   195	    }
   196	
   197	    fn cache_model_id(&self) -> String {
   198	        // Distinct from "google:{model}" so a cached plan from AI Studio never
   199	        // satisfies a Vertex run (different endpoint + enterprise terms).
   200	        format!("vertex:{}", self.model)
   201	    }
   202	
   203	    fn diff_budget(&self) -> DiffBudget {
   204	        DiffBudget::standard()
   205	    }
   206	}
   207	
   208	fn empty() -> ProviderError {
   209	    ProviderError {
   210	        provider: NAME,
   211	        kind: ErrorKind::EmptyResponse,
   212	    }
   213	}
   214	
   215	fn config_err(msg: String) -> ProviderError {
   216	    ProviderError {
   217	        provider: NAME,
   218	        kind: ErrorKind::Config(msg),
   219	    }
   220	}
   221	
   222	/// A trimmed, non-blank env var value, or `None`.
   223	fn env_nonblank(var: &str) -> Option<String> {
   224	    std::env::var(var)
   225	        .ok()
   226	        .map(|s| s.trim().to_string())
   227	        .filter(|s| !s.is_empty())
   228	}
   229	
   230	/// The first non-blank value among `vars`, in order.
   231	fn first_env(vars: &[&str]) -> Option<String> {
   232	    vars.iter().find_map(|v| env_nonblank(v))
   233	}
   234	
   235	/// Validate a Vertex location (templated into the endpoint HOST, so strict): either
   236	/// `global` or a region like `us-central1` - a lowercase letter start, then
   237	/// lowercase-alphanumeric or `-`. Rejects anything that could malform the host.
   238	fn validate_location(loc: &str) -> Result<(), ProviderError> {
   239	    let ok = loc == DEFAULT_LOCATION
   240	        || (loc.chars().next().is_some_and(|c| c.is_ascii_lowercase())
   241	            && loc
   242	                .chars()
   243	                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
   244	    if ok {
   245	        Ok(())
   246	    } else {
   247	        Err(config_err(format!(
   248	            "invalid Vertex location '{loc}': expected 'global' or a region like 'us-central1'"
   249	        )))
   250	    }
   251	}
   252	
   253	/// Validate a GCP project id (templated into the URL PATH segment, so lenient - only
   254	/// reject characters that would break URL structure). This deliberately accepts legacy
   255	/// domain-scoped ids like `example.com:my-project` (which contain `.` and `:`).
   256	fn validate_project(project: &str) -> Result<(), ProviderError> {
   257	    let bad = project.is_empty()
   258	        || project
   259	            .chars()
   260	            .any(|c| c == '/' || c == '?' || c == '#' || c.is_whitespace() || c.is_control());
   261	    if bad {
   262	        Err(config_err(format!(
   263	            "invalid Vertex project '{project}': contains characters not allowed in a GCP project id"
   264	        )))
   265	    } else {
   266	        Ok(())
   267	    }
   268	}
   269	
   270	/// Shell out to `gcloud auth application-default print-access-token` under a bounded
   271	/// timeout. Distinguishes "gcloud not installed" (io::ErrorKind::NotFound) from
   272	/// "installed but ADC not initialized" so the two hints are correct (design §4.2 P8).
   273	fn gcloud_token() -> Result<String, ProviderError> {
   274	    let mut cmd = Command::new("gcloud");
   275	    cmd.args(["auth", "application-default", "print-access-token"])
   276	        .stdout(Stdio::piped())
   277	        .stderr(Stdio::piped());
   278	
   279	    let child = match cmd.spawn() {
   280	        Ok(c) => c,
   281	        Err(e) if e.kind() == io::ErrorKind::NotFound => {
   282	            return Err(config_err(
   283	                "gcloud not found: install the Google Cloud SDK (https://cloud.google.com/sdk), \
   284	                 or set GCM_VERTEX_TOKEN with a valid access token."
   285	                    .to_string(),
   286	            ));
   287	        }
   288	        Err(e) => return Err(config_err(format!("failed to run gcloud: {e}"))),
   289	    };
   290	
   291	    // Bounded wait: a background thread drains stdout/stderr (avoids the pipe-fill
   292	    // deadlock) and sends the output; the main thread gives up after GCLOUD_TIMEOUT.
   293	    let (tx, rx) = mpsc::channel();
   294	    let pid = child.id();
   295	    thread::spawn(move || {
   296	        let _ = tx.send(child.wait_with_output());
   297	    });
   298	
   299	    match rx.recv_timeout(GCLOUD_TIMEOUT) {
   300	        Ok(Ok(out)) if out.status.success() => {
   301	            let tok = String::from_utf8_lossy(&out.stdout).trim().to_string();
   302	            if tok.is_empty() {
   303	                Err(config_err(
   304	                    "gcloud returned an empty access token. Run: gcloud auth application-default login"
   305	                        .to_string(),
   306	                ))
   307	            } else {
   308	                Ok(tok)
   309	            }
   310	        }
   311	        Ok(Ok(out)) => {
   312	            let stderr = String::from_utf8_lossy(&out.stderr);
   313	            Err(config_err(reauth_hint(&stderr)))
   314	        }
   315	        Ok(Err(e)) => Err(config_err(format!("failed to wait on gcloud: {e}"))),
   316	        Err(_) => {
   317	            // Best-effort reap of the timed-out child.
   318	            let _ = Command::new("kill").arg(pid.to_string()).status();
   319	            Err(config_err(format!(
   320	                "gcloud token request timed out after {GCLOUD_TIMEOUT:?}. Check your network, \
   321	                 or run: gcloud auth application-default login"
   322	            )))
   323	        }
   324	    }
   325	}
   326	
   327	/// Turn gcloud stderr into an actionable message, surfacing a reauth hint when the
   328	/// failure looks like an expired/invalid grant.
   329	fn reauth_hint(stderr: &str) -> String {
   330	    let s = stderr.trim();
   331	    let lower = s.to_lowercase();
   332	    if lower.contains("invalid_grant")
   333	        || lower.contains("reauth")
   334	        || lower.contains("could not automatically determine credentials")
   335	        || lower.contains("application default credentials")
   336	    {
   337	        return format!(
   338	            "gcloud could not provide an access token (run: gcloud auth application-default login). Details: {s}"
   339	        );
   340	    }
   341	    format!("gcloud failed to print an access token: {s}")
   342	}
   343	
   344	/// Wizard readiness probe (CLO-537): try to acquire an ADC token now, bounded by the
   345	/// same timeout as the hot path. Returns `Ok(())` or a short human-readable reason.
   346	/// Not used on the commit path (the wizard calls it once, non-blocking).
   347	pub(super) fn probe_adc() -> Result<(), String> {
   348	    Vertex::new(String::new())
   349	        .access_token()
   350	        .map(|_| ())
   351	        .map_err(|e| e.to_string())
   352	}
   353	
   354	#[cfg(test)]
   355	mod tests {
   356	    use super::*;
   357	
   358	    #[test]
   359	    fn request_url_global_uses_bare_aiplatform_host() {
   360	        let v = Vertex::new("gemini-3.1-flash-lite".to_string());

exec
/bin/zsh -lc "nl -ba src/config.rs | sed -n '774,1165p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
   774	pub fn run_provider_wizard() -> Result<bool, GcmError> {
   775	    use cliclack::{intro, multiselect, outro, password, select, spinner};
   776	    use console::style;
   777	
   778	    let existing = load();
   779	
   780	    intro(style(" gcm-provider ").on_cyan().black()).map_err(wizard_io)?;
   781	
   782	    // 1. Provider (radio list, current default pre-highlighted, type-to-filter).
   783	    let all = all_providers();
   784	    let current_default = existing
   785	        .as_ref()
   786	        .map(|c| c.default)
   787	        .unwrap_or(ProviderId::Groq);
   788	    let provider_items: Vec<(ProviderId, &'static str, &'static str)> =
   789	        all.iter().map(|&id| (id, provider_label(id), "")).collect();
   790	    let id = match select::<ProviderId>("Provider")
   791	        .items(&provider_items)
   792	        .initial_value(current_default)
   793	        .filter_mode()
   794	        .max_rows(15)
   795	        .interact()
   796	    {
   797	        Ok(v) => v,
   798	        Err(_) => return wizard_cancelled(),
   799	    };
   800	
   801	    let existing_pc = existing
   802	        .as_ref()
   803	        .and_then(|c| c.providers.iter().find(|p| p.id == id));
   804	
   805	    // 2. Credential / endpoint resolution BEFORE the fetch (D5 step 3). The key is
   806	    // held only in memory and persisted (inline `0600`) solely on completion.
   807	    let mut fetch_key: Option<String> = None;
   808	    let mut persist_key: Option<String> = None;
   809	    let mut fetch_endpoint: Option<String> = None;
   810	    let mut persist_endpoint: Option<String> = None;
   811	    let mut persist_project: Option<String> = None;
   812	    let mut persist_location: Option<String> = None;
   813	    match id.auth_method() {
   814	        AuthMethod::ApiKey => {
   815	            // ApiKey providers always have a key env var; skip defensively if not.
   816	            if let Some(var) = id.key_env_var() {
   817	                let env_key = env_value(var);
   818	                let cfg_key = existing_pc.and_then(|p| p.key.clone());
   819	                if let Some(k) = env_key {
   820	                    // Env wins for the fetch, but never copy an env-derived secret into
   821	                    // the file; preserve any existing inline key (a fallback for when
   822	                    // the env var is unset) rather than erasing it.
   823	                    fetch_key = Some(k);
   824	                    persist_key = cfg_key;
   825	                } else if let Some(k) = cfg_key {
   826	                    fetch_key = Some(k.clone());
   827	                    persist_key = Some(k); // preserve the existing inline key
   828	                } else {
   829	                    let typed = match password(format!(
   830	                        "{} API key (press Enter to skip)",
   831	                        provider_label(id)
   832	                    ))
   833	                    .mask('*')
   834	                    .interact()
   835	                    {
   836	                        Ok(s) => s,
   837	                        Err(_) => return wizard_cancelled(),
   838	                    };
   839	                    let (f, p) = wizard_persist_key(&typed);
   840	                    fetch_key = f;
   841	                    persist_key = p;
   842	                }
   843	            }
   844	        }
   845	        AuthMethod::KeylessEndpoint => {
   846	            // Ollama: resolve/prompt the endpoint before `/api/tags`. An env override
   847	            // wins over the saved config (matching runtime precedence, review M2).
   848	            let default_ep = ollama_wizard_default_endpoint(
   849	                &effective_ollama_endpoint(),
   850	                existing_pc.and_then(|p| p.endpoint.as_deref()),
   851	            );
   852	            let ep = match cliclack::input("Ollama endpoint")
   853	                .default_input(&default_ep)
   854	                .validate(|s: &String| validate_endpoint_url(s).map(|_| ()))
   855	                .interact::<String>()
   856	            {
   857	                Ok(s) => s,
   858	                Err(_) => return wizard_cancelled(),
   859	            };
   860	            let ep = ep.trim().to_string();
   861	            fetch_endpoint = Some(ep.clone());
   862	            if ep != DEFAULT_OLLAMA_ENDPOINT {
   863	                persist_endpoint = Some(ep);
   864	            }
   865	        }
   866	        AuthMethod::KeylessAdc => {
   867	            // Vertex: project (required) + location (default global); no key, no
   868	            // endpoint. The model list comes from the static Gemini set (the fetch
   869	            // below short-circuits Vertex).
   870	            let default_project = existing_pc
   871	                .and_then(|p| p.project.clone())
   872	                .or_else(|| env_value("GCM_VERTEX_PROJECT"))
   873	                .or_else(|| env_value("GOOGLE_CLOUD_PROJECT"))
   874	                .unwrap_or_default();
   875	            let mut project_input = cliclack::input("GCP project (required for Vertex)");
   876	            if !default_project.trim().is_empty() {
   877	                project_input = project_input.default_input(default_project.trim());
   878	            }
   879	            let project = match project_input
   880	                .validate(|s: &String| {
   881	                    if s.trim().is_empty() {
   882	                        Err("a GCP project is required".to_string())
   883	                    } else {
   884	                        Ok(())
   885	                    }
   886	                })
   887	                .interact::<String>()
   888	            {
   889	                Ok(s) => s.trim().to_string(),
   890	                Err(_) => return wizard_cancelled(),
   891	            };
   892	            let default_location = existing_pc
   893	                .and_then(|p| p.location.clone())
   894	                .unwrap_or_else(|| "global".to_string());
   895	            let location = match cliclack::input("Vertex location")
   896	                .default_input(&default_location)
   897	                .interact::<String>()
   898	            {
   899	                Ok(s) => s.trim().to_string(),
   900	                Err(_) => return wizard_cancelled(),
   901	            };
   902	            persist_project = Some(project);
   903	            // Keep the file minimal: omit location at the default `global`.
   904	            persist_location = if location.is_empty() || location == "global" {
   905	                None
   906	            } else {
   907	                Some(location)
   908	            };
   909	            // Non-blocking ADC probe (warns; never blocks a keyless setup).
   910	            let sp = spinner();
   911	            sp.start("Checking gcloud ADC...");
   912	            match crate::provider::vertex_adc_probe() {
   913	                Ok(()) => sp.stop("gcloud ADC ready"),
   914	                Err(msg) => sp.stop(format!(
   915	                    "ADC not ready: {msg} (set GCM_VERTEX_TOKEN or run `gcloud auth application-default login`)"
   916	                )),
   917	            }
   918	        }
   919	    }
   920	
   921	    // 3. Fetch the model list (spinner; never fails - falls back).
   922	    let sp = spinner();
   923	    sp.start("Fetching supported models...");
   924	    let outcome = crate::provider::fetch_supported_models(
   925	        id,
   926	        fetch_key.as_deref(),
   927	        fetch_endpoint.as_deref(),
   928	    );
   929	    match outcome.source {
   930	        crate::provider::FetchSource::Live => {
   931	            sp.stop(format!("Fetched {} models", outcome.models.len()))
   932	        }
   933	        crate::provider::FetchSource::Fallback => sp.stop(
   934	            outcome
   935	                .warning
   936	                .clone()
   937	                .unwrap_or_else(|| "Using the built-in model list".to_string()),
   938	        ),
   939	    }
   940	
   941	    // 4. Multiselect the enabled set (type-to-filter; >=1 required). The candidate
   942	    // list keeps the current enabled set + default selectable even if the live list
   943	    // omitted them (D7.3 wizard-side merge).
   944	    let current_enabled: Vec<String> = existing_pc.map(|p| p.models.clone()).unwrap_or_default();
   945	    let current_model = existing_pc.and_then(|p| p.model.clone());
   946	    let candidates = wizard_model_list(
   947	        id,
   948	        &outcome.models,
   949	        &current_enabled,
   950	        current_model.as_deref(),
   951	    );
   952	    let model_items: Vec<(String, String, &'static str)> = candidates
   953	        .iter()
   954	        .map(|m| (m.clone(), m.clone(), ""))
   955	        .collect();
   956	    // Pre-select the candidates whose canonical form matches a currently-enabled
   957	    // model, so a migrated `llama3` / `models/gemini-x` still highlights (review L1).
   958	    let initial_enabled: Vec<String> = candidates
   959	        .iter()
   960	        .filter(|c| {
   961	            current_enabled
   962	                .iter()
   963	                .any(|e| canonicalize_model(id, e) == canonicalize_model(id, c))
   964	        })
   965	        .cloned()
   966	        .collect();
   967	    let selected = match multiselect::<String>("Enable models (space toggles, type to filter)")
   968	        .items(&model_items)
   969	        .initial_values(initial_enabled)
   970	        .required(true)
   971	        .filter_mode()
   972	        .max_rows(15)
   973	        .interact()
   974	    {
   975	        Ok(v) => v,
   976	        Err(_) => return wizard_cancelled(),
   977	    };
   978	
   979	    // 5. Choose exactly one default among the selected models.
   980	    let default_items: Vec<(String, String, &'static str)> = selected
   981	        .iter()
   982	        .map(|m| (m.clone(), m.clone(), ""))
   983	        .collect();
   984	    let mut default_select = select::<String>("Default model")
   985	        .items(&default_items)
   986	        .filter_mode()
   987	        .max_rows(15);
   988	    if let Some(d) = initial_default_model(id, &selected, current_model.as_deref()) {
   989	        default_select = default_select.initial_value(d);
   990	    }
   991	    let default_model = match default_select.interact() {
   992	        Ok(v) => v,
   993	        Err(_) => return wizard_cancelled(),
   994	    };
   995	
   996	    // 6. Build (pure, AC-4 invariants), merge (preserving other providers), persist.
   997	    let mut updated =
   998	        build_provider_config(id, persist_key, persist_endpoint, default_model, selected)
   999	            .map_err(GcmError::Git)?;
  1000	    // Vertex carries project/location instead of a key/endpoint (None for others).
  1001	    updated.project = persist_project;
  1002	    updated.location = persist_location;
  1003	    let merged = merge_provider_config(existing.as_ref(), updated, true);
  1004	    save(&merged).map_err(|e| GcmError::Git(format!("could not save configuration: {e}")))?;
  1005	    let where_ = config_path()
  1006	        .map(|p| p.display().to_string())
  1007	        .unwrap_or_else(|| "the config file".to_string());
  1008	    outro(format!(
  1009	        "Saved {} configuration to {where_}",
  1010	        provider_label(id)
  1011	    ))
  1012	    .map_err(wizard_io)?;
  1013	    Ok(true)
  1014	}
  1015	
  1016	/// Print the cancellation outro and signal "no change" (nothing persisted).
  1017	fn wizard_cancelled() -> Result<bool, GcmError> {
  1018	    let _ = cliclack::outro_cancel("Cancelled - no changes made.");
  1019	    Ok(false)
  1020	}
  1021	
  1022	/// Map a wizard I/O error into the workflow error type.
  1023	fn wizard_io(e: io::Error) -> GcmError {
  1024	    GcmError::Git(format!("provider wizard I/O error: {e}"))
  1025	}
  1026	
  1027	/// The multiselect candidate list (D7.3, wizard side): fetched ∪ current enabled ∪
  1028	/// current default, deduped, fetched first - so the user's existing selections and
  1029	/// default stay selectable even if the live list omitted them. Membership is by
  1030	/// canonical form (review L1), so a migrated `llama3` doesn't duplicate a fetched
  1031	/// `llama3:latest`. Pure.
  1032	fn wizard_model_list(
  1033	    id: ProviderId,
  1034	    fetched: &[String],
  1035	    current_enabled: &[String],
  1036	    current_default: Option<&str>,
  1037	) -> Vec<String> {
  1038	    let mut out: Vec<String> = fetched.to_vec();
  1039	    let push_if_new = |m: &str, out: &mut Vec<String>| {
  1040	        let c = canonicalize_model(id, m);
  1041	        if !out.iter().any(|x| canonicalize_model(id, x) == c) {
  1042	            out.push(m.to_string());
  1043	        }
  1044	    };
  1045	    for m in current_enabled {
  1046	        push_if_new(m, &mut out);
  1047	    }
  1048	    if let Some(d) = current_default {
  1049	        push_if_new(d, &mut out);
  1050	    }
  1051	    out
  1052	}
  1053	
  1054	/// The pre-selected default model: the current default if it survived into
  1055	/// `selected` (canonical match, review L1), else the first selected (None only when
  1056	/// `selected` is empty). Returns the matching `selected` entry. Pure.
  1057	fn initial_default_model(
  1058	    id: ProviderId,
  1059	    selected: &[String],
  1060	    current_default: Option<&str>,
  1061	) -> Option<String> {
  1062	    if let Some(d) = current_default {
  1063	        let c = canonicalize_model(id, d);
  1064	        if let Some(hit) = selected.iter().find(|m| canonicalize_model(id, m) == c) {
  1065	            return Some(hit.clone());
  1066	        }
  1067	    }
  1068	    selected.first().cloned()
  1069	}
  1070	
  1071	/// The wizard's Ollama endpoint default, mirroring runtime precedence
  1072	/// (`GCM_OLLAMA_BASE_URL` > `OLLAMA_HOST` > saved config > default): a non-default
  1073	/// `effective` means an env override is present and wins over the saved config;
  1074	/// otherwise the saved config, else the default. Pure (review M2).
  1075	fn ollama_wizard_default_endpoint(effective: &str, config_endpoint: Option<&str>) -> String {
  1076	    if effective != DEFAULT_OLLAMA_ENDPOINT {
  1077	        effective.to_string()
  1078	    } else {
  1079	        config_endpoint
  1080	            .map(str::to_string)
  1081	            .unwrap_or_else(|| effective.to_string())
  1082	    }
  1083	}
  1084	
  1085	/// Assemble the wizard's `ProviderConfig` (pure), enforcing the AC-4 invariants so
  1086	/// they are unit-testable rather than only guaranteed by the cliclack flow: at
  1087	/// least one enabled model, and the default among them.
  1088	fn build_provider_config(
  1089	    id: ProviderId,
  1090	    key: Option<String>,
  1091	    endpoint: Option<String>,
  1092	    default_model: String,
  1093	    models: Vec<String>,
  1094	) -> Result<ProviderConfig, String> {
  1095	    if models.is_empty() {
  1096	        return Err("at least one model must be enabled".to_string());
  1097	    }
  1098	    if !models.iter().any(|m| m == &default_model) {
  1099	        return Err(format!(
  1100	            "default model '{default_model}' is not among the enabled models"
  1101	        ));
  1102	    }
  1103	    Ok(ProviderConfig {
  1104	        id,
  1105	        key,
  1106	        endpoint,
  1107	        model: Some(default_model),
  1108	        models,
  1109	        project: None,
  1110	        location: None,
  1111	    })
  1112	}
  1113	
  1114	/// Decide `(fetch_key, persist_key)` from a freshly-typed key: a blank entry is
  1115	/// "skip" (no key, nothing stored); a non-blank entry is used for the fetch and
  1116	/// stored inline. Pure (keeps the secret-handling rule unit-testable). Pure.
  1117	fn wizard_persist_key(typed: &str) -> (Option<String>, Option<String>) {
  1118	    let t = typed.trim();
  1119	    if t.is_empty() {
  1120	        (None, None)
  1121	    } else {
  1122	        (Some(t.to_string()), Some(t.to_string()))
  1123	    }
  1124	}
  1125	
  1126	// ── non-TTY guidance ────────────────────────────────────────────────────────
  1127	
  1128	/// Render the non-TTY guidance: the `export` lines for each provider's key env
  1129	/// var plus a `config.toml` template, so an unattended first run can self-serve.
  1130	pub fn non_tty_instructions() -> String {
  1131	    let mut s = String::new();
  1132	    s.push_str(
  1133	        "gcm is not configured and there is no terminal available for interactive setup.\n\n",
  1134	    );
  1135	    s.push_str("Option A - export a provider key and select it, for example:\n");
  1136	    for id in cloud_providers() {
  1137	        if let Some(var) = id.key_env_var() {
  1138	            s.push_str(&format!("  export {var}=<your-key>\n"));
  1139	        }
  1140	    }
  1141	    s.push_str("  export GCM_PROVIDER=groq   # or google, openai, anthropic, ollama\n\n");
  1142	    s.push_str("Option B - write a 0600 config file (see ADR-001 Decision 4 for the path):\n\n");
  1143	    s.push_str(sample_toml_template());
  1144	    s
  1145	}
  1146	
  1147	/// A minimal, copy-pasteable `config.toml` template for the non-TTY path.
  1148	fn sample_toml_template() -> &'static str {
  1149	    "version = 2\n\
  1150	     default = \"groq\"\n\
  1151	     \n\
  1152	     [[providers]]\n\
  1153	     id = \"groq\"\n\
  1154	     # key = \"<inline-secret>\"   # omit to read GROQ_API_KEY from the environment\n\
  1155	     # models = [\"openai/gpt-oss-120b\"]   # enabled set (only these usable); empty = any\n\
  1156	     \n\
  1157	     [[providers]]\n\
  1158	     id = \"ollama\"\n\
  1159	     endpoint = \"http://localhost:11434\"\n"
  1160	}
  1161	
  1162	// ── secret entry (echo-suppressed) ──────────────────────────────────────────
  1163	
  1164	/// RAII guard that disables terminal echo on creation and restores it on drop -
  1165	/// covering the normal return path and an unwinding panic (mirroring `ui`'s

exec
/bin/zsh -lc "nl -ba src/config.rs | sed -n '299,620p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
   299	fn commented_reference() -> String {
   300	    let mut s = String::new();
   301	    s.push_str("# ── Reference: all available settings ──────────────────────────────────────\n");
   302	    s.push_str("# Copy an entry into the section above, uncomment, and edit. A provider entry\n");
   303	    s.push_str("# supports: model (chosen default), models (enabled set), key (cloud),\n");
   304	    s.push_str("# endpoint (Ollama only), project+location (Vertex only). Matching env vars\n");
   305	    s.push_str("# override this file\n");
   306	    s.push_str("# (e.g. GCM_OPENAI_MODEL=…, OPENAI_API_KEY=…). An empty/absent `models`\n");
   307	    s.push_str("# means unrestricted; set it via `gcm provider` to restrict usage.\n");
   308	    s.push_str("#\n");
   309	    for id in all_providers() {
   310	        let token = provider_token(id);
   311	        let model = id.default_model();
   312	        let model_var = id.model_env_vars()[0];
   313	        s.push_str("# [[providers]]\n");
   314	        s.push_str(&format!("# id = \"{token}\"\n"));
   315	        s.push_str(&format!(
   316	            "# model = \"{model}\"   # default; or set {model_var}\n"
   317	        ));
   318	        s.push_str(&format!(
   319	            "# models = [\"{model}\"]   # enabled set (only these are usable); empty = any\n"
   320	        ));
   321	        match id.auth_method() {
   322	            AuthMethod::ApiKey => {
   323	                if let Some(key_var) = id.key_env_var() {
   324	                    s.push_str(&format!(
   325	                        "# key = \"…\"   # inline secret, or set {key_var}\n"
   326	                    ));
   327	                }
   328	            }
   329	            AuthMethod::KeylessEndpoint => {
   330	                s.push_str(&format!(
   331	                    "# endpoint = \"{DEFAULT_OLLAMA_ENDPOINT}\"   # or set GCM_OLLAMA_BASE_URL / OLLAMA_HOST\n"
   332	                ));
   333	            }
   334	            AuthMethod::KeylessAdc => {
   335	                s.push_str(
   336	                    "# project = \"my-gcp-project\"   # required; or set GCM_VERTEX_PROJECT / GOOGLE_CLOUD_PROJECT\n",
   337	                );
   338	                s.push_str(
   339	                    "# location = \"global\"   # or set GCM_VERTEX_LOCATION / GOOGLE_CLOUD_LOCATION\n",
   340	                );
   341	            }
   342	        }
   343	        s.push_str("#\n");
   344	    }
   345	    s
   346	}
   347	
   348	// ── first-run detection ─────────────────────────────────────────────────────
   349	
   350	/// True iff onboarding should fire after [`load`] returned no usable config: no
   351	/// `--provider`, no non-blank `GCM_PROVIDER`, and no cloud key env var set. An
   352	/// env-configured user is never interrupted.
   353	pub fn needs_onboarding(cli_provider: Option<ProviderId>) -> bool {
   354	    should_onboard(
   355	        cli_provider,
   356	        std::env::var("GCM_PROVIDER").ok().as_deref(),
   357	        any_cloud_key_set(),
   358	    )
   359	}
   360	
   361	/// Pure onboarding decision (the body of [`needs_onboarding`]): no flag, no
   362	/// non-blank `GCM_PROVIDER`, and no cloud key present. (Config-file presence is
   363	/// handled upstream by [`load`] returning `Some`, which short-circuits this.)
   364	fn should_onboard(
   365	    cli_provider: Option<ProviderId>,
   366	    gcm_provider: Option<&str>,
   367	    any_cloud_key: bool,
   368	) -> bool {
   369	    cli_provider.is_none() && gcm_provider.is_none_or(|s| s.trim().is_empty()) && !any_cloud_key
   370	}
   371	
   372	/// Whether any cloud provider's key env var is set and non-blank.
   373	fn any_cloud_key_set() -> bool {
   374	    cloud_providers()
   375	        .iter()
   376	        .filter_map(|id| id.key_env_var())
   377	        .any(env_nonblank)
   378	}
   379	
   380	// ── env bridge ──────────────────────────────────────────────────────────────
   381	
   382	/// Bridge a loaded config into the (unchanged) provider layer by setting env
   383	/// vars it has not already been given. Env always wins: a pre-set var is never
   384	/// overwritten. Best-effort.
   385	pub fn apply_to_env(config: &Config) {
   386	    for (var, value) in env_plan(config, env_nonblank) {
   387	        // edition 2021: `set_var` is safe, and hydration runs once at startup
   388	        // before any provider call or thread spawn (design Assumptions).
   389	        std::env::set_var(var, value);
   390	    }
   391	}
   392	
   393	/// Pure planning for [`apply_to_env`]: given `is_set` (does this env var already
   394	/// hold a non-blank value), return the `(var, value)` assignments to apply. Only
   395	/// currently-unset vars are returned, so env precedence is preserved.
   396	fn env_plan(config: &Config, is_set: impl Fn(&str) -> bool) -> Vec<(&'static str, String)> {
   397	    let mut out = Vec::new();
   398	    for pc in &config.providers {
   399	        match pc.id.auth_method() {
   400	            AuthMethod::ApiKey => {
   401	                if let Some(var) = pc.id.key_env_var() {
   402	                    if let Some(key) = pc.key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
   403	                        if !is_set(var) {
   404	                            out.push((var, key.to_string()));
   405	                        }
   406	                    }
   407	                }
   408	            }
   409	            AuthMethod::KeylessEndpoint => {
   410	                // Ollama: set the base URL only when neither gcm's own var nor
   411	                // the Ollama-native OLLAMA_HOST is already set.
   412	                if let Some(ep) = pc.endpoint.as_deref().filter(|e| !e.trim().is_empty()) {
   413	                    if !is_set("GCM_OLLAMA_BASE_URL") && !is_set("OLLAMA_HOST") {
   414	                        out.push(("GCM_OLLAMA_BASE_URL", ep.to_string()));
   415	                    }
   416	                }
   417	            }
   418	            AuthMethod::KeylessAdc => {
   419	                // Vertex: bridge project/location into the gcm-namespaced vars only
   420	                // when unset (env still wins: flag > env > config > default).
   421	                if let Some(p) = pc
   422	                    .project
   423	                    .as_deref()
   424	                    .map(str::trim)
   425	                    .filter(|p| !p.is_empty())
   426	                {
   427	                    if !is_set("GCM_VERTEX_PROJECT") {
   428	                        out.push(("GCM_VERTEX_PROJECT", p.to_string()));
   429	                    }
   430	                }
   431	                if let Some(l) = pc
   432	                    .location
   433	                    .as_deref()
   434	                    .map(str::trim)
   435	                    .filter(|l| !l.is_empty())
   436	                {
   437	                    if !is_set("GCM_VERTEX_LOCATION") {
   438	                        out.push(("GCM_VERTEX_LOCATION", l.to_string()));
   439	                    }
   440	                }
   441	            }
   442	        }
   443	        // Bridge a config model into the provider's primary model env var, but
   444	        // only when NONE of its model env vars is already set - any user-set var
   445	        // (including an alias like GCM_GOOGLE_MODEL, which resolve_model honors)
   446	        // must win, keeping precedence flag > env > config > default.
   447	        if let Some(model) = pc.model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
   448	            let vars = pc.id.model_env_vars();
   449	            if !vars.iter().any(|v| is_set(v)) {
   450	                out.push((vars[0], model.to_string()));
   451	            }
   452	        }
   453	    }
   454	    if !is_set("GCM_PROVIDER") {
   455	        out.push(("GCM_PROVIDER", provider_token(config.default)));
   456	    }
   457	    out
   458	}
   459	
   460	// ── interactive wizard ──────────────────────────────────────────────────────
   461	
   462	/// Run the interactive wizard end to end (enable providers, capture keys from
   463	/// the environment or a prompt, choose a default) and return the assembled
   464	/// `Config`. Cloud keys already exported are recorded as `key: None` (env-only);
   465	/// an empty key input is also env-only. Invalid menu selections re-prompt.
   466	pub fn run_wizard() -> Result<Config, GcmError> {
   467	    let all = all_providers();
   468	    eprintln!("gcm first-run setup");
   469	    eprintln!(
   470	        "Pick the provider(s) you want to use. You can re-run this anytime with `gcm config`.\n"
   471	    );
   472	
   473	    // 1. Choose which providers to enable (re-prompt until at least one valid).
   474	    let selected = loop {
   475	        for (i, id) in all.iter().enumerate() {
   476	            eprintln!("  {}. {}", i + 1, provider_label(*id));
   477	        }
   478	        let input = wizard_read_line("Enable which providers? (comma-separated numbers): ")?;
   479	        match parse_selection(&input, all.len()) {
   480	            Ok(idxs) => break idxs,
   481	            Err(msg) => eprintln!("  {msg}. Try again.\n"),
   482	        }
   483	    };
   484	
   485	    // 2. Capture each enabled provider's key (env or prompt) or Ollama endpoint.
   486	    let mut enabled: Vec<ProviderConfig> = Vec::new();
   487	    for idx in selected {
   488	        let id = all[idx];
   489	        match id.auth_method() {
   490	            AuthMethod::ApiKey => {
   491	                // ApiKey providers always have a key env var; skip defensively if not.
   492	                let Some(var) = id.key_env_var() else {
   493	                    continue;
   494	                };
   495	                if env_nonblank(var) {
   496	                    eprintln!(
   497	                        "  {} key found in {var} - using the environment variable.",
   498	                        provider_label(id)
   499	                    );
   500	                    enabled.push(cloud_provider_config(id, true, None));
   501	                } else {
   502	                    let typed = read_secret(&format!(
   503	                        "  Enter the {} API key for {} (or press Enter to set {var} yourself later): ",
   504	                        var,
   505	                        provider_label(id)
   506	                    ))
   507	                    .map_err(|e| GcmError::Git(format!("could not read key input: {e}")))?;
   508	                    enabled.push(cloud_provider_config(id, false, Some(&typed)));
   509	                }
   510	            }
   511	            AuthMethod::KeylessEndpoint => {
   512	                let endpoint = prompt_ollama_endpoint()?;
   513	                enabled.push(ProviderConfig {
   514	                    id,
   515	                    key: None,
   516	                    endpoint,
   517	                    model: None,
   518	                    models: Vec::new(),
   519	                    project: None,
   520	                    location: None,
   521	                });
   522	            }
   523	            AuthMethod::KeylessAdc => {
   524	                // Vertex: project + location (no key, no endpoint) - fixes the bug
   525	                // where selecting Vertex in first-run onboarding prompted for an
   526	                // Ollama endpoint (CLO-537 round-2 A2/P1).
   527	                let (project, location) = prompt_vertex_target()?;
   528	                enabled.push(ProviderConfig {
   529	                    id,
   530	                    key: None,
   531	                    endpoint: None,
   532	                    model: None,
   533	                    models: Vec::new(),
   534	                    project: Some(project),
   535	                    location,
   536	                });
   537	            }
   538	        }
   539	    }
   540	
   541	    // 3. Choose the default from the enabled set (re-prompt until valid).
   542	    let default = loop {
   543	        eprintln!("\nWhich provider should be the default?");
   544	        for (i, pc) in enabled.iter().enumerate() {
   545	            eprintln!("  {}. {}", i + 1, provider_label(pc.id));
   546	        }
   547	        let input = wizard_read_line("Default provider (number): ")?;
   548	        match parse_one(&input, enabled.len()) {
   549	            Some(i) => break enabled[i].id,
   550	            None => eprintln!("  Please enter a number from the list."),
   551	        }
   552	    };
   553	
   554	    // Carry forward any enabled-model whitelist (and inline model default) the user
   555	    // set previously via `gcm provider`, so this minimal wizard never erases it.
   556	    preserve_existing_models(&mut enabled, load().as_ref());
   557	
   558	    build_config(&enabled, default).map_err(|msg| {
   559	        // Unreachable: `default` is chosen from `enabled`. Surfaced defensively.
   560	        eprintln!("gcm: {msg}");
   561	        GcmError::OnboardingRequired
   562	    })
   563	}
   564	
   565	/// First-run prompt for the Vertex target: GCP project (required; prefilled from
   566	/// `GCM_VERTEX_PROJECT` / `GOOGLE_CLOUD_PROJECT`) and location (default `global`).
   567	/// Returns `(project, location)` where `location` is `None` at the default so the
   568	/// config file stays minimal. Runs a non-blocking ADC probe (warns, never blocks).
   569	fn prompt_vertex_target() -> Result<(String, Option<String>), GcmError> {
   570	    let prefill = std::env::var("GCM_VERTEX_PROJECT")
   571	        .ok()
   572	        .or_else(|| std::env::var("GOOGLE_CLOUD_PROJECT").ok())
   573	        .map(|s| s.trim().to_string())
   574	        .filter(|s| !s.is_empty());
   575	    let project = loop {
   576	        let hint = prefill
   577	            .as_deref()
   578	            .map(|p| format!(" [{p}]"))
   579	            .unwrap_or_default();
   580	        let input = wizard_read_line(&format!("  Vertex GCP project{hint}: "))?;
   581	        let v = input.trim();
   582	        if !v.is_empty() {
   583	            break v.to_string();
   584	        }
   585	        if let Some(p) = prefill.as_deref() {
   586	            break p.to_string();
   587	        }
   588	        eprintln!("  A GCP project is required for Vertex. Try again.");
   589	    };
   590	    let loc_input = wizard_read_line("  Vertex location [global]: ")?;
   591	    let loc = loc_input.trim();
   592	    let location = if loc.is_empty() || loc == "global" {
   593	        None
   594	    } else {
   595	        Some(loc.to_string())
   596	    };
   597	    match crate::provider::vertex_adc_probe() {
   598	        Ok(()) => eprintln!("  gcloud ADC ready."),
   599	        Err(msg) => eprintln!(
   600	            "  note: gcloud ADC not ready ({msg}). Set GCM_VERTEX_TOKEN or run `gcloud auth application-default login` before committing."
   601	        ),
   602	    }
   603	    Ok((project, location))
   604	}
   605	
   606	/// Prompt for the Ollama endpoint (default offered), validate it, probe the
   607	/// daemon, and return `Some(endpoint)` when non-default (so the file stays
   608	/// minimal) or `None` for the default.
   609	fn prompt_ollama_endpoint() -> Result<Option<String>, GcmError> {
   610	    // Seed the default + probe from the effective runtime endpoint so an
   611	    // existing OLLAMA_HOST / GCM_OLLAMA_BASE_URL is honored (not ignored).
   612	    let effective = effective_ollama_endpoint();
   613	    let url = loop {
   614	        let input = wizard_read_line(&format!("  Ollama endpoint [{effective}]: "))?;
   615	        let raw = input.trim();
   616	        if raw.is_empty() {
   617	            break effective.clone();
   618	        }
   619	        match validate_endpoint_url(raw) {
   620	            Ok(u) => break u,

exec
/bin/zsh -lc "nl -ba src/config.rs | sed -n '1310,1365p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
  1310	
  1311	// ── small shared helpers ────────────────────────────────────────────────────
  1312	
  1313	/// The five v1 providers, cloud first then Ollama (the wizard's menu order).
  1314	/// Every selectable provider, in wizard/reference display order (CLO-537 renamed this
  1315	/// from `cloud_then_ollama` and added Vertex; the old name implied a key-bearing/Ollama
  1316	/// dichotomy that no longer holds). This is the single source of truth iterated by the
  1317	/// reference template and both wizards - a provider absent here is invisible in the UI.
  1318	fn all_providers() -> [ProviderId; 6] {
  1319	    [
  1320	        ProviderId::Groq,
  1321	        ProviderId::Google,
  1322	        ProviderId::Vertex,
  1323	        ProviderId::Openai,
  1324	        ProviderId::Anthropic,
  1325	        ProviderId::Ollama,
  1326	    ]
  1327	}
  1328	
  1329	/// The four cloud (key-bearing) providers.
  1330	fn cloud_providers() -> [ProviderId; 4] {
  1331	    [
  1332	        ProviderId::Groq,
  1333	        ProviderId::Google,
  1334	        ProviderId::Openai,
  1335	        ProviderId::Anthropic,
  1336	    ]
  1337	}
  1338	
  1339	/// Human label for a provider in wizard prompts.
  1340	fn provider_label(id: ProviderId) -> &'static str {
  1341	    match id {
  1342	        ProviderId::Groq => "Groq",
  1343	        ProviderId::Google => "Google (Gemini)",
  1344	        ProviderId::Openai => "OpenAI",
  1345	        ProviderId::Anthropic => "Anthropic",
  1346	        ProviderId::Ollama => "Ollama (local, no key)",
  1347	        ProviderId::Vertex => "Google (Vertex AI)",
  1348	    }
  1349	}
  1350	
  1351	/// The lowercase token for a provider (the value written to TOML / `GCM_PROVIDER`).
  1352	fn provider_token(id: ProviderId) -> String {
  1353	    serde_json::to_value(id)
  1354	        .ok()
  1355	        .and_then(|v| v.as_str().map(String::from))
  1356	        .unwrap_or_else(|| "groq".to_string())
  1357	}
  1358	
  1359	/// Read a non-empty, trimmed env var as a bool "is set".
  1360	fn env_nonblank(name: &str) -> bool {
  1361	    env_value(name).is_some()
  1362	}
  1363	
  1364	/// Read a non-empty, trimmed env var value, else `None`.
  1365	fn env_value(name: &str) -> Option<String> {

exec
/bin/zsh -lc "nl -ba src/status.rs | sed -n '1,430p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
exec
/bin/zsh -lc "nl -ba src/status.rs | sed -n '430,620p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
   430	    config: Option<&Config>,
   431	    env_lookup: &impl Fn(&str) -> Option<String>,
   432	) -> (String, String) {
   433	    if let Some(v) = env_value(env_lookup, "GCM_OLLAMA_BASE_URL") {
   434	        return (v, "env var GCM_OLLAMA_BASE_URL".to_string());
   435	    }
   436	    if let Some(v) = env_value(env_lookup, "OLLAMA_HOST") {
   437	        return (
   438	            ollama::normalize_host(&v),
   439	            "env var OLLAMA_HOST".to_string(),
   440	        );
   441	    }
   442	    if let Some(ep) = config
   443	        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Ollama))
   444	        .and_then(|pc| pc.endpoint.as_deref())
   445	        .map(str::trim)
   446	        .filter(|e| !e.is_empty())
   447	    {
   448	        return (ep.to_string(), "config file".to_string());
   449	    }
   450	    (ollama::DEFAULT_BASE_URL.to_string(), "default".to_string())
   451	}
   452	
   453	fn model_source_label(src: ModelSource) -> String {
   454	    match src {
   455	        ModelSource::Flag => "flag".to_string(),
   456	        ModelSource::Env(var) => format!("env var {var}"),
   457	        ModelSource::Default => "default".to_string(),
   458	    }
   459	}
   460	
   461	/// True when an env var is present and non-blank (trimmed).
   462	fn env_nonblank(env_lookup: &impl Fn(&str) -> Option<String>, name: &str) -> bool {
   463	    env_value(env_lookup, name).is_some()
   464	}
   465	
   466	/// The trimmed, non-empty value of an env var, else `None`.
   467	fn env_value(env_lookup: &impl Fn(&str) -> Option<String>, name: &str) -> Option<String> {
   468	    env_lookup(name)
   469	        .map(|v| v.trim().to_string())
   470	        .filter(|v| !v.is_empty())
   471	}
   472	
   473	/// Render the default human view to stdout: Version / Paths / a `Selected`
   474	/// headline (what the next run will use) / providers grouped into Activated and
   475	/// Not activated. Pure formatting over the already-resolved [`StatusReport`];
   476	/// the `--json` payload is rendered separately and is unaffected.
   477	fn print_human(report: &StatusReport) {
   478	    println!("gcm {}", report.version);
   479	
   480	    println!("\nPaths:");
   481	    println!("  config dir source: {}", report.paths.config_dir_source);
   482	    match &report.paths.config_dir {
   483	        Some(d) => println!("  config dir:        {}", d.display()),
   484	        None => println!("  config dir:        (no OS config dir available)"),
   485	    }
   486	    match &report.paths.config_file_path {
   487	        Some(p) => {
   488	            // present-but-unusable (exists yet not loaded) vs present vs absent
   489	            let state = match (
   490	                report.paths.config_file_exists,
   491	                report.paths.config_file_loaded,
   492	            ) {
   493	                (true, true) => "exists",
   494	                (true, false) => {
   495	                    "present but NOT usable (malformed/wrong version/insecure - see stderr)"
   496	                }
   497	                (false, _) => "no config file",
   498	            };
   499	            println!("  config file:       {} ({state})", p.display());
   500	        }
   501	        None => println!("  config file:       (no OS config dir available)"),
   502	    }
   503	
   504	    // The one fact `status` exists to answer: what will the next `gcm` run use?
   505	    println!("\nSelected (gcm will use this):");
   506	    if report.provider_error.is_some() {
   507	        // An invalid GCM_PROVIDER is fatal at runtime: the next run errors out
   508	        // before any provider is chosen. The provider flagged `selected` below is
   509	        // only a display fallback for the listing - see the Warning.
   510	        println!("  (none - GCM_PROVIDER is invalid; the next run would fail - see Warning below)");
   511	    } else if let Some(p) = report.providers.iter().find(|p| p.selected) {
   512	        let tag = locality_tag(p)
   513	            .map(|t| format!(" [{t}]"))
   514	            .unwrap_or_default();
   515	        // Truthful runtime caveat. Only a cloud provider missing its key
   516	        // necessarily errors; Ollama is key-free and falls back to the local
   517	        // daemon, so an unconfigured Ollama selection can still run.
   518	        let note = match (p.activated, p.endpoint.as_deref()) {
   519	            (true, _) => String::new(),
   520	            (false, Some(ep)) => {
   521	                format!(" (not configured - will try the local Ollama daemon at {ep})")
   522	            }
   523	            (false, None) => {
   524	                " (NOT activated - no API key; gcm would error on a real run)".to_string()
   525	            }
   526	        };
   527	        println!(
   528	            "  {} -> {} ({}){tag}{note}",
   529	            p.name.as_str(),
   530	            p.model,
   531	            p.model_source
   532	        );
   533	    } else {
   534	        println!("  (none)");
   535	    }
   536	
   537	    // GCM_PROVIDER parse failures stay between the headline and the sections.
   538	    if let Some(err) = &report.provider_error {
   539	        println!("\nWarning: {err}");
   540	    }
   541	
   542	    print_provider_section(report, "Activated:", true);
   543	    print_provider_section(report, "Not activated:", false);
   544	}
   545	
   546	/// The neutral `cloud` / `local` descriptor for an Ollama model, or `None` for a
   547	/// cloud provider (the distinction is only meaningful for Ollama). Driven by the
   548	/// already-computed `zero_egress` field; never recomputed here.
   549	fn locality_tag(p: &ProviderStatus) -> Option<&'static str> {
   550	    match p.zero_egress {
   551	        Some(true) => Some("local"),
   552	        Some(false) => Some("cloud"),
   553	        None => None,
   554	    }
   555	}
   556	
   557	/// Print one provider section. Members are those matching `activated`; the
   558	/// selected provider leads (marked `>`), the rest keep canonical order - a stable
   559	/// sort on `!selected` floats the selection without disturbing the others. An
   560	/// empty section prints `(none)`.
   561	fn print_provider_section(report: &StatusReport, heading: &str, activated: bool) {
   562	    println!("\n{heading}");
   563	    let mut members: Vec<&ProviderStatus> = report
   564	        .providers
   565	        .iter()
   566	        .filter(|p| p.activated == activated)
   567	        .collect();
   568	    members.sort_by_key(|p| !p.selected);
   569	    if members.is_empty() {
   570	        println!("  (none)");
   571	        return;
   572	    }
   573	    for p in members {
   574	        print_provider_block(p);
   575	    }
   576	}
   577	
   578	/// Print a single de-noised provider block: the name line (marked `>` when
   579	/// selected, two spaces otherwise) and the same indented detail lines as before.
   580	/// Ollama appends a `[cloud]` / `[local]` tag to its model line.
   581	fn print_provider_block(p: &ProviderStatus) {
   582	    let marker = if p.selected { ">" } else { " " };
   583	    println!("{marker} {}", p.name.as_str());
   584	    if let Some(ks) = &p.key_source {
   585	        println!("    key:   {ks}");
   586	    }
   587	    if let Some(ep) = &p.endpoint {
   588	        let src = p.endpoint_source.as_deref().unwrap_or("unknown");
   589	        println!("    endpoint: {ep} ({src})");
   590	    }
   591	    if let Some(proj) = &p.project {
   592	        println!("    project: {proj}");
   593	    }
   594	    if let Some(loc) = &p.location {
   595	        println!("    location: {loc}");
   596	    }
   597	    if let Some(auth) = &p.auth_source {
   598	        println!("    auth:  {auth}");
   599	    }
   600	    match locality_tag(p) {
   601	        // `model:` padded to align its value under the Ollama `endpoint:` line.
   602	        Some(tag) => println!("    model:    {} ({}) [{tag}]", p.model, p.model_source),
   603	        None => println!("    model: {} ({})", p.model, p.model_source),
   604	    }
   605	}
   606	
   607	#[cfg(test)]
   608	mod tests {
   609	    use super::*;
   610	    use crate::config::{ConflictConfig, ProviderConfig};
   611	
   612	    fn cfg(default: ProviderId, providers: Vec<ProviderConfig>) -> Config {
   613	        Config {
   614	            version: 1,
   615	            default,
   616	            providers,
   617	            conflict: ConflictConfig::default(),
   618	        }
   619	    }
   620	

 succeeded in 0ms:
     1	//! Read-only configuration / provider introspection for `gcm status` (CLO-515).
     2	//!
     3	//! The command answers "what will gcm do right now, and why" without any network
     4	//! call, diff read, or LLM request. The hard part is **source attribution**: a
     5	//! value alone is not enough, so each provider reports where its key, model, and
     6	//! (for Ollama) endpoint came from.
     7	//!
     8	//! Attribution mirrors the precedence the runtime actually applies, computed here
     9	//! **without** calling [`crate::config::apply_to_env`] (which would copy inline
    10	//! config keys into the environment and destroy attribution):
    11	//!   * **key**:   env var (non-blank) > inline config `key` > not set
    12	//!   * **model**: `--model` flag (selected provider only) > per-provider env > default
    13	//!   * **selected provider**: `--provider` flag > `GCM_PROVIDER` > `config.default` > Groq
    14	//!   * **Ollama endpoint**: `GCM_OLLAMA_BASE_URL` > `OLLAMA_HOST` (normalized) >
    15	//!     config `endpoint` > default `http://localhost:11434`
    16	//!
    17	//! All attribution helpers are pure (they take the loaded config plus an
    18	//! `env_lookup` closure) so they are unit-testable without touching process env,
    19	//! mirroring the `config_path_from` / `env_plan(is_set)` style elsewhere.
    20	
    21	use std::path::PathBuf;
    22	
    23	use serde::Serialize;
    24	
    25	use crate::cli::Cli;
    26	use crate::config::{self, Config};
    27	use crate::output::SCHEMA_VERSION;
    28	use crate::provider::{ollama, resolve_model_with_source, AuthMethod, ModelSource, ProviderId};
    29	
    30	/// Canonical provider order for output (matches the wizard's `all_providers`).
    31	const PROVIDER_ORDER: [ProviderId; 6] = [
    32	    ProviderId::Groq,
    33	    ProviderId::Google,
    34	    ProviderId::Vertex,
    35	    ProviderId::Openai,
    36	    ProviderId::Anthropic,
    37	    ProviderId::Ollama,
    38	];
    39	
    40	/// The full `gcm status` payload. Versioned (`v`) like the commit `Envelope` but a
    41	/// distinct shape - it is NOT an `output::Envelope` (that enum is commit-only).
    42	/// JSON consumers should ignore unknown fields so this can grow without a `v` bump.
    43	#[derive(Debug, Serialize)]
    44	pub struct StatusReport {
    45	    pub v: i32,
    46	    pub version: &'static str,
    47	    pub paths: PathsStatus,
    48	    pub providers: Vec<ProviderStatus>,
    49	    /// Set only when `GCM_PROVIDER` holds an unknown value (reported, not fatal).
    50	    #[serde(skip_serializing_if = "Option::is_none")]
    51	    pub provider_error: Option<String>,
    52	}
    53	
    54	#[derive(Debug, Serialize)]
    55	pub struct PathsStatus {
    56	    /// `env var GCM_CONFIG` or `default dir`.
    57	    pub config_dir_source: String,
    58	    /// Resolved config directory (parent of the file), or `None` if no OS config
    59	    /// dir is available.
    60	    #[serde(skip_serializing_if = "Option::is_none")]
    61	    pub config_dir: Option<PathBuf>,
    62	    /// Resolved `config.toml` path, or `None` if no OS config dir is available.
    63	    #[serde(skip_serializing_if = "Option::is_none")]
    64	    pub config_file_path: Option<PathBuf>,
    65	    /// Whether the config file exists on disk.
    66	    pub config_file_exists: bool,
    67	    /// Whether the config file was parsed and is usable (`config::load` returned
    68	    /// `Some`). `false` while `config_file_exists` is `true` means the file is
    69	    /// present but malformed / wrong-version / insecure (a stderr warning was
    70	    /// emitted); the report falls back to env-derived state.
    71	    pub config_file_loaded: bool,
    72	}
    73	
    74	#[derive(Debug, Serialize)]
    75	pub struct ProviderStatus {
    76	    /// Canonical lowercase token (`groq`, `google`, ...).
    77	    pub name: ProviderId,
    78	    /// The effective selected provider this invocation (flag > env > config > groq).
    79	    pub selected: bool,
    80	    /// Whether the provider is activated (see [`is_activated`]).
    81	    pub activated: bool,
    82	    /// Key source for cloud providers; `None` for key-free Ollama.
    83	    #[serde(skip_serializing_if = "Option::is_none")]
    84	    pub key_source: Option<String>,
    85	    /// Ollama endpoint; `None` for cloud providers.
    86	    #[serde(skip_serializing_if = "Option::is_none")]
    87	    pub endpoint: Option<String>,
    88	    /// Ollama endpoint source; `None` for cloud providers.
    89	    #[serde(skip_serializing_if = "Option::is_none")]
    90	    pub endpoint_source: Option<String>,
    91	    /// Resolved effective model.
    92	    pub model: String,
    93	    /// Model source: `default` / `env var <NAME>` / `flag`.
    94	    pub model_source: String,
    95	    /// For Ollama only: false when the model routes off-machine (a cloud-tagged
    96	    /// `:cloud` / `-cloud` model); see [`ollama::is_cloud_model`].
    97	    #[serde(skip_serializing_if = "Option::is_none")]
    98	    pub zero_egress: Option<bool>,
    99	    /// Vertex only (CLO-537): GCP project as `value (source)`; `None` otherwise.
   100	    #[serde(skip_serializing_if = "Option::is_none")]
   101	    pub project: Option<String>,
   102	    /// Vertex only: location as `value (source)` (default `global`); `None` otherwise.
   103	    #[serde(skip_serializing_if = "Option::is_none")]
   104	    pub location: Option<String>,
   105	    /// Vertex only: inferred auth source (`GCM_VERTEX_TOKEN` or `gcloud ADC`). Never a
   106	    /// secret, and never verified by a gcloud call (status stays no-subprocess).
   107	    #[serde(skip_serializing_if = "Option::is_none")]
   108	    pub auth_source: Option<String>,
   109	}
   110	
   111	/// Entry point for the `status` subcommand. Pure introspection: loads the config
   112	/// and reads the environment, builds the report, prints it (JSON or human), and
   113	/// returns exit code 0 (misconfiguration is reported as fields, not a failure).
   114	/// A non-zero exit is reserved for a catastrophic internal error - per AC-9, a
   115	/// JSON serialization failure (infallible for these owned types in practice) is
   116	/// the one such case. Dispatched at the top of `run()` before any repo/provider/
   117	/// LLM work.
   118	pub fn run_status_subcommand(args: &Cli) -> i32 {
   119	    let config = config::load();
   120	    let report = build_report(
   121	        args.provider,
   122	        args.model.as_deref(),
   123	        config.as_ref(),
   124	        |var| std::env::var(var).ok(),
   125	    );
   126	
   127	    if args.json {
   128	        match serde_json::to_string(&report) {
   129	            Ok(json) => println!("{json}"),
   130	            Err(e) => {
   131	                // AC-9: catastrophic internal error -> stderr + non-zero exit.
   132	                eprintln!("gcm: error: could not serialize status report: {e}");
   133	                return 1;
   134	            }
   135	        }
   136	    } else {
   137	        print_human(&report);
   138	    }
   139	    0
   140	}
   141	
   142	/// Build the report from explicit inputs (pure; the body of
   143	/// [`run_status_subcommand`]), so the whole shape is unit-testable without env.
   144	fn build_report(
   145	    cli_provider: Option<ProviderId>,
   146	    cli_model: Option<&str>,
   147	    config: Option<&Config>,
   148	    env_lookup: impl Fn(&str) -> Option<String>,
   149	) -> StatusReport {
   150	    let paths = paths_status(&env_lookup, config::config_path(), config.is_some());
   151	    let (selected, provider_error) = selected_provider(cli_provider, config, &env_lookup);
   152	
   153	    let providers = PROVIDER_ORDER
   154	        .iter()
   155	        .map(|&id| {
   156	            let is_selected = id == selected;
   157	            // The --model flag applies ONLY to the selected provider; others
   158	            // resolve from env/default so they aren't mislabeled `flag`.
   159	            let model_flag = if is_selected { cli_model } else { None };
   160	            let (model, msrc) = resolve_model_with_source(id, model_flag, &env_lookup);
   161	            // If neither a flag nor an env var produced the model, a model set in
   162	            // the config file is what the next run will actually use (bridged via
   163	            // apply_to_env), so attribute it to the config rather than `default`.
   164	            let (model, model_source) = match msrc {
   165	                ModelSource::Default => match config_model(config, id) {
   166	                    Some(m) => (m, "config file".to_string()),
   167	                    None => (model, model_source_label(msrc)),
   168	                },
   169	                _ => (model, model_source_label(msrc)),
   170	            };
   171	
   172	            let (
   173	                key_source,
   174	                endpoint,
   175	                endpoint_source,
   176	                zero_egress,
   177	                project,
   178	                location,
   179	                auth_source,
   180	            ) = match id.auth_method() {
   181	                AuthMethod::KeylessEndpoint => {
   182	                    let (ep, src) = ollama_endpoint(config, &env_lookup);
   183	                    let zero = Some(!ollama::is_cloud_model(&model));
   184	                    (None, Some(ep), Some(src), zero, None, None, None)
   185	                }
   186	                AuthMethod::KeylessAdc => (
   187	                    None,
   188	                    None,
   189	                    None,
   190	                    None,
   191	                    vertex_project(config, &env_lookup),
   192	                    Some(vertex_location(config, &env_lookup)),
   193	                    Some(vertex_auth_source(&env_lookup)),
   194	                ),
   195	                AuthMethod::ApiKey => (
   196	                    Some(key_source(id, config, &env_lookup)),
   197	                    None,
   198	                    None,
   199	                    None,
   200	                    None,
   201	                    None,
   202	                    None,
   203	                ),
   204	            };
   205	
   206	            ProviderStatus {
   207	                name: id,
   208	                selected: is_selected,
   209	                activated: is_activated(id, config, &env_lookup),
   210	                key_source,
   211	                endpoint,
   212	                endpoint_source,
   213	                model,
   214	                model_source,
   215	                zero_egress,
   216	                project,
   217	                location,
   218	                auth_source,
   219	            }
   220	        })
   221	        .collect();
   222	
   223	    StatusReport {
   224	        v: SCHEMA_VERSION,
   225	        version: crate::cli::VERSION,
   226	        paths,
   227	        providers,
   228	        provider_error,
   229	    }
   230	}
   231	
   232	/// Resolve the config dir source, dir, path, existence, and whether a present
   233	/// file actually loaded. Handles the no-config-dir case gracefully (all `None` /
   234	/// `false`). `config_path` is injected (the resolved `config::config_path()`) so
   235	/// the no-OS-config-dir branch is unit-testable; `config_loaded` is whether
   236	/// `config::load` returned `Some`, so a present-but-unusable file is
   237	/// distinguishable from an absent one.
   238	fn paths_status(
   239	    env_lookup: &impl Fn(&str) -> Option<String>,
   240	    config_path: Option<PathBuf>,
   241	    config_loaded: bool,
   242	) -> PathsStatus {
   243	    let from_env = env_lookup("GCM_CONFIG")
   244	        .map(|v| v.trim().to_string())
   245	        .is_some_and(|v| !v.is_empty());
   246	    let config_dir_source = if from_env {
   247	        "env var GCM_CONFIG".to_string()
   248	    } else {
   249	        "default dir".to_string()
   250	    };
   251	    let path = config_path;
   252	    let config_dir = path.as_ref().and_then(|p| p.parent().map(PathBuf::from));
   253	    let config_file_exists = path.as_ref().is_some_and(|p| p.exists());
   254	    PathsStatus {
   255	        config_dir_source,
   256	        config_dir,
   257	        config_file_path: path,
   258	        config_file_exists,
   259	        config_file_loaded: config_loaded,
   260	    }
   261	}
   262	
   263	/// The effective selected provider and an optional error note. Precedence
   264	/// `--provider` flag > `GCM_PROVIDER` env > `config.default` > built-in `Groq`.
   265	/// An unknown non-blank `GCM_PROVIDER` is reported (not fatal): at runtime it
   266	/// would be a fatal config error regardless of `config.default` (the env bridge
   267	/// never overwrites a set `GCM_PROVIDER`), so the display falls back to `Groq`,
   268	/// not `config.default`, to avoid implying a selection the runtime would reject.
   269	fn selected_provider(
   270	    cli_provider: Option<ProviderId>,
   271	    config: Option<&Config>,
   272	    env_lookup: &impl Fn(&str) -> Option<String>,
   273	) -> (ProviderId, Option<String>) {
   274	    if let Some(id) = cli_provider {
   275	        return (id, None);
   276	    }
   277	    if let Some(raw) = env_lookup("GCM_PROVIDER")
   278	        .as_deref()
   279	        .map(str::trim)
   280	        .filter(|s| !s.is_empty())
   281	        .map(str::to_string)
   282	    {
   283	        return match ProviderId::parse(&raw) {
   284	            Some(id) => (id, None),
   285	            None => (
   286	                ProviderId::Groq,
   287	                Some(format!(
   288	                    "unknown provider '{raw}' in GCM_PROVIDER (valid: groq, google, vertex, \
   289	                     openai, anthropic, ollama); it would be a fatal error on a normal run - \
   290	                     showing groq as the display fallback"
   291	                )),
   292	            ),
   293	        };
   294	    }
   295	    if let Some(c) = config {
   296	        return (c.default, None);
   297	    }
   298	    (ProviderId::Groq, None)
   299	}
   300	
   301	/// Whether a provider is "activated". Cloud: listed in config OR its key env var
   302	/// is set & non-blank. Ollama: listed in config OR `OLLAMA_HOST` /
   303	/// `GCM_OLLAMA_BASE_URL` is set & non-blank (never "active by default").
   304	fn is_activated(
   305	    id: ProviderId,
   306	    config: Option<&Config>,
   307	    env_lookup: &impl Fn(&str) -> Option<String>,
   308	) -> bool {
   309	    if config.is_some_and(|c| c.providers.iter().any(|p| p.id == id)) {
   310	        return true;
   311	    }
   312	    match id {
   313	        ProviderId::Ollama => {
   314	            env_nonblank(env_lookup, "GCM_OLLAMA_BASE_URL")
   315	                || env_nonblank(env_lookup, "OLLAMA_HOST")
   316	        }
   317	        // Vertex (keyless): activated when a project resolves from the env (a
   318	        // config-listed Vertex already returned true above).
   319	        ProviderId::Vertex => {
   320	            env_nonblank(env_lookup, "GCM_VERTEX_PROJECT")
   321	                || env_nonblank(env_lookup, "GOOGLE_CLOUD_PROJECT")
   322	                || env_nonblank(env_lookup, "GCP_PROJECT")
   323	        }
   324	        _ => id
   325	            .key_env_var()
   326	            .is_some_and(|var| env_nonblank(env_lookup, var)),
   327	    }
   328	}
   329	
   330	/// Vertex GCP project as a `value (source)` display string (CLO-537): env precedence
   331	/// `GCM_VERTEX_PROJECT` > `GOOGLE_CLOUD_PROJECT` > `GCP_PROJECT` > inline config;
   332	/// `None` when unset.
   333	fn vertex_project(
   334	    config: Option<&Config>,
   335	    env_lookup: &impl Fn(&str) -> Option<String>,
   336	) -> Option<String> {
   337	    for (var, label) in [
   338	        ("GCM_VERTEX_PROJECT", "env var GCM_VERTEX_PROJECT"),
   339	        ("GOOGLE_CLOUD_PROJECT", "env var GOOGLE_CLOUD_PROJECT"),
   340	        ("GCP_PROJECT", "env var GCP_PROJECT"),
   341	    ] {
   342	        if let Some(v) = env_value(env_lookup, var) {
   343	            return Some(format!("{v} ({label})"));
   344	        }
   345	    }
   346	    config
   347	        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Vertex))
   348	        .and_then(|pc| pc.project.as_deref())
   349	        .map(str::trim)
   350	        .filter(|p| !p.is_empty())
   351	        .map(|p| format!("{p} (config file)"))
   352	}
   353	
   354	/// Vertex location as a `value (source)` display string; env precedence
   355	/// `GCM_VERTEX_LOCATION` > `GOOGLE_CLOUD_LOCATION` > `GCP_REGION` > inline config,
   356	/// else the effective default `global`.
   357	fn vertex_location(
   358	    config: Option<&Config>,
   359	    env_lookup: &impl Fn(&str) -> Option<String>,
   360	) -> String {
   361	    for (var, label) in [
   362	        ("GCM_VERTEX_LOCATION", "env var GCM_VERTEX_LOCATION"),
   363	        ("GOOGLE_CLOUD_LOCATION", "env var GOOGLE_CLOUD_LOCATION"),
   364	        ("GCP_REGION", "env var GCP_REGION"),
   365	    ] {
   366	        if let Some(v) = env_value(env_lookup, var) {
   367	            return format!("{v} ({label})");
   368	        }
   369	    }
   370	    if let Some(loc) = config
   371	        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Vertex))
   372	        .and_then(|pc| pc.location.as_deref())
   373	        .map(str::trim)
   374	        .filter(|l| !l.is_empty())
   375	    {
   376	        return format!("{loc} (config file)");
   377	    }
   378	    "global (default)".to_string()
   379	}
   380	
   381	/// Vertex auth source, inferred WITHOUT a gcloud call (status stays no-subprocess):
   382	/// `GCM_VERTEX_TOKEN` if that env var is set, else `gcloud ADC`. Reflects which path
   383	/// would be used, not that a token was actually obtained. Never a secret.
   384	fn vertex_auth_source(env_lookup: &impl Fn(&str) -> Option<String>) -> String {
   385	    if env_nonblank(env_lookup, "GCM_VERTEX_TOKEN") {
   386	        "GCM_VERTEX_TOKEN".to_string()
   387	    } else {
   388	        "gcloud ADC".to_string()
   389	    }
   390	}
   391	
   392	/// Key source for a cloud provider, applying env > inline-config precedence (the
   393	/// effective runtime precedence, since the env bridge only fills an unset var).
   394	fn key_source(
   395	    id: ProviderId,
   396	    config: Option<&Config>,
   397	    env_lookup: &impl Fn(&str) -> Option<String>,
   398	) -> String {
   399	    if let Some(var) = id.key_env_var() {
   400	        if env_nonblank(env_lookup, var) {
   401	            return format!("env var {var}");
   402	        }
   403	    }
   404	    // A blank inline key is treated as "not set" to match the runtime: `env_plan`
   405	    // trims and filters empty inline keys before bridging them.
   406	    let inline_nonblank = config
   407	        .and_then(|c| c.providers.iter().find(|p| p.id == id))
   408	        .and_then(|pc| pc.key.as_deref())
   409	        .map(str::trim)
   410	        .is_some_and(|k| !k.is_empty());
   411	    if inline_nonblank {
   412	        return "config file".to_string();
   413	    }
   414	    "not set".to_string()
   415	}
   416	
   417	/// A provider's non-blank `model` from the loaded config, if any. Blank/whitespace
   418	/// is treated as unset, matching the `env_plan` bridge that trims before applying.
   419	fn config_model(config: Option<&Config>, id: ProviderId) -> Option<String> {
   420	    config
   421	        .and_then(|c| c.providers.iter().find(|p| p.id == id))
   422	        .and_then(|pc| pc.model.as_deref())
   423	        .map(str::trim)
   424	        .filter(|m| !m.is_empty())
   425	        .map(String::from)
   426	}
   427	
   428	/// Resolve the Ollama endpoint and its source without calling `apply_to_env`.
   429	fn ollama_endpoint(
   430	    config: Option<&Config>,

exec
/bin/zsh -lc "nl -ba tests/status.rs | sed -n '1,470p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
     1	//! `gcm status` integration tests (CLO-515). Each test drives the built binary as
     2	//! a subprocess with a cleared provider environment and a throwaway `GCM_CONFIG`
     3	//! dir, so attribution is deterministic and hermetic. `gcm status` is read-only:
     4	//! it needs no git repo and makes no network/LLM call, so tests run in a plain
     5	//! temp dir and never hang.
     6	
     7	use std::fs;
     8	use std::path::Path;
     9	use std::process::{Command, Output, Stdio};
    10	
    11	/// Every provider/model/endpoint env var cleared so attribution starts from a
    12	/// known-empty baseline; individual tests re-add only what they assert on.
    13	const CLEARED_ENV: &[&str] = &[
    14	    "GROQ_API_KEY",
    15	    "GEMINI_API_KEY",
    16	    "OPENAI_API_KEY",
    17	    "ANTHROPIC_API_KEY",
    18	    "GCM_PROVIDER",
    19	    "OLLAMA_HOST",
    20	    "GCM_OLLAMA_BASE_URL",
    21	    "GCM_GROQ_MODEL",
    22	    "GCM_GEMINI_MODEL",
    23	    "GCM_GOOGLE_MODEL",
    24	    "GCM_OPENAI_MODEL",
    25	    "GCM_ANTHROPIC_MODEL",
    26	    "GCM_OLLAMA_MODEL",
    27	];
    28	
    29	/// Run `gcm` with a cleared provider env, `GCM_CONFIG` pointed at `config_dir`,
    30	/// plus any `extra_env` (name, value) pairs. The working directory is set to a
    31	/// throwaway non-git temp dir so the test also proves `gcm status` works outside
    32	/// any repository (AC-1). Returns the captured output.
    33	fn run_status(config_dir: &Path, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
    34	    let cwd = tempfile::tempdir().expect("cwd tempdir"); // not a git repo
    35	    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    36	    cmd.current_dir(cwd.path())
    37	        .args(args)
    38	        .env("GCM_CONFIG", config_dir)
    39	        .stdin(Stdio::null())
    40	        .stdout(Stdio::piped())
    41	        .stderr(Stdio::piped());
    42	    for var in CLEARED_ENV {
    43	        cmd.env_remove(var);
    44	    }
    45	    for (k, v) in extra_env {
    46	        cmd.env(k, v);
    47	    }
    48	    cmd.output().expect("run gcm status")
    49	}
    50	
    51	fn stdout_of(out: &Output) -> String {
    52	    String::from_utf8_lossy(&out.stdout).to_string()
    53	}
    54	
    55	/// Write a 0600 config.toml into `dir` (the GCM_CONFIG dir).
    56	fn write_config(dir: &Path, body: &str) {
    57	    let path = dir.join("config.toml");
    58	    fs::write(&path, body).unwrap();
    59	    #[cfg(unix)]
    60	    {
    61	        use std::os::unix::fs::PermissionsExt;
    62	        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    63	    }
    64	}
    65	
    66	#[test]
    67	fn status_no_config_clean_env_exits_zero() {
    68	    let cfg = tempfile::tempdir().unwrap();
    69	    let out = run_status(cfg.path(), &["status"], &[]);
    70	    assert!(out.status.success(), "exit 0");
    71	    let stdout = stdout_of(&out);
    72	    assert!(stdout.contains("no config file"), "{stdout}");
    73	    // every cloud provider with no key -> not activated, not set
    74	    assert!(!stdout.contains("groq [selected, activated]"), "{stdout}");
    75	    assert!(stdout.contains("not set"), "{stdout}");
    76	    // default models surface
    77	    assert!(stdout.contains("openai/gpt-oss-120b (default)"), "{stdout}");
    78	    assert!(
    79	        stdout.contains("gemini-3.1-flash-lite (default)"),
    80	        "{stdout}"
    81	    );
    82	}
    83	
    84	#[test]
    85	fn status_env_key_and_model_attribution() {
    86	    let cfg = tempfile::tempdir().unwrap();
    87	    let out = run_status(
    88	        cfg.path(),
    89	        &["status"],
    90	        &[("GROQ_API_KEY", "sk-secret123"), ("GCM_GROQ_MODEL", "m-x")],
    91	    );
    92	    assert!(out.status.success());
    93	    let stdout = stdout_of(&out);
    94	    assert!(stdout.contains("key:   env var GROQ_API_KEY"), "{stdout}");
    95	    assert!(
    96	        stdout.contains("model: m-x (env var GCM_GROQ_MODEL)"),
    97	        "{stdout}"
    98	    );
    99	}
   100	
   101	#[test]
   102	fn status_never_prints_raw_secret() {
   103	    let cfg = tempfile::tempdir().unwrap();
   104	    // human mode
   105	    let out = run_status(cfg.path(), &["status"], &[("GROQ_API_KEY", "sk-secret123")]);
   106	    assert!(
   107	        !stdout_of(&out).contains("sk-secret123"),
   108	        "human leaked secret"
   109	    );
   110	    // json mode
   111	    let out = run_status(
   112	        cfg.path(),
   113	        &["status", "--json"],
   114	        &[("GROQ_API_KEY", "sk-secret123")],
   115	    );
   116	    assert!(
   117	        !stdout_of(&out).contains("sk-secret123"),
   118	        "json leaked secret"
   119	    );
   120	}
   121	
   122	#[test]
   123	fn status_mixed_inline_and_env_key_attribution() {
   124	    let cfg = tempfile::tempdir().unwrap();
   125	    // groq has an inline key; openai relies on the env var
   126	    write_config(
   127	        cfg.path(),
   128	        "version = 1\n\
   129	         default = \"groq\"\n\
   130	         \n\
   131	         [[providers]]\n\
   132	         id = \"groq\"\n\
   133	         key = \"sk-inline-xyz\"\n\
   134	         \n\
   135	         [[providers]]\n\
   136	         id = \"openai\"\n",
   137	    );
   138	    let out = run_status(
   139	        cfg.path(),
   140	        &["status", "--json"],
   141	        &[("OPENAI_API_KEY", "sk-env-openai")],
   142	    );
   143	    assert!(out.status.success());
   144	    let stdout = stdout_of(&out);
   145	    assert!(
   146	        !stdout.contains("sk-inline-xyz"),
   147	        "inline key leaked: {stdout}"
   148	    );
   149	    assert!(
   150	        !stdout.contains("sk-env-openai"),
   151	        "env key leaked: {stdout}"
   152	    );
   153	    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
   154	    let providers = json["providers"].as_array().unwrap();
   155	    let groq = providers.iter().find(|p| p["name"] == "groq").unwrap();
   156	    let openai = providers.iter().find(|p| p["name"] == "openai").unwrap();
   157	    assert_eq!(groq["key_source"], "config file");
   158	    assert_eq!(openai["key_source"], "env var OPENAI_API_KEY");
   159	    // config.default = groq -> groq is the selected provider
   160	    assert_eq!(groq["selected"], true);
   161	}
   162	
   163	#[test]
   164	fn status_json_valid_both_flag_positions() {
   165	    let cfg = tempfile::tempdir().unwrap();
   166	    for args in [["status", "--json"], ["--json", "status"]] {
   167	        let out = run_status(cfg.path(), &args, &[]);
   168	        assert!(out.status.success(), "exit 0 for {args:?}");
   169	        let json: serde_json::Value = serde_json::from_slice(&out.stdout)
   170	            .unwrap_or_else(|e| panic!("invalid JSON for {args:?}: {e}"));
   171	        assert_eq!(json["v"], 1);
   172	        assert_eq!(json["providers"].as_array().unwrap().len(), 6);
   173	    }
   174	}
   175	
   176	#[test]
   177	fn status_ollama_endpoint_source() {
   178	    let cfg = tempfile::tempdir().unwrap();
   179	    let out = run_status(cfg.path(), &["status"], &[("OLLAMA_HOST", "remote:8080")]);
   180	    assert!(out.status.success());
   181	    let stdout = stdout_of(&out);
   182	    assert!(
   183	        stdout.contains("endpoint: http://remote:8080 (env var OLLAMA_HOST)"),
   184	        "{stdout}"
   185	    );
   186	}
   187	
   188	#[test]
   189	fn status_model_flag_scoped_to_selected_provider() {
   190	    let cfg = tempfile::tempdir().unwrap();
   191	    // --provider openai --model custom: only openai reports flag
   192	    let out = run_status(
   193	        cfg.path(),
   194	        &["--provider", "openai", "--model", "custom-model", "status"],
   195	        &[],
   196	    );
   197	    assert!(out.status.success());
   198	    let stdout = stdout_of(&out);
   199	    // headline names the selected provider; its block carries the `>` marker
   200	    assert!(stdout.contains("Selected (gcm will use this):"), "{stdout}");
   201	    assert!(stdout.contains("> openai"), "{stdout}");
   202	    assert!(stdout.contains("model: custom-model (flag)"), "{stdout}");
   203	    // de-noised: the old bracket tags are gone
   204	    assert!(!stdout.contains("[selected"), "{stdout}");
   205	    assert!(!stdout.contains("[activated]"), "{stdout}");
   206	    // other providers keep their defaults
   207	    assert!(stdout.contains("claude-haiku-4-5 (default)"), "{stdout}");
   208	}
   209	
   210	#[test]
   211	fn status_human_layout_groups_and_cloud_tag() {
   212	    let cfg = tempfile::tempdir().unwrap();
   213	    // ollama is config.default (selected) with a -cloud model; groq is activated via key.
   214	    write_config(
   215	        cfg.path(),
   216	        "version = 1\n\
   217	         default = \"ollama\"\n\
   218	         \n\
   219	         [[providers]]\n\
   220	         id = \"ollama\"\n\
   221	         model = \"nemotron-3-nano:30b-cloud\"\n",
   222	    );
   223	    let out = run_status(cfg.path(), &["status"], &[("GROQ_API_KEY", "sk-x")]);
   224	    assert!(out.status.success());
   225	    let stdout = stdout_of(&out);
   226	    // headline: selected provider + model + neutral cloud tag (no warning framing)
   227	    assert!(
   228	        stdout.contains("ollama -> nemotron-3-nano:30b-cloud (config file) [cloud]"),
   229	        "{stdout}"
   230	    );
   231	    assert!(!stdout.contains("not zero-egress"), "{stdout}");
   232	    // both grouped sections present; selected ollama leads Activated, marked
   233	    assert!(stdout.contains("Activated:"), "{stdout}");
   234	    assert!(stdout.contains("Not activated:"), "{stdout}");
   235	    assert!(stdout.contains("> ollama"), "{stdout}");
   236	    // anthropic has no key -> Not activated section
   237	    assert!(stdout.contains("anthropic"), "{stdout}");
   238	}
   239	
   240	#[test]
   241	fn status_invalid_gcm_provider_headline_does_not_claim_use() {
   242	    // GCM_PROVIDER=bogus is fatal at runtime; groq is only a display fallback.
   243	    // Even with GROQ_API_KEY set, the headline must NOT assert groq will be used.
   244	    let cfg = tempfile::tempdir().unwrap();
   245	    let out = run_status(
   246	        cfg.path(),
   247	        &["status"],
   248	        &[("GCM_PROVIDER", "bogus"), ("GROQ_API_KEY", "sk-x")],
   249	    );
   250	    assert!(
   251	        out.status.success(),
   252	        "invalid provider is not fatal for status"
   253	    );
   254	    let stdout = stdout_of(&out);
   255	    assert!(stdout.contains("Selected (gcm will use this):"), "{stdout}");
   256	    // headline reports no usable selection and points to the warning...
   257	    assert!(stdout.contains("(none -"), "{stdout}");
   258	    assert!(stdout.contains("the next run would fail"), "{stdout}");
   259	    // ...and never claims the groq fallback as what the next run uses
   260	    // (the `->` arrow appears only in the headline, never in the section blocks).
   261	    assert!(!stdout.contains("groq ->"), "{stdout}");
   262	    // the explanatory warning still prints
   263	    assert!(
   264	        stdout.contains("Warning:") && stdout.contains("bogus"),
   265	        "{stdout}"
   266	    );
   267	}
   268	
   269	#[test]
   270	fn status_ollama_selected_unconfigured_does_not_claim_error() {
   271	    // --provider ollama on a clean machine: not "activated" (no config entry, no
   272	    // endpoint env), but Ollama is key-free and defaults to the local daemon, so a
   273	    // real run can succeed. The headline must NOT claim it would error.
   274	    let cfg = tempfile::tempdir().unwrap();
   275	    let out = run_status(cfg.path(), &["--provider", "ollama", "status"], &[]);
   276	    assert!(out.status.success());
   277	    let stdout = stdout_of(&out);
   278	    assert!(stdout.contains("Selected (gcm will use this):"), "{stdout}");
   279	    // ollama is named as the selection...
   280	    assert!(stdout.contains("ollama ->"), "{stdout}");
   281	    // ...with the truthful local-daemon note, not an error claim
   282	    assert!(
   283	        stdout.contains("will try the local Ollama daemon"),
   284	        "{stdout}"
   285	    );
   286	    assert!(!stdout.contains("would error on a real run"), "{stdout}");
   287	}
   288	
   289	#[test]
   290	fn status_invalid_gcm_provider_reported_exit_zero() {
   291	    let cfg = tempfile::tempdir().unwrap();
   292	    let out = run_status(
   293	        cfg.path(),
   294	        &["status", "--json"],
   295	        &[("GCM_PROVIDER", "bogus")],
   296	    );
   297	    assert!(out.status.success(), "invalid provider is not fatal");
   298	    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
   299	    let err = json["provider_error"].as_str().expect("provider_error set");
   300	    assert!(err.contains("bogus"), "{err}");
   301	    // falls back to groq as the displayed selection
   302	    let groq = json["providers"]
   303	        .as_array()
   304	        .unwrap()
   305	        .iter()
   306	        .find(|p| p["name"] == "groq")
   307	        .unwrap();
   308	    assert_eq!(groq["selected"], true);
   309	}
   310	
   311	#[test]
   312	fn status_malformed_config_falls_back_to_env_state() {
   313	    let cfg = tempfile::tempdir().unwrap();
   314	    write_config(cfg.path(), "this is not = valid toml [");
   315	    let out = run_status(cfg.path(), &["status", "--json"], &[]);
   316	    assert!(out.status.success(), "malformed config is not fatal");
   317	    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
   318	    assert_eq!(json["v"], 1);
   319	    assert_eq!(json["providers"].as_array().unwrap().len(), 6);
   320	    // machine-readable: the file exists but is not usable (distinguishable from absent)
   321	    assert_eq!(json["paths"]["config_file_exists"], true);
   322	    assert_eq!(json["paths"]["config_file_loaded"], false);
   323	}
   324	
   325	#[test]
   326	fn status_reports_config_dir_and_loaded_state() {
   327	    // AC-2: resolved config dir is present; a good config loads.
   328	    let cfg = tempfile::tempdir().unwrap();
   329	    write_config(
   330	        cfg.path(),
   331	        "version = 1\ndefault = \"groq\"\n\n[[providers]]\nid = \"groq\"\n",
   332	    );
   333	    let out = run_status(cfg.path(), &["status", "--json"], &[]);
   334	    assert!(out.status.success());
   335	    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
   336	    assert_eq!(json["paths"]["config_dir"], cfg.path().to_str().unwrap());
   337	    assert_eq!(json["paths"]["config_dir_source"], "env var GCM_CONFIG");
   338	    assert_eq!(json["paths"]["config_file_loaded"], true);
   339	}
   340	
   341	#[test]
   342	fn status_help_lists_subcommand() {
   343	    // AC-8: the subcommand is discoverable in help output.
   344	    let cfg = tempfile::tempdir().unwrap();
   345	    let out = run_status(cfg.path(), &["--help"], &[]);
   346	    assert!(out.status.success());
   347	    assert!(stdout_of(&out).contains("status"), "status in --help");
   348	    // `gcm status --help` also works (subcommand help)
   349	    let out = run_status(cfg.path(), &["status", "--help"], &[]);
   350	    assert!(out.status.success());
   351	}
   352	
   353	#[test]
   354	fn status_invalid_gcm_provider_with_config_default_still_shows_groq() {
   355	    // config.default = openai, but GCM_PROVIDER=bogus -> groq displayed + error.
   356	    let cfg = tempfile::tempdir().unwrap();
   357	    write_config(
   358	        cfg.path(),
   359	        "version = 1\ndefault = \"openai\"\n\n[[providers]]\nid = \"openai\"\n",
   360	    );
   361	    let out = run_status(
   362	        cfg.path(),
   363	        &["status", "--json"],
   364	        &[("GCM_PROVIDER", "bogus")],
   365	    );
   366	    assert!(out.status.success());
   367	    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
   368	    assert!(json["provider_error"].as_str().unwrap().contains("bogus"));
   369	    let providers = json["providers"].as_array().unwrap();
   370	    let groq = providers.iter().find(|p| p["name"] == "groq").unwrap();
   371	    let openai = providers.iter().find(|p| p["name"] == "openai").unwrap();
   372	    assert_eq!(groq["selected"], true, "groq is the display fallback");
   373	    assert_eq!(
   374	        openai["selected"], false,
   375	        "config.default is NOT shown selected"
   376	    );
   377	}
   378	
   379	#[test]
   380	fn status_config_default_drives_selection_without_env() {
   381	    let cfg = tempfile::tempdir().unwrap();
   382	    write_config(
   383	        cfg.path(),
   384	        "version = 1\n\
   385	         default = \"openai\"\n\
   386	         \n\
   387	         [[providers]]\n\
   388	         id = \"openai\"\n",
   389	    );
   390	    // no --provider, no GCM_PROVIDER -> config.default (openai) is selected
   391	    let out = run_status(cfg.path(), &["status", "--json"], &[]);
   392	    assert!(out.status.success());
   393	    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
   394	    let openai = json["providers"]
   395	        .as_array()
   396	        .unwrap()
   397	        .iter()
   398	        .find(|p| p["name"] == "openai")
   399	        .unwrap();
   400	    assert_eq!(
   401	        openai["selected"], true,
   402	        "config.default drives selection: {json}"
   403	    );
   404	}
   405	
   406	#[test]
   407	fn status_google_dual_env_precedence() {
   408	    let cfg = tempfile::tempdir().unwrap();
   409	    let out = run_status(
   410	        cfg.path(),
   411	        &["status", "--json"],
   412	        &[
   413	            ("GCM_GEMINI_MODEL", "gem-a"),
   414	            ("GCM_GOOGLE_MODEL", "goog-b"),
   415	        ],
   416	    );
   417	    assert!(out.status.success());
   418	    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
   419	    let google = json["providers"]
   420	        .as_array()
   421	        .unwrap()
   422	        .iter()
   423	        .find(|p| p["name"] == "google")
   424	        .unwrap();
   425	    assert_eq!(google["model"], "gem-a");
   426	    assert_eq!(google["model_source"], "env var GCM_GEMINI_MODEL");
   427	}

exec
/bin/zsh -lc "nl -ba tests/status.rs | sed -n '470,620p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
exec
/bin/zsh -lc "nl -ba tests/vertex.rs | sed -n '1,320p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
     1	//! End-to-end acceptance test for the Vertex AI provider (CLO-537).
     2	//!
     3	//! Drives the built `gcm` binary against a local mock `generateContent` server via
     4	//! `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN`, so the full Vertex HTTP path is
     5	//! exercised hermetically (no gcloud, no network): `request()` builds the Vertex URL,
     6	//! sends the Bearer token via `extra_headers` with `auth: None`, `post_json` performs
     7	//! the round-trip, and `gemini::extract_text` parses the Gemini-shaped response. The
     8	//! live variant (a real 200 against a GCP project) is the HITL step, out of scope here.
     9	
    10	use std::fs;
    11	use std::io::{Read, Write};
    12	use std::net::TcpListener;
    13	use std::path::Path;
    14	use std::process::{Command, Output, Stdio};
    15	use std::thread;
    16	
    17	/// Env vars scrubbed before each run so a developer's real Vertex/GCP config can't
    18	/// leak into the hermetic test.
    19	const SCRUB_ENV: &[&str] = &[
    20	    "GROQ_API_KEY",
    21	    "GEMINI_API_KEY",
    22	    "OPENAI_API_KEY",
    23	    "ANTHROPIC_API_KEY",
    24	    "GCM_PROVIDER",
    25	    "GCM_VERTEX_PROJECT",
    26	    "GCM_VERTEX_LOCATION",
    27	    "GCM_VERTEX_TOKEN",
    28	    "GCM_VERTEX_BASE_URL",
    29	    "GOOGLE_CLOUD_PROJECT",
    30	    "GCP_PROJECT",
    31	    "GOOGLE_CLOUD_LOCATION",
    32	    "GCP_REGION",
    33	];
    34	
    35	fn git_init(dir: &Path) {
    36	    for args in [
    37	        vec!["init", "-q"],
    38	        vec!["config", "user.email", "t@example.com"],
    39	        vec!["config", "user.name", "Test"],
    40	    ] {
    41	        assert!(
    42	            Command::new("git")
    43	                .args(&args)
    44	                .current_dir(dir)
    45	                .status()
    46	                .expect("run git")
    47	                .success(),
    48	            "git {args:?} failed"
    49	        );
    50	    }
    51	}
    52	
    53	fn write_config(dir: &Path, body: &str) {
    54	    let path = dir.join("config.toml");
    55	    fs::write(&path, body).unwrap();
    56	    #[cfg(unix)]
    57	    {
    58	        use std::os::unix::fs::PermissionsExt;
    59	        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    60	    }
    61	}
    62	
    63	fn run_gcm(repo: &Path, config_dir: &Path, extra_env: &[(&str, &str)], args: &[&str]) -> Output {
    64	    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    65	    cmd.current_dir(repo)
    66	        .args(args)
    67	        .env("GCM_CONFIG", config_dir)
    68	        .env_remove("GCM_DEBUG")
    69	        .stdin(Stdio::null());
    70	    for var in SCRUB_ENV {
    71	        cmd.env_remove(var);
    72	    }
    73	    for (k, v) in extra_env {
    74	        cmd.env(k, v);
    75	    }
    76	    cmd.output().expect("run gcm")
    77	}
    78	
    79	/// A Gemini-shaped `generateContent` response whose single text part carries the
    80	/// resolve JSON (`{"resolutions":[{hunk_index, replacement}]}`), matching what
    81	/// `gemini::extract_text` (reused by Vertex) expects.
    82	fn mock_vertex_resolve_response(replacement: &str) -> String {
    83	    let inner = serde_json::json!({
    84	        "resolutions": [{ "hunk_index": 0, "replacement": replacement }]
    85	    })
    86	    .to_string();
    87	    serde_json::json!({
    88	        "candidates": [{
    89	            "content": { "parts": [{ "text": inner }] },
    90	            "finishReason": "STOP"
    91	        }]
    92	    })
    93	    .to_string()
    94	}
    95	
    96	/// Serve `body` once on a random port, capturing the request line/headers so the test
    97	/// can assert the Vertex URL + Bearer header were actually sent. Returns
    98	/// `(base_url, handle)`; `handle.join()` yields the captured request bytes.
    99	fn mock_server(body: String) -> (String, thread::JoinHandle<String>) {
   100	    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
   101	    let port = listener.local_addr().unwrap().port();
   102	    let handle = thread::spawn(move || {
   103	        listener.set_nonblocking(true).ok();
   104	        let start = std::time::Instant::now();
   105	        loop {
   106	            match listener.accept() {
   107	                Ok((mut stream, _)) => {
   108	                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
   109	                    let mut buf = [0u8; 8192];
   110	                    let n = stream.read(&mut buf).unwrap_or(0);
   111	                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
   112	                    let response = format!(
   113	                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
   114	                        body.len(),
   115	                        body
   116	                    );
   117	                    let _ = stream.write_all(response.as_bytes());
   118	                    return req;
   119	                }
   120	                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
   121	                    if start.elapsed() > std::time::Duration::from_secs(10) {
   122	                        return String::new();
   123	                    }
   124	                    thread::sleep(std::time::Duration::from_millis(50));
   125	                }
   126	                Err(_) => return String::new(),
   127	            }
   128	        }
   129	    });
   130	    (format!("http://127.0.0.1:{port}"), handle)
   131	}
   132	
   133	fn git(repo: &Path, args: &[&str]) {
   134	    assert!(
   135	        Command::new("git")
   136	            .args(args)
   137	            .current_dir(repo)
   138	            .output()
   139	            .expect("run git")
   140	            .status
   141	            .success(),
   142	        "git {args:?} failed"
   143	    );
   144	}
   145	
   146	/// A real 3-way conflict on `f.txt` (base/feature/mainline all differ), so the
   147	/// resolver classifies it Complex and calls the provider.
   148	fn create_conflict(repo: &Path) {
   149	    fs::write(repo.join("f.txt"), "base\n").unwrap();
   150	    git(repo, &["add", "-A"]);
   151	    git(repo, &["commit", "-q", "-m", "base"]);
   152	    let base = String::from_utf8_lossy(
   153	        &Command::new("git")
   154	            .args(["branch", "--show-current"])
   155	            .current_dir(repo)
   156	            .output()
   157	            .unwrap()
   158	            .stdout,
   159	    )
   160	    .trim()
   161	    .to_string();
   162	    git(repo, &["switch", "-q", "-c", "feature"]);
   163	    fs::write(repo.join("f.txt"), "feature\n").unwrap();
   164	    git(repo, &["commit", "-qam", "feature"]);
   165	    git(repo, &["switch", "-q", &base]);
   166	    fs::write(repo.join("f.txt"), "mainline\n").unwrap();
   167	    git(repo, &["commit", "-qam", "mainline"]);
   168	    // Expected to fail (leaves conflict markers) - that's the state resolve acts on.
   169	    let _ = Command::new("git")
   170	        .args(["merge", "feature"])
   171	        .current_dir(repo)
   172	        .output();
   173	}
   174	
   175	const VERTEX_CONFIG: &str = r#"version = 2
   176	default = "vertex"
   177	
   178	[[providers]]
   179	id = "vertex"
   180	project = "test-proj"
   181	"#;
   182	
   183	#[test]
   184	fn resolve_via_vertex_hits_mock_generatecontent_with_bearer_and_resolves() {
   185	    let dir = tempfile::tempdir().unwrap();
   186	    let repo = dir.path();
   187	    git_init(repo);
   188	    create_conflict(repo);
   189	
   190	    let (url, server) = mock_server(mock_vertex_resolve_response("RESOLVED_BY_VERTEX\n"));
   191	
   192	    let cfg_dir = tempfile::tempdir().unwrap();
   193	    write_config(cfg_dir.path(), VERTEX_CONFIG);
   194	
   195	    let out = run_gcm(
   196	        repo,
   197	        cfg_dir.path(),
   198	        &[
   199	            ("GCM_VERTEX_BASE_URL", &url),
   200	            ("GCM_VERTEX_TOKEN", "fake-adc-token"),
   201	        ],
   202	        &["resolve", "--yes", "--provider", "vertex"],
   203	    );
   204	    let request = server.join().unwrap();
   205	
   206	    assert!(
   207	        out.status.success(),
   208	        "gcm resolve --provider vertex failed.\nstdout: {}\nstderr: {}",
   209	        String::from_utf8_lossy(&out.stdout),
   210	        String::from_utf8_lossy(&out.stderr)
   211	    );
   212	
   213	    // The Vertex request actually reached the server with the right URL shape + auth.
   214	    assert!(
   215	        request.contains("/v1/projects/test-proj/locations/global/publishers/google/models/"),
   216	        "request line missing Vertex URL path: {request}"
   217	    );
   218	    assert!(
   219	        request.contains(":generateContent"),
   220	        "request missing :generateContent: {request}"
   221	    );
   222	    assert!(
   223	        request
   224	            .to_lowercase()
   225	            .contains("authorization: bearer fake-adc-token"),
   226	        "request missing Bearer token header: {request}"
   227	    );
   228	
   229	    // The provider's resolution was applied: markers gone, replacement present.
   230	    let after = fs::read_to_string(repo.join("f.txt")).unwrap();
   231	    assert!(
   232	        !after.contains("<<<<<<<") && !after.contains(">>>>>>>"),
   233	        "conflict markers should be gone after resolve: {after:?}"
   234	    );
   235	    assert!(
   236	        after.contains("RESOLVED_BY_VERTEX"),
   237	        "resolved content should come from the mock Vertex response: {after:?}"
   238	    );
   239	}
   240	
   241	/// Resolve a tool's absolute path from the current PATH (via `command -v`).
   242	#[cfg(unix)]
   243	fn tool_path(name: &str) -> String {
   244	    let out = Command::new("sh")
   245	        .args(["-c", &format!("command -v {name}")])
   246	        .output()
   247	        .expect("run sh");
   248	    String::from_utf8_lossy(&out.stdout).trim().to_string()
   249	}
   250	
   251	#[cfg(unix)]
   252	#[test]
   253	fn vertex_missing_gcloud_and_token_is_actionable_not_a_panic() {
   254	    // AC-3: no GCM_VERTEX_TOKEN and gcloud absent -> an actionable, typed error (no
   255	    // panic, no "check <env_var>"). Build a clean bin dir that has git + sh symlinked
   256	    // (so gcm's own git work still runs) but NOT gcloud, then point PATH there.
   257	    use std::os::unix::fs::symlink;
   258	
   259	    let git = tool_path("git");
   260	    let sh = tool_path("sh");
   261	    if git.is_empty() || sh.is_empty() {
   262	        return; // environment without a resolvable git/sh; skip (never on CI).
   263	    }
   264	
   265	    let dir = tempfile::tempdir().unwrap();
   266	    let repo = dir.path();
   267	    git_init(repo);
   268	    create_conflict(repo);
   269	
   270	    let bin = tempfile::tempdir().unwrap();
   271	    symlink(&git, bin.path().join("git")).unwrap();
   272	    symlink(&sh, bin.path().join("sh")).unwrap();
   273	
   274	    let cfg_dir = tempfile::tempdir().unwrap();
   275	    write_config(cfg_dir.path(), VERTEX_CONFIG);
   276	
   277	    let out = run_gcm(
   278	        repo,
   279	        cfg_dir.path(),
   280	        &[("PATH", &bin.path().to_string_lossy())],
   281	        &["resolve", "--yes", "--provider", "vertex"],
   282	    );
   283	    let combined = format!(
   284	        "{}{}",
   285	        String::from_utf8_lossy(&out.stdout),
   286	        String::from_utf8_lossy(&out.stderr)
   287	    );
   288	    assert!(
   289	        !out.status.success(),
   290	        "expected failure without a token: {combined}"
   291	    );
   292	    assert!(!combined.contains("panicked"), "must not panic: {combined}");
   293	    assert!(
   294	        combined.contains("gcloud") || combined.contains("GCM_VERTEX_TOKEN"),
   295	        "error should mention gcloud/ADC or the token env var: {combined}"
   296	    );
   297	}

exec
/bin/zsh -lc "nl -ba src/provider/gemini.rs | sed -n '1,220p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
     1	//! Google Gemini backend (CLO-489). Divergent from the OpenAI-compatible shape:
     2	//! the `:generateContent` endpoint, `x-goog-api-key` auth, structured output via
     3	//! `generationConfig.responseSchema` (OpenAPI-3.0 subset, [`crate::plan::gemini_schema`]),
     4	//! and reasoning suppression via `thinkingConfig.thinkingLevel` (3.x floor is
     5	//! `MINIMAL`; no hard off). The response extractor checks `finishReason` /
     6	//! `promptFeedback` for safety blocks BEFORE reading content (round-2 review pt 3).
     7	
     8	use serde::Deserialize;
     9	use serde_json::{json, Value};
    10	
    11	use super::http::{self, HttpRequest};
    12	use super::{ErrorKind, Provider, ProviderError};
    13	use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
    14	use crate::plan::Plan;
    15	
    16	const NAME: &str = "Google";
    17	const API_KEY_ENV: &str = "GEMINI_API_KEY";
    18	const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";
    19	
    20	pub struct Gemini {
    21	    model: String,
    22	}
    23	
    24	impl Gemini {
    25	    pub fn new(model: String) -> Self {
    26	        Gemini { model }
    27	    }
    28	
    29	    fn api_key(&self) -> Result<String, ProviderError> {
    30	        std::env::var(API_KEY_ENV)
    31	            .ok()
    32	            .filter(|k| !k.trim().is_empty())
    33	            .ok_or(ProviderError {
    34	                provider: NAME,
    35	                kind: ErrorKind::MissingKey {
    36	                    env_var: API_KEY_ENV,
    37	                },
    38	            })
    39	    }
    40	
    41	    /// Base URL: `GCM_GEMINI_BASE_URL` (primary) or `GCM_GOOGLE_BASE_URL` (alias),
    42	    /// else the default (round-2 review pt 4).
    43	    fn base_url(&self) -> String {
    44	        std::env::var("GCM_GEMINI_BASE_URL")
    45	            .ok()
    46	            .filter(|u| !u.trim().is_empty())
    47	            .or_else(|| {
    48	                std::env::var("GCM_GOOGLE_BASE_URL")
    49	                    .ok()
    50	                    .filter(|u| !u.trim().is_empty())
    51	            })
    52	            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
    53	    }
    54	
    55	    fn request<'a>(&self, key: &str, payload: &'a Value) -> HttpRequest<'a> {
    56	        HttpRequest {
    57	            provider: NAME,
    58	            auth_env_var: API_KEY_ENV,
    59	            endpoint: format!(
    60	                "{}/v1beta/models/{}:generateContent",
    61	                self.base_url().trim_end_matches('/'),
    62	                self.model
    63	            ),
    64	            auth: Some(("x-goog-api-key", key.to_string())),
    65	            extra_headers: Vec::new(),
    66	            payload,
    67	        }
    68	    }
    69	}
    70	
    71	impl Provider for Gemini {
    72	    fn name(&self) -> &'static str {
    73	        NAME
    74	    }
    75	
    76	    fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError> {
    77	        let key = self.api_key()?;
    78	        let payload = build_plan_payload(ctx);
    79	        let raw = http::post_json(&self.request(&key, &payload))?;
    80	        let json = extract_text(&raw)?;
    81	        if json.is_empty() {
    82	            return Err(empty());
    83	        }
    84	        crate::plan::parse_defensive(&json).map_err(|e| ProviderError {
    85	            provider: NAME,
    86	            kind: ErrorKind::Deserialize(e.to_string()),
    87	        })
    88	    }
    89	
    90	    fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError> {
    91	        let key = self.api_key()?;
    92	        let payload = build_message_payload(&super::message_user_content(diff));
    93	        let raw = http::post_json(&self.request(&key, &payload))?;
    94	        let message = extract_text(&raw)?;
    95	        if message.is_empty() {
    96	            return Err(empty());
    97	        }
    98	        Ok(message)
    99	    }
   100	
   101	    fn resolve_hunks(
   102	        &self,
   103	        ctx: &super::ResolveContext,
   104	    ) -> Result<Vec<super::Resolution>, ProviderError> {
   105	        let key = self.api_key()?;
   106	        let payload = build_resolve_payload(ctx);
   107	        let raw = http::post_json(&self.request(&key, &payload))?;
   108	        let json = extract_text(&raw)?;
   109	        if json.is_empty() {
   110	            return Err(empty());
   111	        }
   112	        super::parse_resolutions(NAME, &json, ctx.hunks.len())
   113	    }
   114	
   115	    fn cache_model_id(&self) -> String {
   116	        format!("google:{}", self.model)
   117	    }
   118	
   119	    fn diff_budget(&self) -> DiffBudget {
   120	        // gemini-3.1-flash-lite has a very large context; standard budget for v1.
   121	        DiffBudget::standard()
   122	    }
   123	}
   124	
   125	fn empty() -> ProviderError {
   126	    ProviderError {
   127	        provider: NAME,
   128	        kind: ErrorKind::EmptyResponse,
   129	    }
   130	}
   131	
   132	pub(super) fn build_resolve_payload(ctx: &super::ResolveContext) -> Value {
   133	    json!({
   134	        "systemInstruction": { "parts": [ { "text": super::RESOLVE_SYSTEM_PROMPT } ] },
   135	        "contents": [ { "role": "user", "parts": [ { "text": super::resolve_user_content(ctx) } ] } ],
   136	        "generationConfig": {
   137	            "temperature": ctx.temperature,
   138	            "responseMimeType": "application/json",
   139	            "responseSchema": super::gemini_resolve_schema(),
   140	            "thinkingConfig": { "thinkingLevel": "MINIMAL" }
   141	        }
   142	    })
   143	}
   144	
   145	pub(super) fn build_plan_payload(ctx: &GroupingContext) -> Value {
   146	    json!({
   147	        "systemInstruction": { "parts": [ { "text": super::GROUPING_SYSTEM_PROMPT } ] },
   148	        "contents": [ { "role": "user", "parts": [ { "text": super::grouping_user_content(ctx) } ] } ],
   149	        "generationConfig": {
   150	            "responseMimeType": "application/json",
   151	            "responseSchema": crate::plan::gemini_schema(),
   152	            "thinkingConfig": { "thinkingLevel": "MINIMAL" }
   153	        }
   154	    })
   155	}
   156	
   157	pub(super) fn build_message_payload(user_content: &str) -> Value {
   158	    json!({
   159	        "systemInstruction": { "parts": [ { "text": super::SYSTEM_PROMPT } ] },
   160	        "contents": [ { "role": "user", "parts": [ { "text": user_content } ] } ],
   161	        "generationConfig": {
   162	            "thinkingConfig": { "thinkingLevel": "MINIMAL" }
   163	        }
   164	    })
   165	}
   166	
   167	#[derive(Deserialize)]
   168	struct GeminiResponse {
   169	    candidates: Option<Vec<Candidate>>,
   170	    #[serde(rename = "promptFeedback")]
   171	    prompt_feedback: Option<PromptFeedback>,
   172	}
   173	
   174	#[derive(Deserialize)]
   175	struct PromptFeedback {
   176	    #[serde(rename = "blockReason")]
   177	    block_reason: Option<String>,
   178	}
   179	
   180	#[derive(Deserialize)]
   181	struct Candidate {
   182	    content: Option<Content>,
   183	    #[serde(rename = "finishReason")]
   184	    finish_reason: Option<String>,
   185	}
   186	
   187	#[derive(Deserialize)]
   188	struct Content {
   189	    parts: Option<Vec<Part>>,
   190	}
   191	
   192	#[derive(Deserialize)]
   193	struct Part {
   194	    text: Option<String>,
   195	    thought: Option<bool>,
   196	}
   197	
   198	/// Extract the answer text from a Gemini response. Checks for prompt- and
   199	/// candidate-level blocks BEFORE reading content (a safety block returns 200 OK
   200	/// with no content), concatenates non-thought parts, and applies the `<think>`
   201	/// backstop. Returns `Ok("")` for a genuinely empty STOP response (the caller
   202	/// maps that to `EmptyResponse`); blocks/truncation are typed errors.
   203	pub(super) fn extract_text(raw: &str) -> Result<String, ProviderError> {
   204	    let err = |kind| ProviderError {
   205	        provider: NAME,
   206	        kind,
   207	    };
   208	    let resp: GeminiResponse =
   209	        serde_json::from_str(raw).map_err(|e| err(ErrorKind::Deserialize(e.to_string())))?;
   210	
   211	    if let Some(reason) = resp
   212	        .prompt_feedback
   213	        .as_ref()
   214	        .and_then(|p| p.block_reason.as_deref())
   215	        .map(str::trim)
   216	        .filter(|r| !r.is_empty())
   217	    {
   218	        return Err(err(ErrorKind::BadRequest {
   219	            detail: Some(format!("Gemini blocked the prompt (reason: {reason})")),
   220	        }));

codex
I’ve found one likely drift area outside the new provider code itself: some older user-guidance and test harness code still reflects the pre-Vertex provider set. I’m checking whether those are just docs/test isolation issues or whether they affect real runtime behavior.
exec
/bin/zsh -lc "rg -n \"CLEARED_ENV|SCRUB_ENV|env_remove\\(|GCM_VERTEX_PROJECT|GOOGLE_CLOUD_PROJECT|GCM_VERTEX_LOCATION|GCP_REGION|GCM_VERTEX_TOKEN|vertex\" tests src/config.rs src/cli.rs" in /Users/mk/Code/gcm--feat-clo-537-vertex
exec
/bin/zsh -lc 'rg -n "GCM_PROVIDER=groq|groq, google, openai, anthropic, ollama|groq, google, vertex, openai, anthropic, ollama|google, openai, anthropic, ollama" src/config.rs src/cli.rs src/status.rs src/provider/mod.rs tests' in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
src/cli.rs:20:PROVIDER: select with --provider (groq, google, vertex, openai, anthropic, ollama) or GCM_PROVIDER\n\
src/config.rs:1141:    s.push_str("  export GCM_PROVIDER=groq   # or google, openai, anthropic, ollama\n\n");
src/provider/mod.rs:491:                        "unknown provider '{t}'. Set --provider/GCM_PROVIDER to one of: groq, google, vertex, openai, anthropic, ollama."

 succeeded in 0ms:
src/cli.rs:20:PROVIDER: select with --provider (groq, google, vertex, openai, anthropic, ollama) or GCM_PROVIDER\n\
src/config.rs:78:    /// Vertex only (CLO-537): the GCP project. Bridged into `GCM_VERTEX_PROJECT` by
src/config.rs:84:    /// Bridged into `GCM_VERTEX_LOCATION` by [`apply_to_env`] when unset.
src/config.rs:336:                    "# project = \"my-gcp-project\"   # required; or set GCM_VERTEX_PROJECT / GOOGLE_CLOUD_PROJECT\n",
src/config.rs:339:                    "# location = \"global\"   # or set GCM_VERTEX_LOCATION / GOOGLE_CLOUD_LOCATION\n",
src/config.rs:427:                    if !is_set("GCM_VERTEX_PROJECT") {
src/config.rs:428:                        out.push(("GCM_VERTEX_PROJECT", p.to_string()));
src/config.rs:437:                    if !is_set("GCM_VERTEX_LOCATION") {
src/config.rs:438:                        out.push(("GCM_VERTEX_LOCATION", l.to_string()));
src/config.rs:527:                let (project, location) = prompt_vertex_target()?;
src/config.rs:566:/// `GCM_VERTEX_PROJECT` / `GOOGLE_CLOUD_PROJECT`) and location (default `global`).
src/config.rs:569:fn prompt_vertex_target() -> Result<(String, Option<String>), GcmError> {
src/config.rs:570:    let prefill = std::env::var("GCM_VERTEX_PROJECT")
src/config.rs:572:        .or_else(|| std::env::var("GOOGLE_CLOUD_PROJECT").ok())
src/config.rs:597:    match crate::provider::vertex_adc_probe() {
src/config.rs:600:            "  note: gcloud ADC not ready ({msg}). Set GCM_VERTEX_TOKEN or run `gcloud auth application-default login` before committing."
src/config.rs:872:                .or_else(|| env_value("GCM_VERTEX_PROJECT"))
src/config.rs:873:                .or_else(|| env_value("GOOGLE_CLOUD_PROJECT"))
src/config.rs:912:            match crate::provider::vertex_adc_probe() {
src/config.rs:915:                    "ADC not ready: {msg} (set GCM_VERTEX_TOKEN or run `gcloud auth application-default login`)"
src/config.rs:1743:    fn env_plan_bridges_vertex_project_and_location() {
src/config.rs:1760:        assert!(plan.contains(&("GCM_VERTEX_PROJECT", "my-proj".to_string())));
src/config.rs:1761:        assert!(plan.contains(&("GCM_VERTEX_LOCATION", "us-central1".to_string())));
src/config.rs:1763:        let plan2 = env_plan(&cfg, |v| v == "GCM_VERTEX_PROJECT");
src/config.rs:1764:        assert!(!plan2.iter().any(|(k, _)| *k == "GCM_VERTEX_PROJECT"));
src/config.rs:1765:        assert!(plan2.contains(&("GCM_VERTEX_LOCATION", "us-central1".to_string())));
src/config.rs:1769:    fn vertex_project_location_round_trip_and_skip_when_none() {
tests/resolve_remote.rs:224:        .env_remove("GCM_DEBUG")
tests/resolve_remote.rs:227:        cmd.env_remove(var);
tests/resolve_remote.rs:278:        .env_remove("GCM_DEBUG")
tests/resolve_remote.rs:281:        cmd.env_remove(var);
tests/resolve_remote.rs:307:        .env_remove("GCM_DEBUG")
tests/resolve_remote.rs:310:        cmd.env_remove(var);
tests/resolve_remote.rs:349:        .env_remove("GCM_DEBUG")
tests/resolve_remote.rs:352:        cmd.env_remove(var);
tests/status.rs:13:const CLEARED_ENV: &[&str] = &[
tests/status.rs:42:    for var in CLEARED_ENV {
tests/status.rs:43:        cmd.env_remove(var);
tests/provider.rs:68:        cmd.env_remove(var);
tests/provider.rs:177:        cmd.env_remove(var);
tests/onboarding.rs:53:        cmd.env_remove(var);
tests/onboarding.rs:130:    cmd.env_remove("GCM_PROVIDER");
tests/onboarding.rs:202:        cmd.env_remove(var);
tests/resolve_integration.rs:59:        .env_remove("GCM_DEBUG")
tests/resolve_integration.rs:62:        cmd.env_remove(var);
tests/vertex.rs:4://! `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN`, so the full Vertex HTTP path is
tests/vertex.rs:19:const SCRUB_ENV: &[&str] = &[
tests/vertex.rs:25:    "GCM_VERTEX_PROJECT",
tests/vertex.rs:26:    "GCM_VERTEX_LOCATION",
tests/vertex.rs:27:    "GCM_VERTEX_TOKEN",
tests/vertex.rs:29:    "GOOGLE_CLOUD_PROJECT",
tests/vertex.rs:32:    "GCP_REGION",
tests/vertex.rs:68:        .env_remove("GCM_DEBUG")
tests/vertex.rs:70:    for var in SCRUB_ENV {
tests/vertex.rs:71:        cmd.env_remove(var);
tests/vertex.rs:82:fn mock_vertex_resolve_response(replacement: &str) -> String {
tests/vertex.rs:176:default = "vertex"
tests/vertex.rs:179:id = "vertex"
tests/vertex.rs:184:fn resolve_via_vertex_hits_mock_generatecontent_with_bearer_and_resolves() {
tests/vertex.rs:190:    let (url, server) = mock_server(mock_vertex_resolve_response("RESOLVED_BY_VERTEX\n"));
tests/vertex.rs:200:            ("GCM_VERTEX_TOKEN", "fake-adc-token"),
tests/vertex.rs:202:        &["resolve", "--yes", "--provider", "vertex"],
tests/vertex.rs:208:        "gcm resolve --provider vertex failed.\nstdout: {}\nstderr: {}",
tests/vertex.rs:253:fn vertex_missing_gcloud_and_token_is_actionable_not_a_panic() {
tests/vertex.rs:254:    // AC-3: no GCM_VERTEX_TOKEN and gcloud absent -> an actionable, typed error (no
tests/vertex.rs:281:        &["resolve", "--yes", "--provider", "vertex"],
tests/vertex.rs:294:        combined.contains("gcloud") || combined.contains("GCM_VERTEX_TOKEN"),

exec
/bin/zsh -lc 'rg -n "GCM_VERTEX_PROJECT|GOOGLE_CLOUD_PROJECT|GCP_PROJECT|GOOGLE_CLOUD_LOCATION|GCP_REGION|GCM_VERTEX_TOKEN|GCM_VERTEX_BASE_URL" tests' in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
tests/vertex.rs:4://! `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN`, so the full Vertex HTTP path is
tests/vertex.rs:25:    "GCM_VERTEX_PROJECT",
tests/vertex.rs:27:    "GCM_VERTEX_TOKEN",
tests/vertex.rs:28:    "GCM_VERTEX_BASE_URL",
tests/vertex.rs:29:    "GOOGLE_CLOUD_PROJECT",
tests/vertex.rs:30:    "GCP_PROJECT",
tests/vertex.rs:31:    "GOOGLE_CLOUD_LOCATION",
tests/vertex.rs:32:    "GCP_REGION",
tests/vertex.rs:199:            ("GCM_VERTEX_BASE_URL", &url),
tests/vertex.rs:200:            ("GCM_VERTEX_TOKEN", "fake-adc-token"),
tests/vertex.rs:254:    // AC-3: no GCM_VERTEX_TOKEN and gcloud absent -> an actionable, typed error (no
tests/vertex.rs:294:        combined.contains("gcloud") || combined.contains("GCM_VERTEX_TOKEN"),

exec
/bin/zsh -lc "nl -ba tests/provider.rs | sed -n '1,120p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
     1	//! Integration tests for the `gcm provider` enabled-model whitelist + the v1->v2
     2	//! config migration (CLO-516). Each drives the built `gcm` binary as a subprocess
     3	//! with its own throwaway git repo + `GCM_CONFIG` dir and a cleared provider
     4	//! environment, so the on-disk config is the only driver. No network: enforcement
     5	//! and migration run before any provider call, and base URLs point at a closed
     6	//! local port for an immediate refusal where a request would otherwise happen.
     7	//!
     8	//! The interactive cliclack wizard itself (AC-1/AC-3) reads `/dev/tty` and is
     9	//! verified manually; here we cover everything reachable without a TTY: the
    10	//! non-TTY guard, migration, and runtime enforcement (incl. the clean-repo timing).
    11	
    12	use std::fs;
    13	use std::path::Path;
    14	use std::process::{Command, Output, Stdio};
    15	
    16	const PROVIDER_ENV: &[&str] = &[
    17	    "GROQ_API_KEY",
    18	    "GEMINI_API_KEY",
    19	    "OPENAI_API_KEY",
    20	    "ANTHROPIC_API_KEY",
    21	    "GCM_PROVIDER",
    22	    "OLLAMA_HOST",
    23	    "GCM_OLLAMA_BASE_URL",
    24	    "GCM_OPENAI_BASE_URL",
    25	    "GCM_GROQ_BASE_URL",
    26	];
    27	
    28	fn git_init(dir: &Path) {
    29	    for args in [
    30	        vec!["init", "-q"],
    31	        vec!["config", "user.email", "t@example.com"],
    32	        vec!["config", "user.name", "Test"],
    33	    ] {
    34	        let ok = Command::new("git")
    35	            .args(&args)
    36	            .current_dir(dir)
    37	            .status()
    38	            .expect("run git")
    39	            .success();
    40	        assert!(ok, "git {args:?} failed");
    41	    }
    42	}
    43	
    44	/// Write a `0600` config file (load() rejects a world-readable one).
    45	fn write_config(dir: &Path, body: &str) {
    46	    let path = dir.join("config.toml");
    47	    fs::write(&path, body).unwrap();
    48	    #[cfg(unix)]
    49	    {
    50	        use std::os::unix::fs::PermissionsExt;
    51	        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    52	    }
    53	}
    54	
    55	/// Run `gcm` in `repo` with a cleared provider env, `GCM_CONFIG` at `config_dir`,
    56	/// stdin closed (non-TTY), and bounded network so the suite never hangs.
    57	fn run_gcm(repo: &Path, config_dir: &Path, args: &[&str]) -> Output {
    58	    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    59	    cmd.current_dir(repo)
    60	        .args(args)
    61	        .env("GCM_CONFIG", config_dir)
    62	        .env("GCM_HTTP_TIMEOUT_SECS", "2")
    63	        .env("GCM_RETRY_MAX", "0")
    64	        .stdin(Stdio::null())
    65	        .stdout(Stdio::piped())
    66	        .stderr(Stdio::piped());
    67	    for var in PROVIDER_ENV {
    68	        cmd.env_remove(var);
    69	    }
    70	    cmd.output().expect("run gcm")
    71	}
    72	
    73	fn error_code(stdout: &str) -> String {
    74	    let parsed: serde_json::Value =
    75	        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("json ({e}): {stdout}"));
    76	    parsed["error"]["code"].as_str().unwrap_or("").to_string()
    77	}
    78	
    79	#[test]
    80	fn provider_subcommand_non_tty_fails_with_guidance() {
    81	    let repo = tempfile::tempdir().unwrap();
    82	    let cfg = tempfile::tempdir().unwrap();
    83	    git_init(repo.path());
    84	
    85	    let out = run_gcm(repo.path(), cfg.path(), &["provider"]);
    86	    let stderr = String::from_utf8_lossy(&out.stderr);
    87	    assert!(!out.status.success(), "must exit non-zero: {stderr}");
    88	    assert!(
    89	        stderr.contains("interactive terminal") || stderr.contains("[[providers]]"),
    90	        "guidance on stderr: {stderr}"
    91	    );
    92	}
    93	
    94	#[test]
    95	fn v1_config_loads_after_version_bump() {
    96	    // A pre-CLO-516 v1 config must still hydrate after the bump (migration), not be
    97	    // treated as a first run. Inline groq key + a closed base URL -> the run reaches
    98	    // the dead endpoint and fails with a reach error, never OnboardingRequired.
    99	    let repo = tempfile::tempdir().unwrap();
   100	    let cfg = tempfile::tempdir().unwrap();
   101	    git_init(repo.path());
   102	    fs::write(repo.path().join("a.txt"), "x\n").unwrap();
   103	    write_config(
   104	        cfg.path(),
   105	        "version = 1\ndefault = \"groq\"\n\n[[providers]]\nid = \"groq\"\nkey = \"sk-inline\"\n",
   106	    );
   107	
   108	    let out = run_gcm(
   109	        repo.path(),
   110	        cfg.path(),
   111	        &["--json", "--yes", "--provider", "groq"],
   112	    );
   113	    let stdout = String::from_utf8_lossy(&out.stdout);
   114	    let code = error_code(&stdout);
   115	    assert_ne!(
   116	        code, "OnboardingRequired",
   117	        "v1 migrated, not a first run: {stdout}"
   118	    );
   119	    assert_ne!(code, "MissingKey", "inline key hydrated: {stdout}");
   120	}

exec
/bin/zsh -lc "nl -ba tests/onboarding.rs | sed -n '1,100p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
     1	//! First-run onboarding integration tests (CLO-496). Each test drives the built
     2	//! `gcm` binary as a subprocess with its own environment (so there is no
     3	//! in-process env race) and a throwaway git repo + `GCM_CONFIG` dir. No network:
     4	//! onboarding fires before any provider call, and the hydration test points
     5	//! Ollama at a closed local port for an immediate connection refusal.
     6	
     7	use std::fs;
     8	use std::path::Path;
     9	use std::process::{Command, Output, Stdio};
    10	
    11	/// Provider env vars cleared so a first run is genuinely unconfigured.
    12	const PROVIDER_ENV: &[&str] = &[
    13	    "GROQ_API_KEY",
    14	    "GEMINI_API_KEY",
    15	    "OPENAI_API_KEY",
    16	    "ANTHROPIC_API_KEY",
    17	    "GCM_PROVIDER",
    18	    "OLLAMA_HOST",
    19	    "GCM_OLLAMA_BASE_URL",
    20	];
    21	
    22	/// Initialize a minimal git work tree at `dir` (so `Repo::discover` succeeds).
    23	fn git_init(dir: &Path) {
    24	    for args in [
    25	        vec!["init", "-q"],
    26	        vec!["config", "user.email", "t@example.com"],
    27	        vec!["config", "user.name", "Test"],
    28	    ] {
    29	        let ok = Command::new("git")
    30	            .args(&args)
    31	            .current_dir(dir)
    32	            .status()
    33	            .expect("run git")
    34	            .success();
    35	        assert!(ok, "git {args:?} failed");
    36	    }
    37	}
    38	
    39	/// Run `gcm` in `repo` with a clean provider environment, `GCM_CONFIG` pointed at
    40	/// `config_dir`, and stdin closed (non-TTY). Returns the captured output.
    41	fn run_gcm(repo: &Path, config_dir: &Path, extra_args: &[&str]) -> Output {
    42	    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    43	    cmd.current_dir(repo)
    44	        .args(extra_args)
    45	        .env("GCM_CONFIG", config_dir)
    46	        // bound any (unexpected) network attempt so the suite never hangs
    47	        .env("GCM_HTTP_TIMEOUT_SECS", "2")
    48	        .env("GCM_RETRY_MAX", "0")
    49	        .stdin(Stdio::null())
    50	        .stdout(Stdio::piped())
    51	        .stderr(Stdio::piped());
    52	    for var in PROVIDER_ENV {
    53	        cmd.env_remove(var);
    54	    }
    55	    cmd.output().expect("run gcm")
    56	}
    57	
    58	#[test]
    59	fn first_run_non_tty_prints_instructions_and_exits_nonzero() {
    60	    let repo = tempfile::tempdir().unwrap();
    61	    let cfg = tempfile::tempdir().unwrap();
    62	    git_init(repo.path());
    63	
    64	    let out = run_gcm(repo.path(), cfg.path(), &[]);
    65	    let stderr = String::from_utf8_lossy(&out.stderr);
    66	
    67	    assert!(
    68	        !out.status.success(),
    69	        "first run must exit non-zero: {stderr}"
    70	    );
    71	    // the human instructions (template + an export line) land on stderr
    72	    assert!(
    73	        stderr.contains("[[providers]]"),
    74	        "TOML template on stderr: {stderr}"
    75	    );
    76	    assert!(
    77	        stderr.contains("export GROQ_API_KEY="),
    78	        "export line on stderr: {stderr}"
    79	    );
    80	}
    81	
    82	#[test]
    83	fn first_run_json_non_tty_emits_envelope_not_prompts() {
    84	    let repo = tempfile::tempdir().unwrap();
    85	    let cfg = tempfile::tempdir().unwrap();
    86	    git_init(repo.path());
    87	
    88	    let out = run_gcm(repo.path(), cfg.path(), &["--json"]);
    89	    let stdout = String::from_utf8_lossy(&out.stdout);
    90	    let stderr = String::from_utf8_lossy(&out.stderr);
    91	
    92	    assert!(!out.status.success(), "exit non-zero: {stdout} / {stderr}");
    93	    // stdout is exactly one JSON error envelope - no prompt text
    94	    let trimmed = stdout.trim();
    95	    let parsed: serde_json::Value = serde_json::from_str(trimmed)
    96	        .unwrap_or_else(|e| panic!("stdout is one JSON object ({e}): {trimmed}"));
    97	    assert_eq!(parsed["status"], "error", "envelope: {trimmed}");
    98	    assert_eq!(
    99	        parsed["error"]["code"], "OnboardingRequired",
   100	        "code: {trimmed}"

codex
I found a second “old cloud == API key” assumption, this time in `gcm status` human output rather than config plumbing. I’m grepping for any remaining status/onboarding text that still assumes every non-Ollama provider is key-based.
exec
/bin/zsh -lc 'rg -n "API key|no API key|key-free|keyless|gcloud ADC|Vertex|project is required|Ollama" src/config.rs src/status.rs src/cli.rs tests' in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
src/cli.rs:24:Ollama is local and needs NO key - it talks to http://localhost:11434 (override with\n\
src/cli.rs:32:local model, nothing leaves the machine (zero-egress); an Ollama `:cloud` model routes\n\
src/cli.rs:33:through Ollama Cloud and is NOT zero-egress. See the README for each provider's data policy.\n\
src/status.rs:6://! (for Ollama) endpoint came from.
src/status.rs:14://!   * **Ollama endpoint**: `GCM_OLLAMA_BASE_URL` > `OLLAMA_HOST` (normalized) >
src/status.rs:34:    ProviderId::Vertex,
src/status.rs:37:    ProviderId::Ollama,
src/status.rs:82:    /// Key source for cloud providers; `None` for key-free Ollama.
src/status.rs:85:    /// Ollama endpoint; `None` for cloud providers.
src/status.rs:88:    /// Ollama endpoint source; `None` for cloud providers.
src/status.rs:95:    /// For Ollama only: false when the model routes off-machine (a cloud-tagged
src/status.rs:99:    /// Vertex only (CLO-537): GCP project as `value (source)`; `None` otherwise.
src/status.rs:102:    /// Vertex only: location as `value (source)` (default `global`); `None` otherwise.
src/status.rs:105:    /// Vertex only: inferred auth source (`GCM_VERTEX_TOKEN` or `gcloud ADC`). Never a
src/status.rs:302:/// is set & non-blank. Ollama: listed in config OR `OLLAMA_HOST` /
src/status.rs:313:        ProviderId::Ollama => {
src/status.rs:317:        // Vertex (keyless): activated when a project resolves from the env (a
src/status.rs:318:        // config-listed Vertex already returned true above).
src/status.rs:319:        ProviderId::Vertex => {
src/status.rs:330:/// Vertex GCP project as a `value (source)` display string (CLO-537): env precedence
src/status.rs:347:        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Vertex))
src/status.rs:354:/// Vertex location as a `value (source)` display string; env precedence
src/status.rs:371:        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Vertex))
src/status.rs:381:/// Vertex auth source, inferred WITHOUT a gcloud call (status stays no-subprocess):
src/status.rs:382:/// `GCM_VERTEX_TOKEN` if that env var is set, else `gcloud ADC`. Reflects which path
src/status.rs:388:        "gcloud ADC".to_string()
src/status.rs:428:/// Resolve the Ollama endpoint and its source without calling `apply_to_env`.
src/status.rs:443:        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Ollama))
src/status.rs:516:        // necessarily errors; Ollama is key-free and falls back to the local
src/status.rs:517:        // daemon, so an unconfigured Ollama selection can still run.
src/status.rs:521:                format!(" (not configured - will try the local Ollama daemon at {ep})")
src/status.rs:524:                " (NOT activated - no API key; gcm would error on a real run)".to_string()
src/status.rs:546:/// The neutral `cloud` / `local` descriptor for an Ollama model, or `None` for a
src/status.rs:547:/// cloud provider (the distinction is only meaningful for Ollama). Driven by the
src/status.rs:580:/// Ollama appends a `[cloud]` / `[local]` tag to its model line.
src/status.rs:601:        // `model:` padded to align its value under the Ollama `endpoint:` line.
src/status.rs:716:        // Ollama: NOT active by default on a clean machine...
src/status.rs:717:        assert!(!is_activated(ProviderId::Ollama, None, &env(&[])));
src/status.rs:720:            ProviderId::Ollama,
src/status.rs:725:        let oc = cfg(ProviderId::Ollama, vec![pc(ProviderId::Ollama, None, None)]);
src/status.rs:726:        assert!(is_activated(ProviderId::Ollama, Some(&oc), &env(&[])));
src/status.rs:745:            ProviderId::Ollama
src/status.rs:794:            ProviderId::Ollama,
src/status.rs:795:            vec![pc(ProviderId::Ollama, None, Some("http://cfg:2"))],
src/status.rs:848:        // Vertex reports project/location/auth-source and NO key row (N2/P4).
src/status.rs:850:            Some(ProviderId::Vertex),
src/status.rs:861:            .find(|p| p.name == ProviderId::Vertex)
src/status.rs:864:        assert!(v.key_source.is_none(), "no key row for Vertex: {v:?}");
src/status.rs:870:        assert_eq!(v.auth_source.as_deref(), Some("gcloud ADC"));
src/status.rs:875:            Some(ProviderId::Vertex),
src/status.rs:883:            .find(|p| p.name == ProviderId::Vertex)
src/status.rs:976:            .find(|p| p.name == ProviderId::Ollama)
src/status.rs:989:            .find(|p| p.name == ProviderId::Ollama)
src/status.rs:992:        // -cloud model (Ollama Cloud naming) -> also off-machine, zero_egress false
src/status.rs:1002:            .find(|p| p.name == ProviderId::Ollama)
src/config.rs:6://! the Ollama endpoint, and the default provider.
src/config.rs:37:/// Default Ollama endpoint (mirrors `provider::ollama`'s default base URL).
src/config.rs:39:/// Connection timeout for the wizard's Ollama daemon probe (ADR-001 Decision 8):
src/config.rs:59:/// `None` for key-free Ollama, which uses `endpoint` instead.
src/config.rs:78:    /// Vertex only (CLO-537): the GCP project. Bridged into `GCM_VERTEX_PROJECT` by
src/config.rs:80:    /// provider, so a pre-Vertex config file parses unchanged (no version bump).
src/config.rs:83:    /// Vertex only (CLO-537): the location/region (effective default `global`).
src/config.rs:304:    s.push_str("# endpoint (Ollama only), project+location (Vertex only). Matching env vars\n");
src/config.rs:410:                // Ollama: set the base URL only when neither gcm's own var nor
src/config.rs:411:                // the Ollama-native OLLAMA_HOST is already set.
src/config.rs:419:                // Vertex: bridge project/location into the gcm-namespaced vars only
src/config.rs:485:    // 2. Capture each enabled provider's key (env or prompt) or Ollama endpoint.
src/config.rs:503:                        "  Enter the {} API key for {} (or press Enter to set {var} yourself later): ",
src/config.rs:524:                // Vertex: project + location (no key, no endpoint) - fixes the bug
src/config.rs:525:                // where selecting Vertex in first-run onboarding prompted for an
src/config.rs:526:                // Ollama endpoint (CLO-537 round-2 A2/P1).
src/config.rs:565:/// First-run prompt for the Vertex target: GCP project (required; prefilled from
src/config.rs:580:        let input = wizard_read_line(&format!("  Vertex GCP project{hint}: "))?;
src/config.rs:588:        eprintln!("  A GCP project is required for Vertex. Try again.");
src/config.rs:590:    let loc_input = wizard_read_line("  Vertex location [global]: ")?;
src/config.rs:598:        Ok(()) => eprintln!("  gcloud ADC ready."),
src/config.rs:600:            "  note: gcloud ADC not ready ({msg}). Set GCM_VERTEX_TOKEN or run `gcloud auth application-default login` before committing."
src/config.rs:606:/// Prompt for the Ollama endpoint (default offered), validate it, probe the
src/config.rs:614:        let input = wizard_read_line(&format!("  Ollama endpoint [{effective}]: "))?;
src/config.rs:625:        eprintln!("  Ollama is reachable at {url}.");
src/config.rs:628:            "  Warning: could not reach Ollama at {url} within {}s. Start it with `ollama serve` \
src/config.rs:644:/// leading `models/` (its list endpoint returns prefixed names); Ollama treats a
src/config.rs:651:        ProviderId::Ollama if !m.contains(':') => format!("{m}:latest"),
src/config.rs:830:                        "{} API key (press Enter to skip)",
src/config.rs:846:            // Ollama: resolve/prompt the endpoint before `/api/tags`. An env override
src/config.rs:852:            let ep = match cliclack::input("Ollama endpoint")
src/config.rs:867:            // Vertex: project (required) + location (default global); no key, no
src/config.rs:869:            // below short-circuits Vertex).
src/config.rs:875:            let mut project_input = cliclack::input("GCP project (required for Vertex)");
src/config.rs:882:                        Err("a GCP project is required".to_string())
src/config.rs:895:            let location = match cliclack::input("Vertex location")
src/config.rs:909:            // Non-blocking ADC probe (warns; never blocks a keyless setup).
src/config.rs:911:            sp.start("Checking gcloud ADC...");
src/config.rs:913:                Ok(()) => sp.stop("gcloud ADC ready"),
src/config.rs:1000:    // Vertex carries project/location instead of a key/endpoint (None for others).
src/config.rs:1071:/// The wizard's Ollama endpoint default, mirroring runtime precedence
src/config.rs:1239:// ── Ollama probe ────────────────────────────────────────────────────────────
src/config.rs:1241:/// The effective Ollama base URL the backend would use, so the wizard seeds its
src/config.rs:1273:/// Probe the Ollama daemon with the bounded [`PROBE_TIMEOUT`] (does not hang on
src/config.rs:1288:/// Validate an Ollama endpoint URL (no `url` dependency): must be `http(s)://`
src/config.rs:1313:/// The five v1 providers, cloud first then Ollama (the wizard's menu order).
src/config.rs:1315:/// from `cloud_then_ollama` and added Vertex; the old name implied a key-bearing/Ollama
src/config.rs:1322:        ProviderId::Vertex,
src/config.rs:1325:        ProviderId::Ollama,
src/config.rs:1346:        ProviderId::Ollama => "Ollama (local, no key)",
src/config.rs:1347:        ProviderId::Vertex => "Google (Vertex AI)",
src/config.rs:1525:                pc(ProviderId::Ollama, None, Some("http://localhost:11434")),
src/config.rs:1550:        assert_eq!(cfg.providers[1].id, ProviderId::Ollama);
src/config.rs:1662:                pc(ProviderId::Ollama, None, Some("http://host:1234")),
src/config.rs:1677:            default: ProviderId::Ollama,
src/config.rs:1678:            providers: vec![pc(ProviderId::Ollama, None, Some("http://host:1234"))],
src/config.rs:1680:        // OLLAMA_HOST present -> do not set GCM_OLLAMA_BASE_URL (Ollama-native wins)
src/config.rs:1747:            default: ProviderId::Vertex,
src/config.rs:1749:                id: ProviderId::Vertex,
src/config.rs:1772:            id: ProviderId::Vertex,
src/config.rs:1784:        // None -> both keys skip-serialize (a pre-Vertex file needs no version bump).
src/config.rs:1789:        // A pre-Vertex file (no project/location keys) still parses.
src/config.rs:1990:        assert_eq!(provider_token(ProviderId::Ollama), "ollama");
src/config.rs:2021:            default: ProviderId::Ollama,
src/config.rs:2024:                pc(ProviderId::Ollama, None, Some("http://host:1234")),
src/config.rs:2156:        // Ollama: a tagless `--model` matches an enabled `:latest` entry.
src/config.rs:2160:            default: ProviderId::Ollama,
src/config.rs:2162:                ProviderId::Ollama,
src/config.rs:2167:        assert!(model_is_enabled(&ollama, ProviderId::Ollama, "llama3").is_ok());
src/config.rs:2168:        assert!(model_is_enabled(&ollama, ProviderId::Ollama, "llama3:latest").is_ok());
src/config.rs:2189:            canonicalize_model(ProviderId::Ollama, "llama3"),
src/config.rs:2193:            canonicalize_model(ProviderId::Ollama, "llama3:8b"),
src/config.rs:2207:                pc(ProviderId::Ollama, None, Some("http://h:1")),
src/config.rs:2221:        // ...Ollama preserved verbatim.
src/config.rs:2225:            .find(|p| p.id == ProviderId::Ollama)
src/config.rs:2314:        // Ollama: a migrated tagless `llama3` must not duplicate a fetched
src/config.rs:2318:        let list = wizard_model_list(ProviderId::Ollama, &fetched, &enabled, Some("llama3"));
src/config.rs:2347:            initial_default_model(ProviderId::Ollama, &sel, Some("llama3")).as_deref(),
tests/status.rs:272:    // endpoint env), but Ollama is key-free and defaults to the local daemon, so a
tests/status.rs:283:        stdout.contains("will try the local Ollama daemon"),
tests/onboarding.rs:5://! Ollama at a closed local port for an immediate connection refusal.
tests/onboarding.rs:169:    // not hydrated, the keyless groq default would fail with MissingKey.
tests/onboarding.rs:222:    // hydrated into the environment: the run selects Ollama and fails to REACH it
tests/onboarding.rs:230:    // a saved 0600 config pointing Ollama at a closed port (load() rejects a
tests/onboarding.rs:265:        "selected provider is Ollama (proves GCM_PROVIDER hydrated): {stdout}"
tests/resolve_integration.rs:144:/// Build a mock Ollama chat response that returns a resolution JSON.
tests/resolve_integration.rs:158:    // The outer Ollama response
tests/vertex.rs:1://! End-to-end acceptance test for the Vertex AI provider (CLO-537).
tests/vertex.rs:4://! `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN`, so the full Vertex HTTP path is
tests/vertex.rs:5://! exercised hermetically (no gcloud, no network): `request()` builds the Vertex URL,
tests/vertex.rs:17:/// Env vars scrubbed before each run so a developer's real Vertex/GCP config can't
tests/vertex.rs:81:/// `gemini::extract_text` (reused by Vertex) expects.
tests/vertex.rs:97:/// can assert the Vertex URL + Bearer header were actually sent. Returns
tests/vertex.rs:213:    // The Vertex request actually reached the server with the right URL shape + auth.
tests/vertex.rs:216:        "request line missing Vertex URL path: {request}"
tests/vertex.rs:237:        "resolved content should come from the mock Vertex response: {after:?}"

exec
/bin/zsh -lc 'rg -n "not configured - will try the local Ollama daemon|NOT activated - no API key|export GCM_PROVIDER=groq|Option A - export a provider key" src/config.rs src/status.rs tests' in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
src/status.rs:521:                format!(" (not configured - will try the local Ollama daemon at {ep})")
src/status.rs:524:                " (NOT activated - no API key; gcm would error on a real run)".to_string()
src/config.rs:1135:    s.push_str("Option A - export a provider key and select it, for example:\n");
src/config.rs:1141:    s.push_str("  export GCM_PROVIDER=groq   # or google, openai, anthropic, ollama\n\n");

codex
I have enough code context for the review findings. I’m pulling exact doc line numbers now for the plan/design acceptance items that the branch still hasn’t actually closed, so the “missing items” section is precise.
exec
/bin/zsh -lc "nl -ba docs/plans/clo-537-vertex-provider.md | sed -n '110,145p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
exec
/bin/zsh -lc "nl -ba docs/designs/clo-537-vertex-provider.md | sed -n '219,230p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
   219	- **`vertex.rs` unit:** `request()` URL for `global` (bare `aiplatform`) vs a regional location (`{loc}-aiplatform`); `Authorization: Bearer` header; `GCM_VERTEX_BASE_URL` override; `cache_model_id()` prefix. Token precedence (`GCM_VERTEX_TOKEN` beats gcloud); missing project → typed `Config` error; token-acquisition failure → actionable typed error.
   220	- **Shared-payload parity:** assert Vertex uses the same `build_plan_payload`/`build_resolve_payload` output as Gemini (extractor already covered by `gemini.rs` tests).
   221	- **Config:** `project`/`location` serialize round-trip; `skip_serializing_if` omits them when `None`; a v2 file (no fields) still loads; `apply_to_env` sets the vertex env vars only when unset.
   222	- **Wizard:** pure resolution helpers (project prefill from `GOOGLE_CLOUD_PROJECT`, location default `global`).
   223	- **Auth-method coverage (§4.6):** `auth_method()` returns `KeylessAdc` for Vertex; `env_plan` bridges `project`/`location` (not an endpoint) for Vertex; `commented_reference` emits `project`/`location` (not `endpoint`) for Vertex; first-run + `run_provider_wizard` route Vertex to the project/location branch.
   224	- **Error mapping (N1):** a mock `401`/expired → the gcloud-reauth message (not "check `<env_var>`"); `403` → the IAM/API-not-enabled message. Assert the env-var text never appears for Vertex auth failures.
   225	- **Input validation (D1):** a malformed `location` (e.g. `us central1/../`) → typed `Config` error **before** any request (URL never built); a **legacy domain-scoped `project`** (`example.com:my-project`) is **accepted** (regression guard for the round-2 fix).
   226	- **Timeout (A3):** a token-acquisition that exceeds the bound → typed error, not a hang (inject a slow fake `gcloud`/token fn).
   227	- **Status (N2):** `gcm status --provider vertex` prints project/location/auth-source and **no** key row; `auth_source` = `GCM_VERTEX_TOKEN` when set else `gcloud ADC`; `--json` carries the new fields; `PROVIDER_ORDER` includes Vertex.
   228	- **Acceptance:** `gcm status --provider vertex` with `GCM_VERTEX_PROJECT` set; an end-to-end `gcm --provider vertex` against a local mock `generateContent` server via `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN` (no gcloud needed in CI).
   229	- **Live (HITL):** one manual `generateContent` 200 against the maintainer's GCP project via real gcloud ADC.
   230	

 succeeded in 0ms:
   110	  - [x] Confirm `keep_chat_model` (:236) needs no arm (has `_ => true`)
   111	
   112	### Phase 7: Testing & Validation
   113	
   114	- [x] Task 19: vertex.rs unit tests (design §5)
   115	  - [x] `request()` URL: global (bare `aiplatform`) vs regional (`{loc}-aiplatform`); `Authorization: Bearer`; `GCM_VERTEX_BASE_URL` override; `cache_model_id()` prefix
   116	  - [x] Token precedence (`GCM_VERTEX_TOKEN` beats gcloud); missing project -> typed `Config` error; token-acquisition failure -> actionable typed error; timeout -> typed error (slow fake gcloud)
   117	  - [x] Error mapping: mock 401 -> gcloud-reauth text (never "check <env_var>"); 403 -> IAM/API-not-enabled text
   118	  - [x] Input validation: malformed `location` rejected pre-request; legacy domain-scoped `project` accepted
   119	- [x] Task 20: shared-payload parity + config tests
   120	  - [x] Vertex uses the same `build_plan_payload`/`build_resolve_payload` output as Gemini
   121	  - [x] `project`/`location` round-trip; `skip_serializing_if` omits when None; v2 file (no fields) loads; `apply_to_env` sets vertex env vars only when unset
   122	- [x] Task 21: wizard + status tests
   123	  - [x] Wizard resolution helpers (project prefill from `GOOGLE_CLOUD_PROJECT`; location default `global`)
   124	  - [x] `gcm status --provider vertex`: project/location/auth-source, no key row; `--json` carries new fields; `PROVIDER_ORDER` includes Vertex
   125	- [x] Task 22: acceptance test (no gcloud in CI)
   126	  - [x] End-to-end `gcm --provider vertex` against a local mock `generateContent` server via `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN`
   127	- [x] Task 23: pre-flight gates
   128	  - [x] `cargo fmt --check` clean
   129	  - [x] `cargo clippy` clean (no new warnings)
   130	  - [x] `cargo test` green (unit + integration)
   131	- [ ] Task 24: Live verification (HITL)
   132	  - [ ] One manual `generateContent` 200 against the maintainer's GCP project via real gcloud ADC (`gcm --provider vertex` and `gcm status --provider vertex`)
   133	
   134	### Phase 8: Finalization
   135	
   136	- [ ] Task 25: Create PR
   137	  - [ ] Verify commits follow `feat(CLO-537): ...` conventional format
   138	  - [ ] Push branch `feat/clo-537-vertex`
   139	  - [ ] `gh pr create` with a body covering the auth-method refactor + Vertex backend + acceptance criteria
   140	  - [ ] Link PR to CLO-537; request review
   141	
   142	---
   143	
   144	## Module Structure
   145	

exec
/bin/zsh -lc 'rg -n "vertex_status_shows_project_location_auth_and_no_key|status_clean|provider_error|vertex" src/status.rs' in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
51:    pub provider_error: Option<String>,
151:    let (selected, provider_error) = selected_provider(cli_provider, config, &env_lookup);
191:                    vertex_project(config, &env_lookup),
192:                    Some(vertex_location(config, &env_lookup)),
193:                    Some(vertex_auth_source(&env_lookup)),
228:        provider_error,
288:                    "unknown provider '{raw}' in GCM_PROVIDER (valid: groq, google, vertex, \
333:fn vertex_project(
357:fn vertex_location(
384:fn vertex_auth_source(env_lookup: &impl Fn(&str) -> Option<String>) -> String {
506:    if report.provider_error.is_some() {
538:    if let Some(err) = &report.provider_error {
824:            ["groq", "google", "vertex", "openai", "anthropic", "ollama"]
847:    fn vertex_status_shows_project_location_auth_and_no_key() {

exec
/bin/zsh -lc "rg -n \"apply_to_env\\(\" src/main.rs src -g'*.rs'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
src/main.rs:267:        config::apply_to_env(&cfg);
src/main.rs:272:        config::apply_to_env(&cfg);
src/main.rs:285:        config::apply_to_env(&cfg);
src/main.rs:267:        config::apply_to_env(&cfg);
src/main.rs:272:        config::apply_to_env(&cfg);
src/main.rs:285:        config::apply_to_env(&cfg);
src/config.rs:385:pub fn apply_to_env(config: &Config) {
src/resolve/mod.rs:96:        crate::config::apply_to_env(&cfg);

exec
/bin/zsh -lc "rg -n \"GCM_VERTEX_MODEL|ProviderId::Vertex\" src/provider/mod.rs src/status.rs src/config.rs | sed -n '1,200p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
src/status.rs:34:    ProviderId::Vertex,
src/status.rs:319:        ProviderId::Vertex => {
src/status.rs:347:        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Vertex))
src/status.rs:371:        .and_then(|c| c.providers.iter().find(|p| p.id == ProviderId::Vertex))
src/status.rs:850:            Some(ProviderId::Vertex),
src/status.rs:861:            .find(|p| p.name == ProviderId::Vertex)
src/status.rs:875:            Some(ProviderId::Vertex),
src/status.rs:883:            .find(|p| p.name == ProviderId::Vertex)
src/provider/mod.rs:367:            ProviderId::Vertex => None,
src/provider/mod.rs:381:            ProviderId::Vertex => "gemini-3.1-flash-lite",
src/provider/mod.rs:395:            ProviderId::Vertex => &["GCM_VERTEX_MODEL"],
src/provider/mod.rs:414:            ProviderId::Vertex => "vertex",
src/provider/mod.rs:424:            ProviderId::Vertex => AuthMethod::KeylessAdc,
src/provider/mod.rs:454:        ProviderId::Vertex => Box::new(vertex::Vertex::new(model)),
src/config.rs:1323:        ProviderId::Vertex,
src/config.rs:1348:        ProviderId::Vertex => "Google (Vertex AI)",
src/config.rs:1748:            default: ProviderId::Vertex,
src/config.rs:1750:                id: ProviderId::Vertex,
src/config.rs:1773:            id: ProviderId::Vertex,

exec
/bin/zsh -lc "nl -ba /Users/mk/.codex/memories/MEMORY.md | sed -n '190,205p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
   190	- when the user said "produce a real, critical, code-grounded design review and WRITE IT to a file" -> default to skeptical, evidence-first review plus an on-disk artifact, not a chat summary [Task 1]
   191	- when the user said "Do NOT trust the doc's claims; open and verify each one" -> verify every claim against source and write `UNVERIFIED` when evidence is missing instead of inheriting doc assertions [Task 1]
   192	- when the user required "using EXACTLY these sections" -> preserve the requested markdown structure exactly and avoid inventing extra headings [Task 1]
   193	- when the user said "After writing the file, reply with just the verdict and the count of blocking vs non-blocking actionable items" -> end with a minimal closeout after artifact creation, not a repeated findings recap [Task 1]
   194	
   195	## Reusable knowledge
   196	
   197	- `Provider::Local` is still non-diarizing in current code, so a local-diarization feature has to update both the gate and the persisted meeting metadata semantics rather than only the transcription branch [Task 1]
   198	- The existing Deepgram single-request path already solves timestamp rebasing with `SingleRequestSegmentMapper`; any new concatenated local path needs an equivalent meeting-time mapping or it will drift at chunk and sub-window boundaries [Task 1]
   199	- `parakeet-rs 0.3.4` exposes `TimestampMode::Words` and token timestamps, so word-level alignment is available without changing the crate itself [Task 1]
   200	- The validated output artifact for this review pattern is an on-disk review file with exact file:line citations; this run wrote `docs/reviews/clo-512-review-codex.md` and validated it as `NEEDS_REVISION` with 5 blocking and 5 non-blocking items [Task 1]
   201	
   202	## Failures and how to do differently
   203	
   204	- Symptom: a local-diarization design sounds plausible because the local model can emit word timestamps -> Cause: the design skipped the meeting-time rebasing contract that the Deepgram path already handles -> Fix: treat any "single concatenated buffer" flow as incomplete until it defines equivalent coordinate remapping [Task 1]
   205	- Symptom: a clustering proposal looks acceptable because it groups by `local_idx` -> Cause: the merge-only behavior hides the "can merge but cannot split" under-count risk -> Fix: test or reason explicitly about over-collapse before accepting the design [Task 1]

exec
/bin/zsh -lc "rg -n \"Task Group: gcm Google model env-over-config precedence review|Google model env-over-config precedence review|GCM_GOOGLE_MODEL|resolve_model\" /Users/mk/.codex/memories/MEMORY.md && nl -ba /Users/mk/.codex/memories/MEMORY.md | sed -n '240,290p'" in /Users/mk/Code/gcm--feat-clo-537-vertex
 succeeded in 0ms:
221:- design-review, clo-516, provider-wizard, config-migration, forward-compat, apply_to_env, resolve_model_with_source, google-model-alias, ureq, sync-http, cliclack, onboarding-tests, status-attribution
232:- `resolve_model_with_source()` in `src/provider/mod.rs` already models Google env ordering as `GCM_GEMINI_MODEL` before the alias `GCM_GOOGLE_MODEL`, and status attribution mirrors the same env-vs-config split without calling `apply_to_env` [Task 1]
244:# Task Group: gcm Google model env-over-config precedence review
257:- review, precedence bug, Google provider, GCM_GOOGLE_MODEL, GCM_GEMINI_MODEL, src/config.rs, resolve_model, env-over-config, gcm status
261:- when reviewing config precedence in `gcm`, the review feedback itself enforced the documented rule `"env > config"` -> treat documented alias env vars like `GCM_GOOGLE_MODEL` as first-class environment input, not fallback-only behavior [Task 1]
265:- In `src/config.rs` around lines 323-325, Google model handling must account for both the primary env var `GCM_GEMINI_MODEL` and the documented alias `GCM_GOOGLE_MODEL` [Task 1]
266:- `resolve_model` reads `GCM_GEMINI_MODEL` before `GCM_GOOGLE_MODEL`, so bridge logic that copies config into the primary env var can accidentally shadow the alias if it only checks the primary env var first [Task 1]
271:- Symptom: a Google provider config with `model` set overrides a user-set `GCM_GOOGLE_MODEL` -> Cause: the bridge logic checked only `GCM_GEMINI_MODEL` before writing the config model into that primary env var -> Fix: verify both the primary env var and the documented alias before bridging config values, or otherwise preserve alias precedence when `env > config` should hold [Task 1]
   240	- Symptom: forward compatibility is described as harmless fallback -> Cause: older binaries currently treat unknown versions as no usable config, which can re-trigger onboarding and overwrite newer-format files -> Fix: review down-level overwrite behavior explicitly before accepting the migration plan [Task 1]
   241	- Symptom: the design’s Google model-list or auth details look harmlessly approximate -> Cause: the doc drifted from the current Gemini backend shape -> Fix: verify endpoint and auth claims against `src/provider/gemini.rs` before approving provider-fetch behavior [Task 1]
   242	- Symptom: helper-level tests pass but the interactive flow is still unproven -> Cause: cliclack behavior remains manual-only without a PTY harness -> Fix: keep UI-interaction claims scoped to manual verification and unit-test only the pure helper portions in-tree [Task 1]
   243	
   244	# Task Group: gcm Google model env-over-config precedence review
   245	
   246	scope: Review findings for Google provider model resolution in `/Users/mk/Code/gcm`; use when touching `src/config.rs`, Google model env vars, or `gcm status` model reporting for Google providers.
   247	applies_to: cwd=/Users/mk/Code/gcm; reuse_rule=safe for similar Google provider/config precedence reviews in this checkout, but re-check exact env-var names and resolution order if `src/config.rs` changes.
   248	
   249	## Task 1: Review Google model precedence, found alias-shadowing regression
   250	
   251	### rollout_summary_files
   252	
   253	- rollout_summaries/2026-06-26T21-15-03-Ri4t-review_google_model_precedence_bug.md (cwd=/Users/mk/Code/gcm, rollout_path=/Users/mk/.codex/sessions/2026/06/26/rollout-2026-06-26T23-15-03-019f05c9-4111-7aa0-9dea-72297340c4d5.jsonl, updated_at=2026-06-26T21:17:45+00:00, thread_id=019f05c9-4111-7aa0-9dea-72297340c4d5, outcome=fail)
   254	
   255	### keywords
   256	
   257	- review, precedence bug, Google provider, GCM_GOOGLE_MODEL, GCM_GEMINI_MODEL, src/config.rs, resolve_model, env-over-config, gcm status
   258	
   259	## User preferences
   260	
   261	- when reviewing config precedence in `gcm`, the review feedback itself enforced the documented rule `"env > config"` -> treat documented alias env vars like `GCM_GOOGLE_MODEL` as first-class environment input, not fallback-only behavior [Task 1]
   262	
   263	## Reusable knowledge
   264	
   265	- In `src/config.rs` around lines 323-325, Google model handling must account for both the primary env var `GCM_GEMINI_MODEL` and the documented alias `GCM_GOOGLE_MODEL` [Task 1]
   266	- `resolve_model` reads `GCM_GEMINI_MODEL` before `GCM_GOOGLE_MODEL`, so bridge logic that copies config into the primary env var can accidentally shadow the alias if it only checks the primary env var first [Task 1]
   267	- A precedence bug here can leak into user-visible status: `gcm status` reads from the resolved model path, so alias-shadowing can make status disagree with the runtime model choice [Task 1]
   268	
   269	## Failures and how to do differently
   270	
   271	- Symptom: a Google provider config with `model` set overrides a user-set `GCM_GOOGLE_MODEL` -> Cause: the bridge logic checked only `GCM_GEMINI_MODEL` before writing the config model into that primary env var -> Fix: verify both the primary env var and the documented alias before bridging config values, or otherwise preserve alias precedence when `env > config` should hold [Task 1]
   272	- Symptom: `gcm status` shows a different model than the user expected from env config -> Cause: model precedence and status resolution were validated separately -> Fix: check both the status output path and runtime resolution together when changing Google model selection [Task 1]
   273	
   274	# Task Group: rs-wisper CLO-510 segmentation review and implementation
   275	
   276	scope: Reviewing the CLO-510 segmentation design against `main` and then implementing the missing runner/manager/test coverage in the feature checkout; use when a design-review request is really a code-evidence remediation pass on local diarization or segmentation plumbing.
   277	applies_to: cwd=/Users/mk/Code/wisper/rs-wisper--feat-clo-510-segmentation; reuse_rule=safe for similar CLO-510 or `src-tauri` segmentation work in this checkout family, but treat exact code diff, model paths, and validation outcomes as branch-specific evidence.
   278	
   279	## Task 1: Review CLO-510 segmentation design against branch and main, identified concrete revision targets
   280	
   281	### rollout_summary_files
   282	
   283	- rollout_summaries/2026-06-23T12-37-24-YK6R-clo_510_segmentation_review_and_implementation.md (cwd=/Users/mk/Code/wisper/rs-wisper--feat-clo-510-segmentation, rollout_path=/Users/mk/.codex/sessions/2026/06/23/rollout-2026-06-23T14-37-24-019ef47c-4191-7543-9989-1b85a209b9f1.jsonl, updated_at=2026-06-23T13:19:35+00:00, thread_id=019ef47c-4191-7543-9989-1b85a209b9f1, outcome=success)
   284	
   285	### keywords
   286	
   287	- CLO-510, segmentation, docs/designs/clo-510-segmentation-stage.md, main...HEAD, NEEDS_REVISION, stale source anchors, stitching semantics, long-meeting observability, u8 overflow, Hungarian, pyannote-3.0
   288	
   289	- Related skill: skills/code-evidence-decision-analysis/SKILL.md
   290	

codex
## Verdict: FAIL
## Findings
- `MEDIUM` [src/provider/vertex.rs:256](/Users/mk/Code/gcm--feat-clo-537-vertex/src/provider/vertex.rs:256), [src/provider/vertex.rs:108](/Users/mk/Code/gcm--feat-clo-537-vertex/src/provider/vertex.rs:108): `validate_project()` still allows `%`, and the project value is interpolated directly into the request path. Percent-encoded delimiters like `%2F` therefore survive templating, which undercuts the branch’s stated security goal of rejecting URL-structural input before building the Vertex URL.
- `LOW` [src/status.rs:518](/Users/mk/Code/gcm--feat-clo-537-vertex/src/status.rs:518): the human `gcm status` headline still only distinguishes “Ollama endpoint” vs “API-key cloud”. If `vertex` is selected but not configured, the headline says `no API key`, which is the wrong remediation path for a keyless provider and is a leftover of the retired Ollama-vs-key assumption.
- `LOW` [src/cli.rs:84](/Users/mk/Code/gcm--feat-clo-537-vertex/src/cli.rs:84), [src/config.rs:1135](/Users/mk/Code/gcm--feat-clo-537-vertex/src/config.rs:1135), [src/config.rs:1141](/Users/mk/Code/gcm--feat-clo-537-vertex/src/config.rs:1141): built-in help/onboarding text is still pre-Vertex. The `--provider` option help omits Vertex entirely, and the non-TTY instructions only describe key-based setup plus an old `GCM_PROVIDER` example list. That leaves the new provider undiscoverable in the exact fallback flows users hit when they cannot run the wizard.

## Missing Items
- [docs/plans/clo-537-vertex-provider.md:125](/Users/mk/Code/gcm--feat-clo-537-vertex/docs/plans/clo-537-vertex-provider.md:125), [tests/vertex.rs:195](/Users/mk/Code/gcm--feat-clo-537-vertex/tests/vertex.rs:195): the plan says Task 22 is an end-to-end `gcm --provider vertex` acceptance test, but the only binary-level acceptance test currently drives `gcm resolve --provider vertex`. The normal commit path (`generate_plan` + `generate_message`) still is not covered end to end.
- [docs/plans/clo-537-vertex-provider.md:131](/Users/mk/Code/gcm--feat-clo-537-vertex/docs/plans/clo-537-vertex-provider.md:131), [docs/designs/clo-537-vertex-provider.md:229](/Users/mk/Code/gcm--feat-clo-537-vertex/docs/designs/clo-537-vertex-provider.md:229): the live ADC-backed HITL verification is still explicitly open.

## Recommendations
- Tighten `validate_project()` to reject `%` as well as `/?#` and whitespace/control, and add a regression test for percent-encoded delimiters.
- Special-case Vertex in the selected-provider status headline, and update all built-in help/non-TTY guidance to describe the keyless Vertex setup path.
- Add one integration test for the normal `gcm --provider vertex` commit flow against the mock server, then close the live HITL item after a real ADC-backed request succeeds.

I did not rerun `cargo test`/`clippy` in this read-only sandbox.


hook: Stop
hook: Stop Completed
tokens used
185,187
## Verdict: FAIL
## Findings
- `MEDIUM` [src/provider/vertex.rs:256](/Users/mk/Code/gcm--feat-clo-537-vertex/src/provider/vertex.rs:256), [src/provider/vertex.rs:108](/Users/mk/Code/gcm--feat-clo-537-vertex/src/provider/vertex.rs:108): `validate_project()` still allows `%`, and the project value is interpolated directly into the request path. Percent-encoded delimiters like `%2F` therefore survive templating, which undercuts the branch’s stated security goal of rejecting URL-structural input before building the Vertex URL.
- `LOW` [src/status.rs:518](/Users/mk/Code/gcm--feat-clo-537-vertex/src/status.rs:518): the human `gcm status` headline still only distinguishes “Ollama endpoint” vs “API-key cloud”. If `vertex` is selected but not configured, the headline says `no API key`, which is the wrong remediation path for a keyless provider and is a leftover of the retired Ollama-vs-key assumption.
- `LOW` [src/cli.rs:84](/Users/mk/Code/gcm--feat-clo-537-vertex/src/cli.rs:84), [src/config.rs:1135](/Users/mk/Code/gcm--feat-clo-537-vertex/src/config.rs:1135), [src/config.rs:1141](/Users/mk/Code/gcm--feat-clo-537-vertex/src/config.rs:1141): built-in help/onboarding text is still pre-Vertex. The `--provider` option help omits Vertex entirely, and the non-TTY instructions only describe key-based setup plus an old `GCM_PROVIDER` example list. That leaves the new provider undiscoverable in the exact fallback flows users hit when they cannot run the wizard.

## Missing Items
- [docs/plans/clo-537-vertex-provider.md:125](/Users/mk/Code/gcm--feat-clo-537-vertex/docs/plans/clo-537-vertex-provider.md:125), [tests/vertex.rs:195](/Users/mk/Code/gcm--feat-clo-537-vertex/tests/vertex.rs:195): the plan says Task 22 is an end-to-end `gcm --provider vertex` acceptance test, but the only binary-level acceptance test currently drives `gcm resolve --provider vertex`. The normal commit path (`generate_plan` + `generate_message`) still is not covered end to end.
- [docs/plans/clo-537-vertex-provider.md:131](/Users/mk/Code/gcm--feat-clo-537-vertex/docs/plans/clo-537-vertex-provider.md:131), [docs/designs/clo-537-vertex-provider.md:229](/Users/mk/Code/gcm--feat-clo-537-vertex/docs/designs/clo-537-vertex-provider.md:229): the live ADC-backed HITL verification is still explicitly open.

## Recommendations
- Tighten `validate_project()` to reject `%` as well as `/?#` and whitespace/control, and add a regression test for percent-encoded delimiters.
- Special-case Vertex in the selected-provider status headline, and update all built-in help/non-TTY guidance to describe the keyless Vertex setup path.
- Add one integration test for the normal `gcm --provider vertex` commit flow against the mock server, then close the live HITL item after a real ADC-backed request succeeds.

I did not rerun `cargo test`/`clippy` in this read-only sandbox.


