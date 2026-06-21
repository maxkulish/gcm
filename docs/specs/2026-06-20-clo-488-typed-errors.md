# Spec: Resilient provider calls - typed error taxonomy + bounded retry/backoff + defensive parsing

**Created**: 2026-06-20
**Task**: [CLO-488](https://linear.app/cloud-ai/issue/CLO-488) (slice S4)
**Estimated scope**: M (~6 files touched, ~6 sub-tasks)
**Extends**: CLO-486 single-commit tracer ([spec](2026-06-19-clo-486-single-commit-tracer.md)) and CLO-487 grouping ([spec](2026-06-20-clo-487-semantic-grouping.md)); architecture locked by [ADR-001](../adrs/001-foundational-architecture-decisions.md) Decision 2 (blocking `ureq` client, no async runtime) and Decision 3 ("typed errors (FR-21) and retries (FR-22)" named as a driver). Capability matrix (Appendix A) records Groq's 429/400 / `strict` json_schema semantics.
**Covers FR**: 20 (defensive fallback parsing, Should), 21 (typed error taxonomy, Must), 22 (retry with backoff on transient errors, Must)

---

## 1. Problem Statement

The merged provider layer (`src/groq.rs`) collapses every failure mode into a coarse, mostly-untyped surface and never retries. `GroqError` (`src/groq.rs:39-47`) has `MissingKey`, `Http(u16)`, `Timeout`, `Transport`, `EmptyResponse`, `Deserialize` - its own doc comment says it is "a light taxonomy for the tracer; the full typed-error/retry surface (FR-21/22) lands in CLO-488." Today an HTTP 429 (rate limit), an HTTP 400 (malformed request / unsupported parameter), and an HTTP 503 (transient server blip) are all indistinguishable: `send_chat` sets `http_status_as_error(true)` (`src/groq.rs:105`) so every non-2xx becomes `ureq::Error::StatusCode(code)` -> `GroqError::Http(code)` (`src/groq.rs:213`), with no body, no `Retry-After` header, and no retry. This reproduces the exact failure the rewrite exists to fix: the bash tool's `2>/dev/null` collapsed every failure to "empty response" and routed everything to the single-commit fallback (PRD §"Problems", line 25), so a rate limit looked identical to a real bug and there was no way to retry, back off, or diagnose.

Three concrete gaps this slice closes:

1. **No typed taxonomy (FR-21, Must).** The tool cannot distinguish rate limit (429), bad request (400), server error (5xx), timeout, auth failure (401/403), and parse failure. The PRD success metric (line 45) requires ">= 4 distinguishable error types surfaced in logs and exit behavior"; today there is effectively one (`Http(code)` plus the I/O variants).

2. **No retry/backoff (FR-22, Must).** A 429 or 5xx aborts the grouping call immediately and falls straight to single-commit, when a short bounded backoff would let the transient condition self-heal with no user-visible failure. There is currently zero retry logic anywhere in `src/groq.rs`.

3. **Brittle plan parsing (FR-20, Should).** `generate_plan` (`src/groq.rs:161-170`) does a single `serde_json::from_str::<Plan>` on the model content. With Groq `strict: true` json_schema that almost always conforms, but the moment structured output is unavailable or the model wraps/fences its JSON, the parse fails outright and the run degrades to single-commit. FR-20 wants a layered defensive extractor (direct -> markdown-fence strip -> wrapper-key unwrap -> generic `groups` search) so a recoverable plan is still recovered, with the `<think>` strip remaining only as a last-resort defense (already applied upstream in `first_choice_content`, `src/groq.rs:123-133`).

**Who is affected**: every gcm user on a free/rate-limited Groq tier (the shipped default, ADR-001 Decision 5), and anyone hitting a transient provider blip mid-run. **What triggers it**: a 429/5xx/400/timeout/auth response, or a non-strict/wrapped model response. **Why it matters**: rate limits and blips must self-heal instead of masquerading as tool bugs, and real failures (bad key, malformed request) must fail fast with an actionable message instead of looping or silently collapsing.

**This slice is Groq-only.** The provider trait and non-Groq backends are CLO-489; the full plan validation and richer fallback policy are CLO-492. CLO-488 hardens the existing single Groq provider call in place, behind the same blocking `ureq` client (ADR-001 Decision 2).

---

## 2. Acceptance Criteria

Mapped from the Linear task's acceptance criteria, made testable:

- [ ] **AC-1 (429 retries then succeeds, no user-visible failure)**: When the provider returns HTTP 429 for the first N (< max) attempts and then 200, gcm retries with backoff and ultimately succeeds, exiting 0 with the normal output and **no** error printed to the user. The captured request count equals N+1 (proving the retries actually happened).
- [ ] **AC-2 (400 fails fast, no retry loop)**: An HTTP 400 response is **not** retried - gcm issues exactly one request to that endpoint, fails fast, and prints an actionable, 400-specific message (distinct from a parse failure or a rate limit). Verified by request count == 1 on the single-commit (`--all`) path.
- [ ] **AC-3 (auth fails fast, actionable, no retry loop)**: An HTTP 401/403 response is **not** retried (exactly one request on the `--all` path), exits non-zero, and the message names the API key as the thing to check (`GROQ_API_KEY`).
- [ ] **AC-4 (5xx retries with bounded backoff, then gives up)**: An endpoint that always returns 5xx is retried up to the bounded attempt count (`max_retries`), then surfaces a 5xx-specific server-error message and a non-zero exit on the single-commit path. Request count == `max_retries + 1`. (HTTP 504 is included in the 5xx retryable class - it is `Server(504)`, not the client-side `Timeout` variant.)
- [ ] **AC-5 (typed taxonomy, >= 6 distinct types)**: `GroqError` distinguishes at least rate-limit (429), bad-request (400), auth (401/403), server (5xx), timeout, and parse/deserialize as separate variants, each with a distinct `Display` string. A unit test asserts the six core variants produce six different, non-empty messages.
- [ ] **AC-6 (retry policy is correct and pure)**: Unit tests on the pure policy prove: 429 and 5xx are retryable; 400, auth, parse, timeout, transport, missing-key, empty-response are **not**; the backoff schedule for attempts 0..n is `min(base * 2^attempt, max)`; a present `Retry-After` (429 only) is honored as `min(retry_after, max)`.
- [ ] **AC-7 (retry loop drives the policy without real sleeps)**: A unit test with an injected sleeper and a scripted sequence of results proves: 429x2 then Ok -> Ok with 2 recorded sleeps and 3 op calls; 400 -> Err with 0 sleeps and 1 op call; 5xx x(max+1) -> Err with `max` sleeps and `max+1` op calls; recorded sleep durations match the backoff schedule.
- [ ] **AC-8 (error type visible in debug logs)**: With `GCM_DEBUG` set, each classified provider error and each retry attempt is written to stderr including the typed variant (e.g. the `{:?}` form contains `RateLimit` / `BadRequest` / `Server`). With `GCM_DEBUG` unset, no `[debug]` lines are emitted. Verified by grepping stderr in `acceptance.sh`.
- [ ] **AC-9 (defensive plan parsing, FR-20)**: `plan::parse_defensive` recovers a typed `Plan` from: (a) direct strict JSON; (b) a ```` ```json ```` markdown-fenced block; (c) prose followed by one or more fenced/inline JSON blocks (recovers from the first valid one); (d) a wrapper object `{"commit_plan": {...}}` / `{"plan": {...}}`; (e) a nested object exposing a `groups` array under another key (precedence: top-level -> wrapper keys -> recursive search); and returns a `PlanError::Parse` (not a panic) for unrecoverable garbage, whose `Display` renders `plan parse error: <msg>`. `<think>` blocks are already stripped upstream and must not break recovery.
- [ ] **AC-10 (transient retries are side-effect-free)**: All retries happen strictly before any index mutation or commit. On final failure the index/working tree is byte-identical to the pre-run state (the FR-47 transaction is preserved by the existing `snapshot_index`/`restore_index` path; retries add no new mutation).
- [ ] **AC-11 (no new dependencies, no async, quality gates)**: No new crate is added to `Cargo.toml`; no `tokio`/async is introduced (ADR-001 Decision 2). `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` are clean; `scripts/acceptance.sh` passes including the new retry/taxonomy cases.

**Verification method**: unit tests (`cargo test`) for the pure taxonomy/policy/parse logic (status classification, `Retry-After` parse, `is_retryable`, `backoff_delay`, the `retry_with` loop via an injected sleeper, `parse_defensive` layers, `Display` distinctness); `scripts/acceptance.sh` integration cases driven by an extended stateful mock-Groq server (a counter-backed `/retry429/` route, plus `/fail400/`, `/fail401/`) for AC-1/2/3/4/8; `GCM_RETRY_BASE_MS=1` exported in `acceptance.sh` so real backoff sleeps are sub-millisecond.

---

## 3. Constraints

**Must**:
- Replace the coarse `GroqError::Http(u16)` surface with a typed taxonomy that distinguishes at least: `RateLimit` (429), `BadRequest` (400), `Auth` (401/403), `Server` (5xx), `Timeout` (client-side), `Transport` (connection/IO), plus the existing `MissingKey`, `EmptyResponse`, and `Deserialize` (parse). Keep a catch-all `Http(u16)` for any other unexpected non-2xx. Each variant must have a distinct, actionable `Display` string (FR-21).
- Switch `send_chat` to `http_status_as_error(false)` so non-2xx responses are inspected directly: read `response.status()`, the `Retry-After` header (case-insensitive lookup; seconds form), and a **bounded** best-effort body string (for the 400 detail message). Classify via a **pure** `classify_status(status, retry_after, detail) -> GroqError` helper (unit-testable, no network). Pre-response transport failures (`.send()` Err: connection refused, DNS, client timeout) keep mapping through `map_ureq_error` to `Timeout`/`Transport`.
- **Cap the error-body read at 4096 bytes** (review High): a non-2xx response can be a large HTML error page (e.g. a Cloudflare 502/504), so read it through a limited reader - `response.body_mut().as_reader().take(4096).read_to_string(&mut s)` - never unbounded, to avoid memory/latency blowup. (The 2xx success-body read is unchanged.)
- **Extract an actionable `detail` for `BadRequest`** (review Medium): Groq 400s return `{"error":{"message":"..."}}`; attempt to parse the capped body as JSON and pull `error.message` into `BadRequest.detail`; if that fails, fall back to the raw body truncated to <=200 chars. This keeps the CLI message clean and names the real cause (e.g. an unsupported parameter).
- **Look up `Retry-After` case-insensitively** (review Medium): HTTP header names are case-insensitive; use ureq's case-insensitive header getter (or check both `Retry-After`/`retry-after`) so a lowercased header is not missed.
- Retry **only** transient errors with bounded exponential backoff (FR-22): the retryable set is **`RateLimit` (429) and `Server` (5xx)**. `BadRequest` (400), `Auth` (401/403), `Timeout`, `Transport`, `EmptyResponse`, `Deserialize`, `MissingKey`, and catch-all `Http` are **never** retried. Decision via a pure `is_retryable(&GroqError) -> bool`.
- Bound the retry: default `max_retries = 3` (so up to 4 total attempts), `base = 500ms`, per-sleep cap `max = 8s`. Backoff for 0-based `attempt` = `min(base * 2^attempt, max)`. When a 429 carries a parseable `Retry-After` (seconds), honor `min(retry_after, max)` instead (capped so a large `Retry-After` can never hang the CLI); 5xx uses pure exponential backoff. All three knobs are overridable via env (`GCM_RETRY_MAX`, `GCM_RETRY_BASE_MS`, `GCM_RETRY_MAX_MS`) for testability and power users; defaults baked in.
- Sleep via `std::thread::sleep` (blocking client, ADR-001 Decision 2 - no async runtime). The retry loop (`retry_with`) must take an **injected sleeper** typed `mut sleep: impl FnMut(Duration)` (and `mut op: impl FnMut() -> Result<T, GroqError>`) so a unit test's sleeper can record durations into a captured `Vec<Duration>` directly, with no `RefCell`/`Mutex` interior-mutability boilerplate (review Medium). The loop is generic over the result type and operates on `GroqError`.
- Wrap **only the HTTP round-trip** (`send_chat_once`) in the retry loop. Response parsing (`first_choice_content`, plan deserialization, empty-content detection) happens **after** a successful HTTP response and is **not** retried (parse/empty failures are deterministic, not transient).
- Implement FR-20 defensive plan parsing in `src/plan.rs` as `parse_defensive(content: &str) -> Result<Plan, PlanError>` using a **candidate-and-recover** strategy - NOT a single linear fence-strip + first-brace extract, which is brittle (round-2 review points 1-3, all empirically confirmed: `from_str::<Value>` on prose-with-JSON fails outright, naive multi-fence stripping concatenates blocks into invalid JSON, and "first balanced brace" grabs a decoy `{...}` in leading prose and never reaches the real block). Build an **ordered candidate list** and return the **first** candidate that yields a valid `Plan`:
  - **Candidates, in order:** (a) the inner content of each ```` ```json ````/```` ``` ```` fenced block, in document order (extract the text *between* fence markers per block - never delete the markers and concatenate); (b) every **balanced `{...}` object substring** found by a brace-depth scan over the whole content - the scan MUST be string- and escape-aware (a `{` or `}` inside a JSON `"..."` string, or after a `\`, does not change depth) so a path containing a brace cannot truncate the candidate - in document order; (c) the whole trimmed content as a final candidate.
  - **Recovery, per candidate, in order:** (1) `serde_json::from_str::<Plan>(cand)`; (2) `serde_json::from_str::<Value>(cand)` then `recover_groups(&value) -> Option<Value>` returning the `groups` **array**, then `serde_json::from_value::<Plan>(json!({ "groups": arr }))`. **The recovered value is a bare array and MUST be re-wrapped as `{"groups": arr}` before `from_value` - `Plan` is a struct, so `from_value::<Plan>(array)` fails with a type-mismatch error (empirically confirmed: `Err("invalid type: map, expected a sequence")`).**
  - **`recover_groups` precedence:** top-level `groups` array -> a `groups` array under a wrapper key tried in order `commit_plan` -> `plan` -> `result` -> `data` -> `response` -> a depth-first recursive search for the first object key named `groups` whose value is an array.
  - If no candidate yields a `Plan`, return `PlanError::Parse(String)`.
  - Add a `PlanError::Parse(String)` variant **and its `Display` arm** (`write!(f, "plan parse error: {msg}")`). The `<think>` strip stays upstream in `first_choice_content` (last-resort defense), so all candidates are already think-free.
- **Map `PlanError` -> `GroqError::Deserialize` in `generate_plan`** (review Medium, resolves the signature mismatch): `generate_plan` returns `Result<Plan, GroqError>`, so it calls `plan::parse_defensive(&json).map_err(|e| GroqError::Deserialize(e.to_string()))` instead of the inline `serde_json::from_str`.
- Make the typed error visible in debug logs (AC of the task): add a minimal env-gated debug helper (`src/debug.rs`: `enabled()` reads `GCM_DEBUG`; `log(&str)` prints `gcm: [debug] ...` to stderr). Emit a debug line (a) at the point each non-2xx is classified in `send_chat_once`, and (b) on each retry attempt in `retry_with`, both including the `{:?}` form of the error so the variant name is visible. In `main.rs run()`, when debug is enabled, also print `gcm: [debug] {e:?}` for the final error.
- Route the new fatal variants in `main.rs build_plan` (`src/main.rs:95-99`): `MissingKey | Auth(_)` are `Fatal` (a missing/invalid key fails the single-commit fallback identically - do not pretend to recover); every other `GroqError` stays `Fallback` (unchanged behavior - the simpler message call may still succeed where the json_schema plan call did not).
- Preserve all existing behavior: `include_reasoning: false` / `reasoning_effort` suppression (`apply_reasoning_suppression`), the `strip_think` backstop, the 30s `TIMEOUT_SECS`, the shared `send_chat` between `generate_commit_message` and `generate_plan`, and the exit-code contract (0 ok/abort, 1 runtime error, 2 usage).

**Must-not**:
- Must not add any new crate dependency (no `tokio`, no `rand`, no `tracing`/`log`). Use only `std` (`thread::sleep`, `time::Duration`, `env`) plus the already-present `serde`/`serde_json`/`ureq`. Binary-size / cold-start NFRs and ADR-001 Decision 2 forbid an async runtime.
- Must not add a logging framework, log levels, or `--json`/structured logging - that is CLO-493 (FR-37/38). The `GCM_DEBUG` stderr helper is the minimal vehicle for "error type visible in debug logs" and may be superseded by CLO-493.
- Must not introduce a provider trait or any non-Groq backend - that is CLO-489 (FR-11). The retry/taxonomy live in `src/groq.rs` operating on `GroqError`; note in a comment that CLO-489 may lift retry into the provider layer.
- Must not change the plan **schema** or `validate_basic` **semantics** (CLO-487 owns FR-23 basic; CLO-492 owns FR-23 full). `parse_defensive` only changes how the JSON string becomes a `Plan`; the recovered plan still flows through the unchanged `validate_basic` in `main.rs`.
- Must not change the single-commit fallback **policy** beyond the `Auth`->`Fatal` routing above (CLO-492 owns the richer fallback). Do not add new fallback branches.
- Must not retry timeouts or transport errors in this slice (a 30s client timeout already waited long; retrying up to 4x risks a multi-minute CLI hang). Broadening the retryable set to transient network errors is an explicit non-goal here (possible follow-up).
- Must not jitter the backoff with a random source (would add a dependency and break deterministic tests). Deterministic exponential backoff is sufficient for a single-client CLI; note jitter as a possible future enhancement.

**Prefer**:
- Keep the retry policy as small free functions + a `RetryConfig` struct in `src/groq.rs` (not a new generic abstraction). CLO-489's provider trait is the right place to generalize retry; do it then, not now.
- Honor `Retry-After` on 429 only (Groq sends it on rate limits); let 5xx use exponential backoff. Honoring it for 503 too is a reasonable later refinement, not required here.
- Carry an optional `detail: Option<String>` on `BadRequest` (the provider's error-body message) so the 400 message can be actionable (e.g. surfacing an unsupported-parameter error), and an optional `retry_after: Option<Duration>` on `RateLimit`.
- Reuse the existing mock-Groq acceptance harness; extend it with stateful routes rather than introduce a new test mechanism.
- Keep `Display` messages short, specific, and actionable (name the HTTP code and the next action). Reserve verbose diagnostics for the `GCM_DEBUG` path.

**Escalate when**:
- A Groq HTTP 400 indicates `strict: true` json_schema (or another required parameter) is unsupported for the configured model at runtime (capability drift vs ADR-001 Decision 5 / the CLO-487 escalation). Surface it via the `BadRequest` detail message; do **not** silently downgrade to non-strict. (Runtime 400 still falls back to single-commit per existing routing, which is AC-7 of CLO-487; the escalation is about not masking it.)
- Implementing the spec would require touching the provider abstraction (CLO-489), the plan schema / full validation (CLO-492), or the cache (CLO-491) - that is scope creep into another slice.

---

## 3b. Error Taxonomy + Retry Policy (concrete contract)

*The taxonomy and policy must be concrete in the spec so implementation is mechanical.*

**HTTP status -> `GroqError` classification** (pure `classify_status`):

| HTTP status | `GroqError` variant | Retryable | Display intent |
|-------------|---------------------|-----------|----------------|
| 400 | `BadRequest { detail }` | no | "Groq rejected the request (HTTP 400)[: detail]. Likely an unsupported model/parameter or a gcm bug; please report." |
| 401, 403 | `Auth(u16)` | no | "Groq rejected the API key (HTTP {code}); check that GROQ_API_KEY is valid and not expired." |
| 429 | `RateLimit { retry_after }` | **yes** | "Groq rate limit reached (HTTP 429); wait a moment and retry, or use a different provider." |
| 500-599 | `Server(code)` | **yes** | "Groq server error (HTTP {code}); this is usually transient - retry shortly." |
| other non-2xx | `Http(code)` | no | "Groq API returned HTTP {code}." |

> **Display wording (round-2 review point 5):** the `RateLimit`/`Server` messages deliberately do NOT claim "retries were exhausted" - that would be inaccurate at the `GCM_RETRY_MAX=0` boundary (first failure, zero retries permitted). The retry *count*/trace lives in the `GCM_DEBUG` log, not the user-facing message.
>
> **504 classification (round-2 review point 4):** HTTP 504 (Gateway Timeout, common from CDN/load balancers) arrives as status `504` and is classified `Server(504)` (**retryable**) by the `500..=599` arm. Do NOT special-case it to the client-side `Timeout` variant despite "timeout" in its reason phrase - only a client-side `ureq` timeout maps to `Timeout`.

Non-HTTP variants (unchanged semantics, refined messages): `MissingKey` (config, fatal, not retried), `Timeout` (client-side 30s timeout, not retried), `Transport(msg)` (connection/IO, not retried), `EmptyResponse` (2xx with no usable content, not retried), `Deserialize(msg)` (response/plan parse failure, not retried). (Naming note: the variant is `Auth(u16)`; `Auth(code)` in a `Display` arm and `Auth(_)` in a routing match are just bindings of that one variant.)

**Error-body read (capped)**: on any non-2xx, read the body through a limited reader capped at 4096 bytes (`response.body_mut().as_reader().take(4096).read_to_string(..)`) - never unbounded (review High; guards against large HTML error pages).

**`BadRequest` detail extraction**: parse the capped body as JSON and take `error.message`; on failure, use the raw body truncated to <=200 chars. Stored on `BadRequest.detail`.

**`Retry-After` parsing**: read the `Retry-After` response header **case-insensitively** on a 429; parse the integer-seconds form (`header.trim().parse::<u64>().ok().map(Duration::from_secs)`); the HTTP-date form parses to `None` (falls back to exponential). Stored on `RateLimit.retry_after`.

**Retry policy** (pure functions + `RetryConfig`):
- `RetryConfig { max_retries: u32 = 3, base: Duration = 500ms, max: Duration = 8s }`, with `from_env()` reading `GCM_RETRY_MAX` (u32), `GCM_RETRY_BASE_MS` (u64 ms), `GCM_RETRY_MAX_MS` (u64 ms); invalid/absent -> default.
- `is_retryable(&GroqError) -> bool`: `true` for `RateLimit{..}` and `Server(_)`; `false` otherwise.
- `retry_after_hint(&GroqError) -> Option<Duration>`: the `RateLimit.retry_after`; `None` otherwise.
- `backoff_delay(attempt: u32, hint: Option<Duration>, cfg: &RetryConfig) -> Duration`: if `hint` is `Some(d)` -> `d.min(cfg.max)`; else `cfg.base.saturating_mul(2u32.pow(attempt.min(16))).min(cfg.max)`.

**Retry loop** (generic, injected sleeper - `FnMut` so a recording test sleeper needs no interior mutability):
```
fn retry_with<T>(cfg: &RetryConfig, mut sleep: impl FnMut(Duration), mut op: impl FnMut() -> Result<T, GroqError>)
    -> Result<T, GroqError>
// attempt = 0; loop { match op() {
//   Ok(v) => return Ok(v),
//   Err(e) if attempt < cfg.max_retries && is_retryable(&e) => {
//       let d = backoff_delay(attempt, retry_after_hint(&e), cfg);
//       debug::log(.. attempt+1, e:?, d ..); sleep(d); attempt += 1; }
//   Err(e) => return Err(e),
// } }
```
Production wiring: `send_chat(...) = retry_with(&RetryConfig::from_env(), std::thread::sleep, || send_chat_once(key, base_url, payload))`. `send_chat_once` is the current `send_chat` body, minus `http_status_as_error(true)`, plus status/header/body inspection and `classify_status`.

**Defensive plan parse** (FR-20, `plan::parse_defensive`): **candidate-and-recover** - build an ordered candidate list (each fenced block's inner content -> every string/escape-aware balanced `{...}` substring -> the whole trimmed content) and return the first candidate that yields a `Plan` via `from_str::<Plan>` or via `recover_groups` (top-level -> wrapper key -> recursive `groups`) re-wrapped as `from_value::<Plan>(json!({"groups": arr}))`; else `Err(Parse)`. `<think>` already stripped upstream.

---

## 4. Decomposition

1. **Debug helper** - new `src/debug.rs`: `pub fn enabled() -> bool` (`GCM_DEBUG` set and not `""`/`"0"`); `pub fn log(msg: &str)` (prints `gcm: [debug] {msg}` to stderr when enabled). Module decl in `src/main.rs`. Leaf module; land first so later sub-tasks can log. Unit test: `enabled()` reflects the env var. - files: `src/debug.rs`, `src/main.rs`
2. **Typed taxonomy + status inspection** - `src/groq.rs`: redesign `GroqError` (add `RateLimit{retry_after}`, `Auth(u16)`, `BadRequest{detail}`, `Server(u16)`; keep `MissingKey`/`Http`/`Timeout`/`Transport`/`EmptyResponse`/`Deserialize`) with distinct actionable `Display`; pure `classify_status`; `parse_retry_after` (case-insensitive header lookup); a `bad_request_detail(body)` helper that pulls `error.message` from JSON or truncates raw to <=200 chars; rewrite `send_chat` -> `send_chat_once` with `http_status_as_error(false)`, status inspection, a **4096-byte-capped** error-body read, `classify_status`, and a classification debug log. Update `map_ureq_error` (drop the now-unused `StatusCode` happy path but keep it defensive). Unit tests: `classify_status` for 400/401/403/429/500/418; `parse_retry_after` integer vs date vs absent vs lowercased header; `bad_request_detail` JSON-message vs raw-truncate; six-variant `Display` distinctness (AC-5). - files: `src/groq.rs`
3. **Retry policy + loop** - `src/groq.rs`: `RetryConfig` (+`from_env`), `is_retryable`, `retry_after_hint`, `backoff_delay`, generic `retry_with(cfg, mut sleep: impl FnMut(Duration), mut op: impl FnMut())`; wire `send_chat = retry_with(.., std::thread::sleep, send_chat_once)`. Unit tests (AC-6/AC-7): retryable set; backoff schedule incl. cap; `Retry-After` honoring + cap; loop via an injected recording `FnMut` sleeper (pushes to a captured `Vec<Duration>`, no `RefCell`) for 429x2->Ok, 400->Err, 5xx-exhaustion (assert op-call count, sleep count, sleep durations). - files: `src/groq.rs`
4. **Defensive plan parsing (FR-20)** - `src/plan.rs`: add `PlanError::Parse(String)` **and its `Display` arm** (`"plan parse error: {msg}"`); `parse_defensive(content) -> Result<Plan, PlanError>` via the **candidate-and-recover** strategy (ordered candidates: per-fence inner content -> string/escape-aware balanced `{...}` substrings -> whole content; per-candidate recovery: `from_str::<Plan>` then `recover_groups` re-wrapped as `from_value::<Plan>(json!({"groups": arr}))`); `recover_groups` precedence top-level -> wrapper keys `commit_plan`/`plan`/`result`/`data`/`response` -> DFS. `generate_plan` in `src/groq.rs` calls it and maps `PlanError -> GroqError::Deserialize(e.to_string())`. Unit tests (AC-9): direct, fenced, **multi-fence**, **prose with a decoy `{...}` before the real block**, wrapper-key, nested-key, **bare-array re-wrap**, garbage->Parse, and the `Parse` `Display` string. - files: `src/plan.rs`, `src/groq.rs`
5. **main.rs routing + final-error debug** - `src/main.rs`: `build_plan` match -> `MissingKey | Auth(_) => Fatal`, `other => Fallback`; in `run()`, when `debug::enabled()`, print `gcm: [debug] {e:?}` alongside the user message. Confirm the index transaction is untouched (no code change needed; AC-10 is a property of the existing snapshot/restore path - retries are pre-staging). - files: `src/main.rs`
6. **Acceptance harness + docs** - `scripts/acceptance.sh`: extend the mock with a counter-backed `/retry429/` route (429 for the first 2 calls via a counter file, then the normal 200), `/fail400/` (400 + JSON error body), `/fail401/` (401); export `GCM_RETRY_BASE_MS=1` near the top so retries are sub-ms; add cases AC-1 (429->success via `--all --dry-run`, assert 3 captured requests + exit 0 + no error), AC-2 (`/fail400/ --all` -> exactly 1 request + exit 1 + 400 message), AC-3 (`/fail401/ --all` -> 1 request + exit 1 + key message), AC-4 (`/fail500/ --all` -> `max+1` requests + exit 1), AC-8 (`GCM_DEBUG=1` -> stderr contains the variant name; unset -> none). Document retries + `GCM_DEBUG`/`GCM_RETRY_*` in `--help`/README. - files: `scripts/acceptance.sh`, `src/cli.rs` (help text), `README.md`

**Dependency order**: 1 (debug) is a leaf - land first. 2 (taxonomy) before 3 (retry needs the variants) and before 4 (`generate_plan` change touches the same file). 4 is otherwise independent of 3. 5 depends on 2. 6 depends on 1-5. Suggested sequence: 1 -> 2 -> 3 -> 4 -> 5 -> 6.

---

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Mock returns 429 twice then 200 (`/retry429/`), `gcm --all --dry-run` | Exit 0; message printed; **no** error to user; capture shows 3 requests | `acceptance.sh` AC-1 (counter mock) |
| 2 | `gcm --all` against `/fail400/` | Exit 1; exactly **1** request captured; stderr has a 400-specific message (not "parse"/"rate limit") | `acceptance.sh` AC-2 |
| 3 | `gcm --all` against `/fail401/` | Exit 1; exactly 1 request; stderr names `GROQ_API_KEY` | `acceptance.sh` AC-3 |
| 4 | `gcm --all` against `/fail500/` (always 500) | Exit 1; `max_retries+1` requests captured; stderr has a 5xx "retries exhausted" message | `acceptance.sh` AC-4 |
| 5 | `classify_status` for 400/401/403/429/500/599/418 | `BadRequest`/`Auth`/`Auth`/`RateLimit`/`Server`/`Server`/`Http(418)` respectively | `cargo test groq::` (unit) |
| 6 | `Retry-After` header parse: `"2"`, `"Wed, 21 Oct 2026 07:28:00 GMT"`, absent | `Some(2s)`, `None`, `None` | `cargo test groq::` (unit) |
| 7 | `is_retryable` over all variants | `true` for `RateLimit`,`Server`; `false` for the other 8 | `cargo test groq::` (unit) |
| 8 | `backoff_delay` for attempts 0,1,2,20 with base=100ms,max=1s; and with `hint=Some(5s)` | 100ms,200ms,400ms,1s (capped); hint -> 1s (capped to max) | `cargo test groq::` (unit) |
| 9 | `retry_with` with recording sleeper: [429,429,Ok] | `Ok`; op called 3x; 2 sleeps; sleeps == backoff schedule | `cargo test groq::` (unit) |
| 10 | `retry_with`: [400] | `Err(BadRequest)`; op called 1x; 0 sleeps | `cargo test groq::` (unit) |
| 11 | `retry_with`: 5xx x(max+1) | `Err(Server)`; op called max+1; max sleeps | `cargo test groq::` (unit) |
| 12 | Six core variants' `Display` | 6 distinct, non-empty strings; each names its HTTP code/cause | `cargo test groq::` (unit, AC-5) |
| 13 | `parse_defensive` on direct strict JSON | `Ok(Plan)` with the right groups | `cargo test plan::` (unit) |
| 14 | `parse_defensive` on a ```` ```json {..} ``` ```` fenced block | `Ok(Plan)` (fence stripped) | `cargo test plan::` (unit) |
| 15 | `parse_defensive` on `{"commit_plan":{"groups":[..]}}` and `{"result":{"groups":[..]}}` | `Ok(Plan)` (wrapper/nested recovery) | `cargo test plan::` (unit) |
| 16 | `parse_defensive` on `"prose... {\"groups\":[..]} trailing"` | `Ok(Plan)` (balanced-object extraction) | `cargo test plan::` (unit) |
| 17 | `parse_defensive` on `"not json at all"` | `Err(PlanError::Parse(..))`; no panic | `cargo test plan::` (unit) |
| 18 | `GCM_DEBUG=1` run hitting `/fail400/`; then with `GCM_DEBUG` unset | stderr contains `BadRequest` (variant visible) when set; no `[debug]` lines when unset | `acceptance.sh` AC-8 |
| 19 | Quality gates | `cargo fmt --check` clean; `cargo clippy -D warnings` clean; `cargo test` all pass; no new dep in `Cargo.toml` | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` |
| 20 | Index transaction after an exhausted-retry failure | index/working tree byte-identical to pre-run (no staging happened) | `acceptance.sh` AC-4 index check |
| 21 | Non-2xx with a >4096-byte body (simulated large HTML error page) | body read is capped at 4096 bytes; no unbounded allocation; still classifies + surfaces | `cargo test groq::` (unit on the capped reader) |
| 22 | `PlanError::Parse("x")` `Display` | renders `plan parse error: x` (non-empty, descriptive) | `cargo test plan::` (unit) |
| 23 | `parse_defensive` on prose + **two** fenced JSON blocks (first is the plan) | `Ok(Plan)` from the first valid block; no panic | `cargo test plan::` (unit) |
| 24 | `bad_request_detail` on `{"error":{"message":"bad model"}}` and on raw non-JSON >200 chars | `Some("bad model")`; `Some(<=200-char truncation)` | `cargo test groq::` (unit) |
| 25 | `parse_defensive` on prose with a decoy `{...}` before the real JSON: `Here is my plan {it's solid}: {"groups":[..]}` | `Ok(Plan)` - decoy candidate is tried and rejected, the real block recovers (round-2 point 1) | `cargo test plan::` (unit) |
| 26 | `parse_defensive` on two fenced blocks where only the first is a plan, plus interleaved prose | `Ok(Plan)` from the first block; blocks are never concatenated (round-2 point 3) | `cargo test plan::` (unit) |
| 27 | `recover_groups` returns a bare array; wrapped via `json!({"groups": arr})` before `from_value` | `Ok(Plan)`; a direct `from_value::<Plan>(array)` would error (round-2 point 2) | `cargo test plan::` (unit) |
| 28 | `classify_status(504)` | `Server(504)` (retryable), NOT `Timeout` (round-2 point 4) | `cargo test groq::` (unit) |
| 29 | balanced-brace scan over a JSON whose string value contains `}`/`{` (e.g. a path `a}b`) | the candidate is the whole object, not truncated at the in-string brace (string/escape-aware scan) | `cargo test plan::` (unit) |

**Edge cases to verify**:
- 429 with a parseable `Retry-After: 0` -> retries immediately (no long sleep); honored and capped.
- 429 with an absurd `Retry-After: 99999` -> capped at `max` (8s default), never hangs the CLI; with `GCM_RETRY_MAX_MS` lowered in tests, capped accordingly.
- `Retry-After` in HTTP-date form -> ignored gracefully, exponential backoff used.
- `max_retries = 0` (via `GCM_RETRY_MAX=0`) -> a retryable error fails on the first attempt with no sleep (boundary).
- 5xx then 200 within the budget -> succeeds (same path as 429 success, different code class).
- A 400 on the grouping (`--yes`) path -> `Fallback` -> single-commit call also 400 -> exit 1 (2 requests, neither retried); `--all` path isolates it to 1 request for the AC-2 assertion.
- Auth (401) on the grouping path -> `Fatal` (no wasted fallback call), exit 1.
- `parse_defensive` must not be fooled into recovering from a `<think>{"groups":...}</think>` blob into a wrong plan - `<think>` is stripped upstream first, so the recovered JSON is the real content.
- `parse_defensive` recovering a structurally-valid-but-wrong plan (e.g. unknown files) still flows through `validate_basic` -> announced fallback (defense in depth; recovery does not bypass validation).
- `backoff_delay` integer overflow guard: large `attempt` is clamped (`.min(16)` + `saturating_mul`) so the shift never panics.
- No real network in unit tests; no real sleep in `retry_with` tests (injected sleeper); `acceptance.sh` retries are sub-ms via `GCM_RETRY_BASE_MS=1`.
- Existing acceptance cases still pass: AC-12 (unreachable host) stays a fast single-attempt failure (Transport not retried); AC-12b (`/fail500/`) now retries-then-fails but still exits 1 (kept fast by `GCM_RETRY_BASE_MS=1`).
