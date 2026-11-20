# Lessons: CLO-944 AC-488-1 retry-notice drift

Durable rules from the AC-488-1 acceptance failure on main.

---

## L1 - A change to default stderr output must run the full acceptance suite, and "no failure" asserts must match the exact error text

**Source incident:** CLO-944 (`docs/status/clo-944-ac-488-1-retry-notice.md`). CLO-798 made the retry notice `rate limited (HTTP 429); attempt N of M` visible by default (`src/provider/http.rs:433`). AC-488-1 (`scripts/acceptance.sh:797`) asserted that no "rate limit" text appeared, so a 429 that recovered started to fail the suite. The failure was first recorded during CLO-940, two tasks later.

**Rule:** Acceptance cases assert on stderr text, so any change to the default log level or to a default-on message can break an older case without a compile or unit-test failure. An assertion that means "the user saw no failure" must match the exact terminal error `Display` text (`rate limit reached`, `src/provider/identity.rs:37`), not a word that progress messages share.

**How to apply:** When a task adds or rewords stderr output that shows without `GCM_DEBUG`, rebuild the release binary and run `scripts/acceptance.sh` before the PR. When you write a negative stderr assertion, prove it with a run that produces the real failure (for AC-488-1: `GCM_RETRY_MAX=1` against `/retry429/v1`).
