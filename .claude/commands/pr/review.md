# /pr:review - Handle PR Review Feedback

**Purpose**: Check for PR review comments, analyze feedback, make necessary changes, and respond to reviewers. Automates the review feedback cycle.

**Usage**:
- `/pr:review CLO-XX` - Check and address reviews for specific task
- `/pr:review` - Interactive mode (detects from branch)

---

## Workflow

```
┌─────────────────────────────────────────────────────────────────┐
│                    PR Review Cycle                              │
├─────────────────────────────────────────────────────────────────┤
│  1. Fetch PR reviews and comments (with pagination)             │
│  2. Analyze feedback (blocking vs suggestions)                  │
│  3. Detect stale comments (line changed since comment)          │
│  4. Make code changes to address feedback                       │
│  5. Commit changes with descriptive message                     │
│  6. Push to branch (BEFORE replying)                            │
│  7. Reply to EVERY comment (MANDATORY - track N/N)              │
│  8. Repeat if new comments arrive                               │
└─────────────────────────────────────────────────────────────────┘
```

### MANDATORY: Reply to Every Comment

**Every review comment MUST receive a reply.** This is not optional.
No comment may be left without a response - whether the fix was applied, declined with rationale, or acknowledged. The PR review is not complete until all comments have replies posted via the GitHub API.

| Decision | Required Reply |
|----------|---------------|
| Fixed | "Fixed in [SHA]. [what changed]" |
| Declined | "Intentionally kept as-is: [rationale]" |
| Question answered | "[explanation]. [reference to design doc if relevant]" |
| Deferred | "Tracked as follow-up in [task/issue]. [reason for deferral]" |

For `gemini-code-assist` comments, every reply MUST end with `/gemini review` on its own line to trigger re-validation.

---

## Command Execution Instructions

### Step 1: Extract Task and PR Info

1. **Get task number** from argument or branch name
2. **Find PR number**:

```bash
gh pr list --head "feat/clo-XX-description" --json number,url,state
```

**If no PR found**:
```
ERROR: No PR found for CLO-XX

Expected PR from branch: feat/clo-XX-*

Create one first: /pr:create CLO-XX
```
Exit command.

### Step 2: Fetch PR Status

```bash
gh pr view [number] --json state,reviews,reviewDecision,comments,mergeable
```

Extract:
- `state`: open, closed, merged
- `reviews`: List of reviews with state (APPROVED, CHANGES_REQUESTED, COMMENTED)
- `reviewDecision`: Overall decision
- `comments`: General PR comments
- `mergeable`: Whether PR can be merged

**If PR is merged**:
```
PR #[number] is already merged.

Merged at: [timestamp]

No review action needed.
```
Exit command.

### Step 3: Fetch Review Comments

**Use `--paginate` on all gh api calls** to ensure no comments are missed on PRs with many reviews (default page size is 30).

```bash
# Get review comments (inline code comments) - paginated
gh api repos/{owner}/{repo}/pulls/[number]/comments --paginate \
  --jq '.[] | {id, path, line, original_line, body, user: .user.login, created_at, commit_id: .original_commit_id}'

# Get review threads - paginated
gh api repos/{owner}/{repo}/pulls/[number]/reviews --paginate \
  --jq '.[] | {id, state, body, user: .user.login}'

# Get issue comments (general discussion)
gh pr view [number] --json comments --jq '.comments[] | {id, body, author: .author.login}'
```

**Note**: The `commit_id` and `original_line` fields are used for stale comment detection in Step 4.5.

### Step 4: Categorize Feedback

Group comments by type:

| Category | Priority | Action Required |
|----------|----------|-----------------|
| `CHANGES_REQUESTED` | High | Must address before merge |
| `COMMENTED` (blocking) | Medium | Should address |
| `COMMENTED` (suggestion) | Low | Optional, acknowledge |
| `APPROVED` | None | No action needed |

**Identify blocking feedback**:
- Explicit change requests
- Questions about implementation
- Security concerns
- Bug reports

**Identify non-blocking**:
- Style suggestions
- "Nice to have" improvements
- Positive feedback

### Step 4.5: Detect Stale Comments

For each inline comment, check if the referenced code has changed since the comment was posted:

1. The comment's `original_commit_id` tells you what commit the reviewer saw
2. Compare the file at that commit vs HEAD:
   ```bash
   git diff [original_commit_id]..HEAD -- [file_path]
   ```
3. If the diff includes changes around the commented line (within 5 lines), flag the comment as **potentially stale**

**Stale comments are presented to the user but marked clearly:**
```
[STALE?] @reviewer on src/backend/retry.rs:45
  "Consider adding jitter to retry delay"
  Note: Lines 40-50 of this file changed in commit abc1234 after this comment.
```

The user decides whether stale comments still need action. Do not auto-skip them.

### Step 5: Display Review Summary

```
========================================
PR REVIEW STATUS: CLO-XX
========================================

PR #[number]: [title]
State: Open
Mergeable: Yes/No

Reviews:
  - @reviewer1: APPROVED
  - @reviewer2: CHANGES_REQUESTED

Overall Decision: [APPROVED / CHANGES_REQUESTED / PENDING]

Comments to Address: [count]

---

BLOCKING FEEDBACK:

1. @reviewer2 on src/websocket/handler.rs:45
   "Consider using async/await pattern here"
   Status: Unresolved

2. @reviewer2 general comment
   "Please add documentation for the public API"
   Status: Unresolved

---

SUGGESTIONS (optional):

1. @reviewer1 on src/websocket/parser.rs:12
   "Consider adding validation for input"
   Status: Unresolved

---

Options:
1. [address] - Address blocking feedback
2. [address-all] - Address all feedback including suggestions
3. [skip] - Skip for now
4. [details] - Show full comment details

Your choice:
```

### Step 6: Address Feedback

For each piece of blocking feedback:

#### 6.1: Analyze the Comment

Read the comment and:
1. Identify the file and line referenced
2. Understand the requested change
3. Read surrounding code for context
4. Determine the fix

#### 6.2: Make Code Changes

Use appropriate tools to implement the fix:

```bash
# Read the file
Read tool: [file path]

# Make changes
Edit tool: [modifications]

# Validate
cargo build && cargo test
```

#### 6.3: Track Changes Made

Keep a list of changes for commit message:
- `[file]: [change description]`

### Step 7: Create Review Response Commit

After addressing feedback, commit with descriptive message:

```bash
git add [modified files]
git commit -m "$(cat <<'EOF'
fix(CLO-XX): address PR review feedback

Changes:
- src/websocket/handler.rs: Use async/await pattern
- src/websocket/mod.rs: Add public API documentation

Resolves review comments from @reviewer2

Related: PR #[number]
EOF
)"
```

### Step 8: Push Changes

Push BEFORE replying so the commit SHA is visible on GitHub when reviewers read your replies.

```bash
git push origin feat/clo-XX-description
```

### Step 9: Reply to EVERY Comment (MANDATORY)

**This step is REQUIRED. Do not skip it. Do not proceed to Step 10 until every comment has a reply.**

For EACH comment (fixed, declined, or question), post a reply via the GitHub API:

```bash
# Reply to a review comment
gh api repos/{owner}/{repo}/pulls/[number]/comments/[comment_id]/replies \
  -X POST -f body="Fixed in [commit SHA]. [Brief explanation of change]"
```

**Rules**:
1. Every comment gets a reply - no exceptions
2. Reference the commit SHA that contains the fix
3. If declining a suggestion, explain why (reference design docs, ADRs, or project constraints)
4. For `gemini-code-assist` comments: every reply MUST end with `/gemini review` on its own line
5. For `copilot-pull-request-reviewer` comments: reply with fix details (no special trigger needed)
6. Track reply count - the final summary must show `Replies Posted: N/N`

**Reply templates by reviewer type**:

| Reviewer | Decision | Reply Template |
|----------|----------|----------------|
| Human | Bug fix | "Fixed in abc1234. Good catch!" |
| Human | Declined | "Intentionally kept as-is: [rationale]. Happy to discuss." |
| `gemini-code-assist` | Fixed | "Fixed in abc1234. [details]\n\n/gemini review" |
| `gemini-code-assist` | Declined | "Kept as-is: [reason]\n\n/gemini review" |
| `copilot-pull-request-reviewer` | Fixed | "Fixed in abc1234. [details]" |
| `copilot-pull-request-reviewer` | Declined | "Intentionally kept as-is: [rationale]" |

**Batch replies** (for multiple comments):

```bash
COMMIT_SHA=$(git rev-parse --short HEAD)
COMMENTS=(
  "COMMENT_ID_1|Fixed: description of change"
  "COMMENT_ID_2|Declined: rationale for keeping as-is"
)

for item in "${COMMENTS[@]}"; do
  ID="${item%%|*}"
  MSG="${item#*|}"
  gh api repos/{owner}/{repo}/pulls/[number]/comments/${ID}/replies \
    -X POST -f body="${MSG} (${COMMIT_SHA})"
done
```

For `gemini-code-assist` comments, append `/gemini review` to each reply body.

### Step 10: Update Workflow State (if exists)

```yaml
phases:
  pr:
    reviews_addressed: [increment count]

history:
  - timestamp: [ISO timestamp]
    action: review_addressed
    phase: pr
    details: "Addressed [count] review comments, pushed [commit SHA]"
```

### Step 11: Re-check Review Status

After pushing, check if new comments appeared:

```bash
gh pr view [number] --json reviews,reviewDecision
```

**If new changes requested**:
```
New feedback received after your push.

[New comments]

Would you like to address these? (yes/no)
```

**If approved**:
```
SUCCESS: PR is now approved!

All reviewers have approved.
Ready to merge.

Next steps:
1. Merge: gh pr merge [number] --squash
2. Or continue with orchestrator: /task:orchestrate CLO-XX
```

### Step 12: Update Linear

Post update to Linear:

```
mcp__linear-server__create_comment(
  issueId="CLO-XX",
  body="## PR Review Update

**PR**: #[number]
**Status**: [Addressed feedback / Approved]

**Changes Made**:
- [Change 1]
- [Change 2]

**Commits**: [SHA]

**Review Status**:
- @reviewer1: Approved
- @reviewer2: [Updated status]"
)
```

### Step 13: Confirm to User

**GATE**: Do not display this summary until ALL comments have replies posted.
If any comment is missing a reply, go back to Step 9 and post it.

```
========================================
REVIEW FEEDBACK ADDRESSED
========================================

PR #[number]: [title]

Changes Made:
- [file1]: [change]
- [file2]: [change]

Commits: [count]
Pushed: Yes

Replies Posted: [N/N] (MUST be N/N - all comments replied to)
- Comment [id]: [fixed/declined/answered] (@reviewer)
- Comment [id]: [fixed/declined/answered] (@gemini-code-assist, with /gemini review)
- Comment [id]: [fixed/declined/answered] (@copilot-pull-request-reviewer)
- ...

Stale Comments: [count skipped with user approval]

Review Status:
- @reviewer1: APPROVED
- @reviewer2: CHANGES_REQUESTED -> [pending re-review]

Next steps:
1. Wait for re-review
2. Run /pr:review CLO-XX again if needed
3. After approval: /task:orchestrate CLO-XX
```

---

## Handling Different Feedback Types

### Type 1: Code Change Request

```markdown
Reviewer: "Use async/await instead of callbacks"
File: src/websocket/handler.rs:45

Action:
1. Read src/websocket/handler.rs
2. Find line 45
3. Refactor to async/await
4. Commit and reply
```

### Type 2: Missing Functionality

```markdown
Reviewer: "Please add input validation"

Action:
1. Identify the relevant function
2. Add validation logic
3. Add tests for validation
4. Commit and reply
```

### Type 3: Documentation Request

```markdown
Reviewer: "Add usage examples to the module documentation"

Action:
1. Find or create doc comments
2. Add usage examples
3. Commit and reply
```

### Type 4: Question (No Code Change)

```markdown
Reviewer: "Why did you choose this approach?"

Action:
1. Reply with explanation
2. Reference design doc if applicable
3. No code change needed
```

### Type 5: Suggestion (Optional)

```markdown
Reviewer: "Nice to have: could add logging here"

Action:
1. Evaluate effort vs value
2. If quick: implement and reply
3. If complex: reply explaining decision to defer
```

---

## Batch Processing

When multiple comments exist:

1. **Group by file**: Address all comments on same file together
2. **Order by priority**: Blocking first, then suggestions
3. **Single commit per file group**: Avoid many small commits
4. **Batch replies**: Reply to all addressed comments

Example commit for batch:

```
fix(CLO-XX): address PR review feedback (batch)

src/websocket/handler.rs:
- Line 45: Use async/await pattern
- Line 67: Add error context

src/websocket/parser.rs:
- Add input validation
- Add documentation

Resolves: 4 review comments from @reviewer2
```

---

## Special Cases

### Case 1: Conflicting Feedback

When two reviewers give conflicting feedback:

```
CONFLICT DETECTED: Reviewers disagree

@reviewer1: "Use sync approach for simplicity"
@reviewer2: "Use async for performance"

Please decide:
1. Follow @reviewer1's suggestion
2. Follow @reviewer2's suggestion
3. Find a compromise
4. Discuss in PR comments

Your choice:
```

### Case 2: Feedback Requires Design Change

```
SIGNIFICANT CHANGE REQUESTED

@reviewer1: "This should use a completely different architecture"

This feedback suggests changes beyond implementation fixes.

Options:
1. [discuss] - Comment asking for clarification
2. [update-design] - Revisit design document
3. [escalate] - Tag project lead for decision
4. [implement] - Try to implement anyway

Your choice:
```

### Case 3: Stale Comments

Stale comments are now detected automatically in Step 4.5. When a comment references code that has changed since the comment was posted, it is flagged with `[STALE?]` in the review summary.

The user decides how to handle stale comments:
- **Already addressed**: Reply explaining the change resolves the concern
- **Still relevant**: Address as normal
- **No longer applicable**: Reply noting the code has been restructured

### Case 4: All Approved

```
SUCCESS: All reviews approved!

No action needed.

PR is ready to merge.

Options:
1. [merge] - Merge the PR
2. [wait] - Wait for more reviews
3. [exit] - Exit (merge manually)

Your choice:
```

---

## AI Code Review: gemini-code-assist

When `gemini-code-assist` is configured on the repository, it automatically reviews PRs and leaves inline comments with code suggestions.

### Fetching gemini-code-assist Comments

**Get all comments from gemini-code-assist**:

```bash
# Fetch comments filtered by user
gh api repos/{owner}/{repo}/pulls/[number]/comments \
  --jq '.[] | select(.user.login == "gemini-code-assist") | {id, path, line, body}'
```

**Example output**:
```json
{
  "id": 2707454116,
  "path": "src/backend/claude.rs",
  "line": 50,
  "body": "**Severity**: high\n\nConsider using serde's tagged enum..."
}
```

### Understanding gemini-code-assist Severity Levels

| Severity | Priority | Action |
|----------|----------|--------|
| `high` | Must fix | Address before merge |
| `medium` | Should fix | Strongly recommended |
| `low` | Optional | Nice to have |

**Parse severity from comment body**:
- Look for `**Severity**: high/medium/low` pattern
- Comments without severity default to `medium`

### Workflow for gemini-code-assist Feedback

```
┌─────────────────────────────────────────────────────────────────┐
│              gemini-code-assist Review Cycle                    │
├─────────────────────────────────────────────────────────────────┤
│  1. Fetch comments from gemini-code-assist                      │
│  2. Categorize by severity (high → medium → low)                │
│  3. Address issues (code changes)                               │
│  4. Commit fixes with descriptive message                       │
│  5. Push changes to branch                                      │
│  6. Reply to EACH comment with fix details + /gemini review     │
│  7. Gemini re-validates the changes automatically               │
└─────────────────────────────────────────────────────────────────┘
```

### Step-by-Step: Address gemini-code-assist Feedback

#### 1. Fetch and Display Comments

```bash
# Get all gemini-code-assist comments with details
gh api repos/{owner}/{repo}/pulls/[number]/comments \
  --jq '.[] | select(.user.login == "gemini-code-assist") | {
    id: .id,
    file: .path,
    line: .original_line,
    body: .body
  }'
```

#### 2. Analyze Each Comment

For each comment, identify:
- **File**: Which file needs changes
- **Line**: The specific line referenced
- **Issue**: What problem gemini found
- **Suggestion**: The recommended fix

#### 3. Make Code Changes

Address all issues, then commit:

```bash
git add [modified files]
git commit -m "$(cat <<'EOF'
fix(CLO-XX): address gemini-code-assist review feedback

- src/audio/error.rs: Use tagged enum serialization
- docs/design-docs: Fix documentation inconsistency
- src/audio/capture.rs: Optimize memory allocation

Resolves gemini-code-assist comments
EOF
)"
```

#### 4. Push Changes

```bash
git push origin feat/clo-XX-description
```

#### 5. Reply to Each Comment with Re-validation Trigger

**CRITICAL**: After pushing fixes, reply to EACH comment explaining the fix AND include `/gemini review` to trigger re-validation.

```bash
# Reply to comment explaining the fix
gh api repos/{owner}/{repo}/pulls/[number]/comments/[comment_id]/replies \
  -X POST -f body="Fixed in [commit SHA]. [Brief explanation of change]

/gemini review"
```

**Example replies by severity**:

| Severity | Reply Template |
|----------|----------------|
| High | `"Fixed in abc1234. Changed to use #[serde(tag = \"type\")] for proper tagged enum serialization.\n\n/gemini review"` |
| Medium | `"Fixed in abc1234. Updated documentation to match implementation (SincFixedIn, not FftFixedIn).\n\n/gemini review"` |
| Medium | `"Fixed in abc1234. Added reusable buffer to eliminate per-call allocation.\n\n/gemini review"` |
| Low | `"Good suggestion. Implemented in abc1234.\n\n/gemini review"` |

### Batch Reply Script

When addressing multiple gemini-code-assist comments:

```bash
# Store comment IDs and their fix descriptions
COMMENTS=(
  "2707454116|Fixed AudioError serialization with tagged enum"
  "2707454125|Updated docs to reference SincFixedIn"
  "2707454129|Added reusable drain_buffer to avoid allocation"
)

COMMIT_SHA=$(git rev-parse --short HEAD)

for item in "${COMMENTS[@]}"; do
  ID="${item%%|*}"
  MSG="${item#*|}"

  gh api repos/{owner}/{repo}/pulls/[number]/comments/${ID}/replies \
    -X POST -f body="Fixed in ${COMMIT_SHA}. ${MSG}

/gemini review"
done
```

### What `/gemini review` Does

When you include `/gemini review` in a comment reply:

1. **Triggers Re-analysis**: Gemini re-reads the updated files
2. **Validates Fixes**: Checks if your changes address the original concern
3. **Updates Status**: May mark the conversation as resolved
4. **Posts Follow-up**: If issues remain, posts additional feedback

### gemini-code-assist Summary Display

```
========================================
GEMINI-CODE-ASSIST REVIEW: CLO-XX
========================================

PR #[number]: [title]

Comments Found: 3

HIGH PRIORITY:
1. [ID: 2707454116] src/audio/error.rs:50
   "Consider using serde's tagged enum..."
   Status: Needs fix

MEDIUM PRIORITY:
2. [ID: 2707454125] docs/design-docs/clo-47-audio-capture.md:142
   "Documentation says FftFixedIn but code uses SincFixedIn"
   Status: Needs fix

3. [ID: 2707454129] src/audio/capture.rs:132
   "drain_to_storage allocates Vec on each call"
   Status: Needs fix

---

Options:
1. [address-all] - Fix all issues
2. [address-high] - Fix high priority only
3. [details ID] - Show full comment for specific ID
4. [skip] - Skip for now

Your choice:
```

### After Addressing All Feedback

```
========================================
GEMINI-CODE-ASSIST FEEDBACK ADDRESSED
========================================

PR #[number]: [title]

Issues Fixed: 3/3
Commit: cfbcd70

Replies Posted:
- Comment 2707454116: ✓ (with /gemini review)
- Comment 2707454125: ✓ (with /gemini review)
- Comment 2707454129: ✓ (with /gemini review)

Gemini will automatically re-validate the changes.

Next steps:
1. Wait for gemini re-review (~1-2 minutes)
2. Check for new comments: /pr:review CLO-XX
3. After approval: merge or continue workflow
```

---

## AI Code Review: copilot-pull-request-reviewer

When GitHub Copilot is configured as a PR reviewer, it leaves inline code suggestions similar to gemini-code-assist but without severity levels.

### Fetching copilot Comments

```bash
gh api repos/{owner}/{repo}/pulls/[number]/comments --paginate \
  --jq '.[] | select(.user.login == "copilot-pull-request-reviewer") | {id, path, line, body}'
```

### Key Differences from gemini-code-assist

| Aspect | gemini-code-assist | copilot-pull-request-reviewer |
|--------|-------------------|-------------------------------|
| Severity levels | Yes (`**Severity**: high/medium/low`) | No - treat all as medium |
| Re-validation trigger | `/gemini review` in reply | None needed - auto re-reviews on push |
| Suggestion format | Markdown with severity header | Markdown, often with code blocks |
| Reply format | Must include `/gemini review` | Standard reply, no special suffix |

### Handling copilot Feedback

1. **Fetch comments** filtered by `copilot-pull-request-reviewer`
2. **Treat all as medium priority** (no severity parsing needed)
3. **Address issues** the same way as other review comments
4. **Reply to each comment** with fix details (no `/gemini review` needed)
5. Copilot automatically re-reviews when new commits are pushed

---

## Integration Notes

**Called by**: `/task:orchestrate` during PR phase

**Follows**: `/pr:create`

**Precedes**: Merge (via orchestrator or manual)

**Updates**:
- Code files (to address feedback)
- Git repository (commits)
- PR comments (replies)
- Linear task (status update)
- Workflow state file
