# Lessons: CLO-533 remote MR/PR resolve orchestration

Durable rules from CLO-533 (`gcm resolve --pr/--mr`) that should inform sibling tasks touching remote orchestration, subprocess execution, or integration tests.

---

## L1 - Decide durable artifact semantics before implementing temp-dir workflows

**Source incident:** CLO-533 pre-PR validation surfaced a spec contradiction: AC7 required printing a scratch repo path and leaving a resolution branch for manual push, while AC13 originally required scratch cleanup on every success path.

**Rule:** If a workflow reports a local artifact path or branch as user-actionable output, success must preserve that artifact or provide another durable artifact (patch/bundle/pushed ref). Cleanup requirements must explicitly distinguish success from error/abort.

**How to apply:** During spec/design review, search for pairs like "print temp path" + "cleanup on success". Resolve them before implementation, and add tests for both success preservation and error cleanup.

---

## L2 - Timed subprocess wrappers must drain stdout/stderr while the child runs

**Source incident:** CLO-533 validation caught timeout wrappers that polled `try_wait()` and only read piped stdout/stderr after process exit. Chatty `git`/`gh`/`glab` commands can fill the pipe buffer, block on write, and be falsely killed as timed out.

**Rule:** Any wrapper that sets `stdout(Stdio::piped())` or `stderr(Stdio::piped())` must drain those pipes concurrently with waiting, or use an API such as `wait_with_output()` in a bounded wait strategy.

**How to apply:** Review new subprocess helpers for `try_wait()` loops plus piped IO. Add tests or code review gates for pipe-drain behavior before using the helper around network or VCS commands.

---

## L3 - Integration tests must not depend on ambient PATH, global git identity, or default branch names

**Source incident:** CLO-533 PR CI failed after local pre-flight passed. Root causes: tests mutated global `PATH` in parallel, missing-host tests assumed no system `gh` in `/usr/bin`, scratch merges lacked local git identity on fresh runners, and one assertion assumed the default branch was `main`.

**Rule:** Integration tests for CLI/VCS orchestration must isolate PATH per child process, never mutate global environment in parallel tests, configure local git identity in throwaway repos before merge/commit operations, and set branch names explicitly before asserting them.

**How to apply:** Prefer per-command `.env("PATH", ...)` shims over `std::env::set_var`; install only required fake binaries into test bin dirs; run `git config user.name/user.email` and `git branch -M main` in every test repo helper; avoid assertions tied to platform default branch names unless the helper sets them.
