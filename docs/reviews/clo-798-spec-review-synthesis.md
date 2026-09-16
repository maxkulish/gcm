# Spec Review Synthesis: clo-798

**Synthesized**: 2026-09-16
**Pipeline**: lok spec-review

---

## Reviewer Status

| Source | Result |
|---|---|
| Gemini | ✅ Valid — APPROVE_WITH_SUGGESTIONS |
| Ollama | ❌ REVIEW_FAILED — model pull failed (`glm-5:cloud` manifest does not exist) |
| Claude fallback | Skipped (external reviewer succeeded) |

Single valid source. No cross-referencing possible, so every item below is single-reviewer confidence. Treat the "Agreement" section as unverified by a second model.

## Agreement (High Confidence)

Not applicable — only one reviewer returned. Findings are listed under Novel Insights.

## Disagreement (Needs Human Decision)

None. No second position exists to disagree.

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---|---|---|
| 1 | `src/debug.rs` compiles into both the `gcm` lib and the `gcm` bin. If the binary's macro expansions resolve to `crate::debug` while the library resolves to `gcm::debug`, the ticker/log coordination statics (`COORD`, `TICKER_LIVE`) land in two distinct memory locations and state never synchronizes. The spec correctly anchors the static in `gcm::debug`, but nothing in it forces the binary copy to delegate. Recommend an explicit delegation shim so both compilation paths hit one static. | Gemini | High |
| 2 | Ticker line clearing by writing N spaces is fragile under terminal resize — the padding wraps and corrupts the screen. On a TTY (`std::io::IsTerminal`), use `\r` plus ANSI erase-line `ESC[K` instead: no allocation, resize-immune. | Gemini | Medium |
| 3 | `stderr` is unbuffered in Rust, but `\r`-without-newline writes are delayed by some terminals. Every ticker frame needs an explicit `std::io::stderr().flush()`. | Gemini | Low |
| 4 | AC coverage is complete: timing bounds (<2s first render, <5s gaps), `--json` suppression, error-extraction hazards (truncated payloads, sibling JSON keys), and log/ticker interleave clearing are all specified and testable. The 18-case eval table covers all 12 ACs; the `std::net::TcpListener` stub strategy is non-flaky. | Gemini | Informational |
| 5 | Decomposition is clean — 7 sub-tasks, linear 1→7 dependency chain, none over the 2-hour window. No restructuring needed. | Gemini | Informational |
| 6 | The decision *not* to hide the cursor is endorsed: it avoids leaving the terminal in a corrupted state after `Ctrl-C`. | Gemini | Informational |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

Gemini returned APPROVE_WITH_SUGGESTIONS; no reviewer returned NEEDS_REVISION. Confidence is lower than a normal two-source synthesis — the second opinion never ran.

## Priority Actions

1. **Add the delegation shim to `src/debug.rs`** so the binary's copy forwards coordination state to the library's static rather than instantiating its own. This is the only finding that can silently produce a wrong-but-compiling implementation, and it invalidates every ticker/log interleave test if missed. Fold it into sub-task 1 as an explicit requirement.
2. **Switch TTY line clearing to `\r` + `ESC[K`.** Cheap edit to the sub-task that owns the ticker render loop; removes the resize corruption class entirely.
3. **Flush stderr after each ticker frame.** One line in the render loop.
4. **Optional:** add an eval case for terminal resize during an active ticker, if action 2 is taken as specified rather than as a blind space-padding fallback.

## Note on the failed reviewer

Ollama never ran — `glm-5:cloud` could not be pulled ("pull model manifest: file does not exist"). Either the model tag is wrong in the reviewer config or the cloud model is unavailable. Worth fixing before the next review round so the synthesis has genuine cross-referencing; a re-run against a working local model would raise confidence on finding #1 in particular, since it is the one architectural claim in the whole review.
