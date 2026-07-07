# Lessons: CLO-531 — `gcm resolve` LLM-assisted merge conflict resolver

Source: `docs/status/clo-531-workflow.yaml`, `docs/reviews/clo-531-validation-synthesis.md`
Date: 2026-07-07

---

## L1 - Secret scan must apply to all egress paths, not just the pre-flight check

**Source incident**: CLO-531 validation gate (Codex review, Finding #1).
The resolve path only called `privacy.scan_text()` when mode was `Abort`,
then sent original hunk text to the provider. `Redact` mode was silently
ignored — credentials could leak to the provider even with `--secret-scan=redact`.

**Rule**: When sending text to an LLM provider, apply `scan_text()` for
ALL non-Off modes:
- `Abort`: pre-scan all text and fail with `SecretDetected` before any provider call.
- `Redact`: transform the text (replace secrets with `[REDACTED: secret]`) before
  building the provider request payload.
- `Off`: no filtering.

**How to apply**: Any new code path that sends user content to a provider
must call `privacy.scan_text()` on the text before it leaves the process.
The pre-flight check (Abort) is necessary but not sufficient — Redact mode
must also transform the text. Check both modes in the egress path, not
just the pre-flight check.

---

## L2 - `--dry-run` must not mutate the working tree, including side-effect git operations

**Source incident**: CLO-531 validation gate (Codex review, Finding #2).
`git checkout --conflict=zdiff3` and `mergiraf` both run before the dry-run
check in `resolve_file()`, mutating the working tree even when `--dry-run`
is set. The user expects a dry run to be read-only.

**Rule**: In `--dry-run` mode, skip ALL operations that mutate the working
tree, including:
- `git checkout --conflict=zdiff3` (re-materializes conflict markers)
- `mergiraf` (writes resolved files)
- `repo.write_file()` (writes the resolution)

Read files as-is and parse whatever markers exist. The file may not have
zdiff3 base blocks (plain diff3), but the parser handles `base: None`.

**How to apply**: Guard every working-tree mutation with `if !args.dry_run`.
The zdiff3 checkout is the most subtle case — it's a git operation that
looks like a "reset" but actually modifies files. Treat it as a write.

---

## L3 - Tests that only need a directory CWD should not shell out to `git init`

**Source incident**: CLO-531 CI failure on ubuntu-latest.
`resolve::validate::tests` ran `git init -q` in a temp directory before each
test, but the validate function only uses `repo.root()` as the CWD for
`validate_cmd`. On the ubuntu CI runner, parallel `git init` subprocesses
failed with ENOENT (race condition), causing 4 test failures.

**Rule**: If a test only needs a directory path (e.g., for `Command::new("sh")
.current_dir(root)`), do NOT shell out to `git init`. Just create a
`tempfile::tempdir()` and pass it to `Repo::at_root()`. The git subprocess
is unnecessary overhead and introduces a race condition in parallel test
execution.

**How to apply**: Before adding `git init` to a test helper, check whether
the code under test actually needs a real git repository. If it only needs
a directory path, skip `git init`. This eliminates a class of flaky CI
failures.

---

## L4 - Non-interactive safety: EOF on stdin must not auto-accept

**Source incident**: CLO-531 validation gate (Codex review, Finding #3).
The `confirm_file()` function treated EOF (empty `read_line`) as "Accept",
which means a non-interactive process (CI, piped stdin) would silently
accept all resolutions without `--yes`.

**Rule**: Before prompting the user, check `needs_terminal_but_absent()`.
If stdin is not a TTY and `--yes`/`--dry-run` are not set, error with
`GcmError::NonInteractive` instead of auto-accepting.

**How to apply**: The existing `ui::needs_terminal_but_absent(auto_yes, dry_run)`
function already implements this check. Call it at the top of the
resolve orchestrator, before the per-file loop. This pattern applies to
any subcommand with a `[Y/n/e]` prompt.

---

## L5 - Validation gate retry should cover all validation failure types

**Source incident**: CLO-531 validation gate (Codex review, Finding #4).
`ValidationError::ConflictMarkers` triggered a bounded retry, but
`ValidationError::ValidateCmdFailed` escalated immediately without retry.
The design requires exactly one bounded retry for validation failures.

**Rule**: All validation failure types should trigger the same bounded
retry path. The retry asks the provider to fix its own output. If the
retry also fails, then escalate.

**How to apply**: In the validation gate, treat both `ConflictMarkers`
and `ValidateCmdFailed` as retryable. Route both through the same
`attempt_validation_retry()` function. Only escalate if the retry also
fails.