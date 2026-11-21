# Pre-PR Validation: clo-805

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Validated**: 2026-11-21
**Pipeline**: implement.md validation gate

---

## Verdict: FAIL

The wizard implements the specified confirmation and SPACE hint. The branch falls short on AC9 because the PTY harness can pass without observing EOF and can miss a config rewrite during cancellation.

I reviewed `b0e7626` against `main` at `4c04ce0` and verified that the supplied diff matches. All nine wizard unit tests and all four focused PTY scenarios passed against the existing branch binaries with color enabled and private log paths. The workflow record supplies the full-suite, public-surface, and pre-fix baseline results. [Workflow record](/Users/mk/Code/gcm--fix-clo-805-enable-models/docs/status/clo-805-workflow.yaml:71)

## Findings

- **MEDIUM — The final EOF check can time out and still pass.** Bare `expect eof` falls through on timeout, then `wait` blocks for the child. I verified that a child exiting after the deadline can still produce exit status 0. A child that stays alive can hang the suite. [acceptance.sh](/Users/mk/Code/gcm--fix-clo-805-enable-models/scripts/acceptance.sh:1688)
- **MEDIUM — The cancellation checks can miss a config rewrite.** The snapshot uses a CRC and a whole-second mtime. A same-content rewrite within the same second passes both checks even when the file’s actual mtime changed. [acceptance.sh](/Users/mk/Code/gcm--fix-clo-805-enable-models/scripts/acceptance.sh:1610)
- **MEDIUM — PTY logs use predictable paths in shared `/tmp`.** Concurrent runs can overwrite one another’s logs. The shell redirection also follows an existing symlink where the OS permits it, which can truncate a writable target. The cases already have private fixture directories. [Log redirection](/Users/mk/Code/gcm--fix-clo-805-enable-models/scripts/acceptance.sh:1692) · [Log path](/Users/mk/Code/gcm--fix-clo-805-enable-models/scripts/acceptance.sh:1698)
- **LOW — The PTY test fails when terminal styling is disabled.** Its redraw matcher requires the ANSI reverse-video sequence, but the spawned process inherits the caller’s color settings. I reproduced exit 24 with color disabled. [Redraw matcher](/Users/mk/Code/gcm--fix-clo-805-enable-models/scripts/acceptance.sh:1634)
- **LOW — The project dashboard reports conflicting task states.** CLO-805 occupies an active slot, but the same page says no slot is in use. The row also says `Spec` while the workflow is in `implement`. [Project dashboard](/Users/mk/Code/gcm--fix-clo-805-enable-models/docs/PROJECT.md:99) · [Workflow state](/Users/mk/Code/gcm--fix-clo-805-enable-models/docs/status/clo-805-workflow.yaml:25)

## Missing Items

AC9 still needs an explicit EOF deadline in all four cases and exact byte and full-precision mtime comparisons in both cancellation cases. The wizard’s AC6 cancellation behavior passed the focused PTY run. [Specification](/Users/mk/Code/gcm--fix-clo-805-enable-models/specs/2026-11-21-clo-805-enable-models-unchanged-confirm.md:36)

## Recommendations

Fail explicitly on EOF timeout, reap the child, and validate the `wait` result. Compare file bytes and `st_mtime_ns` using the Python dependency the harness already requires. Put each log inside its private fixture directory and make the redraw matcher independent of inherited color settings. Then rerun the four cases against a fresh release and rerun the pre-fix baseline for the PR evidence.