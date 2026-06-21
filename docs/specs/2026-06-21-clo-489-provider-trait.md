# Spec: Provider trait + registry - Gemini and OpenAI direct-HTTP backends

**Created**: 2026-06-21
**Task**: [CLO-489](https://linear.app/cloud-ai/issue/CLO-489) (slice S6)
**Estimated scope**: L (~13 files touched, ~9 sub-tasks)
**Extends**: CLO-486 single-commit tracer ([spec](2026-06-19-clo-486-single-commit-tracer.md)), CLO-487 grouping ([spec](2026-06-20-clo-487-semantic-grouping.md)), CLO-488 typed errors + retries ([spec](2026-06-20-clo-488-typed-errors.md)). Architecture locked by [ADR-001](../adrs/001-foundational-architecture-decisions.md): Decision 2 (blocking `ureq` client, no async), trait signature `fn generate(&self, req: PlanRequest) -> Result<Plan, ProviderError>` (line 64), "Provider trait shape (FR-11)" knock-on (lines 280-284), Decision 5 (shipped default = Groq), Decision 7 (OpenAI `gpt-4o-mini-2024-07-18`, alias `gcmo`), and the verified Provider Capability Matrix (Appendix A).
**Covers FR**: 11 (provider abstraction behind a trait, Must), 12 (selection via flag/env precedence flag>env>default, Must), 13 partial (Groq+Google+OpenAI callable via direct HTTP; Anthropic=CLO-494, Ollama=CLO-495), 14 (per-invocation `--model` + per-provider model env, Should), 17 (per-model reasoning suppression, Must), 18 Gemini/OpenAI (resolve `GEMINI_API_KEY`/`OPENAI_API_KEY`, Must), 52 consume (build each integration per its verified capability-matrix row)

---

## 1. Problem Statement

Today every LLM call is hardwired to Groq. `src/groq.rs` exposes two free functions - `generate_plan(ctx) -> Result<Plan, GroqError>` (`src/groq.rs:395`, structured-output grouping) and `generate_commit_message(diff) -> Result<String, GroqError>` (`src/groq.rs:372`, the single-commit/fallback/per-group message path) - and `src/main.rs` calls them directly (`main.rs:126`, `main.rs:160`, `main.rs:235`) alongside `groq::resolved_model()` for the cache fingerprint (`main.rs:84`). The error type `GroqError` (`src/groq.rs:53`), its `Display`, the retry engine (`RetryConfig`/`is_retryable`/`backoff_delay`/`retry_with`/`send_chat`, `src/groq.rs:212-352`), and `error.rs`'s `GcmError::Groq(GroqError)` wrapper are all Groq-shaped. There is no way to call any other provider.

The PRD requires a **provider abstraction behind a trait** (FR-11) so that "adding a provider requires implementing one trait and registering it; core flow is unchanged", **selection by flag/env** (FR-12, precedence flag > env > default), and the **active provider matrix** (FR-13) - Groq, Google (Gemini), Anthropic, OpenAI via direct HTTP. This slice delivers the trait + registry and the first two new backends (Gemini, OpenAI); Anthropic (CLO-494) and Ollama (CLO-495) follow on the same seam, and the onboarding wizard (CLO-496) and release cutover (CLO-497) depend on it.

**Who is affected**: anyone who wants to run gcm against Gemini or OpenAI (today impossible), and every future provider slice (CLO-494/495) blocked on the trait. **What triggers it**: `gcm --provider=google`, `gcm --provider=openai`, `GCM_PROVIDER=...`, or `--model=...`. **Why it matters**: the provider trait is the foundation of the multi-provider product (PRD O1 "shareable") and unblocks four downstream tasks; the three providers expose structured output and reasoning suppression three different ways (ADR Appendix A), so the abstraction must accommodate all three shapes without leaking chain-of-thought (FR-17) into the plan or commit message.

**This slice adds Gemini + OpenAI and refactors Groq onto the trait.** It does NOT add Anthropic (CLO-494, forced tool-use) or Ollama (CLO-495, local endpoint), does NOT build the onboarding wizard or a config file (CLO-496 - selection here is flag/env only), and does NOT change the grouping/cache/commit/fallback semantics (CLO-487/491/492 own those). The blocking `ureq` client and synchronous trait are fixed by ADR-001 Decision 2.

---

## 2. Acceptance Criteria

- [ ] **AC-1** A `Provider` trait abstracts both calls; Groq, Google (Gemini), and OpenAI each implement it; `main.rs` calls only the trait (no `groq::generate_*` free-function calls remain in `main.rs`). Adding a provider is one new file implementing the trait + one registry arm.
- [ ] **AC-2** `gcm --provider=google` produces a grouped commit against a Gemini-shaped endpoint (`responseSchema` structured output), proven offline by the mock-Gemini acceptance route; `gcm --provider=openai` does the same against an OpenAI-compatible endpoint (strict `json_schema`).
- [ ] **AC-3** Provider selection precedence is **flag > env (`GCM_PROVIDER`) > default (`groq`)**. An unknown provider name (from either source) fails fast with an actionable error listing the valid names; it is never silently defaulted.
- [ ] **AC-4** `--model <m>` overrides the model for the selected provider; with no flag, the per-provider env (`GCM_GROQ_MODEL` / `GCM_GOOGLE_MODEL` / `GCM_OPENAI_MODEL`) is used; else the provider default (`openai/gpt-oss-120b` / `gemini-3.1-flash-lite` / `gpt-4o-mini-2024-07-18`). Precedence: flag > per-provider env > default.
- [ ] **AC-5** A reasoning model emits **no chain-of-thought** into the plan or the commit message: each backend sets its capability-matrix reasoning suppression (Groq `include_reasoning:false`/`reasoning_effort:none`; Gemini `thinkingConfig.thinkingLevel:"MINIMAL"`; OpenAI `reasoning_effort` only for reasoning families, none for `gpt-4o-mini`), and the universal `<think>` strip + `parse_defensive` remain the backstop (FR-20).
- [ ] **AC-6** A missing key for the selected provider names the **correct** env var (`GROQ_API_KEY` / `GEMINI_API_KEY` / `OPENAI_API_KEY`) and is fatal (no retry, no fallback) - same routing as today (`MissingKey`/`Auth` -> `Fatal` in `main.rs`).
- [ ] **AC-7** Typed errors + bounded retry (CLO-488, FR-21/22) work identically for **all** providers: 429/5xx retried with backoff, 400/auth not retried, `Retry-After` honored, error bodies capped. The retry engine is shared, not re-implemented per provider.
- [ ] **AC-8** Per-provider diff budgets: each provider declares a diff budget (total + per-file bytes) with concrete defaults (Groq/Google `350_000`/`8192`; OpenAI tighter `256_000`/`8192` for `gpt-4o-mini`'s 128k window - table in §3b), env-overridable; the diff gatherer honors the selected provider's budget.
- [ ] **AC-9** Behavioral parity (PRD O3): a bare `gcm` (no flag/env) behaves exactly as before - Groq, same default model, same grouping/cache/commit/fallback. Existing unit + acceptance tests pass unchanged (except renames of `GroqError` -> `ProviderError` symbols).

**Verification method**: `cargo test` for pure logic (selection precedence resolution, model resolution, provider name parsing incl. unknown, each backend's payload builder shape, each backend's response extractor incl. Gemini multi-part/thought filtering, Gemini schema shape, `ProviderError` `Display` distinctness + correct-env-var naming, shared retry on `ProviderError`, diff-budget resolution). `scripts/acceptance.sh` integration via the mock server extended with a **Gemini-shaped route** (`/gemini/...:generateContent`) and an **OpenAI-compatible route** reusing the existing chat-completions mock, driving `--provider=google` and `--provider=openai` end-to-end (grouped commit, dry-run, missing-key, unknown-provider, `--model` override). `cargo build --release`, `cargo clippy --all-targets -D warnings`, `cargo fmt --check`.

**Egress note** (carried from CLO-486/487/488): real Gemini/OpenAI HTTP is not exercised in-sandbox; correctness is proven by unit tests (payload/parse shapes, no network) + the stateful mock harness. The binary reaches the real APIs in the user's environment. ADR confidence caveats apply: smoke-test one Groq gpt-oss request; validate Gemini output app-side against the `Plan` struct regardless of `minimal` thinking.

---

## 3. Constraints

**Must**:
- Synchronous trait, blocking `ureq` (ADR-001 Decision 2) - no `async fn`, no `tokio`, no new runtime. The trait method shape follows ADR line 64.
- One trait covers both calls used by `main.rs`: structured grouping plan **and** single commit message. Both are required (tracer, grouping fallback, and per-group message regeneration on an advanced cache hit, `main.rs:160`).
- Reuse the shared retry/backoff/classification engine (CLO-488) for every provider - move it out of `groq.rs` into a shared HTTP module operating on `ProviderError`; do not duplicate it per backend.
- `Plan`/`Group` structs and `plan::parse_defensive` (FR-20) are shared, provider-agnostic. The `<think>` strip stays the universal backstop.
- FR-18: each backend resolves its own key (`GROQ_API_KEY`/`GEMINI_API_KEY`/`OPENAI_API_KEY`) and a missing key names that exact var; key resolution must NOT be required to resolve the model (the cache fingerprint reads the model with no key, like today's `resolved_model`).
- Selection precedence flag > env > default and model precedence flag > per-provider-env > default, both resolvable without network or key.
- Default provider = `groq`, default models per ADR (Groq `openai/gpt-oss-120b`, Google `gemini-3.1-flash-lite`, OpenAI `gpt-4o-mini-2024-07-18`).
- No hardcoded model IDs in the call path beyond the named per-provider defaults (ADR compliance check); models resolve from flag/env/default.
- Base-URL env overrides for every provider (`GCM_GROQ_BASE_URL` exists; add `GCM_OPENAI_BASE_URL`, and for Google both `GCM_GEMINI_BASE_URL` (primary) and `GCM_GOOGLE_BASE_URL` (alias)) so the acceptance mock can target each backend.

**Must-not**:
- Must not break behavioral parity (O3): a bare `gcm` is unchanged (Groq, same model/grouping/cache/commit/fallback). No flag/env -> identical behavior.
- Must not change the cache **key** (FR-25: `sha256(repo-root)`, shared across providers). The freshness fingerprint (FR-27, CLO-491) may additionally fold the resolved provider+model so a provider/model switch re-analyzes - the key/file location is unchanged.
- Must not let chain-of-thought reach stdout/the commit (FR-17) on any provider.
- Must not add Anthropic, Ollama, a config file, or onboarding (out of scope; CLO-494/495/496).
- Must not retry 400/auth on any provider; must not re-implement retry per backend.
- Must not require an API key to run `--dry-run` model resolution or the cache fingerprint.

**Prefer**:
- A `provider/` module directory (`mod.rs` trait+registry+errors, `http.rs` shared transport, `groq.rs`/`gemini.rs`/`openai.rs` backends) over more flat `src/*.rs` files - the OpenAI-compatible shape (Groq, OpenAI) is shared in `http.rs`; Gemini is the divergent shape.
- `clap` `ValueEnum` for `--provider`/`--model` ergonomics and `--help`, with the same name parser reused for `GCM_PROVIDER` so flag and env validate identically.
- Keep `ProviderError` variants 1:1 with today's `GroqError` (rename + add provider context) so CLO-488's tests and retry logic carry over with minimal churn.
- TDD the pure logic (RED -> GREEN) as in CLO-488: resolution, parsing, payload/extractor shapes, `Display`.

**Escalate when**:
- A capability-matrix fact proves wrong under a real smoke test (e.g. Gemini `responseSchema` rejects the schema, OpenAI strict rejects `gpt-4o-mini`) - record and ask before working around it.
- The refactor would force an `async` runtime or a new heavyweight dependency (contradicts ADR-001 Decision 2).
- Behavioral parity for bare `gcm` cannot be preserved without a user-visible change.

---

## 3b. Concrete contract

### Provider trait (`src/provider/mod.rs`)

```rust
pub trait Provider {
    /// Stable display name, e.g. "Groq" / "Google" / "OpenAI" (for messages/errors).
    fn name(&self) -> &'static str;
    /// Structured grouping plan (FR-11 core contract). Defensive-parsed into Plan.
    fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError>;
    /// Single conventional-commit message (tracer / grouping fallback / per-group regen).
    fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError>;
    /// Provider-qualified model id folded into the cache freshness fingerprint
    /// (FR-27). Resolvable with NO api key. e.g. "groq:openai/gpt-oss-120b".
    fn cache_model_id(&self) -> String;
    /// Per-provider diff budget (FR-13a / "per-provider diff budgets"); env-overridable.
    fn diff_budget(&self) -> DiffBudget;
}
```

### Selection + model resolution (registry, `src/provider/mod.rs`)

- `ProviderId` enum `{ Groq, Google, OpenAI }`. `FromStr`/`ValueEnum`: canonical `groq` | `google` | `openai`; accept `gemini` as an alias for `google`. Unknown -> `Err` listing valid names.
- `select(cli_provider: Option<ProviderId>, cli_model: Option<&str>) -> Result<Box<dyn Provider>, ProviderError>`:
  1. id = `cli_provider` else parse `GCM_PROVIDER` else `ProviderId::Groq` (default). A **non-empty** invalid `GCM_PROVIDER` is a fatal `ProviderError` (not a silent default); an **empty/whitespace** `GCM_PROVIDER` is treated as unset -> default (Gemini review P2.8, resilience).
  2. model = `cli_model` (non-empty after trim) else per-provider env (non-empty) else the provider default. An empty/whitespace `--model` falls through to env-then-default (Gemini review P1.5) - it is never treated as a literal model id. Per-provider model env vars: `GCM_GROQ_MODEL`, `GCM_OPENAI_MODEL`, and for Google **both `GCM_GEMINI_MODEL` and `GCM_GOOGLE_MODEL`** are read (round-2 review pt 4 - the API key is `GEMINI_API_KEY`, so a user will reach for `GCM_GEMINI_*`; `GCM_GEMINI_*` is the documented primary and `GCM_GOOGLE_*` an accepted alias, primary winning if both are set). Same dual-name rule for the base-URL override.
  3. construct the concrete backend with `model` (no key read here - keys read lazily in `generate_*`).
- No client-side model-id validation in v1 (Gemini review P3.11): an unknown/unsupported model is rejected by the provider API as a `BadRequest` with the provider's own actionable message; gcm does not maintain a model allowlist.
- Both steps are pure (env + args), no network/key, so the cache path and `--dry-run` resolve a provider without a key.

### Error type (`src/provider/mod.rs`) - generalize `GroqError`

```rust
pub struct ProviderError { pub provider: &'static str, pub kind: ErrorKind }
pub enum ErrorKind {
    MissingKey { env_var: &'static str },   // fatal
    RateLimit { retry_after: Option<Duration> }, // retryable
    Auth { status: u16, env_var: &'static str }, // fatal
    BadRequest { detail: Option<String> },  // not retried
    Server(u16),                            // retryable (incl. 504)
    Http(u16),                              // not retried
    Timeout,                                // not retried
    Transport(String),                      // not retried
    EmptyResponse,                          // not retried
    Deserialize(String),                    // not retried
}
```

- `Display` names the provider + the correct env var: `MissingKey` -> "{provider} API key is not set. Export {env_var}=... and retry."; `Auth` -> "{provider} rejected the API key (HTTP {status}); check that {env_var} is valid and not expired."; others mirror today's wording with "{provider}" substituted.
- `is_retryable`/`retry_after_hint` match on `kind` only (`RateLimit | Server` retryable - unchanged from CLO-488).
- `classify_status(status, retry_after, detail, env_var) -> ErrorKind` stays pure (same 400/401|403/429/5xx/other mapping incl. 504->Server); the HTTP layer wraps with the active provider name.
- `error.rs`: `GcmError::Groq(GroqError)` -> `GcmError::Provider(ProviderError)`; `From<ProviderError>`; `Display` delegates. `main.rs` fatal routing: `kind` is `MissingKey | Auth` -> `Fatal`, else `Fallback` (unchanged logic, new matcher).

### Shared transport (`src/provider/http.rs`)

- Moves CLO-488's `RetryConfig`/`from_env`/`is_retryable`/`retry_after_hint`/`backoff_delay`/`retry_with`/`classify_status`/`parse_retry_after`/`bad_request_detail`/`truncate`/`map_ureq_error` out of `groq.rs`, retyped to `ProviderError`. `MAX_ERROR_BODY_BYTES=4096`, retry defaults `GCM_RETRY_MAX/BASE_MS/MAX_MS` unchanged.
- **`TIMEOUT_SECS` moves here and is bumped 30 -> 60s** (round-2 review pt 2): reasoning models (`o1`/`o3`-style) and large diffs routinely take 45-90s to first token; a 30s `ureq` global timeout reliably kills them before they finish. 60s is the v1 floor; `GCM_HTTP_TIMEOUT_SECS` env override added for power users / very slow models. Still shared across providers (per-provider/per-model timeouts deferred).
- `post_json(provider, auth, endpoint, payload) -> Result<String, ProviderError>` - one HTTP attempt wrapped in `retry_with`; returns the raw 2xx body. `auth: (&str, &str)` is a `(header_name, header_value)` tuple passed straight to `ureq`'s `.header(name, value)` (round-2 review pt 5): Groq/OpenAI pass `("Authorization", &format!("Bearer {key}"))`, Gemini passes `("x-goog-api-key", key)` - no in-`http.rs` string parsing.

### OpenAI-compatible backends (`groq.rs`, `openai.rs`)

- Endpoint `{base}/chat/completions`; header `Authorization: Bearer {key}`.
- Request: `{model, temperature:0.2, messages:[{system},{user}], response_format:{type:"json_schema", json_schema:{name:"commit_plan", strict:<bool>, schema: plan::schema()}}}` for the plan; plain (no `response_format`) for the message. `strict:true` for Groq gpt-oss and OpenAI gpt-4o-mini; Groq qwen uses `strict:false` (best-effort - matrix).
- Response: `choices[0].message.content` (existing `first_choice_content`), `<think>`-stripped, then `parse_defensive` for the plan.
- Reasoning suppression: Groq = existing `apply_reasoning_suppression` (qwen `reasoning_effort:"none"`, gpt-oss `include_reasoning:false`). OpenAI = nothing for `gpt-4o-mini` (non-reasoning, the default). Defaults: Groq base `https://api.groq.com/openai/v1`, OpenAI base `https://api.openai.com/v1`.
- **OpenAI reasoning-family (`o1`/`o3`/`o4`-style) compatibility** (round-2 review pt 1): the o-series is **not** drop-in chat-compatible. When the resolved model matches a reasoning family, the OpenAI payload builder must (a) **omit `temperature`** (o-series 400s on any non-default temperature), (b) **not send a `system` role** - fold the system prompt into the `user` message (or send it as `developer`), and (c) set `reasoning_effort` (e.g. `"low"`). `gpt-4o-mini` (the ADR-locked default) takes none of this and is the only OpenAI model verified for this slice; o-series support is best-effort for `--model` overrides, hardened so the call does not 400 on `temperature`/`system`. A still-unsupported combination surfaces as a typed `BadRequest` with the API's own message (no client-side allowlist).

### Gemini backend (`src/provider/gemini.rs`) - divergent shape

- Endpoint `{base}/v1beta/models/{model}:generateContent`; header `x-goog-api-key: {key}` (key from `GEMINI_API_KEY`). Default base `https://generativelanguage.googleapis.com` (override `GCM_GEMINI_BASE_URL` primary / `GCM_GOOGLE_BASE_URL` alias).
- Request: `{ systemInstruction:{parts:[{text: <grouping/system prompt>}]}, contents:[{role:"user", parts:[{text: <user content>}]}], generationConfig:{ responseMimeType:"application/json", responseSchema:<gemini_schema()>, thinkingConfig:{thinkingLevel:"MINIMAL"} } }`. The message call omits `responseSchema`/`responseMimeType`.
- `plan::gemini_schema()` (lives in `src/plan.rs` next to the existing `plan::schema()`, Gemini review P1.3, for consistency + shared unit testing): OpenAPI-3.0-subset form (matrix: `$ref`/`allOf`/`oneOf`/`not` ignored, no `additionalProperties`): `{type:"OBJECT", properties:{groups:{type:"ARRAY", items:{type:"OBJECT", properties:{files:{type:"ARRAY",items:{type:"STRING"}}, summary:{type:"STRING"}, commit_message:{type:"STRING", nullable:true}}, required:["files","summary","commit_message"], propertyOrdering:["files","summary","commit_message"]}}}, required:["groups"]}`. (Distinct from `plan::schema()`, which is JSON-Schema strict for the OpenAI-compatible shape.)
- Response extractor (round-2 review pt 3 - check status BEFORE extracting content):
  1. **Prompt-level block**: if `promptFeedback.blockReason` is set, surface a non-retryable `BadRequest { detail: "Gemini blocked the prompt (reason: <blockReason>)" }`.
  2. **Candidate `finishReason`**: read `candidates[0].finishReason`. `SAFETY`/`RECITATION`/`BLOCKLIST`/`PROHIBITED_CONTENT` -> non-retryable `BadRequest { detail: "Gemini blocked the response (finishReason: <reason>)" }` (Gemini returns `200 OK` with **no** `content` block in these cases, so a blind `parts` extract would yield a confusing `EmptyResponse`). `MAX_TOKENS` -> `BadRequest`/`Deserialize` with a clear truncation message.
  3. Otherwise concatenate `candidates[0].content.parts[*].text` for parts where `thought != true` (drop thought parts); then `<think>` strip + `parse_defensive`. A genuinely empty `STOP`-finish response -> `EmptyResponse`.
  - All field access is defensive (missing `candidates`/`content`/`parts` -> a typed error, never an `unwrap` panic).
- Reasoning: `thinkingLevel:"MINIMAL"` (3.x floor; no hard off) + thought-part filtering + `<think>` strip + JSON-mode schema = no CoT in output (matrix caveat: validate app-side regardless).

### Diff budget (`src/diff.rs`)

- `pub struct DiffBudget { pub total_bytes: usize, pub per_file_bytes: usize }`. `gather_for_grouping`/`gather`/`gather_for_files` take a `&DiffBudget` instead of the module constants `MAX_TOTAL_BYTES`/`PER_FILE_DIFF_BYTES` (untracked caps stay as-is or scale with total). Env overrides `GCM_DIFF_TOTAL_BYTES`/`GCM_DIFF_PER_FILE_BYTES` apply across providers (parsed non-empty `usize`, else the provider default). `main.rs` selects the provider first, then gathers with `provider.diff_budget()`. Concrete per-provider defaults (Gemini review P1.1/P2.9):

  | Provider | `total_bytes` | `per_file_bytes` | Rationale |
  |----------|---------------|------------------|-----------|
  | Groq     | `350_000`     | `8192`           | Current defaults - behavioral parity (O3) |
  | Google   | `350_000`     | `8192`           | `gemini-3.1-flash-lite` ~1M-token window easily fits this; Groq defaults for v1 (raise in a later slice if useful) |
  | OpenAI   | `256_000`     | `8192`           | `gpt-4o-mini` 128k-token window -> tighter total; per-file unchanged |

### Cache fingerprint integration (`src/cache.rs`, Gemini review P1.2)

- Today `cache.rs:29` has `const PROVIDER: &str = "groq"` folded into the digest (`digest_fingerprint`, `cache.rs:185`) alongside `model` (`cache.rs:187`). Remove the hardcoded `PROVIDER` fold; instead pass `provider.cache_model_id()` (a provider-qualified string, e.g. `"groq:openai/gpt-oss-120b"`) as the existing `model: &str` argument to `cache::load/save/advance`. The provider is thus embedded in the fingerprint via `cache_model_id`, so switching provider OR model re-analyzes.
- The cache **key**/file location (`repo_key`/`cache_file_name` = `sha256(repo-root)`) is untouched - FR-25 ("shared across providers for the same repo") preserved. Only the freshness fingerprint (FR-27) gains provider awareness. Bump `FINGERPRINT_VERSION` (`cache.rs:25`) since the fold changes.

### main.rs wiring

- `execute`: `let provider = provider::select(args.provider, args.model.as_deref())?;` once, near the top (after the repo/changes checks). Replace `groq::resolved_model()` -> `provider.cache_model_id()`; `groq::generate_plan(&ctx)` -> `provider.generate_plan(&ctx)`; `groq::generate_commit_message(..)` -> `provider.generate_message(..)` (build_plan, commit_first_group, single_commit_flow take `&dyn Provider`). Diff gathering uses `provider.diff_budget()`. Fatal/fallback routing matches on `ProviderError.kind`.

### Timeouts (note)

- `TIMEOUT_SECS` (30s) stays **shared across providers** in v1 (Gemini review P2.7). Per-provider timeouts (e.g. a slower Gemini thinking pass) are a future refinement, not this slice.

### cli.rs

- Add `#[arg(long, value_enum)] pub provider: Option<ProviderId>` and `#[arg(long)] pub model: Option<String>`. Update the `about`/egress disclosure to mention provider selection (no longer "via Groq" only).

---

## 4. Decomposition

1. **`provider` module skeleton + error generalization** - new `src/provider/mod.rs`: `Provider` trait, `ProviderId` (+ `FromStr`/`ValueEnum`/unknown-name error), `ProviderError{provider,kind}` + `ErrorKind` + `Display`, `is_retryable`/`retry_after_hint`. files: `src/provider/mod.rs`, `src/error.rs` (`Provider` variant + `From`). TDD: name parsing, `Display` distinctness + env-var naming, `is_retryable`. Verify `From<ProviderError> for GcmError` keeps `GcmError::Provider(e)` displaying the provider-qualified kind message verbatim (Gemini review P3.10).
2. **Shared transport** - move CLO-488 retry/classify/body-cap helpers into `src/provider/http.rs`, retyped to `ProviderError`; `post_json`. files: `src/provider/http.rs`. TDD: `classify_status`, `parse_retry_after`, `bad_request_detail`, `backoff_delay`, `retry_with` (injected sleeper) - port existing CLO-488 tests.
3. **Selection + model resolution** - `select()` + `resolve_*` (flag>env>default; model flag>env>default; no key/network). files: `src/provider/mod.rs`. TDD: precedence matrices, unknown provider, key-free resolution.
4. **Groq backend on the trait** - move `src/groq.rs` -> `src/provider/groq.rs`, implement `Provider` (reuse `apply_reasoning_suppression`, `build_plan_payload`, `first_choice_content`, `strip_think`, `resolved_model` -> `cache_model_id`), call shared `http::post_json`. files: `src/provider/groq.rs`. TDD: payload shape (port existing), `cache_model_id`, message vs plan path.
5. **OpenAI backend** - `src/provider/openai.rs`: OpenAI-compatible, `strict:true` for `gpt-4o-mini`, Bearer auth, default `gpt-4o-mini-2024-07-18`, base `api.openai.com/v1`, reasoning suppression only for reasoning families. files: `src/provider/openai.rs`. TDD: payload (strict json_schema), extractor reuse, no-reasoning default.
6. **Gemini backend** - `src/provider/gemini.rs`: `generateContent` endpoint, `x-goog-api-key`, `responseSchema` via `plan::gemini_schema()`, `thinkingConfig.thinkingLevel:MINIMAL`, parts/thought extractor. files: `src/provider/gemini.rs`, `src/plan.rs` (add `gemini_schema()` beside `schema()`). TDD: payload shape, `gemini_schema` shape, multi-part + thought-filtered extractor, thought-only -> EmptyResponse.
7. **Per-provider diff budget** - `DiffBudget` in `src/diff.rs`; gather fns take `&DiffBudget`; env overrides; per-provider defaults via `diff_budget()`. files: `src/diff.rs`, backends. TDD: budget resolution + env override; gather honors budget.
8. **CLI + main wiring + cache fingerprint** - `--provider`/`--model` flags; `provider::select` once in `execute`; trait-dispatched calls; `cache_model_id` into the fingerprint; fatal/fallback match on `kind`. files: `src/cli.rs`, `src/main.rs`. Parity check: bare `gcm` unchanged.
9. **Acceptance harness + docs** - extend `scripts/acceptance.sh` mock with a Gemini `:generateContent` route + OpenAI-compatible route; cases for `--provider=google`/`--provider=openai` grouped commit, unknown provider, `--model` override, missing-key per provider, Gemini safety-block (`finishReason:SAFETY` -> actionable error); `GCM_GEMINI_BASE_URL`/`GCM_GOOGLE_BASE_URL`/`GCM_OPENAI_BASE_URL`. Update README + `--help`/egress text. files: `scripts/acceptance.sh`, `README.md`. **Cache isolation** (round-2 review pt 6): keep the existing hermetic pattern - each case runs in a fresh `mktemp -d` repo (unique repo-root -> unique cache key) under the shared throwaway `GCM_CACHE_DIR` (`acceptance.sh:36`), so the mock is always hit. The new provider-folded fingerprint additionally busts any cross-provider hit on the same tree; pass `--reset` on any case that deliberately re-runs the same tree across a provider/model switch to make intent explicit.

**Dependency order**: 1 -> 2 -> 3 (mod skeleton, transport, selection) form the core. 4 (Groq) depends on 1-2 and is the parity anchor. 5 (OpenAI) and 6 (Gemini) depend on 1-4 and are mutually independent. 7 (budget) depends on the trait (1). 8 (wiring) depends on 3-7. 9 (acceptance/docs) depends on 8. Suggested sequence 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7 -> 8 -> 9.

---

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | `ProviderId` parse canonical + alias | `groq`/`google`/`openai`/`gemini`(->Google) parse; `foo` -> Err listing valid names | `cargo test provider::` |
| 2 | Provider selection precedence | flag wins over `GCM_PROVIDER` over default `groq`; invalid `GCM_PROVIDER` -> Err | `cargo test select_` |
| 3 | Model resolution precedence | `--model` > `GCM_<P>_MODEL` > provider default; resolves with no key | `cargo test resolve_model` |
| 4 | `ProviderError` Display | each variant distinct + non-empty; `MissingKey`/`Auth` name the correct env var per provider | `cargo test error_display` |
| 5 | Shared retry on `ProviderError` | 429/5xx retried, 400/auth not, `Retry-After` honored, body capped (ported CLO-488 tests green) | `cargo test http::` |
| 6 | Groq payload parity | plan payload still `json_schema`/`strict:true`/`commit_plan`; gpt-oss `include_reasoning:false` | `cargo test groq::` |
| 7 | OpenAI payload | `response_format` strict json_schema; default `gpt-4o-mini-2024-07-18`; Bearer; no reasoning param for gpt-4o-mini | `cargo test openai::` |
| 8 | Gemini payload | `generateContent` body has `responseSchema`(OpenAPI shape)+`responseMimeType`+`thinkingLevel:MINIMAL`; `x-goog-api-key` | `cargo test gemini::payload` |
| 9 | Gemini extractor | concatenates non-thought parts; drops `thought:true`; empty -> `EmptyResponse`; `<think>` stripped | `cargo test gemini::extract` |
| 10 | `gemini_schema` shape | `type:"OBJECT"`, `commit_message` `nullable:true`, `required` lists all, no `additionalProperties` | `cargo test gemini_schema` |
| 11 | Diff budget | per-provider default (OpenAI tighter); env override; gather truncates to budget | `cargo test diff_budget` |
| 12 | Parity: bare `gcm` | identical to pre-refactor (Groq, default model, grouping/cache) | `scripts/acceptance.sh` existing cases |
| 13 | AC: `--provider=google` grouped commit | one signed commit via mock-Gemini route | `scripts/acceptance.sh` AC-489-google |
| 14 | AC: `--provider=openai` grouped commit | one signed commit via mock-OpenAI route | `scripts/acceptance.sh` AC-489-openai |
| 15 | AC: `--model` override | mock receives the overridden model id | `scripts/acceptance.sh` AC-489-model |
| 16 | AC: unknown provider | exit 1, error lists valid names, no network call | `scripts/acceptance.sh` AC-489-unknown |
| 17 | AC: missing key per provider | `--provider=google` w/o `GEMINI_API_KEY` -> fatal naming `GEMINI_API_KEY`; same for openai | `scripts/acceptance.sh` AC-489-missingkey |
| 18 | Reasoning suppression | mock asserts Gemini `thinkingLevel:MINIMAL` present; committed message/plan carry no `<think>`/CoT | `scripts/acceptance.sh` AC-489-reasoning |
| 19 | Provider alias parse | `ProviderId::from_str("gemini")` -> `Ok(Google)`; `"GOOGLE"`/`" google "` accepted (case/space-insensitive) | `cargo test provider_alias` |
| 20 | Empty `--model` fallthrough | `--model ""`/`"  "` falls through to per-provider env then default; never a literal model id | `cargo test resolve_model_empty` |
| 21 | Gemini thought-only response | `candidates[0].content.parts` all `thought:true` (no answer text) -> `EmptyResponse`, no panic | `cargo test gemini::extract_thought_only` |
| 22 | OpenAI o-series payload (round-2 pt1) | for an `o1`/`o3` model the payload has **no** `temperature`, **no** `system` role (folded to user/developer), and `reasoning_effort` set; `gpt-4o-mini` keeps `temperature` + `system` | `cargo test openai::o_series_payload` |
| 23 | Gemini safety block (round-2 pt3) | `finishReason:"SAFETY"` (or `promptFeedback.blockReason`) -> non-retryable error naming the reason; no panic; not `EmptyResponse` | `cargo test gemini::safety_block` |
| 24 | Gemini env alias (round-2 pt4) | `GCM_GEMINI_MODEL` and `GCM_GOOGLE_MODEL` both read; `GCM_GEMINI_*` wins if both set; same for base-URL | `cargo test resolve_model_gemini_alias` |
| 25 | HTTP timeout (round-2 pt2) | default 60s; `GCM_HTTP_TIMEOUT_SECS` override honored | `cargo test http_timeout` |
| 26 | `post_json` auth tuple (round-2 pt5) | Groq/OpenAI send `Authorization: Bearer ...`, Gemini sends `x-goog-api-key`; asserted by the mock's captured headers | `scripts/acceptance.sh` header capture |

**Edge cases to verify**:
- `GCM_PROVIDER=GOOGLE` (uppercase) and surrounding whitespace - parse is case- and space-insensitive (decided); empty/whitespace `GCM_PROVIDER` -> treated as unset -> default `groq`.
- `--provider=openai --model=gpt-4o-mini` while `GCM_GROQ_MODEL` is set - the Groq env must NOT leak into the OpenAI model (per-provider env only).
- Gemini returns the plan as a bare top-level array (no `{groups}`) - `parse_defensive` already recovers it (shared).
- Gemini response with only a `thought:true` part (no answer) -> `EmptyResponse`, not a panic.
- A 4096-byte-capped Gemini error body still yields a `BadRequest.detail` (shared http path).
- Bare `gcm` with an unreachable Groq still retries 5xx then fails exactly as CLO-488 (parity).
- Cache: run `gcmq` then `gcm --provider=google` on the same unchanged tree - the fingerprint folds provider+model so the second run re-analyzes (not a stale Groq plan), while the cache file/key is unchanged (FR-25).
