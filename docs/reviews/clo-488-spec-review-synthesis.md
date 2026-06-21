# Spec Review Synthesis: clo-488

**Synthesized**: 2026-06-20
**Pipeline**: lok spec-review

---

## Synthesis: CLO-488 Typed Errors Spec Review

**Sources:** Gemini (valid, substantive), Ollama (valid, structural confirmation only), Claude (skipped — at least one external reviewer succeeded).

> Note: Ollama's response was a meta-level structural confirmation (verified the 9-section format and verdict were present) rather than independent substantive findings. It corroborates Gemini's verdict but surfaced no distinct technical claims. Consequently, nearly all substantive findings are single-reviewer (Gemini).

## Agreement (High Confidence)
| # | Finding | Severity |
|---|---------|----------|
| 1 | Spec is well-structured: complete 9-section format, clear problem statement, explicit verdict | Info |
| 2 | Verdict is **APPROVE_WITH_SUGGESTIONS** — spec is sound, suggestions are non-blocking | Info |
| 3 | Actionable feedback is priority-tiered with concrete implementation guidance | Info |

## Disagreement (Needs Human Decision)
| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| - | None | No substantive conflicts | Provided no competing technical positions | Skipped (fallback not invoked) |

No genuine disagreements surfaced. Ollama did not contest any Gemini finding; Claude did not run.

## Novel Insights (Single Reviewer)
| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | **Unbounded body read on errors** — Cloudflare-style HTML error pages (502/504) read into a `String` unbounded risk memory/latency blowup; cap with `.take(4096)` | Gemini | High |
| 2 | **`FnMut` vs `Fn` for injected sleeper** — tests need to mutate captured state (record `Vec<Duration>`); `impl Fn` forces `RefCell`/`Mutex` boilerplate. Change `retry_with` signature to `FnMut` | Gemini | Medium |
| 3 | **Groq error JSON extraction** — 400s return `{"error":{"message":...}}`; spec should extract `error.message` for `detail`, falling back to truncated raw (≤200 chars) | Gemini | Medium |
| 4 | **Case-insensitive `Retry-After` lookup** — HTTP headers are case-insensitive; ACs don't mandate case-insensitive header lookup | Gemini | Medium |
| 5 | **Sub-task 4 signature mismatch** — `parse_defensive` returns `Result<_, PlanError>` but `generate_plan` returns `Result<_, GroqError>`; spec must specify mapping `PlanError → GroqError::Deserialize` | Gemini | Medium |
| 6 | **`Auth` naming inconsistency** — written as `Auth(u16)`, `Auth(code)`, and `Auth(_)` across sections; normalize to `Auth(u16)` | Gemini | Low |
| 7 | **`PlanError::Parse` Display gap** — AC-9 adds `Parse(String)` but AC-5/Display coverage doesn't assert it produces a descriptive message | Gemini | Low |
| 8 | **Missing multi-fence test** — no eval case for a response with multiple markdown blocks or prose-before-JSON in `parse_defensive` | Gemini | Low |
| 9 | **`parse_defensive` layer-4 precedence undocumented** — should spell out order: top-level `groups` → wrapper keys (`commit_plan`/`plan`/`result`/`data`/`response`) → DFS for `"groups"` array | Gemini | Low |

## Consolidated Verdict
**APPROVE_WITH_SUGGESTIONS**

Both valid reviewers landed on APPROVE_WITH_SUGGESTIONS; no NEEDS_REVISION from any source. The spec is implementable as-is; the items below harden robustness and remove ambiguities before code generation.

## Priority Actions
Ordered by severity (no cross-reviewer agreement items exist beyond the verdict, so ordered by Gemini severity):

1. **[High] Cap error-body reads.** In `send_chat_once`/status inspection, bound non-2xx body reads: `response.body_mut().as_reader().take(4096).read_to_string(&mut s)?`. Prevents memory/latency blowup on HTML error floods.
2. **[Medium] Switch sleeper to `FnMut`.** Update `retry_with` to `mut sleep: impl FnMut(Duration)` (and `op: impl FnMut() -> ...`) so tests record sleep intervals without interior-mutability boilerplate.
3. **[Medium] Extract `error.message` for `detail`.** On `BadRequest`, parse JSON body and pull `error.message`; fall back to raw truncated to 200 chars.
4. **[Medium] Mandate case-insensitive `Retry-After` lookup** in the ACs/implementation notes.
5. **[Medium] Specify `PlanError → GroqError::Deserialize` mapping** in sub-task 4 to resolve the `generate_plan` signature mismatch.
6. **[Low] Normalize `Auth(u16)`** naming across Section 3, Table 3b, and the GcmError routing.
7. **[Low] Add `PlanError::Parse(msg)` Display** (`write!(f, "plan parse error: {msg}")`) and an AC asserting it.
8. **[Low] Add a multi-fence/prose-prefixed eval case** for `parse_defensive`.
9. **[Low] Document layer-4 search precedence** for `parse_defensive`.
