## Verdict: FAIL

## Findings

HIGH: `--secret-scan=redact` is silently ignored for resolve hunks. The resolve path only calls `privacy.scan_text()` when mode is `Abort`, then sends original hunk text to the provider. That means redaction mode can leak credentials even though G8 requires `--secret-scan` before provider egress. See [src/resolve/mod.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/src/resolve/mod.rs:317) and [src/privacy/mod.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/src/privacy/mod.rs:88).

HIGH: `--dry-run` can still write the working tree. `run_resolve()` re-checks out every unmerged file with zdiff3 before any dry-run branch, and `mergiraf::try_resolve()` is invoked before the dry-run check; the code even notes “mergiraf already wrote the file.” This violates the design’s “preview, no write” acceptance criterion. See [src/resolve/mod.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/src/resolve/mod.rs:59) and [src/resolve/mod.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/src/resolve/mod.rs:274).

HIGH: Non-interactive runs can auto-accept without `--yes`. `confirm_file()` treats EOF from stdin as an empty response and defaults to `Accept`; resolve has no equivalent of the commit flow’s non-TTY guard. A CI/json run without `--yes` can write files without explicit confirmation. See [src/ui.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/src/ui.rs:39) and [src/resolve/mod.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/src/resolve/mod.rs:419).

HIGH: `validate_cmd` failures do not get the required bounded LLM retry. The design requires exactly one retry on validation failure, but `ValidateCmdFailed` immediately escalates, and the retry path later calls `validate(&text, None, ...)`, skipping the configured command. See [docs/designs/clo-531-gcm-resolve.md](/Users/mk/Code/gcm--feat-clo-531-resolve/docs/designs/clo-531-gcm-resolve.md:23), [src/resolve/mod.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/src/resolve/mod.rs:392), and [src/resolve/mod.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/src/resolve/mod.rs:543).

MEDIUM: `conflict.auto_policy` is parsed but not enforced. The classifier always auto-resolves trivial hunks regardless of `AutoPolicy::Complex`, so the config/CLI knob is currently dead behavior. See [src/resolve/mod.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/src/resolve/mod.rs:301).

MEDIUM: The design’s byte/line-ending preservation requirement is not implemented. Files are read with `read_to_string()`, parsed with `str::lines()`, and reconstructed with `\n`, so non-UTF-8 text fails and CRLF conflicts are rewritten as LF. See [docs/designs/clo-531-gcm-resolve.md](/Users/mk/Code/gcm--feat-clo-531-resolve/docs/designs/clo-531-gcm-resolve.md:610), [src/git.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/src/git.rs:311), and [src/resolve/mod.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/src/resolve/mod.rs:550).

## Missing Items

- Conflict-setting env precedence from G6 is not implemented; there are CLI/config/default paths, but no env layer for `[conflict]` settings.
- ST10 coverage is incomplete: no integration tests for trivial conflict, one-side-unchanged, mergiraf absent, skip, or edit. Current tests are listed in [tests/resolve_integration.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/tests/resolve_integration.rs:210).
- Binary detection does not follow the design/plan’s `git diff --numstat --diff-filter=U` approach; current parsing depends on combined diff text. See [src/git.rs](/Users/mk/Code/gcm--feat-clo-531-resolve/src/git.rs:329).

## Recommendations

- Rework dry-run to avoid all working-tree mutation, including zdiff3 checkout and mergiraf. Use a temp worktree/file copy or skip external mutation in dry-run.
- Apply privacy scanning uniformly: `Abort` should fail before request; `Redact` should transform hunk text before building `ResolveContext`.
- Add a non-TTY guard for resolve unless `--yes` or `--dry-run` is set, and treat `read_line == 0` as `NonInteractive`.
- Retry once for any validation failure, including `validate_cmd`, and rerun the full configured validation after retry.
- Enforce `AutoPolicy`, or remove the exposed option until the behavior exists.
- Switch marker parsing/reconstruction to byte-aware handling that preserves original encoding and line endings.

Tests not run: this review environment is read-only, so Cargo could not safely write build artifacts. `git diff --check main...HEAD` passed.


