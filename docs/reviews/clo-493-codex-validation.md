# Pre-PR validation: clo-493

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-06-22
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

- HIGH: `--json` can still print an interactive prompt to stdout when stdin is a TTY and `--yes` is omitted. `ui::confirm(..., quiet: true)` suppresses the message preview, but still executes `print!("Commit with this message?...")` on stdout. That violates the spec's "exactly one JSON object on stdout" rule. See src/ui.rs:19 and src/ui.rs:28.

- MEDIUM: Several JSON error/noop envelopes omit required routing fields. The spec says all envelopes must include `mode`, and the declared `noop`/`error` shapes include provider/model. Current early returns for clean repo, non-interactive, git status, and merge-conflict errors pass `None` for those fields. See src/main.rs:71, src/main.rs:78, and docs spec:137.

- MEDIUM: `cached` is always emitted as `false` for grouped JSON plan output, even when the plan came from `cache::load`. The stable plan envelope includes `cached: bool`, so cache hits need to be observable. See src/main.rs:140 and src/main.rs:313.

- MEDIUM: `--reset --json` behavior is neither implemented as a reset envelope nor documented as intentionally non-emitting. The code clears the cache and then continues into noop/plan/commit behavior, while the spec explicitly requires reset behavior under JSON to be defined and covered. See src/main.rs:63, README.md:62, and docs spec:150.

- LOW: Commit hashes in JSON include the trailing newline from `git rev-parse HEAD`. `last_commit_hash()` returns untrimmed captured stdout, so `commit.hash` is not a clean SHA string. See src/git.rs:68.

## Missing Items

- AC-1 is not complete for TTY JSON runs without `--yes`; stdout can contain the prompt.
- ST6 / reset edge case is not complete for `--reset --json`.
- The stable envelope contract is incomplete for `mode`, provider/model, and accurate `cached`.
- Acceptance tests do not assert `cached: true` on cache hits, no prompt pollution in a TTY JSON path, reset JSON behavior, or exact SHA formatting.

## Recommendations

- Treat `--json` as machine mode: require `--yes`, `--plan-only`, or `--dry-run`, or send any prompt to stderr and still ensure stdout remains one JSON object.
- Carry an explicit execution mode through early-return errors/noops.
- Track whether `cache::load` hit and pass that into `commit_first_group`.
- Define `--reset --json` now: either add `status: "reset"` to the v1 contract or document and test the chosen non-reset envelope behavior.
- Trim `last_commit_hash()` before placing it in `commit.hash`.
- Tests were not run; this review was static against `git diff main...HEAD` in a read-only sandbox.
