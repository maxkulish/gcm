# /pr:finalize - Post-Merge Cleanup and Task Completion

**Purpose**: Handle post-merge cleanup after a PR is merged. Updates aggregation files, Linear task status, and marks the task as complete. Supports both regular branches and git worktrees.

**Usage**:
- `/pr:finalize CLO-XX` - Finalize specific task after merge
- `/pr:finalize` - Interactive mode

---

## When to Use

This command should be run:
1. After PR is merged to main
2. To complete the task lifecycle
3. To update project documentation

---

## Command Execution Instructions

### Step 1: Extract Task Number

1. **Get task number** from argument or detect from context
2. **If not provided**: Ask user or check workflow state

### Step 2: Detect Git Worktree Mode

Check if we're running in a git worktree:

```bash
# Check current directory name for worktree pattern (repo--branch)
basename "$PWD"

# List all worktrees to confirm
git worktree list
```

**Worktree Detection Logic**:
- If directory name matches pattern `*--*` (e.g., `gcm--feat-clo-52-state-machine`), we're in a worktree
- Extract main repo name: everything before `--` (e.g., `gcm`)
- Main repo path: `../<main-repo-name>` (e.g., `../gcm`)

**Set variables for later steps**:
```bash
# Example for worktree: gcm--feat-clo-52-state-machine
CURRENT_DIR=$(basename "$PWD")
if [[ "$CURRENT_DIR" == *"--"* ]]; then
    IS_WORKTREE=true
    MAIN_REPO_NAME="${CURRENT_DIR%%--*}"
    MAIN_REPO_PATH="../$MAIN_REPO_NAME"
else
    IS_WORKTREE=false
    MAIN_REPO_PATH="."
fi
```

Display:
```
Git Mode: [Worktree / Regular Branch]
Main Repo: [path]
Current Branch: [branch name]
```

### Step 3: Verify PR is Merged

```bash
gh pr list --head "feat/clo-XX-description" --json number,state,mergedAt
```

**If PR not merged**:
```
PR #[number] is not yet merged.

State: [Open/Closed]

Options:
1. [merge] - Merge the PR now
2. [wait] - Exit and wait for merge
3. [force] - Continue anyway (cleanup without merge)

Your choice:
```

**If PR is merged**: Continue

### Step 4: Prepare Main Repository for Updates

#### If Worktree Mode:

```bash
# Go to main repo folder
cd "$MAIN_REPO_PATH"

# Verify we're on main branch
git rev-parse --abbrev-ref HEAD  # Should be "main"

# Pull latest (includes merged PR)
git pull origin main
```

Display:
```
Working in main repo: [path]
Pulled latest changes including merged PR.
```

**IMPORTANT**: All subsequent file operations (Steps 6-9) happen in the main repo folder.

#### If Regular Branch Mode:

```bash
# Switch to main branch
git checkout main

# Pull latest changes
git pull origin main
```

Display:
```
Switched to main branch.
Pulled latest changes including merged PR.
```

### Step 5: Update Linear Task Status

```
mcp__linear-server__update_issue(
  id="CLO-XX",
  state="Done"
)
```

Post final comment:

```
mcp__linear-server__create_comment(
  issueId="CLO-XX",
  body="## Task Complete

**Status**: Done
**PR**: #[number] (merged)
**Merged At**: [timestamp]

**Summary**:
[Brief summary of what was implemented]

**Documents**:
- Design: `docs/design-docs/clo-XX-[description].md`
- Plan: `docs/plans/clo-XX-[description].md`
- Status: `docs/status/clo-XX-[description].md`

This task is now complete."
)
```

### Step 6: Update Aggregation Files

**IMPORTANT**: In worktree mode, all file paths are relative to the main repo folder (set in Step 4).

#### 6.1: Update PROJECT.md

Read `docs/PROJECT.md` (in main repo) and update:

**Move task from Active Work to Recently Completed**:

Before:
```markdown
## Active Work
| Task | Status | Phase | Blocked By |
|------|--------|-------|------------|
| [CLO-XX](link) | In Progress | Phase 1 | - |
```

After:
```markdown
## Recently Completed
| Task | Completed | Summary |
|------|-----------|---------|
| [CLO-XX](link) | [Today's date] | [Brief summary] |
```

#### 6.2: Update ROADMAP.md

Read `docs/ROADMAP.md` and update:

**Change task status to Done**:

Before:
```markdown
| CLO-XX | [Title] | In Progress | CLO-YY |
```

After:
```markdown
| CLO-XX | [Title] | Done | CLO-YY |
```

**Update phase completion count** in Summary table:

```markdown
| Phase | Tasks | Completed | Status |
|-------|-------|-----------|--------|
| Phase 1 | 4 | 3 | In Progress |  <- Update count
```

#### 6.3: Update DEPENDENCIES.md

Read `docs/DEPENDENCIES.md` and update:

**Remove from Current Blockers** (if CLO-XX was blocking anything):

Before:
```markdown
## Current Blockers
| Blocked Task | Blocked By | Blocker Status | Notes |
|--------------|------------|----------------|-------|
| CLO-ZZ | CLO-XX | In Progress | ... |
```

After:
```markdown
## Unblocked & Ready
| Task | Dependencies Satisfied | Ready Since |
|------|------------------------|-------------|
| CLO-ZZ | CLO-XX complete | [Today's date] |
```

### Step 7: Commit Aggregation File Updates

**IMPORTANT**: Ensure you're in the main repo folder before committing.

#### If Worktree Mode:
```bash
# Should already be in main repo from Step 4
# Verify current directory
pwd  # Should show main repo path

# Add and commit (include status files from Steps 8-9 if already updated)
git add docs/PROJECT.md docs/ROADMAP.md docs/DEPENDENCIES.md docs/status/
git commit -m "$(cat <<'EOF'
docs(CLO-XX): update aggregation files for completed task

- PROJECT.md: Moved CLO-XX to Recently Completed
- ROADMAP.md: Updated task status to Done
- DEPENDENCIES.md: Updated blockers and unblocked tasks
EOF
)"
git push origin main
```

**Note**: Stay in main repo folder for Steps 8-9 (status file updates).

#### If Regular Branch Mode:
```bash
git add docs/PROJECT.md docs/ROADMAP.md docs/DEPENDENCIES.md
git commit -m "$(cat <<'EOF'
docs(CLO-XX): update aggregation files for completed task

- PROJECT.md: Moved CLO-XX to Recently Completed
- ROADMAP.md: Updated task status to Done
- DEPENDENCIES.md: Updated blockers and unblocked tasks
EOF
)"
git push origin main
```

### Step 8: Update Status File

**In worktree mode**: These files are in the main repo folder.

Update `docs/status/clo-XX-[description].md`:

```markdown
**Last Updated**: [Current Date/Time]

## Current Status: Complete

**Overall Progress**: 100% (X/X tasks)
**Completed**: [Date/Time]
**PR Merged**: [Date/Time]

---

## Final Summary

**Implementation**: Successfully completed all tasks.

**Modules**:
- [List of modules created/modified]

**Total Commits**: [count]
**PR**: #[number] (merged)
```

### Step 9: Update Workflow State (if exists)

**In worktree mode**: This file is in the main repo folder.

Update `docs/status/clo-XX-workflow.yaml`:

```yaml
workflow:
  current_phase: complete
  status: complete

phases:
  complete:
    status: complete
    aggregation_files_updated: true
    merged_at: [ISO timestamp]

history:
  - timestamp: [ISO timestamp]
    action: workflow_complete
    phase: complete
    details: "Task CLO-XX fully completed"
```

#### Commit Status File Updates (Worktree Mode)

If status files were updated in Steps 8-9, commit them:

```bash
# Still in main repo folder
git add docs/status/
git commit -m "$(cat <<'EOF'
docs(CLO-XX): update status files for completed task

- Updated status file with final summary
- Marked workflow as complete
EOF
)"
git push origin main
```

**Alternative**: Include status files in the Step 7 commit by running Steps 6, 8, 9 before Step 7's commit.

### Step 10: Display Completion Summary

Render the canonical summary defined in `.claude/templates/completion-summary.md`.

- Load `docs/status/clo-XX-workflow.yaml`.
- Resolve every placeholder using the field-mapping table in the template (`Source of Data` section).
- Apply the phase-skip rules for the current `task_type`.
- Print the rendered block exactly as specified — preserve box-drawing characters, indentation, emojis, and separator widths.

The footer line `Status: ✅ DONE` is the single authoritative completion signal — emit it only when `workflow.current_phase == complete` AND `workflow.status == complete`.

#### Worktree Mode addendum

After the canonical summary, append exactly this block on a new line (only in worktree mode):

```
NEXT STEPS:
1. Exit Claude Code
2. Run `gd` to delete this worktree and branch
3. You'll be returned to the main repo folder
```

In regular branch mode, do not append anything after the canonical summary.

---

## Aggregation File Templates

### PROJECT.md Recently Completed Entry

```markdown
| [CLO-XX](https://linear.app/cloud-ai/issue/CLO-XX) | [YYYY-MM-DD] | [One line summary] |
```

### ROADMAP.md Updated Entry

```markdown
| [CLO-XX](https://linear.app/cloud-ai/issue/CLO-XX) | [Title] | Done | [Dependencies] |
```

### DEPENDENCIES.md Unblocked Entry

```markdown
| CLO-ZZ | CLO-XX complete | [YYYY-MM-DD] |
```

---

## Special Cases

### Case 1: Aggregation Files Don't Exist

```
WARNING: Aggregation files not found

Expected files:
- docs/PROJECT.md [missing]
- docs/ROADMAP.md [missing]
- docs/DEPENDENCIES.md [found]

Options:
1. [create] - Create missing files from templates
2. [skip] - Skip aggregation updates
3. [cancel] - Cancel finalization

Your choice:
```

### Case 2: Task Not in Aggregation Files

```
NOTE: CLO-XX not found in ROADMAP.md

The task may not have been added to project tracking.

Options:
1. [add] - Add task to files now
2. [skip] - Skip this file
3. [manual] - I'll update manually

Your choice:
```

### Case 3: Merge Conflicts in Aggregation Files

```
WARNING: Aggregation files have conflicts

docs/PROJECT.md has conflicts after git pull.

Options:
1. [resolve] - Attempt auto-resolve
2. [manual] - Exit and resolve manually
3. [skip] - Skip this file

Your choice:
```

### Case 4: Multiple Tasks Were Blocking

When CLO-XX was blocking multiple tasks:

```
UNBLOCKED TASKS

CLO-XX completion unblocks:
- CLO-12: Add authentication
- CLO-15: Implement caching

All will be moved to "Unblocked & Ready" in DEPENDENCIES.md.

Proceed? (yes/edit/cancel)
```

### Case 5: Worktree Main Repo Not Found

```
WARNING: Main repo folder not found

Current directory: gcm--feat-clo-52-state-machine
Expected main repo: ../gcm

Options:
1. [path] - Specify main repo path manually
2. [skip] - Skip aggregation updates (update manually later)
3. [cancel] - Cancel finalization

Your choice:
```

### Case 6: Worktree Main Repo Has Uncommitted Changes

```
WARNING: Main repo has uncommitted changes

Changes in ../gcm:
- docs/PROJECT.md (modified)
- src/main.rs (modified)

Options:
1. [stash] - Stash changes, proceed, then unstash
2. [skip] - Skip aggregation updates
3. [cancel] - Cancel and resolve manually

Your choice:
```

---

## Cleanup Checklist

Before marking complete, verify:

- [ ] PR is merged to main
- [ ] Main branch is up-to-date locally (pulled in main repo)
- [ ] Linear task status is "Done"
- [ ] PROJECT.md updated
- [ ] ROADMAP.md updated
- [ ] DEPENDENCIES.md updated
- [ ] Status file has final summary
- [ ] Aggregation updates committed and pushed to main

**If Worktree Mode** (user handles after exiting):
- [ ] User runs `gd` to delete worktree and branch

---

## Integration Notes

**Called by**: `/task:orchestrate` as final step

**Follows**: PR merge

**Final step in workflow chain**

**Supports**:
- Regular branches (switches to main, optional branch deletion)
- Git worktrees (updates main repo folder, user runs `gd` to cleanup)

**Updates**:
- Aggregation files (all three, committed to main)
- Status file
- Workflow state file
- Linear task (Done status)

**Branch Cleanup**:
- **Regular mode**: Optional branch deletion offered
- **Worktree mode**: User runs `gd` after exiting Claude to delete worktree and branch
