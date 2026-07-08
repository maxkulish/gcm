# Design Review: CLO-537 - Add Vertex AI provider (keyless ADC)

**Reviewed**: 2026-07-08
**Reviewer**: Claude (Opus 4.8, code-grounded)
**Design Document**: docs/designs/clo-537-vertex-provider.md
**Method**: Validated every reuse/integration claim against the actual source (`src/provider/{mod,gemini,http}.rs`, `src/config.rs`, `src/status.rs`, `src/provider/models.rs`, `src/resolve/mod.rs`, ADR-001).

---

## 1. Completeness Check

Present and meaningful: Problem, Goals/Non-goals (G1-G6, clear non-goals), Confirmed decisions (D1-D4 with rationale), Architecture (file-touch list + per-file subsections for the enum, `vertex.rs` auth/request, config, both wizards, status), Testing, Open items.

Missing or thin:
- **No ordered Implementation Plan / phasing.** The file-touch list is a good map but not a sequenced plan (what lands first, what is independently testable).
- **No explicit Acceptance Criteria section.** The Testing section covers unit/acceptance/HITL but there is no crisp, checkable AC list (e.g. "AC-1: `gcm --provider vertex` posts to `{loc}-aiplatform...` with a Bearer header").
- **No rollback/operational section.** Given a new subprocess dependency (gcloud) enters the request path, an operational note (what happens when gcloud is absent/expired/slow) would be worth stating up front rather than only in the `access_token()` pseudocode.

## 2. Architecture Assessment

**Strengths** (verified against code):
- **The reuse thesis holds.** `build_plan_payload` / `build_message_payload` / `build_resolve_payload` / `extract_text` are private fns in `gemini.rs`; promoting them to `pub(super)` is a small, logic-free diff. Because `vertex.rs` lives in the same `provider` module, it already has access to every shared `super::` helper (`GROUPING_SYSTEM_PROMPT`, `grouping_user_content`, `message_user_content`, `parse_resolutions`, `strip_think`, `RESOLVE_SYSTEM_PROMPT`). The "only URL + auth differ" claim is accurate.
- **G6 ("works in `gcm` and `gcm resolve` for free") is confirmed.** Both `src/main.rs:397` and `src/resolve/mod.rs:111` call `crate::provider::select(args.provider, args.model.as_deref())`. A new `ProviderId::Vertex => Box::new(vertex::Vertex::new(model))` arm in `select()` flows to both call sites with no further wiring. Reusing `build_resolve_payload` also inherits the CLO-534 OpenAPI-subset schema fix automatically.
- **Lazy auth preserves the key-free `select()` invariant.** `select()` is documented pure w.r.t. credentials; keys are read inside `generate_*`. Mirroring `gemini.rs::api_key()` for `access_token()` keeps `--dry-run` and cache resolution token-free, exactly as claimed.
- **The transport already fits.** `HttpRequest.auth: Option<(&'static str, String)>` accepts `Some(("Authorization", "Bearer <token>"))` with zero changes to `http.rs`.
- **D2 (gcloud shell-out, zero new deps) is well-founded.** `which = "8"` is already in `Cargo.toml`, and the "optional external binary on PATH" pattern is established in `src/resolve/mergiraf.rs` (`is_available()` via PATH scan, graceful skip). The gcloud approach is idiomatic for this codebase.
- `cache_model_id() = "vertex:{model}"` correctly namespaces away from `"google:{model}"` (see `gemini.rs:116`), so a cached plan never crosses platforms.

**Concerns**:
- **`key_env_var() == None` is overloaded (highest-impact finding).** Today `None` uniquely means "Ollama" and several sites branch on it as such. A keyless Vertex also returns `None`, so those sites misroute Vertex unless each is guarded. See Blind Spots for the enumerated list; this is the single most important thing to capture before implementation.
- **A rejected ADC token surfaces a misleading error.** Because `vertex.rs` sends `auth: Some(...)`, a server-side 401/403 is classified by `http.rs::classify_status` (line 210) into `ErrorKind::Auth { env_var }`, whose `Display` (mod.rs:108) reads "rejected the API key (HTTP 401); check that {env_var} is valid and not expired." The Vertex token comes from gcloud ADC, not an env var, so this tells the user to check something they never set; the correct remedy is `gcloud auth application-default login`. On Vertex a 403 also commonly means IAM permission-denied or "Vertex AI API not enabled on the project" - all funneled into the same "bad key" text. The design's error taxonomy only handles the gcloud-*acquisition* failure, not server-side rejection.
- **No timeout on the gcloud subprocess.** `gcloud auth application-default print-access-token` can perform a blocking network token refresh. No subprocess in the codebase sets a timeout (git uses `.output()`, mergiraf `.status()`), so mirroring "the git pattern" inherits none - but unlike local git, gcloud can hang on the network, and the 60s HTTP timeout does not cover the pre-request token call.

## 3. ADR Compliance

- **Decision 2 (blocking, no async): COMPLIANT.** The `Provider` trait is synchronous; `select()` is synchronous; a blocking gcloud subprocess is consistent with the model.
- **Decision 1 ethos (shell out to the real tool instead of re-implementing / adding a heavy dep): ALIGNED.** The gcloud shell-out is the same reasoning that chose `git` over libgit2. D2's "zero new Rust deps" is accurate.
- **Decision 4 (secrets are env-var-referenced, never plaintext in config; 0600): COMPLIANT.** Vertex is keyless; `project`/`location` are not secrets; the token is never written to config and never printed by `status`. The two new `ProviderConfig` fields default to `None` + `skip_serializing_if`, so the "no config version bump" claim is correct (a v2 file parses unchanged - confirmed against `parse_config`).
- **Decision 3 automated check ("no LLM-CLI subprocess in the runtime"): NOT violated** - gcloud is not an LLM CLI.
- **New-ADR consideration:** this is the first provider that shells out for *auth* (all prior providers are pure HTTP + env key). That is a genuine new pattern for the provider layer. The design flags the payload-sharing question for a future ADR but not the "provider may execute a subprocess to authenticate" precedent. Consider a short ADR addendum.

## 4. Security Review

- **Net improvement.** Keyless ADC (short-lived OAuth token) replaces a long-lived consumer API key, with enterprise data terms. The token is acquired lazily, held only in memory, never persisted, and never emitted by `gcm status` (which reports only the auth *source*). Sound.
- **Unvalidated `location`/`project` feed the endpoint URL (token-redirection risk).** `request()` templates the host from `location` (`https://{location}-aiplatform.googleapis.com`) and the path from `project`. These come from env/config the user controls, but there is no charset/format validation. A stray `location` such as `foo/../` or a value containing `.`/`/` would malform the URL, and in a pathological case redirect the `Authorization: Bearer` token to an unintended host. Low severity (self-inflicted, local config) but cheap to close: validate `location` against `^(global|[a-z0-9-]+)$` and `project` against the GCP project-id charset before templating.
- `GCM_VERTEX_BASE_URL` test seam is acceptable and consistent with the existing `GCM_GEMINI_BASE_URL` override.

## 5. Implementation Concerns

- **`models.rs` needs more than one arm.** `fetch_supported_models` contains five exhaustive `match id` statements (request build ~L105, base URL ~L159, parse ~L186, curated fallback ~L264, provider-name ~L298). Rust exhaustiveness forces a Vertex arm in each. The clean MVP for D4 (static Gemini set, no live fetch) is to **short-circuit Vertex at the top of `fetch_supported_models`** and return the curated Gemini list, rather than only touching the fallback arm as the design implies.
- **`ProviderConfig` +2 fields touches ~15 struct literals** across `config.rs`, `status.rs`, and tests (compiler-enforced; not a correctness risk, just scope to budget).
- **Alias needs both derives.** `ProviderId::Vertex` requires `#[value(alias = "google-vertex")]` (clap) *and* `#[serde(alias = "google-vertex")]` (serde), mirroring the existing `Google`/`gemini` pair (mod.rs:334-336). The design names the alias but not the dual-derive requirement.
- **The hardcoded valid-names string** in `pick_provider_id` (mod.rs:454) must add `vertex` - the design correctly calls this out. `status.rs::selected_provider` (line 248) has a *second* hardcoded valid-names list that also needs updating; not mentioned.
- `provider_token()` (config.rs:1172) serializes the id via `serde_json` to get the TOML/`GCM_PROVIDER` token - fine for Vertex provided the serde rename yields `"vertex"`.

## 6. Blind Spots

1. **First-run onboarding wizard (`run_wizard`, config.rs:420-496) is not addressed.** The design's §4.4 only covers the cliclack `run_provider_wizard`. Both wizards draw their menu from `cloud_then_ollama()` and both branch `match id.key_env_var() { Some => key prompt, None => Ollama endpoint prompt }`. If Vertex is added to the shared menu, selecting it in the *first-run* wizard prompts for an Ollama endpoint. Either exclude Vertex from `run_wizard` or add the third branch there too.
2. **`commented_reference()` (config.rs:299-326)** iterates `cloud_then_ollama()` and, for a `None`-key provider, emits `endpoint = "http://localhost:11434"`. Vertex would get a bogus Ollama-endpoint reference line unless a Vertex arm is added.
3. **`env_plan()` (config.rs:376-396)** `None` arm is Ollama-specific (`GCM_OLLAMA_BASE_URL`). The design says "add a Vertex arm," but the implementer must also guard the existing Ollama branch with `id == Ollama` so Vertex does not fall through it.
4. **`status.rs` is under-specified for a keyless-non-Ollama provider.** `build_report` branches `if id == Ollama {…} else {key_source(…)}` (line 161), so Vertex would print a misleading `key: not set` row. `ProviderStatus` has **no** `project`/`location`/`auth_source` fields, yet §4.5 says status shows them - new struct fields + a third branch + `PROVIDER_ORDER` bump (`[ProviderId; 5]`→`6`) are required. `is_activated` (line 277) `_ => key_env_var().is_some_and(...)` makes Vertex "activated" only via config membership, so a user with working ADC + `GOOGLE_CLOUD_PROJECT` but no config entry shows "not activated" even though `gcm --provider vertex` runs.
5. **Auth-source cannot be verified within status's contract.** `gcm status` is defined to make no network/subprocess call, so it cannot confirm gcloud ADC actually works - the "auth source: gcloud ADC" line is necessarily a guess (present-or-not of `GCM_VERTEX_TOKEN`), which should be stated as such.
6. **Misleading `Auth`/`BadRequest` mapping** for rejected/expired ADC tokens, IAM-denied 403, and "Vertex AI API not enabled" (see §2).
7. **No gcloud subprocess timeout** (see §2).
8. **No `project`/`location` validation** before URL templating (see §4).
9. **Referenced guides are absent from the repo.** The header cites `docs/guides/vertex-local-dev.md` and `docs/guides/vertex-gemini-setup.md`; neither exists under `docs/guides/` (only `cutover-from-bash.md`). If they are external bot-reviewer artifacts, say so; otherwise the reference dangles.
10. **Cache cold-start on platform switch** (minor, correct behavior): moving a repo from `google` to `vertex` changes `cache_model_id` (`google:`→`vertex:`), forcing one re-analysis. Worth a one-line note so it is not mistaken for a bug.

## 7. Verdict

**APPROVE_WITH_SUGGESTIONS**

The core architecture - a thin Vertex backend reusing the exact Gemini `generateContent` payloads, keyless ADC via a gcloud shell-out, lazy token acquisition, and a separate `ProviderId::Vertex` - is sound, verified against the code, ADR-compliant, and a security improvement over the consumer API key. Nothing invalidates the approach, so this does not warrant NEEDS_REVISION. The gaps are integration-completeness and polish: the `key_env_var()==None` overload touching ~5 call sites, the misleading auth error, the missing gcloud timeout, and `project`/`location` validation. These should be captured explicitly in the design/spec before coding so they are not shipped as latent bugs.

## 8. Actionable Feedback

1. **(High) Enumerate every `key_env_var()==None` site and specify Vertex handling for each**: `env_plan`, `run_wizard` (first-run), `run_provider_wizard`, `commented_reference`, `status.rs` build/`is_activated`. Guard the existing Ollama branches with `id == Ollama`.
2. **(High) Define the Vertex error mapping.** Give `vertex.rs` an `auth_env_var`/error path that turns a rejected/expired token into an actionable "run `gcloud auth application-default login`" message, and distinguish 403 IAM-denied / API-not-enabled from a bad key rather than emitting the generic "check that <env_var> is valid."
3. **(High) Specify `status.rs` changes concretely**: new `ProviderStatus` fields (project/location/auth_source), the third render branch, `PROVIDER_ORDER` bump, `is_activated` rule for Vertex, and that auth-source is inferred (no gcloud call).
4. **(Medium) Add a bounded timeout to the gcloud call** (e.g. wrap the subprocess with a timeout, or document the accepted risk explicitly).
5. **(Medium) Validate `location` (`^(global|[a-z0-9-]+)$`) and `project`** before templating the endpoint URL.
6. **(Medium) Specify the `models.rs` short-circuit** (return the static Gemini set at the top of `fetch_supported_models` for Vertex) rather than only the curated arm.
7. **(Low) Note the dual alias derives** (`#[value]` + `#[serde]`), the second hardcoded valid-names list in `status.rs::selected_provider`, and add an ordered implementation plan + explicit ACs.
8. **(Low) Fix or annotate the missing `docs/guides/vertex-*.md` references** and add the cache cold-start note.

---

*This review was generated by validating the design against the current source. Human judgment should be applied when interpreting these suggestions.*
