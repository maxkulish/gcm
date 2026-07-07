# PRD: `gcm resolve` — LLM-assisted merge conflict resolver (Phase 1: local markers)

| Field | Value |
|---|---|
| Author | Max Kulish |
| Status | Draft |
| Created | 2026-07-06 |
| Linear | [CLO-531](https://linear.app/cloud-ai/issue/CLO-531/add-gcm-resolve-llm-assisted-merge-conflict-resolver-phase-1-local) |
| Branch | `feat/clo-531-resolve` |
| Labels | HITL, Feature |
| Depends on | CLO-489 (Provider trait), CLO-487 (structured output), CLO-496/516 (config), CLO-490/514 (secret-scan + `.gcmignore`) |

## 1. Overview

Add a `gcm resolve` subcommand that resolves in-progress git merge/rebase/cherry-pick conflicts using the existing `Provider` layer. The goal is to turn a conflicted working tree into a reviewed, validated resolution without hand-editing every `<<<<<<<` marker. **This task is Phase 1: the local conflict-marker engine.** Remote MR/PR orchestration is intentionally Phase 2 and out of scope.

The command follows a layered reliability pipeline: deterministic stages shrink the problem before the LLM sees it; the LLM is the last resort, not the first mover.

## 2. Problem & Objectives

### Problem

When a merge or rebase produces conflicts, the user today must:

1. Manually inspect every file containing `<<<<<<<` markers.
2. Decide keep-left / keep-right / combine for each hunk.
3. Ensure the result is syntactically valid and preserves both branches' intent.
4. Stage the resolutions and run `git commit` / `git rebase --continue`.

For multi-file conflicts this is tedious and error-prone; for non-trivial code conflicts it is easy to lose semantics or introduce syntax errors.

### Objectives

- **O1:** Automate the mechanical parts of conflict resolution (trivial keep-left/right/both hunks, optional structural pre-resolution via `mergiraf`).
- **O2:** Use the existing `Provider` layer to resolve only the hard hunks, feeding the model `base`/`ours`/`theirs` at the smallest syntactic unit plus local style context.
- **O3:** Keep the human in the loop: preview every proposed resolution with per-file `[Y/n/e]` and never auto-stage or `--continue`.
- **O4:** Validate resolutions before writing: syntax-safe default; optional user-configured `validate_cmd`; exactly one bounded LLM retry on validation failure, then escalate to human.
- **O5:** Honor all existing gcm safety/privacy flags: `--dry-run`, `--json`, `--yes`, `--secret-scan`, and `.gcmignore` matching.

## 3. Scope (Phase 1)

| # | Requirement |
|---|---|
| S1 | New `gcm resolve` clap subcommand. Detect an in-progress merge/rebase/cherry-pick. Enumerate conflicted files with `git diff --name-only --diff-filter=U`. |
| S2 | Re-checkout conflicted files as `zdiff3` markers; parse each hunk into `base`, `ours`, `theirs`. |
| S3 | Optional `mergiraf` pre-stage: detect on `PATH`, run per file, forward only unresolved hunks. Gracefully skip when absent. |
| S4 | Provider integration: reuse the `Provider` trait + structured output; new resolution prompt with anti-hallucination rules. Reuse retry/backoff, `strip_think`, defensive parse. |
| S5 | Validation gate: syntax-safe default; optional `conflict.validate_cmd`; one bounded LLM retry on failure; escalate otherwise. |
| S6 | New `[conflict]` config block + CLI flags: `--conflict-temperature`, `--conflict-validate-cmd`, `--conflict-auto-policy`, `--conflict-sensitive-paths`, `--no-mergiraf`. Layered precedence: flag > env > config > default. |
| S7 | Preview + per-file `[Y/n/e]` confirm loop. Write resolved files only on accept. Never `git add` or `--continue` automatically. |
| S8 | Honor `--dry-run` (preview, no write), `--json` (machine envelope), `--yes` (non-interactive), `.gcmignore` matcher, `--secret-scan` for provider egress. |
| S9 | Docs: README section, ADR note if the pipeline touches ADR-001 decisions, discovery doc under `docs/discovery/`. |

### Out of scope (Phase 2)

- Fetching a GitHub PR / GitLab MR branch, running the merge, invoking this core, pushing to a resolution branch.
- RAG-over-git-history exemplars.

## 4. Functional Requirements

| ID | Requirement | Priority | Acceptance Criteria |
|---|---|---|---|
| FR-61 | Detect in-progress conflict state | Must | `gcm resolve` aborts with a clear error when no merge/rebase/cherry-pick is in progress or when no unmerged files exist. |
| FR-62 | Enumerate conflicted files | Must | Uses `git diff --name-only --diff-filter=U` and respects `.gcmignore` / `--secret-scan` for filtering. |
| FR-63 | Parse `zdiff3` markers | Must | Each conflict hunk is split into labeled `base`, `ours`, `theirs` text. |
| FR-64 | Optional `mergiraf` pre-resolution | Should | If `mergiraf` is on `PATH`, run it per file; unresolved hunks continue to the LLM stage; absence is a no-op. |
| FR-65 | LLM resolution of hard hunks | Must | Hard hunks are sent to the configured provider with a 3-way, function-granularity prompt and anti-hallucination instructions. |
| FR-66 | Validation gate | Must | Default syntax-safe check; optional `conflict.validate_cmd` triggers exactly one bounded LLM retry on failure, then escalates (file left conflicted). |
| FR-67 | Interactive preview | Must | Per-file `[Y/n/e]` loop; `n` skips write; `e` opens `$EDITOR` on the proposed resolution. |
| FR-68 | Non-interactive mode | Should | `--yes` accepts all proposed resolutions without prompting. `--dry-run` previews and writes nothing. |
| FR-69 | Machine envelope | Should | `--json` emits a versioned `ResolveReport` envelope on stdout. |
| FR-70 | Config block | Must | `[conflict]` TOML section supports `temperature`, `validate_cmd`, `sensitive_paths`, `auto_policy`, `mergiraf` on/off. |
| FR-71 | Safety invariants | Must | Never auto `git add`; never auto `--continue`; unparseable LLM output never lands in the working tree; `.gcmignore` and `--secret-scan` apply to provider-bound text. |

## 5. Config vs Defaults

| Setting | Config key | Default | Rationale |
|---|---|---|---|
| Provider / model | reuse existing | current provider | already configured |
| `conflict.temperature` | `temperature` | `0.1` | reproducibility; high temp increases hallucination |
| `conflict.validate_cmd` | `validate_cmd` | none (syntax-only) | gcm is language-agnostic |
| `conflict.sensitive_paths` | `sensitive_paths` (globs) | none | force manual review; reuse `.gcmignore` matcher |
| `conflict.auto_policy` | `auto_policy` | `trivial` auto, rest preview | conservative |
| mergiraf pre-stage | `mergiraf` | on if `mergiraf` present | optional external tool |
| preview-before-write | fixed | always | non-negotiable safety |
| auto `--continue` | fixed | never | non-negotiable safety |
| unparseable LLM output | fixed | escalate to human | non-negotiable safety |

## 6. Data Model / JSON Envelope

The `--json` output emits a versioned envelope:

```json
{
  "v": 1,
  "status": "resolved" | "noop" | "error" | "partial",
  "files": [
    {
      "path": "src/lib.rs",
      "hunks_total": 3,
      "hunks_auto": 1,
      "hunks_llm": 1,
      "hunks_escalated": 1,
      "action": "accepted" | "skipped" | "edited" | "escalated" | "dry_run"
    }
  ]
}
```

Human-oriented prose goes to stderr; stdout contains a single JSON object.

## 7. Acceptance Criteria (mirrors Linear)

- `gcm resolve` on a repo with an in-progress merge lists conflicts and produces per-file resolutions gated by `[Y/n/e]`; nothing is written on `n` and nothing is staged/continued automatically.
- `--dry-run` previews resolutions and writes nothing; `--json` emits a versioned machine envelope; `--yes` runs non-interactively.
- With `mergiraf` on `PATH`, trivially-mergeable hunks are resolved without an LLM call; with it absent, the command still works (pure-LLM path).
- With `conflict.validate_cmd` set, a resolution that fails validation triggers exactly one bounded LLM retry, then escalates (file left conflicted) rather than writing a broken resolution.
- Syntactically invalid or unparseable model output never lands in the working tree.
- `.gcmignore` and `--secret-scan` apply to the code sent to the provider.
- `fmt` + `clippy` clean; unit + acceptance tests cover marker parsing, mergiraf-present/absent paths, validation retry/escalation, and the confirm loop.
