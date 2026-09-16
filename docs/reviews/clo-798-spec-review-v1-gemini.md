# Spec Review: clo-798

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-09-16
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is clear, complete, and highly accurate. It perfectly matches the Linear task description for CLO-798. It goes beyond a simple bug report by beautifully decomposing *why* the silence occurs by construction, citing specific architectural decisions:
* Logging defaults (`debug_log!` emitting at `Level::Debug` which maps to `Level::Off` by default).
* Synchronous blocking transport on `ureq` (ADR-001 Decision 2).
* Long client-side timeout defaults (60s) which can accumulate to ~4 minutes of silence under the standard 3 retries.
* Ambiguous HTTP 400 error mapping (blaming a general "gcm bug" rather than indicating a context-window overflow).

There are no unstated assumptions. It correctly frames the solution as making slow calls self-diagnosing rather than prematurely attempting to resolve the upstream grouping logic or retry strategies (which are CLO-797 concerns).

## 2. Acceptance Criteria Review
**Strong**: 
* Criteria are specific, measurable, and highly testable. 
* The distinction between TTY and non-TTY progress formatting (AC-8) is a great defensive constraint for automated CI runs.
* Restricting the public library surface modification (AC-9) is verified programmatically via the existing `scripts/check-public-surface.sh` script, preventing API regressions.
* Excellent coordination with `--json` (AC-7) ensures that stderr logging does not pollute stdout, leaving stdout byte-identical for automation.

**Gaps**:
* **`gcm resolve` Exclusion**: The spec places `gcm resolve` (`resolve_hunks` calls) out of scope. However, `gcm resolve` runs the same synchronous HTTP transport and is subject to the identical multi-minute silence when resolving large files. Keeping conflict resolution silent represents a UX gap.
* **Prompt Size Definition**: AC-2 specifies "approximate prompt size in bytes" but does not clarify whether this refers to the size of the raw prompt text or the fully serialized JSON payload. Clarifying this avoids implementation discrepancy.

## 3. Constraints Check
**Aligned**:
* Writing all human-readable progress to **stderr** is aligned with gcm's output patterns.
* Enforcing synchronous transport constraints (no tokio/async) strictly adheres to ADR-001 Decision 2.
* Avoiding new external dependencies by leveraging standard `std::io::IsTerminal` and existing `console` types is highly aligned with maintaining a slim CLI target.
* Leaving timeout/retry defaults unchanged ensures no unexpected behavioral drift for existing happy-path runs.

**Concerns**:
* None. The constraints are robust and respect both historical ADR decisions and library/binary separation boundaries.

## 4. Decomposition Quality
**Well-scoped**:
* The proposed sequence of 6 sub-tasks (`1` -> `2` -> `3` -> `4` -> `5` -> `6`) is logical and incremental.
* Each sub-task is small and scoped to under 2 hours.
* Independent concerns (like macros, error text formatting, and prompt-size helper methods) are separated from the trickier thread coordination logic.

**Issues**:
* None. This is a reference-grade, clean decomposition.

## 5. Evaluation Coverage
**Covered**:
* The evaluation section is extremely comprehensive. 
* Writing a dedicated integration test suite (`tests/observability.rs`) that drives a built subprocess against a dummy local `TcpListener` mock is an exceptional, robust approach that avoids flaky live network tests.
* The test table covers all defined ACs.

**Gaps**:
* **`std::process::exit` Destructor Interception**: `gcm` exits by calling `std::process::exit(run(...))` in `main.rs`. Because `std::process::exit` terminates the process immediately without running Rust's destructors, any `CallProgress` guard spawned within `run` must be dropped *before* `run` exits. The evaluation scenarios should verify that the ticker thread is cleanly joined on all error return paths.

## 6. Codebase Alignment
**Violations**:
* *Minor terminology correction*: The prompt asks to check alignment with a "Backend trait contract". In this codebase, the trait is named `Provider` (defined in `src/provider/facade.rs`), not `Backend`. The spec aligns perfectly with this.

**Alignment**:
* The spec preserves the exact variants of `ErrorKind`, which prevents any breakage of third-party crates depending on the library target.
* The `warn_log!` macro aligns beautifully with the existing macro architecture in `src/debug.rs`.

## 7. Blind Spots
* **The `std::process::exit` Destructor Gotcha**: In Rust, stack unwinding and dropping occurs when returning normally or panicking. If a subroutine aborts the process through an external exit or if a thread panics during joining, the ticker thread can get stranded. The design should guarantee that the ticker guard is strictly bounded within a scope that terminates prior to `std::process::exit`.
* **Conflict Resolution Silence**: Leaving `gcm resolve` out of scope leaves a significant UX gap. Because `ui::CallProgress` will be fully implemented and self-contained, it should be trivial to apply it to `resolve_hunks` calls.

## 8. Verdict
**Verdict**: APPROVE_WITH_SUGGESTIONS

## 9. Actionable Feedback
1. **Extend `CallProgress` to `gcm resolve`**: Once `ui::CallProgress` is implemented in Sub-task 5, pull it into `src/resolve/mod.rs` around the `provider.resolve_hunks` call. This ensures conflict resolution shares the same robust progress feedback as the commit path.
2. **Scope the `CallProgress` Guard for Safe Destruction**: Formally add a design constraint stating that the `CallProgress` guard must be wrapped in a tight block or scoped to guarantee its `Drop` implementation joins the background thread before `run(...)` yields control back to `main`'s `std::process::exit(...)`.
3. **Clarify Prompt-Size Calculation**: Explicitly define "approximate prompt size" as the byte count of the generated prompt text segments (`GroupingContext` / `GatheredDiff`) rather than a token count (which would require a complex tokenization dependency).
