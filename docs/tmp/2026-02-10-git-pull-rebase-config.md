# Git Pull Strategy — Global Config

## Date
2026-02-10

## What Changed
Set global Git config to use **rebase + auto-stash** as the default pull strategy.

```bash
git config --global pull.rebase true
git config --global rebase.autoStash true
```

## Why
- **Linear history**: rebase replays local commits on top of remote, avoiding merge commit noise
- **Auto-stash**: dirty working tree is automatically stashed before rebase and re-applied after — no need to manually stash/pop
- **Safe by default**: only rebases local unpushed commits, so no risk of rewriting shared history

## Config Levels Available
| Level | Flag | File | Scope |
|---|---|---|---|
| System | `--system` | `/etc/gitconfig` | All users on machine |
| User | `--global` | `~/.gitconfig` | All repos for this user |
| Project | `--local` | `.git/config` | Single repo only |

Lower levels override higher ones. Project-level `--local` can override the global default per-repo if needed.

## Override Per-Repo
If a specific repo needs merge commits (e.g., open-source contribution guidelines):

```bash
cd /path/to/repo
git config --local pull.rebase false
```

## Conflict Recovery
If a rebase conflict occurs during pull:
- `git rebase --abort` — cancel and return to pre-pull state
- `git rebase --continue` — after resolving conflicts, continue the rebase
