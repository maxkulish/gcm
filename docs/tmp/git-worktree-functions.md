# Git Worktree Management

Shell scripts and functions for managing git worktrees - perfect for creating sandboxes for AI agents to work on fixes while you continue on main branch.

## Why Git Worktrees?

Git worktrees let you have multiple working directories attached to the same repository. Instead of stashing changes or committing half-done work to switch branches, you can:

1. Keep working on `main` in your primary directory
2. Spin up a separate directory with a new branch for an AI agent
3. Both directories share the same `.git` - commits are visible to both
4. When done, cleanly remove the worktree and branch

## Architecture: Scripts + Thin Wrappers

**Important:** This implementation uses standalone scripts in `~/bin` rather than pure shell functions. This solves critical PATH and environment issues.

### Why Scripts Instead of Functions?

Shell functions can fail in unexpected ways:

| Problem | Cause | Script Solution |
|---------|-------|-----------------|
| `command not found: git` | Zsh caches command paths in hash table | Scripts set PATH explicitly |
| PATH not available | Claude Code shell snapshots capture stale state | Scripts inherit current environment |
| `env: zsh: No such file or directory` | `/usr/bin/env` can't find interpreter | Use absolute shebang `#!/bin/zsh` |
| Functions shadow scripts | Old function definitions persist in memory | `unfunction` clears them |

### Component Overview

```
~/bin/ga     # Main script - creates worktree (hardcoded git path)
~/bin/gd     # Main script - deletes worktree
~/bin/gl     # Main script - lists worktrees
~/.zshrc     # Thin wrappers that call scripts + handle cd
```

## Installation

### 1. Create Scripts in ~/bin

```bash
mkdir -p ~/bin
```

#### ~/bin/ga - Create Worktree

```bash
#!/bin/zsh
# ga - Create a new git worktree and branch
# Usage: ga <branch-name>

GIT=/opt/homebrew/bin/git

if [[ -z "$1" ]]; then
    echo "Usage: ga <branch-name>"
    echo "Creates a new worktree at ../<repo>--<branch>"
    exit 1
fi

if ! $GIT rev-parse --git-dir > /dev/null 2>&1; then
    echo "Error: Not in a git repository"
    exit 1
fi

branch="$1"
base="${PWD:t}"
# Replace slashes with dashes in directory name to avoid nested dirs
dir_safe_branch="${branch//\//-}"
path="../${base}--${dir_safe_branch}"

if $GIT show-ref --verify --quiet "refs/heads/$branch"; then
    echo "Error: Branch '$branch' already exists"
    exit 1
fi

if [[ -d "$path" ]]; then
    echo "Error: Directory '$path' already exists"
    exit 1
fi

$GIT worktree add -b "$branch" "$path" || exit 1

echo "Created worktree at $path with branch $branch"
```

#### ~/bin/gd - Delete Worktree

```bash
#!/bin/zsh
# gd - Remove git worktree and branch
# Usage: gd (run from within a worktree directory)

GIT=/opt/homebrew/bin/git

cwd="$PWD"
worktree="${cwd:t}"
root="${worktree%%--*}"
dir_safe_branch="${worktree#*--}"

if [[ "$root" == "$worktree" ]]; then
    echo "Error: Not in a worktree directory (expected format: repo--branch)"
    exit 1
fi

if [[ ! -d "../$root" ]]; then
    echo "Error: Cannot find parent repository at ../$root"
    exit 1
fi

# Get actual branch name from git (handles slash conversion)
actual_branch=$($GIT rev-parse --abbrev-ref HEAD 2>/dev/null)
if [[ -z "$actual_branch" ]]; then
    echo "Error: Could not determine current branch"
    exit 1
fi

echo -n "Remove worktree '$worktree' and branch '$actual_branch'? [y/N] "
read -r response
[[ "$response" =~ ^[Yy]$ ]] || exit 0

cd "../$root"
$GIT worktree remove "$cwd" --force
$GIT branch -D "$actual_branch"
echo "Removed worktree and branch '$actual_branch'"
```

#### ~/bin/gl - List Worktrees

```bash
#!/bin/zsh
# gl - List all git worktrees for current repository

GIT=/opt/homebrew/bin/git

if ! $GIT rev-parse --git-dir > /dev/null 2>&1; then
    echo "Error: Not in a git repository"
    exit 1
fi

echo "Git Worktrees:"
$GIT worktree list
```

### 2. Make Scripts Executable

```bash
chmod +x ~/bin/ga ~/bin/gd ~/bin/gl
```

### 3. Ensure ~/bin is in PATH

Add to `~/.zshrc` (usually already there on macOS):

```bash
export PATH="$HOME/bin:$PATH"
```

### 4. Add Wrapper Functions to ~/.zshrc

These thin wrappers call the scripts and handle directory changes (which scripts alone cannot do).

> **⚠️ Critical:** The `ga` wrapper must use `${branch//\//-}` to convert slashes to dashes. This must match the script's path calculation, or `cd` will fail for branches like `feature/fix`.

```bash
# Git Worktree Management
# Scripts in ~/bin do the real work (with hardcoded git path for reliability)
# These wrappers handle directory changes which scripts can't do
unalias ga gd gl 2>/dev/null
unfunction ga gd gl 2>/dev/null

# ga - Create worktree and cd into it
ga() {
    local branch="$1"
    local base="$(basename "$PWD")"
    local path="../${base}--${branch//\//-}"
    "$HOME/bin/ga" "$@" && cd "$path"
}

# gd - Remove worktree and cd to parent repo
gd() {
    local worktree="$(basename "$PWD")"
    local root="${worktree%%--*}"
    "$HOME/bin/gd" "$@" && cd "../$root"
}

# gl - List worktrees (no cd needed, just call script)
alias gl="$HOME/bin/gl"
```

### 5. Reload Shell

```bash
source ~/.zshrc
```

## Usage

```bash
# In your main repo directory
cd ~/projects/myapp

# Create a worktree for an AI agent to work on a fix
ga feature/agent-fix
# Output: Created worktree at ../myapp--feature-agent-fix with branch feature/agent-fix
# You are now in ~/projects/myapp--feature-agent-fix

# List all worktrees
gl
# Output:
# Git Worktrees:
# /Users/you/projects/myapp                     abc1234 [main]
# /Users/you/projects/myapp--feature-agent-fix  def5678 [feature/agent-fix]

# When the agent is done, clean up from within the worktree
gd
# Prompts: Remove worktree 'myapp--feature-agent-fix' and branch 'feature/agent-fix'? [y/N]
# If confirmed: removes worktree directory and branch, returns to main repo
```

## Key Implementation Details

### Hardcoded Git Path

```bash
GIT=/opt/homebrew/bin/git
```

Using an absolute path to git bypasses all PATH-related issues:
- Zsh command hash table caching
- Claude Code shell snapshots with stale environments
- PATH not being set when script runs

**Note:** Adjust this path if your git is installed elsewhere (run `which git` to find it).

### Slash Handling in Branch Names

Branch names like `feature/my-fix` contain slashes. Without handling, this creates nested directories:

```
# Bad: ../myapp--feature/my-fix becomes nested dirs
myapp--feature/
    my-fix/

# Good: ../myapp--feature-my-fix is flat
myapp--feature-my-fix/
```

The fix uses zsh parameter expansion to replace slashes:

```bash
dir_safe_branch="${branch//\//-}"
# feature/my-fix → feature-my-fix
```

### Smart Branch Detection in gd

Since directory names have slashes converted to dashes, we can't derive the original branch name from the directory. Instead, `gd` asks git directly:

```bash
actual_branch=$($GIT rev-parse --abbrev-ref HEAD)
```

### Zsh Parameter Expansion Reference

| Syntax | Description | Example |
|--------|-------------|---------|
| `${var:t}` | Tail - filename portion (like `basename`) | `/a/b/c` → `c` |
| `${var:h}` | Head - directory portion (like `dirname`) | `/a/b/c` → `/a/b` |
| `${var%%pattern}` | Remove longest match from end | `foo--bar` → `foo` |
| `${var#pattern}` | Remove shortest match from start | `foo--bar` → `bar` |
| `${var//old/new}` | Replace all occurrences | `a/b/c` → `a-b-c` |

### Absolute Shebang

Use `#!/bin/zsh` instead of `#!/usr/bin/env zsh`:

```bash
# Bad - env might not find zsh
#!/usr/bin/env zsh

# Good - direct path always works
#!/bin/zsh
```

The `/usr/bin/env` approach can fail when PATH isn't properly set.

## Troubleshooting

### "command not found: git" in Functions

If you see this error with shell functions:
```
ga:16: command not found: git
```

**Cause:** Zsh caches command locations. The function was loaded before git was in PATH.

**Solutions:**
1. Use the script-based approach (recommended - this document)
2. Use `command git` instead of `git` in functions
3. Run `rehash` to clear zsh's command cache
4. Start a new terminal session

### Claude Code Shell Snapshots

Claude Code captures shell state in snapshot files:
```
~/.claude/shell-snapshots/snapshot-zsh-*.sh
```

These snapshots may contain old function definitions that override your updated `.zshrc`. The script-based approach avoids this because:
1. Scripts are external executables, not shell functions
2. The `unfunction` command clears old definitions
3. `$HOME/bin/ga` always calls the current script file

### Oh-My-Zsh Conflicts

The oh-my-zsh `git` plugin defines `ga`, `gd`, `gl` as aliases. Fix with:

```bash
# Option 1: Remove git plugin from plugins list
plugins=(
    # git  # removed
    zsh-autosuggestions
    z
)

# Option 2: Unalias before your definitions
unalias ga gd gl 2>/dev/null
```

### Scripts Not Found

If `ga` runs the old function instead of the script:

```bash
# Check what ga resolves to
type ga
# Should show: ga is a shell function from /Users/you/.zshrc

# If it shows the old function, reload
source ~/.zshrc

# Or start fresh terminal
```

### cd Fails After Worktree Creation (Slashes in Branch Names)

If you see this error:
```
Created worktree at ../myapp--feature-fix with branch feature/fix
ga:cd:4: no such file or directory: ../myapp--feature/fix
```

**Cause:** The wrapper function in `~/.zshrc` is missing the slash-to-dash conversion. The script creates the directory with dashes (`myapp--feature-fix`), but the wrapper tries to `cd` using the original branch name with slashes (`myapp--feature/fix`).

**The Bug:**
```bash
# Wrong - uses raw branch name
local path="../${base}--${branch}"
```

**The Fix:**
```bash
# Correct - converts slashes to dashes (must match script logic)
local path="../${base}--${branch//\//-}"
```

**Why this happens:** The `~/bin/ga` script and the `~/.zshrc` wrapper must calculate the **same path**. The script uses `${branch//\//-}` to create a flat directory structure. If the wrapper doesn't use the same substitution, it will look for a nested directory that doesn't exist.

**Verify your wrapper:**
```bash
grep -A 5 "^ga()" ~/.zshrc
# Line with "local path=" should contain: ${branch//\//-}
```

## Quick Reference

| Command | Description | Location |
|---------|-------------|----------|
| `ga <branch>` | Create worktree + branch, cd into it | Run from main repo |
| `gd` | Delete worktree + branch, cd to parent | Run from worktree |
| `gl` | List all worktrees | Any repo directory |

## Naming Convention

Directory format: `<repo>--<branch-with-dashes>`

Examples:
- `myapp` + `main` → stays as `myapp`
- `myapp` + `feature/fix` → creates `myapp--feature-fix`
- `myapp` + `bugfix` → creates `myapp--bugfix`

The `--` double-dash separator allows parsing:
- Everything before `--` = original repo name
- Everything after `--` = branch name (with slashes as dashes)

## Files Summary

| File | Purpose |
|------|---------|
| `~/bin/ga` | Script: create worktree (hardcoded `/opt/homebrew/bin/git`) |
| `~/bin/gd` | Script: delete worktree |
| `~/bin/gl` | Script: list worktrees |
| `~/.zshrc` | Wrappers: call scripts + handle `cd` |

## Lessons Learned

1. **Shell functions are fragile** - They depend on shell state at load time
2. **PATH issues are common** - Especially with tools like Claude Code that use shell snapshots
3. **Hardcode paths in scripts** - When reliability matters more than portability
4. **Scripts > Functions** - For commands that need to work across different shell contexts
5. **Zsh modifiers are powerful** - `${var:t}` beats external `basename` command
6. **Branch slashes need handling** - Convert to dashes for flat directory structure
