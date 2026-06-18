# GitHub Multi-Account Management with `gh` CLI

Managing multiple GitHub accounts (personal + work) using the GitHub CLI (`gh`) for seamless authentication across repositories.

## The Problem

When you have multiple GitHub accounts:

| Account | Use Case | Example |
|---------|----------|---------|
| Personal | Side projects, open source | `maxkulish` |
| Work | Company repositories | `admincloud-tech` |

Cloning fails when the wrong account is active:

```bash
$ git clone https://github.com/maxkulish/leadership.git
remote: Repository not found.
fatal: repository 'https://github.com/maxkulish/leadership.git/' not found
```

**Root cause:** Git's credential helper uses whichever `gh` account is currently active.

## How It Works

```
┌─────────────────────────────────────────────────────────────┐
│  git clone https://github.com/user/repo.git                 │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  Git needs credentials for github.com                       │
│  Calls: git credential-helper (configured in ~/.gitconfig)  │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  gh auth git-credential                                     │
│  Returns token for ACTIVE account only                      │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  GitHub validates token against repository                  │
│  ✗ Wrong account = "Repository not found"                   │
│  ✓ Correct account = Clone succeeds                         │
└─────────────────────────────────────────────────────────────┘
```

## Setup: Multiple Accounts

### 1. Login to Both Accounts

```bash
# Login to first account (will open browser)
gh auth login

# Login to second account (adds to existing)
gh auth login
```

During login, select:
- **GitHub.com** (not Enterprise)
- **HTTPS** protocol
- **Login with a web browser**

### 2. Verify Both Accounts

```bash
gh auth status
```

Expected output:

```
github.com
  ✓ Logged in to github.com account admincloud-tech (keyring)
  - Active account: true
  - Git operations protocol: https
  - Token: gho_************************************
  - Token scopes: 'gist', 'read:org', 'repo'

  ✓ Logged in to github.com account maxkulish (keyring)
  - Active account: false
  - Git operations protocol: https
  - Token: gho_************************************
  - Token scopes: 'gist', 'read:org', 'repo', 'workflow'
```

**Key insight:** Only ONE account can be active at a time. The active account is used for all `gh` commands and git credential requests.

## Daily Usage

### Switching Accounts

```bash
# Switch to personal account
gh auth switch -u maxkulish

# Switch to work account
gh auth switch -u admincloud-tech
```

### Quick Status Check

```bash
# See which account is active
gh auth status

# One-liner to show just the active account
gh auth status 2>&1 | grep "Active account: true" -B 1 | head -1
```

### Verify Repository Access

Before cloning, verify you can access the repo:

```bash
# Check if current account can see the repo
gh repo view owner/repo --json name,visibility

# List your accessible repos
gh repo list --limit 10
```

## Workflow Examples

### Clone Personal Repository

```bash
# 1. Switch to personal account
gh auth switch -u maxkulish

# 2. Clone
git clone https://github.com/maxkulish/my-project.git
```

### Clone Work Repository

```bash
# 1. Switch to work account
gh auth switch -u admincloud-tech

# 2. Clone
git clone https://github.com/company/internal-tool.git
```

### Create PR with Correct Account

```bash
# Ensure you're using the right account for the repo owner
gh auth switch -u maxkulish

# Now create PR (will use maxkulish's token)
gh pr create --title "Fix bug" --body "Description"
```

## Alternative: SSH with Host Aliases

For repos you access frequently, SSH aliases bypass the `gh` active account limitation entirely.

### SSH Config Setup

Edit `~/.ssh/config`:

```ssh-config
# Personal GitHub account
Host github.com-personal
    HostName github.com
    User git
    IdentityFile ~/.ssh/id_ed25519_personal
    IdentitiesOnly yes

# Work GitHub account
Host github.com-work
    HostName github.com
    User git
    IdentityFile ~/.ssh/id_ed25519_work
    IdentitiesOnly yes
```

### Using SSH Aliases

```bash
# Clone personal repo
git clone git@github.com-personal:maxkulish/leadership.git

# Clone work repo
git clone git@github.com-work:admincloud-tech/project.git
```

### Convert Existing Repo Remote

```bash
# Check current remote
git remote -v

# Change to SSH alias
git remote set-url origin git@github.com-personal:maxkulish/repo.git
```

## Comparison: HTTPS vs SSH

| Aspect | HTTPS + `gh auth switch` | SSH Aliases |
|--------|--------------------------|-------------|
| Setup complexity | Simple (just login) | Requires SSH key per account |
| Switching | Manual `gh auth switch` | Automatic by host alias |
| Clone URL | Standard GitHub URLs | Custom host in URL |
| Token management | Handled by `gh` | SSH keys in `~/.ssh` |
| Best for | Occasional switching | Frequent multi-account use |

## Troubleshooting

### "Repository not found" on Clone

```bash
# 1. Check which account is active
gh auth status

# 2. Verify you're using the account that owns/has access to the repo
gh repo view owner/repo

# 3. If wrong account, switch
gh auth switch -u correct-username
```

### Token Scopes Missing

If operations fail due to permissions:

```bash
# Re-authenticate with additional scopes
gh auth refresh -s repo,workflow,read:org

# Or logout and login fresh
gh auth logout -u username
gh auth login
```

### Credential Helper Not Set

If git doesn't use `gh` for authentication:

```bash
# Check current helper
git config --global credential.helper

# Set gh as credential helper
gh auth setup-git
```

This adds to `~/.gitconfig`:

```ini
[credential "https://github.com"]
    helper =
    helper = !/opt/homebrew/bin/gh auth git-credential
```

### Keyring Issues (macOS)

If tokens aren't persisting:

```bash
# Check where tokens are stored
gh auth status

# If "keyring" shows issues, can use file-based storage
gh config set -h github.com git_protocol https
```

## Quick Reference Card

```bash
# === Account Management ===
gh auth login                    # Add new account
gh auth logout -u USERNAME       # Remove account
gh auth status                   # Show all accounts + active
gh auth switch -u USERNAME       # Switch active account
gh auth refresh -s SCOPES        # Add token scopes

# === Verification ===
gh repo view OWNER/REPO          # Test repo access
gh api user --jq .login          # Show current user
gh repo list --limit 5           # List accessible repos

# === Git Integration ===
gh auth setup-git                # Configure git credential helper
git config --global credential.helper  # Check current helper

# === SSH Alternative ===
git clone git@github.com-ALIAS:owner/repo.git
git remote set-url origin git@github.com-ALIAS:owner/repo.git
```

## Best Practices

1. **Default to work account** - Most daily work is likely work-related
2. **Switch before cloning** - Verify account before starting work on a repo
3. **Use SSH for frequent repos** - Avoids switching for repos you access often
4. **Check before PR/push** - Wrong account = wrong author attribution

## Token Storage

The `gh` CLI stores tokens securely:

| Platform | Storage Location |
|----------|-----------------|
| macOS | Keychain (`security` command) |
| Linux | Secret Service API or encrypted file |
| Windows | Windows Credential Manager |

View token (for debugging):

```bash
# Shows masked token
gh auth status

# Get actual token (careful - sensitive!)
gh auth token
```

## Related Configuration Files

| File | Purpose |
|------|---------|
| `~/.gitconfig` | Git credential helper configuration |
| `~/.config/gh/hosts.yml` | gh CLI account metadata |
| `~/.ssh/config` | SSH host aliases (if using SSH method) |
| System keychain | Actual token storage |
