# Pre-PR Validation: clo-805

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Validated**: 2026-11-21
**Pipeline**: implement.md validation gate

---

## Verdict: FAIL

Reviewed `b0da917` against `main` at `4c04ce0`. The wizard implements the specified confirmation, selection, cancellation, and SPACE hint behavior. All nine wizard unit tests and all four focused macOS PTY cases passed using existing binaries. Formatting and shell syntax checks passed. The full suite and pre-fix baseline results came from the workflow record. [Workflow record](/Users/mk/Code/gcm--fix-clo-805-enable-models/docs/status/clo-805-workflow.yaml:73)

## Findings

- **MEDIUM** — The PTY harness proceeds after fixture creation fails. If `mktemp -d` returns an empty path, the script attempts to write `/config.toml` and open `/run.log`. If the log opens, the child receives an empty `GCM_CONFIG` and falls back to the runner’s normal config. A failed seed or `chmod` also leaves the spawn enabled. [Fixture setup](/Users/mk/Code/gcm--fix-clo-805-enable-models/scripts/acceptance.sh:1702) · [Config path resolution](/Users/mk/Code/gcm--fix-clo-805-enable-models/src/config/mod.rs:200)
- **MEDIUM** — The `no` PTY case rejects valid Linux output under `LANG=C` or an unset `LANG`. The matcher requires `◼` after SPACE, but the pinned `console` crate selects cliclack’s `[+]` fallback in those environments. `CLICOLOR_FORCE` affects ANSI color only. This failure follows from the pinned source. The runtime check was on macOS. [PTY matcher](/Users/mk/Code/gcm--fix-clo-805-enable-models/scripts/acceptance.sh:1664) · [Locale check](/Users/mk/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/console-0.16.3/src/unix_term.rs:388) · [Checkbox fallback](/Users/mk/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cliclack-0.5.4/src/theme.rs:21)

## Missing Items

AC9 needs a PTY harness that requires a successfully initialized private config and recognizes both checkbox renderings. I found no missing wizard behavior. [Specification](/Users/mk/Code/gcm--fix-clo-805-enable-models/specs/2026-11-21-clo-805-enable-models-unchanged-confirm.md:36)

## Recommendations

- Stop each case before spawning if directory creation, config seeding, or `chmod 600` fails.
- Match both `◼` and `[+]` when waiting for the checked row. Rerun the four cases on Linux under `LANG=C` and a UTF-8 locale.