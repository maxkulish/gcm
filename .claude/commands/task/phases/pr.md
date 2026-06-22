# Phase: Pull Request

**Purpose**: Run pre-flight CI checks, create a pull request, monitor CI, handle reviews, and get approval for merge.

**Entry conditions**: `current_phase: pr`

---

## Status: pending (no PR exists)

### Step 4.0: Pre-flight CI Checks (MANDATORY)

**Before creating a PR, ALL local CI checks must pass.** This prevents wasting CI time on formatting/lint/test failures.

Run all checks:

```bash
# 1. Rust formatting
cargo fmt --check

# 2. Rust linting
cargo clippy -- -D warnings

# 3. TypeScript type checking
cargo clippy

# 4. Rust tests
cargo test
```

**Display checklist after completion:**

```
PRE-FLIGHT CI CHECKS
=====================

  [x] cargo fmt --check
  [x] cargo clippy -- -D warnings
  [x] cargo clippy
  [x] cargo test

All checks passed. Ready to create PR.
```

**If any check fails:**
1. Fix the issue
2. Commit with message: `fix(CLO-XX): fix [formatting|lint|type|test] issues before PR`
3. Add history entry: `pre_flight_fix_applied`
4. **Re-run ALL checks from the beginning** (fixes may introduce new issues)
5. Repeat until all pass

After all pass:
- Add history entry: `pre_flight_checks_passed`
- Update state: `phases.pr.ci_passed: false` (this tracks remote CI, not local pre-flight)

### Step 4.1: Create Pull Request

1. **Invoke**: `/pr:create CLO-XX`
2. Update state:
   - `phases.pr.pr_url: [url]`
   - `phases.pr.pr_number: [number]`
   - `phases.pr.status: in_progress`
3. Add history entry: `pr_created`

### Step 4.1.5: PR Health / Mergeability Gate (MANDATORY before CI/reviews)

After PR creation and after every push, verify GitHub can evaluate the PR before waiting for CI or bot reviewers. A conflicted PR (`mergeable: CONFLICTING`, `mergeStateStatus: DIRTY`, or REST `mergeable_state: dirty`) may prevent CI/bot review from starting; block on conflict resolution instead of bot-review absence.

```bash
PR=[number]
REPO=maxkulish/gcm

PR_HEALTH=$(gh pr view "$PR" --json mergeable,mergeStateStatus,isDraft,headRefOid,statusCheckRollup)
API_HEALTH=$(gh api repos/${REPO}/pulls/${PR} \
  --jq '{mergeable, mergeable_state, rebaseable, head_sha:.head.sha, base_sha:.base.sha}')

MERGEABLE=$(jq -r '.mergeable' <<<"$PR_HEALTH")
MERGE_STATE=$(jq -r '.mergeStateStatus' <<<"$PR_HEALTH")
API_MERGEABLE_STATE=$(jq -r '.mergeable_state' <<<"$API_HEALTH")

if [ "$MERGEABLE" = "CONFLICTING" ] || [ "$MERGE_STATE" = "DIRTY" ] || [ "$API_MERGEABLE_STATE" = "dirty" ]; then
  echo "GATE FAIL: PR has merge conflicts; resolve conflicts before CI/bot-review gates."
  echo "gh pr view: ${PR_HEALTH}"
  echo "REST pull: ${API_HEALTH}"
  exit 1
fi
```

If this gate fails:
1. Add history entry: `workflow_blocked`
2. Set `phases.pr.status: blocked`
3. Resolve conflicts:
   ```bash
   git fetch origin main
   git merge origin/main   # or rebase only if the repo/task explicitly prefers rebase
   # resolve conflicts
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo clippy --tests
   cargo test
   git push
   ```
4. Restart from Step 4.1.5.

If the head SHA changed, restart any bot-review wait deadline from that new reviewable head; do not reuse the original PR creation time.

### Step 4.2: Monitor CI Status

After PR health passes, check CI status:

```bash
gh run list --branch [branch-name] --limit 1
```

- **If CI passes**: Update `phases.pr.ci_passed: true`, add history: `ci_passed`
- **If CI fails**:
  1. Identify failing jobs: `gh run view [run-id] --log-failed`
  2. Fix the issue locally
  3. Re-run pre-flight checks (Step 4.0)
  4. Push the fix
  5. Add history: `ci_fix_applied`
  6. Re-check CI status

---

## Status: in_progress (PR exists)

1. **Check for reviews**
2. **Invoke**: `/pr:review CLO-XX` for reviews
3. If reviews addressed:
   - **Re-run pre-flight checks** (Step 4.0) before pushing
   - Update `phases.pr.reviews_addressed`
   - Push changes
   - Add history entry: `review_addressed`
4. After every push, re-run the PR Health / Mergeability Gate before waiting for CI or bot-review completion. If the head SHA changed, restart bot-review wait timing from that new reviewable head.
5. **CRITICAL - gemini-code-assist replies**: When replying to inline comments from `gemini-code-assist`, every reply MUST end with `/gemini review` on its own line. This triggers Gemini to re-validate the fix. This applies whether you invoke `/pr:review` or handle reviews inline.

---

## Review Cycle

1. After addressing reviews, ask:
   ```
   PR CHECKPOINT

   PR: [url]
   Reviews addressed: [count]
   CI Status: [passing|failing|pending]

   Options:
   1. [check-again] - Check for new comments
   2. [ready] - PR is approved, ready to merge
   3. [pause] - Pause workflow

   Your choice:
   ```

2. **If check-again**:
   - Re-check for new reviews
   - Loop back to review handling

3. **If ready**:
   - Update state:
     - `phases.pr.approved: true`
     - `phases.pr.status: complete`
     - `workflow.current_phase: complete`
     - `workflow.status: in_progress`
   - Add history entry: `pr_approved`
   - **Continue to COMPLETE phase**

4. **If pause**:
   - Save state
   - Exit with resume instructions

---

## YAML Checkpoint (Required before transition)

Before signaling completion to the dispatcher, verify:
- `phases.pr.pr_url` is set (non-null)
- `phases.pr.pr_number` is set (non-null)
- `phases.pr.ci_passed: true`
- `phases.pr.status: complete`
- History contains `pre_flight_checks_passed`, `pr_created`, and (`ci_passed` OR `ci_fix_applied`)
