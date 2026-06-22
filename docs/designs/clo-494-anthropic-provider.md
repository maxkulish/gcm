# Design: CLO-494 — Add Anthropic provider via forced tool-use

**Linear:** [CLO-494](https://linear.app/cloud-ai/issue/CLO-494/add-anthropic-provider-via-forced-tool-use)
**Discovery:** [docs/discovery/clo-494.md](../discovery/clo-494.md)
**ADR:** [ADR-001](../adrs/001-foundational-architecture-decisions.md) Decisions 2, 3
**Approach:** Forced tool-use (`tools` + `tool_choice:{type:"tool"}` + `input_schema`)

## 1. Problem

The Rust rewrite of `gcm` currently supports three providers (Groq, Google
Gemini, OpenAI) behind the `Provider` trait from CLO-489. The primary user's
personal default is Anthropic Haiku — currently served by the subscription
`claude` CLI in the bash tool. ADR-001 Decision 3 mandates the direct Messages
API only (no `claude` CLI in the Rust runtime), and Anthropic has no generic
`response_format` like the OpenAI-compatible providers. Structured output must
instead be obtained via forced tool-use: define a tool whose `input_schema` is
the Plan schema, force the call with `tool_choice: {type:"tool"}`, and extract
the typed plan from the `tool_use` content block. This is the fourth backend,
closing the personal-cutover path and completing FR-13's active provider matrix.

## 2. Goals / Non-goals

### Goals

- `Anthropic` struct implementing `Provider` in `src/provider/anthropic.rs`
- Forced tool-use for the grouping plan (`generate_plan`)
- Plain Messages API call for per-group message regeneration (`generate_message`)
- `ANTHROPIC_API_KEY` env-var resolution with clear missing-key error (FR-18)
- `anthropic-version` header support (requires extending `HttpRequest`)
- `ProviderId::Anthropic` variant + `--provider=anthropic` / `GCM_PROVIDER=anthropic`
- `GCM_ANTHROPIC_MODEL` env var + a sensible default model
- `GCM_ANTHROPIC_BASE_URL` env var override (for proxy/testing)
- Reasoning/thinking output never reaches the commit message (FR-17)
- Updated CLI help text listing `anthropic` as a provider option

### Non-goals

- Onboarding wizard integration (separate task)
- `output_config.format` JSON outputs (newer API, only on Opus 4.8 / Sonnet 4.6 /
  Haiku 4.5 — tool-use is universally available and the issue title specifies it)
- Streaming (ADR-001 Decision 2: blocking HTTP)
- Multi-turn tool-use agentic loop (single-shot: one tool, one forced call)

## 3. Architecture

### Modules

```
src/provider/
├── mod.rs         ← ProviderId enum, select(), resolve_model(), shared helpers
├── anthropic.rs   ← NEW: Anthropic backend (forced tool-use)
├── gemini.rs      ← existing (reference for divergent API shape)
├── groq.rs        ← existing (OpenAI-compatible)
├── http.rs        ← MODIFIED: HttpRequest gains extra_headers field
└── openai.rs      ← existing (OpenAI-compatible)
```

### Data flow

#### generate_plan (structured output via forced tool-use)

```
GroupingContext
  → build_plan_payload(ctx)
      → { model, max_tokens, system, messages, tools:[{name, description, input_schema}],
          tool_choice:{type:"tool", name:"commit_plan"} }
  → http::post_json(request)
  → extract_tool_use_input(raw)           ← NEW response parser
      → deserialize content[].type=="tool_use" → .input (JSON object)
      → if no tool_use block: check stop_reason → typed error or EmptyResponse
  → plan::parse_defensive(&json)           ← shared defensive parser
  → Plan
```

#### generate_message (plain text, no tool-use)

```
GatheredDiff
  → build_message_payload(diff)
      → { model, max_tokens, system, messages }
  → http::post_json(request)
  → extract_text_content(raw)             ← NEW: pull text from content blocks
  → strip_think()                         ← shared backstop
  → String (commit message)
```

### Key design decisions

#### 3a. `HttpRequest` extension for `anthropic-version` header

The current `HttpRequest` struct carries one auth header via `auth:
(&'static str, String)`. Anthropic requires two custom headers:

1. `x-api-key: <ANTHROPIC_API_KEY>` (authentication)
2. `anthropic-version: 2023-06-01` (API version negotiation, required by all
   Messages API calls)

**Change:** Add an `extra_headers: Vec<(&'static str, String)>` field to
`HttpRequest` (defaults to empty `Vec::new()` in every existing backend). The
`send_once` function adds these headers after the auth header. This is backward-
compatible: existing backends pass `extra_headers: Vec::new()` and their requests
are byte-identical.

```rust
pub(super) struct HttpRequest<'a> {
    pub provider: &'static str,
    pub auth_env_var: &'static str,
    pub endpoint: String,
    pub auth: (&'static str, String),
    pub extra_headers: Vec<(&'static str, String)>,  // NEW
    pub payload: &'a Value,
}
```

In `send_once`:
```rust
let mut response = agent
    .post(&req.endpoint)
    .header(req.auth.0, req.auth.1.as_str())
    .header("Content-Type", "application/json");
for (name, value) in &req.extra_headers {
    response = response.header(name, value.as_str());
}
response = response.send(body.as_str())
```

#### 3b. Anthropic request shape (plan)

```json
{
  "model": "claude-haiku-4-5",
  "max_tokens": 4096,
  "system": "<GROUPING_SYSTEM_PROMPT>",
  "messages": [
    { "role": "user", "content": "<grouping_user_content>" }
  ],
  "tools": [{
    "name": "commit_plan",
    "description": "Return the commit grouping plan",
    "input_schema": <plan::schema()>
  }],
  "tool_choice": { "type": "tool", "name": "commit_plan" }
}
```

`plan::schema()` is reused directly — it is standard JSON Schema, which
Anthropic's `input_schema` accepts.

#### 3c. Anthropic response shape (plan)

```json
{
  "content": [
    { "type": "tool_use", "id": "toolu_...", "name": "commit_plan",
      "input": { "groups": [...] } }
  ],
  "stop_reason": "tool_use"
}
```

The `input` field is a JSON object matching the Plan schema. We serialize the
`input` value to a string and feed it to `plan::parse_defensive()` (same path
as every other provider).

If `content` contains no `tool_use` block (model refused or returned text):
- If `stop_reason` is `"max_tokens"`, return a `Deserialize` error: "Anthropic
  response truncated (stop_reason: max_tokens); the diff may be too large"
  (matches Gemini's `MAX_TOKENS` handling in `gemini.rs`).
- If `stop_reason` is `"end_turn"` and there are `text` blocks, extract the text
  and try `parse_defensive()` as a fallback (some models wrap the JSON in prose).
- If `stop_reason` is `"refusal"`, return a `BadRequest` error.
- If no content at all, return `EmptyResponse`.

**Direct deserialization first (review suggestion 3):** The `tool_use` content
block's `input` field is already a parsed `serde_json::Value`. Rather than
serializing it back to a string and calling `parse_defensive()`, first attempt
`serde_json::from_value::<Plan>(input.clone())`. If that succeeds, return the
plan directly. Only if it fails, serialize to a string and fall back to
`parse_defensive()` (handles models that emit slightly non-standard shapes that
`parse_defensive`'s recovery logic can fix).

#### 3d. Anthropic request shape (message)

```json
{
  "model": "claude-haiku-4-5",
  "max_tokens": 1024,
  "system": "<SYSTEM_PROMPT>",
  "messages": [
    { "role": "user", "content": "<message_user_content>" }
  ]
}
```

No `tools`, no `tool_choice` — a plain text completion. The response has `text`
content blocks; we concatenate them and apply `strip_think()`.

#### 3e. Reasoning suppression

Anthropic's adaptive thinking (`thinking: {type: "adaptive"}`) omits CoT
`display` output by default. We do NOT send `thinking` in the request — the
default behavior already suppresses visible CoT. The universal `strip_think()`
backstop handles any leakage (a model that emits `dimd...` despite the default).

If a future reasoning model requires explicit suppression, the payload can add
`"thinking": {"type": "disabled"}` — but this is not needed for the current
model matrix (Haiku 4.5, Sonnet 4.6, Opus 4.8) and is left as a future
enhancement point.

#### 3f. ProviderId::Anthropic

```rust
pub enum ProviderId {
    Groq,
    #[value(alias = "gemini")]
    Google,
    Openai,
    Anthropic,  // NEW
}
```

- `default_model()`: `"claude-haiku-4-5"` (matches the bash tool's Haiku default)
- `model_env_vars()`: `&["GCM_ANTHROPIC_MODEL"]`
- `parse("anthropic")` → `Some(ProviderId::Anthropic)` (automatic via `#[value(rename_all = "lower")]`)

#### 3g. Endpoint and auth

- Endpoint: `https://api.anthropic.com/v1/messages` (overridable via
  `GCM_ANTHROPIC_BASE_URL`)
- Auth header: `("x-api-key", key)` — different from Groq/OpenAI's `Bearer` and
  Gemini's `x-goog-api-key`
- Extra header: `("anthropic-version", "2023-06-01")` — required by the API

#### 3h. Diff budget

`DiffBudget::standard()` — the Anthropic Messages API has a large context
window (200K tokens for Haiku/Sonnet). The standard budget is sufficient for v1;
no per-provider customization needed.

## 4. Public API surface

### New: `src/provider/anthropic.rs`

```rust
pub struct Anthropic {
    model: String,
}

impl Anthropic {
    pub fn new(model: String) -> Self;
}

impl Provider for Anthropic {
    fn name(&self) -> &'static str;           // "Anthropic"
    fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError>;
    fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError>;
    fn cache_model_id(&self) -> String;        // "anthropic:<model>"
    fn diff_budget(&self) -> DiffBudget;        // DiffBudget::standard()
}
```

### Modified: `src/provider/mod.rs`

```rust
pub enum ProviderId {
    Groq,
    #[value(alias = "gemini")]
    Google,
    Openai,
    Anthropic,  // NEW
}

// default_model() match arm:
ProviderId::Anthropic => "claude-haiku-4-5",

// model_env_vars() match arm:
ProviderId::Anthropic => &["GCM_ANTHROPIC_MODEL"],

// select() match arm:
ProviderId::Anthropic => Box::new(anthropic::Anthropic::new(model)),

// pick_provider_id error message:
"unknown provider '{t}'. Set --provider/GCM_PROVIDER to one of: groq, google, openai, anthropic."
```

### Modified: `src/provider/http.rs`

```rust
pub(super) struct HttpRequest<'a> {
    pub provider: &'static str,
    pub auth_env_var: &'static str,
    pub endpoint: String,
    pub auth: (&'static str, String),
    pub extra_headers: Vec<(&'static str, String)>,  // NEW
    pub payload: &'a Value,
}
```

### Modified: `src/provider/{groq,gemini,openai}.rs`

Each backend's `request()` method adds `extra_headers: Vec::new()` to the
`HttpRequest` construction. No behavioral change.

### Modified: `src/cli.rs`

```rust
/// LLM provider: groq (default), google (Gemini), openai, or anthropic.
/// Overrides GCM_PROVIDER (precedence: flag > env > default).
```

Plus the `EGRESS_DISCLOSURE` help text: add `anthropic` to the provider list,
`GCM_ANTHROPIC_MODEL` to the model env list, and `ANTHROPIC_API_KEY` to the key
list.

## 5. Assumptions

| # | Assumption | Confidence | Verification |
|---|---|---|---|
| A1 | `plan::schema()` (JSON Schema with `type: ["string","null"]` for `commit_message`) is accepted as-is by Anthropic's `input_schema` without modification. | high | Unit test: build the payload with `plan::schema()` and verify the API returns `stop_reason: "tool_use"` with a valid plan. Manual integration test against the live API. |
| A2 | `tool_choice: {type: "tool", name: "commit_plan"}` forces the model to call the tool on every request, never producing a plain-text refusal or `end_turn` without a tool call. | high | Integration test: run `gcm --provider=anthropic --dry-run` on a sample diff and confirm the response always contains a `tool_use` block. |
| A3 | The `anthropic-version: 2023-06-01` header value is stable and required for all Messages API calls; newer API versions won't break this value. | medium | Verified against Anthropic API docs (2026-06-22). If Anthropic deprecates this version, the error will surface as an HTTP 400 with a clear message. |
| A4 | Adaptive thinking (the default, no explicit `thinking` parameter) suppresses visible chain-of-thought in the response content blocks. The `strip_think()` backstop handles any leakage. | high | Unit test: verify `extract_tool_use_input` skips `thinking` content blocks. Integration test: confirm no CoT in the tool input. |
| A5 | The default model `claude-haiku-4-5` is a valid Anthropic model ID accepted by the Messages API. | medium | Integration test. If the model ID is wrong, the API returns HTTP 400 with a model-not-found message, which surfaces as a clear `BadRequest` error. |
| A6 | `max_tokens: 4096` is sufficient for the grouping plan response (the plan JSON is typically 200-2000 tokens). | high | Integration test with a large diff (20+ files). If truncated, the API returns `stop_reason: "max_tokens"` and we surface a `Deserialize` error. |
| A7 | The existing `HttpRequest` struct can be extended with `extra_headers` without breaking the retry/backoff engine or response classification. | high | Unit test: all existing HTTP tests pass unchanged. The `send_once` function adds extra headers after the auth header, before `.send()`. |

## 6. Test plan

### Unit tests (in `anthropic.rs`)

1. **`build_plan_payload` shape:** Verify the payload has `tools` with
   `name: "commit_plan"`, `input_schema` = `plan::schema()`, and
   `tool_choice: {type: "tool", name: "commit_plan"}`. Verify `system` and
   `messages` contain the shared prompts.

2. **`build_message_payload` shape:** Verify the payload has NO `tools` or
   `tool_choice` — just `model`, `max_tokens`, `system`, `messages`.

3. **`extract_tool_use_input` happy path:** A response with
   `content: [{type: "tool_use", input: {groups: [...]}}]` yields the `input`
   JSON string.

4. **`extract_tool_use_input` no tool_use block (end_turn):** A response with
   only `text` content blocks and `stop_reason: "end_turn"` → extract text,
   return it for `parse_defensive` fallback.

5. **`extract_tool_use_input` refusal:** `stop_reason: "refusal"` → `BadRequest`
   error with the refusal text.

6. **`extract_tool_use_input` max_tokens:** `stop_reason: "max_tokens"` →
   `Deserialize` error mentioning truncation (matches Gemini's handling).

7. **`extract_tool_use_input` empty content:** `content: []` → `EmptyResponse`.

8. **`extract_tool_use_input` skips thinking blocks:** A response with both
   `thinking` and `tool_use` content blocks → only the `tool_use` block is
   extracted; `thinking` is ignored.

9. **`extract_tool_use_input` direct deserialization:** When `input` is a valid
   Plan JSON value, `from_value::<Plan>` succeeds directly without falling back
   to `parse_defensive`.

10. **`extract_text_content` happy path:** A response with
    `content: [{type: "text", text: "feat: add thing"}]` yields the text, with
    `strip_think` applied.

11. **`extract_text_content` skips thinking:** A response with both `thinking`
    and `text` blocks → only `text` blocks are concatenated.

12. **`cache_model_id`:** `Anthropic::new("claude-haiku-4-5")` →
    `"anthropic:claude-haiku-4-5"`.

13. **`base_url` override:** `GCM_ANTHROPIC_BASE_URL` set → endpoint uses it;
    unset → defaults to `https://api.anthropic.com`.

14. **`api_key` missing:** `ANTHROPIC_API_KEY` unset → `MissingKey` error naming
    `ANTHROPIC_API_KEY`.

### Unit tests (in `mod.rs`)

15. **`ProviderId::parse("anthropic")`** → `Some(ProviderId::Anthropic)`.

16. **`pick_provider_id(None, Some("anthropic"))`** → `ProviderId::Anthropic`.

17. **`default_model()` for Anthropic** → `"claude-haiku-4-5"`.

18. **`model_env_vars()` for Anthropic** → `&["GCM_ANTHROPIC_MODEL"]`.

19. **Error message lists anthropic:** `pick_provider_id(None, Some("bogus"))`
    error string contains `"anthropic"`.

### Unit tests (in `http.rs`)

20. **`extra_headers` in `HttpRequest`:** Since `send_once` makes real network
    calls (no mock layer), this is verified via integration test 25 (base URL
    override to a local mock server that records request headers). Existing
    unit tests pass unchanged with `extra_headers: Vec::new()` (no behavioral
    change).

### Integration tests (require `ANTHROPIC_API_KEY`)

21. **End-to-end plan:** `gcm --provider=anthropic --dry-run` on a sample diff
    produces a valid typed plan with correct grouping.

22. **End-to-end message:** `gcm --provider=anthropic --all --dry-run` produces
    a conventional commit message.

23. **Missing key error:** `ANTHROPIC_API_KEY= gcm --provider=anthropic --dry-run`
    produces a clear error: "Anthropic API key is not set. Export it (e.g.
    `export ANTHROPIC_API_KEY=...`) and retry."

24. **Model override:** `GCM_ANTHROPIC_MODEL=claude-sonnet-4-6 gcm --provider=anthropic
    --dry-run` uses the overridden model (verify via `--dry-run` cache fingerprint
    or `GCM_DEBUG=1`).

25. **No reasoning in output:** Run a full `gcm --provider=anthropic` commit and
    verify the commit message contains no `dimd...` blocks or chain-of-thought.

### Manual tests

26. **Base URL override:** `GCM_ANTHROPIC_BASE_URL=http://localhost:8080 gcm
    --provider=anthropic --dry-run` routes to the local endpoint (verify via
    `GCM_DEBUG=1` or a local mock server).

27. **Large diff:** Run `gcm --provider=anthropic --dry-run` on a 20+ file diff
    and verify the plan is not truncated (`stop_reason: "tool_use"`, not
    `"max_tokens"`).

## 7. Migration / rollout

This is a purely additive change — no existing behavior is modified:

- New `ProviderId::Anthropic` variant (no change to existing variants)
- New `anthropic.rs` module (no change to existing modules)
- `HttpRequest.extra_headers` defaults to empty (byte-identical requests for
  existing providers)
- CLI help text updated to list `anthropic` (informational only)

**No migration needed.** Users who don't set `--provider=anthropic` or
`GCM_PROVIDER=anthropic` see no change. The shipped default remains Groq
(ADR-001 Decision 5).

**Rollout:** The primary user (Max) sets `GCM_PROVIDER=anthropic` (or aliases
`gcm --provider=anthropic`) after the binary is installed. No config migration,
no breaking changes.

## 8. Open questions

All resolved by ADR-001 and the capability matrix:

- **Auth model:** Direct Messages API with `ANTHROPIC_API_KEY` (ADR-001 Decision 3)
- **Structured output:** Forced tool-use (issue title + ADR capability matrix)
- **Reasoning suppression:** Default adaptive thinking + `strip_think()` backstop
- **Default model:** `claude-haiku-4-5` (matches the bash tool's Haiku default)
- **HTTP transport:** Blocking, via shared `http::post_json()` (ADR-001 Decision 2)
- **`anthropic-version` header:** Required, sent as `2023-06-01` (verified against
  Anthropic API docs, 2026-06-22)

No open questions remain.