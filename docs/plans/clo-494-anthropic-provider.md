# Plan: CLO-494 — Add Anthropic provider via forced tool-use

## Context

- **Design:** docs/designs/clo-494-anthropic-provider.md
- **Discovery:** docs/discovery/clo-494.md
- **PRD:** docs/prds/clo-494-anthropic-provider.md
- **Linear:** https://linear.app/cloud-ai/issue/CLO-494/add-anthropic-provider-via-forced-tool-use
- **Branch:** `feat/clo-494-anthropic`
- **Approach:** Forced tool-use (`tools` + `tool_choice:{type:"tool"}` + `input_schema` = `plan::schema()`)
- **Default model:** `claude-haiku-4-5`
- **Endpoint:** `https://api.anthropic.com/v1/messages`
- **Auth:** `x-api-key` + `anthropic-version: 2023-06-01` (extra header)

## Sub-tasks

### ST1 Extend `HttpRequest` with `extra_headers` field

**Files:** `src/provider/http.rs`

**Description:** Add `extra_headers: Vec<(&'static str, String)>` to the
`HttpRequest` struct. Update `send_once` to iterate over `req.extra_headers`
and call `.header(name, value)` for each, after the auth header and
`Content-Type` but before `.send()`. The field defaults to `Vec::new()` in
every existing caller — no behavioral change for existing providers.

**Acceptance:** `cargo test` passes (all existing HTTP unit tests unchanged).
The `HttpRequest` struct compiles with the new field. `send_once` sends extra
headers (verified via integration test with a local mock server in ST4).

**Estimate:** S

### ST2 Update existing providers with `extra_headers: Vec::new()`

**Files:** `src/provider/groq.rs`, `src/provider/gemini.rs`, `src/provider/openai.rs`

**Description:** Add `extra_headers: Vec::new(),` to the `HttpRequest`
construction in each provider's `request()` method. This is a one-line change
per file — the struct literal now requires the new field.

**Acceptance:** `cargo test` passes. Each provider's `request()` compiles with
the new field. No behavioral change (empty vec = no extra headers sent).

**Estimate:** S

### ST3 Add `ProviderId::Anthropic` to `mod.rs`

**Files:** `src/provider/mod.rs`

**Description:** Add `Anthropic` variant to the `ProviderId` enum. Add match
arms to:
- `default_model()` → `"claude-haiku-4-5"`
- `model_env_vars()` → `&["GCM_ANTHROPIC_MODEL"]`
- `select()` → `Box::new(anthropic::Anthropic::new(model))`
- `pick_provider_id` error message → append `"anthropic"` to the list

Add `mod anthropic;` to the module declarations.

**Acceptance:**
- `ProviderId::parse("anthropic")` → `Some(ProviderId::Anthropic)`
- `pick_provider_id(None, Some("anthropic"))` → `ProviderId::Anthropic`
- `default_model()` for Anthropic → `"claude-haiku-4-5"`
- `model_env_vars()` for Anthropic → `&["GCM_ANTHROPIC_MODEL"]`
- `pick_provider_id(None, Some("bogus"))` error string contains `"anthropic"`
- `cargo test` passes

**Estimate:** S

### ST4 Create `src/provider/anthropic.rs` — full module

**Files:** `src/provider/anthropic.rs` (NEW)

**Description:** Implement the full Anthropic provider module:

1. **Struct:** `Anthropic { model: String }` with `Anthropic::new(model)`.

2. **Payload builders:**
   - `build_plan_payload(ctx: &GroupingContext) -> Value`: Returns a JSON
     payload with `model`, `max_tokens: 4096`, `system` (shared
     `GROUPING_SYSTEM_PROMPT`), `messages` (single user message from
     `grouping_user_content`), `tools` (one tool: `name: "commit_plan"`,
     `description: "Return the commit grouping plan"`,
     `input_schema: plan::schema()`), and `tool_choice: {type: "tool",
     name: "commit_plan"}`.
   - `build_message_payload(diff: &GatheredDiff) -> Value`: Returns a JSON
     payload with `model`, `max_tokens: 1024`, `system` (shared
     `SYSTEM_PROMPT`), `messages` (single user message from
     `message_user_content`). No `tools` or `tool_choice`.

3. **Response parsers:**
   - `extract_tool_use_input(provider, raw) -> Result<String, ProviderError>`:
     Deserializes the Anthropic Messages API response. Finds the first
     `content` block with `type: "tool_use"` and returns its `input` as a
     JSON string. Handles edge cases:
     - `stop_reason: "max_tokens"` → `Deserialize` error with truncation
       message
     - `stop_reason: "end_turn"` with text blocks → extract text for
       `parse_defensive` fallback
     - `stop_reason: "refusal"` → `BadRequest` error
     - Empty content → `EmptyResponse`
     - Skips `thinking` content blocks
   - `extract_text_content(provider, raw) -> Result<String, ProviderError>`:
     Deserializes the response, concatenates all `text` content blocks,
     applies `strip_think()`, trims. Returns empty string if no text blocks.

4. **Provider trait impl:**
   - `name()` → `"Anthropic"`
   - `generate_plan()`: Builds plan payload via `build_plan_payload`, creates
     `HttpRequest` with `auth: ("x-api-key", key)`, `extra_headers:
     [("anthropic-version", "2023-06-01")]`, calls `http::post_json`, then
     `extract_tool_use_input`. On success, attempts direct
     `serde_json::from_value::<Plan>(input)` first; falls back to
     `plan::parse_defensive(&json_string)`.
   - `generate_message()`: Builds message payload via `build_message_payload`,
     creates `HttpRequest` (same auth/headers), calls `http::post_json`, then
     `extract_text_content`.
   - `cache_model_id()` → `"anthropic:<model>"`
   - `diff_budget()` → `DiffBudget::standard()`

5. **Helpers:**
   - `base_url()`: Reads `GCM_ANTHROPIC_BASE_URL` env var, defaults to
     `https://api.anthropic.com`.
   - `api_key()`: Reads `ANTHROPIC_API_KEY` env var, returns
     `MissingKey` error if unset/blank.

6. **Unit tests (14 tests):**
   - `build_plan_payload` shape (tools, tool_choice, input_schema = plan::schema)
   - `build_message_payload` shape (no tools/tool_choice)
   - `extract_tool_use_input` happy path
   - `extract_tool_use_input` end_turn fallback
   - `extract_tool_use_input` refusal
   - `extract_tool_use_input` max_tokens
   - `extract_tool_use_input` empty content
   - `extract_tool_use_input` skips thinking blocks
   - `extract_tool_use_input` direct deserialization
   - `extract_text_content` happy path
   - `extract_text_content` skips thinking
   - `cache_model_id`
   - `base_url` override
   - `api_key` missing

**Acceptance:** `cargo test` passes. All 14 unit tests in `anthropic.rs` pass.
`cargo clippy --all-targets -- -D warnings` passes.

**Estimate:** L

### ST5 Update `cli.rs` help text

**Files:** `src/cli.rs`

**Description:** Update the `EGRESS_DISCLOSURE` string to include `anthropic`
in the provider list, `GCM_ANTHROPIC_MODEL` in the model env list, and
`ANTHROPIC_API_KEY` in the key list. Update the `--provider` doc comment to
list `anthropic`.

**Acceptance:** `cargo test` passes. `gcm --help` output contains `"anthropic"`
in the provider list, `"GCM_ANTHROPIC_MODEL"` in the model env list, and
`"ANTHROPIC_API_KEY"` in the key list.

**Estimate:** S

## Pre-merge gate

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## Risks

| # | Risk | Mitigation |
|---|---|---|
| R1 | `plan::schema()` JSON Schema with `type: ["string","null"]` is rejected by Anthropic's `input_schema` | The schema is standard JSON Schema (draft-07 compatible). Anthropic's API accepts JSON Schema for `input_schema`. If rejected, the API returns HTTP 400 with a clear error message. Fallback: simplify the schema to remove `null` from the union. |
| R2 | `anthropic-version: 2023-06-01` header is deprecated | Verified against Anthropic API docs (2026-06-22). If deprecated, the API returns HTTP 400 with a clear message. The version string is a single constant in `anthropic.rs` — trivial to update. |
| R3 | Default model `claude-haiku-4-5` is invalid/renamed | Integration test catches this. The model string is a single constant in `mod.rs` — trivial to update. |
| R4 | `max_tokens: 4096` is insufficient for large diffs | The `extract_tool_use_input` parser checks `stop_reason: "max_tokens"` and returns a descriptive error. If this occurs in practice, bump the constant. |
| R5 | Anthropic API rate limits on the free tier | The shared retry/backoff engine handles 429s. The primary user has a paid API key. |

## Ordering

1. **ST1** (foundation — HttpRequest must be extended first)
2. **ST2** (mechanical consequence of ST1 — existing providers need the new field)
3. **ST3** (enum must exist before the module can be wired in)
4. **ST4** (the bulk of the work — depends on ST1 + ST3)
5. **ST5** (cosmetic — can be done last, no dependencies on other STs)

Total estimate: 1S + 1S + 1S + 1L + 1S = **1L + 4S**