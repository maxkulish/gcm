# Review Synthesis: CLO-537 - Add Vertex AI provider (keyless ADC)

**Synthesized**: 2026-07-08
**Design Document**: docs/designs/clo-537-vertex-provider.md
**Reviewers**: Gemini 2.5 Pro, Claude (Opus 4.8, code-grounded). Ollama/Codex failed.

---

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Gemini | OK | `gemini-3.1-pro-preview` returned empty/malformed output twice; fell back to `gemini-2.5-pro`, which produced a full code-grounded review. |
| Ollama (Codex) | REVIEW_FAILED | Exit 124 (300s timeout), 0 bytes. The `codex exec` invocation hung on stdin; default model `glm-5:cloud` not pulled (substituted `glm-5.2:cloud`). Environment/wiring failure, not a design signal. |
| Claude (code-grounded) | OK | Validated every reuse/integration claim against `src/provider/{mod,gemini,http}.rs`, `src/config.rs`, `src/status.rs`, `src/provider/models.rs`, `src/resolve/mod.rs`, ADR-001. |
| lok pipeline | FAILED | The installed `lok` binary rejects the workflow's `{{ steps.health_check.output }}` template variable; the pipeline aborted after the health check. Reviews were produced by running the models directly. |

Two valid reviews → **Multi Review** synthesis below.

## Agreement (High Confidence)

Items both reviewers independently raised.

| # | Finding | Severity |
|---|---------|----------|
| A1 | **`key_env_var() == None` is overloaded to mean "Ollama."** Adding a keyless Vertex breaks the `None` branch in `config.rs::env_plan` (Ollama-endpoint bridge), `run_wizard` (first-run), `run_provider_wizard`, and `commented_reference`. Each site must explicitly distinguish `Ollama` vs `Vertex` vs key-bearing. The design covers `run_provider_wizard` but **misses `env_plan` and the first-run `run_wizard`**. | High |
| A2 | **First-run `run_wizard` (config.rs:420) not addressed.** Only the cliclack `gcm provider` wizard is in the design; selecting Vertex in first-run onboarding would prompt for an Ollama endpoint. | High |
| A3 | **No timeout on the `gcloud` shell-out.** `gcloud … print-access-token` can block on a network token refresh; the design mirrors the (timeout-less) git subprocess pattern, and the 60s HTTP timeout does not cover the pre-request token call. Add a bounded timeout. | Medium |
| A4 | **`commented_reference()` needs a Vertex arm** (else it emits a bogus Ollama `endpoint =` line for Vertex in the generated config template). | Medium |
| A5 | **Strengths agreed:** the Gemini-payload reuse is the right call (inherits the CLO-534 fix for free), keyless ADC is a security improvement, the change is ADR-001 compliant (sync trait; shell-out ethos; secrets never persisted), and the config change is non-breaking (no version bump). | (positive) |

## Disagreement (Needs Human Decision)

| # | Topic | Position A (Gemini) | Position B (Claude) |
|---|-------|---------------------|---------------------|
| D1 | Security of user-controlled Vertex inputs | The gcloud command is static with no user input → no injection risk; security posture "sound." | Correct about command injection, **but** `location`/`project` are templated into the endpoint host/path (`{location}-aiplatform.googleapis.com`) with no validation, so a crafted `GCM_VERTEX_LOCATION` could malform the URL or redirect the `Bearer` token. Add `location`/`project` format validation. Low severity (self-inflicted) but cheap to close. |

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| N1 | **Misleading auth error on a rejected/expired ADC token.** `vertex.rs` sends `auth: Some(...)`, so a 401/403 is classified into `ErrorKind::Auth { env_var }` → "check that <env_var> is valid," but the token came from gcloud, not an env var; and 403 on Vertex commonly means IAM-denied or "Vertex AI API not enabled." Needs a Vertex-specific error path ("run `gcloud auth application-default login`" / distinguish IAM/API-not-enabled). | Claude | High |
| N2 | **`status.rs` is concretely under-specified.** `build_report` branches `if id==Ollama {…} else {key_source}` (Vertex would print a bogus `key: not set` row); `ProviderStatus` has no `project`/`location`/`auth_source` fields; `is_activated` (`_ => key_env_var().is_some_and`) makes Vertex "activated" only via config membership; `PROVIDER_ORDER` is `[ProviderId;5]`; and auth-source can't be verified without a gcloud call, which `status` forbids. | Claude | Medium |
| N3 | **`models.rs` has five exhaustive `match id` blocks**; MVP should short-circuit Vertex at the top of `fetch_supported_models` to return the static Gemini set (D4), not just patch the curated arm. | Claude | Medium |
| N4 | **Refactor suggestion:** replace the `key_env_var().is_none()` proxy with an explicit `ProviderId::auth_method() -> {ApiKey, KeylessEndpoint, KeylessADC}` so every site branches on intent rather than on key-presence. A cleaner fix than patching each call site. | Gemini | Medium |
| N5 | Dual alias derives required (`#[value(alias="google-vertex")]` + `#[serde(alias="google-vertex")]`); a **second** hardcoded valid-names list lives in `status.rs::selected_provider` (line ~248) and also needs `vertex`. | Claude | Low |
| N6 | Referenced guides `docs/guides/vertex-local-dev.md` / `vertex-gemini-setup.md` (design header) do **not exist** in the repo. Cache cold-start on a `google`→`vertex` switch is expected (distinct `cache_model_id`), worth a note. | Claude | Low |
| N7 | `pub(super)` promotion couples `vertex.rs` to `gemini.rs` internals; acceptable for MVP, revisit with `google_common.rs` if a third Google-shaped backend appears (design already flags this). | Gemini | Low |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** (both reviewers agree; neither found a blocker that invalidates the approach).

The core architecture - a thin Vertex backend reusing the exact Gemini `generateContent` payloads, keyless ADC via a `gcloud` shell-out, lazy token acquisition, and a distinct `ProviderId::Vertex` - is sound, code-verified, ADR-compliant, and a security improvement. The findings are integration-completeness and error/operational polish, all fixable in implementation without reshaping the design. They should be captured in the design/spec first so they aren't shipped as latent bugs.

## Priority Actions

1. **(High, A1/A2/N4)** Replace the `key_env_var()==None` "is-Ollama" proxy with explicit `ProviderId` branching (or an `auth_method()` enum) and update **every** site: `env_plan`, first-run `run_wizard`, `run_provider_wizard`, `commented_reference`.
2. **(High, N1)** Define the Vertex error mapping: rejected/expired token → "run `gcloud auth application-default login`"; distinguish 403 IAM-denied / API-not-enabled from a bad key.
3. **(High, N2)** Specify the `status.rs` changes concretely: new `ProviderStatus` fields, third render branch, `PROVIDER_ORDER` bump, `is_activated` rule, and that auth-source is inferred (no gcloud call).
4. **(Medium, A3)** Add a bounded timeout to the `gcloud` subprocess (or document the accepted risk).
5. **(Medium, D1)** Validate `location` (`^(global|[a-z0-9-]+)$`) and `project` before templating the endpoint URL.
6. **(Medium, N3)** Specify the `models.rs` short-circuit for Vertex (static Gemini set).
7. **(Low, N5/N6)** Dual alias derives; the second valid-names list in `status.rs`; fix/annotate the missing `docs/guides/vertex-*.md`; add an ordered implementation plan + explicit acceptance criteria; note the cache cold-start.
