# Spec Review: clo-798

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-09-16
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem definition is **highly complete, accurate, and self-contained**. It perfectly matches the Linear task description and resolves the root causes of the silence by detailing the current constraints:
- The default log level (`Level::Off`) and macro mechanics.
- The blocking nature of the synchronous transport (`ureq` over ADR-001).
- The distinction between the four separate provider call sites, each mapped to a specific operation label and unique user-remedy advice.
- The key differences between the generation timeout phase (60s) and the wizard fetch timeout phase (5s).

## 2. Acceptance Criteria Review
*   **Strong**: The criteria are exceptionally specific, measurable, and highly testable. 
    *   **AC-1** sets concrete timing bounds ($<2	ext{s}$ first render, $<5	ext{s}$ gaps) and establishes the `--json` suppression rules.
    *   **AC-4** and **AC-5** elegantly address error extraction hazards, such as truncated payloads or sibling JSON keys, and enforce context-specific advice.
    *   **AC-10** specifies correct clearing behavior to avoid interleaving log output with the active ticker.
*   **Gaps**: No structural gaps identified. One minor optimization suggestion:
    *   *Terminal Resize robustness*: If a user resizes their terminal window while the ticker is active, tracking character columns to overwrite with spaces (to clear the line) can wrap and corrupt the screen. Erasing the line using standard ANSI escape codes on TTY simplifies this (see Actionable Feedback).

## 3. Constraints Check
*   **Aligned**: The Must/Must-not constraints are perfectly aligned with established codebase patterns and core mandates:
    *   All human-readable output is routed exclusively to `stderr`, leaving `stdout` pure for automated `--json` consumers.
    *   Public API boundaries (e.g. `ErrorKind` variants and the `Provider` trait signature) are kept strictly intact, preserving the locked public surface.
    *   The transport remains synchronous, avoiding the introduction of async runtimes like `tokio`.
*   **Concerns**: The duplicate compilation of `src/debug.rs` (into both `gcm` lib and `gcm` bin) remains the highest-risk architectural point. If the binary's macro expansions resolve to `crate::debug` while the library resolves to `gcm::debug`, state synchronization will fail. The spec's plan to anchor the static in `gcm::debug` is correct, but the implementation should employ an explicit delegation pattern to enforce this (see Actionable Feedback).

## 4. Decomposition Quality
*   **Well-scoped**: Outstanding. The 7 sub-tasks are highly independent, logical, and sequentially ordered. None of the tasks exceed the 2-hour implementation window.
*   **Issues**: None. The dependency flow ($1 \rightarrow 2 \rightarrow 3 \rightarrow 4 \rightarrow 5 \rightarrow 6 \rightarrow 7$) is extremely clean.

## 5. Evaluation Coverage
*   **Covered**: The 18 test cases in the evaluation table provide 100% coverage of the 12 acceptance criteria and various critical edge cases (such as Ollama cloud passthroughs, fast failures, and bad log level inputs).
*   **Gaps**: Excellent coverage. The testing strategy (spawning a local `std::net::TcpListener` stub in `tests/observability.rs`) is robust and guarantees reproducible, non-flaky verification.

## 6. Codebase Alignment
*   **Violations**: None.
*   **Alignment**: The specification integrates beautifully with the `Provider` contract, the `bad_request_detail` truncation threshold, and the library boundaries defined in ADR-002.

## 7. Blind Spots
*   **Buffering on Stderr**: Rust's standard `stderr` is unbuffered, but when using `` (carriage return) without a newline, some operating system terminals delay rendering. The ticker implementation must explicitly call `.flush()` on `stderr` after each tick to ensure immediate visibility.
*   **Cursor Hiding**: Choosing *not* to hide the cursor during ticker runs is highly endorsed. It completely avoids the common failure mode where terminal states are left corrupted following a `Ctrl-C` interrupt.

## 8. Verdict
**APPROVE_WITH_SUGGESTIONS**

## 9. Actionable Feedback

### 1. Enforce a Pure Delegation Pattern in the Binary's `debug.rs` compilation (High Priority)
Since `src/debug.rs` compiles into both crates, define all coordination states (such as `COORD` or `TICKER_LIVE`) exclusively within the library compilation. In `src/debug.rs`, use conditional compilation or standard delegation so the binary's copy delegates directly to the library:
```rust
// In src/debug.rs
pub fn emit_line(level: Level, args: std::fmt::Arguments) {
    // If compiled in the binary, delegate to the library's static coordination point
    #[cfg(feature = "cli")]
    {
        gcm::debug::emit_line_coordinated(level, args);
    }
    #[cfg(not(feature = "cli"))]
    {
        emit_line_coordinated(level, args);
    }
}
```
This guarantees that both compilation paths modify the identical static memory location, eliminating out-of-sync coordination bugs.

### 2. Use ANSI `[K` for TTY Progress Clearing (Medium Priority)
For TTYs (`std::io::IsTerminal` is true), instead of writing $N$ spaces to erase a previous progress line, write `` followed by the ANSI Clear-Line sequence `[K` (Erase Line from cursor to end). This is faster, does not allocate, and is completely immune to wrapping bugs if the user resizes their terminal window.

### 3. Ensure Explicit Flushing on Stderr ticks (Low Priority)
In the ticker loop, make sure each frame render is immediately flushed:
```rust
eprint!("{}", progress_text);
std::io::stderr().flush().ok();
```
