# /pr:create - Create Pull Requests

**Purpose**: Create a pull request for a completed task implementation. Generates comprehensive PR description from design docs, plans, and commits.

**Usage**:
- `/pr:create CLO-XX` - Create PR for specific task
- `/pr:create` - Interactive mode (detects from branch name)

---

## Prerequisites

Before creating a PR:
- [ ] Implementation is complete (plan at 100%)
- [ ] All commits are pushed to feature branch
- [ ] Tests pass
- [ ] Branch is up-to-date with main

---

## Command Execution Instructions

### Step 1: Extract Task Number

1. **If user provided task number** (e.g., `/pr:create clo-9`):
   - Normalize formats
   - Proceed to Step 2

2. **If no task number provided**:
   - Check current branch name for `clo-XX` pattern
   - If found: Use extracted task number
   - If not found: Ask user

### Step 2: Verify Branch State

```bash
# Check current branch
git rev-parse --abbrev-ref HEAD

# Verify on feature branch
# Expected: feat/clo-XX-*
```

**If on main**:
```
ERROR: Currently on main branch

PRs must be created from feature branches.
Expected: feat/clo-XX-*

Please checkout your feature branch first.
```
Exit command.

**Verify branch matches task**:
```bash
# Branch should contain clo-XX
git rev-parse --abbrev-ref HEAD | grep -i "clo-XX"
```

### Step 3: Check Uncommitted Changes

```bash
git status --porcelain
```

**If uncommitted changes exist**:
```
WARNING: Uncommitted changes detected

[list of files]

Options:
1. [commit] - Commit changes first
2. [stash] - Stash changes and continue
3. [continue] - Create PR anyway (changes won't be included)
4. [cancel] - Cancel

Your choice:
```

### Step 4: Verify Remote Push

```bash
# Check if branch is pushed
git status -sb
# Look for "ahead" indicator
```

**If not pushed or ahead of remote**:
```bash
git push origin feat/clo-XX-description
```

Display: "Pushed [X] commits to remote."

### Step 5: Check for Existing PR

```bash
gh pr list --head "feat/clo-XX-description" --json number,url,state
```

**If PR already exists**:
```
PR already exists for this branch.

PR #[number]: [title]
URL: [url]
State: [open/merged/closed]

Options:
1. [view] - View existing PR
2. [recreate] - Close and create new PR
3. [cancel] - Cancel

Your choice:
```

### Step 6: Gather PR Content

#### Read Design Document

```bash
# Find design doc
ls docs/design-docs/clo-XX-*.md
```

Extract:
- Summary section
- Architecture decisions
- Key components

#### Read Plan File

```bash
# Find plan file
ls docs/plans/clo-XX-*.md
```

Extract:
- Total tasks and phases
- Key implementation details

#### Read Status File

```bash
# Find status file
ls docs/status/clo-XX-*.md
```

Extract:
- Technical decisions made
- Important findings

#### Analyze Commits

```bash
# Get commits on this branch not in main
git log main..HEAD --oneline
```

#### Get Diff Stats

```bash
git diff main --stat
```

### Step 7: Generate PR Title

Format: `feat(CLO-XX): [Short description from task title]`

Examples:
- `feat(CLO-9): implement websocket handler`
- `feat(CLO-10): add cli argument parsing`
- `fix(CLO-15): resolve connection timeout`

### Step 8: Generate PR Body

```markdown
## Summary

[2-3 sentences from design doc summary]

## Changes

### Modules
- `src/[module]/` - [description]
- `tests/[module]/` - [test files]

### Key Decisions
- [Decision 1 from status file]
- [Decision 2]

## Testing

- [ ] Unit tests pass (`cargo test`)
- [ ] Integration tests pass
- [ ] Manual testing completed
- [ ] Code formatted (`cargo fmt`)

## Commits

[List of commit messages]

## Related

- **Linear**: https://linear.app/cloud-ai/issue/CLO-XX
- **Design Doc**: docs/design-docs/clo-XX-[description].md
- **Plan**: docs/plans/clo-XX-[description].md
- **Status**: docs/status/clo-XX-[description].md
```

### Step 9: Create PR

```bash
gh pr create \
  --title "feat(CLO-XX): [description]" \
  --body "$(cat <<'EOF'
[Generated PR body]
EOF
)" \
  --base main \
  --head feat/clo-XX-description
```

Capture PR URL and number from output.

### Step 10: Link PR to Linear

Post comment to Linear task:

```
mcp__linear-server__create_comment(
  issueId="CLO-XX",
  body="## Pull Request Created

**PR**: [URL]
**Status**: Open

**Summary of Changes**:
- [Key change 1]
- [Key change 2]

**Files Changed**: [count]
**Commits**: [count]

Awaiting review."
)
```

### Step 11: Update Linear Task Status

```
mcp__linear-server__update_issue(
  id="CLO-XX",
  state="In Review"
)
```

### Step 12: Update Workflow State (if exists)

If `docs/status/clo-XX-workflow.yaml` exists:

```yaml
phases:
  pr:
    status: in_progress
    pr_url: [url]
    pr_number: [number]

history:
  - timestamp: [ISO timestamp]
    action: pr_created
    phase: pr
    details: "PR #[number] created"
```

### Step 13: Confirm to User

```
SUCCESS: Pull request created!

PR #[number]: feat(CLO-XX): [description]
URL: [url]

Summary:
- Files changed: [count]
- Commits: [count]
- Lines: +[added] -[removed]

Linear:
- Task CLO-XX updated to "In Review"
- Comment posted with PR link

Next steps:
1. Wait for review
2. Address feedback: /pr:review CLO-XX
3. After approval, merge and finalize

Or continue with orchestrator: /task:orchestrate CLO-XX
```

---

## PR Body Template (Full)

```markdown
## Summary

[Extracted from design doc summary section]

## Background

[Brief context - why this change is needed]

## Changes

### New/Modified Modules

| Module | Type | Description |
|--------|------|-------------|
| `src/[name]/` | New | [What it does] |
| `tests/[name]/` | New | [Test coverage] |

### Key Implementation Details

- [Detail 1]
- [Detail 2]

## Architecture

[If applicable, include relevant diagram or reference to design doc]

## Testing

### Validation
- [x] `cargo fmt` - Code formatted
- [x] `cargo clippy` - No warnings
- [x] `cargo test` - All tests pass

### Test Coverage
- Unit tests for [component]
- Integration tests for [component]

## Security Considerations

- [Security item 1]
- [Security item 2]

## Performance Considerations

[Any performance impact or improvements]

## Commits

<details>
<summary>Commit history ([count] commits)</summary>

```
[git log output]
```

</details>

## Related

- **Linear Task**: https://linear.app/cloud-ai/issue/CLO-XX
- **Design Document**: `docs/design-docs/clo-XX-[description].md`
- **Implementation Plan**: `docs/plans/clo-XX-[description].md`
- **Status Log**: `docs/status/clo-XX-[description].md`

## Checklist

- [ ] Code follows project conventions
- [ ] Code formatted (`cargo fmt`)
- [ ] All tests pass
- [ ] Documentation updated
- [ ] Design doc referenced
- [ ] Linear task linked
```

---

## Special Cases

### Case 1: No design document

If design doc doesn't exist:
```
WARNING: No design document found

PR will be created with limited context.
Consider creating a design doc for better documentation.

Continue? (yes/no)
```

### Case 2: Implementation incomplete

If plan shows < 100%:
```
WARNING: Implementation appears incomplete

Plan progress: 75% (15/20 tasks)

Creating a PR for incomplete work is not recommended.

Options:
1. [continue] - Create PR anyway (draft)
2. [complete] - Return to implementation
3. [cancel] - Cancel

Your choice:
```

If continue, add `--draft` flag to `gh pr create`.

### Case 3: Branch behind main

```bash
git fetch origin main
git log HEAD..origin/main --oneline
```

If commits exist:
```
WARNING: Branch is behind main

Your branch is [X] commits behind main.

Options:
1. [rebase] - Rebase on main first
2. [continue] - Create PR anyway (may have conflicts)
3. [cancel] - Cancel

Your choice:
```

---

## Integration Notes

**Called by**: `/task:orchestrate` after implementation complete

**Follows**: `/plan:implement`

**Precedes**: `/pr:review`

**Updates**:
- Linear task (status + comment)
- Workflow state file (if exists)
