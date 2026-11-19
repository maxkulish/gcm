# Spec: Document the `message` field of the single-commit preview in the README `--json` section

**Created**: 2026-11-19
**Linear**: [CLO-943](https://linear.app/cloud-ai/issue/CLO-943)
**Estimated scope**: S (1 file, 1 sub-task)

## 1. Problem Statement

CLO-940 (PR #82) added an optional top-level `message` field to the `--json` envelope. The field holds the generated commit message of a single-commit preview. The README does not name it, so a script author who reads only the README cannot know that the field exists or when to expect it.

The README section "Machine-readable mode (`--json`)" (`README.md:272-284`) names `v`, `status` and `mode`, and gives one `jq` example (`README.md:283`). It says nothing about `message`.

The behavior to document is fixed by the shipped code and its acceptance cases:

- `Envelope.message` is `Option<String>` with `skip_serializing_if = "Option::is_none"` (`src/output.rs:52-55`). Only the builder `output::single_preview` sets it (`src/output.rs:138`).
- `single_preview` has one call site, `src/main.rs:983`. It runs after `provider.generate_message` succeeds on the single-commit path, when `--dry-run` or `--plan-only` is set (`src/main.rs:970-984`).
- The envelope that carries `message` always has `status: "plan"` and `mode: "single"`.

Presence table, from the CLO-940 spec (`docs/specs/2026-11-17-clo-940-plan-only-all-preview.md`, AC-2 to AC-6) and `scripts/acceptance.sh`:

| Run | `message` present | Acceptance case |
|---|---|---|
| `--all --plan-only --json` | yes | AC-493-10 |
| `--all --dry-run --json` | yes | CLO-940-3 |
| `--dry-run --json` without `--plan-only` during a clean merge | yes | CLO-940-4 |
| `--dry-run --json` without `--plan-only` when grouping falls back to a single commit | yes | CLO-940-5 |
| `--plan-only --json` without `--all` during a clean merge, also with `--dry-run` (zero provider requests) | no | CLO-940-4 |
| `--plan-only --json` without `--all` when grouping falls back to a single commit, also with `--dry-run` (no message request; in the CLO-940-5 fixture only the grouping request runs) | no | CLO-940-5 |
| grouped preview (`mode: "plan_only"` or `"dry_run"`) | no | CLO-940-6 |
| `committed`, `fallback`, `noop`, `error` envelopes | no | CLO-940 AC-4; CLO-940-7 (error) |

The table lists successful previews. If message generation fails, the run returns an `error` envelope without `message`; a clean tree returns a `noop` envelope without `message`.

Flag precedence: the CLI accepts `--dry-run --plan-only` together. Without `--all`, `--plan-only` wins on the clean-merge and fallback paths: `single_commit_path` returns the `plan` envelope before it checks `--dry-run` (`src/main.rs:917`), so no message is generated. CLO-940-4 and CLO-940-5 test each flag separately; the combined-flag behavior was confirmed by mock runs during the Codex validation (`docs/reviews/clo-943-codex-validation.md`).

Grouping falls back to a single commit through `run_fallback` on four triggers: an invalid cached plan (`src/main.rs:494-511`), a prompt-budget refusal (`src/main.rs:621`), a provider error classed as fallback (`src/main.rs:689-695`), and a plan that fails validation (`src/main.rs:699`). The README names the case as "when grouping falls back to a single commit" and does not list the triggers. The provider request count on the fallback depends on the trigger: an invalid cached plan and a prompt-budget refusal fall back before any grouping request. The README does not state a request count.

A real commit's message is a different field: `commit.message` inside `CommitSummary` (`src/output.rs:69-74`). Scripts can confuse the two names.

## 2. Acceptance Criteria

- [ ] AC-1: The "Machine-readable mode (`--json`)" section names the top-level `message` field and says that it holds the generated commit message of a single-commit preview (`status: "plan"`, `mode: "single"`).
- [ ] AC-2: The section lists the successful previews that carry `message`: `--plan-only --all`, `--dry-run --all`, and `--dry-run` without `--plan-only` during a clean merge or when grouping falls back to a single commit. It says that without `--all`, `--plan-only` takes precedence over `--dry-run`.
- [ ] AC-3: The section says that `message` is absent when no message was generated: `--plan-only` without `--all` (with or without `--dry-run`) during a clean merge or when grouping falls back to a single commit, grouped previews, and every non-`plan` envelope, including `noop` and `error`. The section does not state a provider request count for the fallback.
- [ ] AC-4: The section says that the message of a real commit is under `commit.message`, not the top-level `message`.
- [ ] AC-5: The section has one new `jq` example that reads `.message`, and that example is valid against the envelope that AC-493-10 produces.
- [ ] AC-6: The existing text, the existing "Example:" label and its `jq` example stay unchanged. The new example has its own lead-in sentence. No other README section changes.

**Verification method**: read the diff of `README.md` against the presence table above; run the new `jq` example against the saved envelope in section 5, test 5; run `scripts/acceptance.sh` on the base commit and on the README revision with the same binary and environment, and confirm that the revision adds no new failure.

## 3. Constraints

**Must**:
- Match the presence table in section 1 exactly. The code and `scripts/acceptance.sh` are the source of truth, not the ticket wording.
- Keep the change inside `README.md`, section "Machine-readable mode (`--json`)".
- Use a regular hyphen, never an em dash, and no contractions in the README text.

**Must-not**:
- Change any code, test or acceptance case.
- Promise `message` on runs not in the table, or describe its content beyond "the generated commit message".
- Bump or mention a schema version change: the field is additive and `v` stays 1.

**Prefer**:
- Keep the addition short: one paragraph plus one example, in the style of the existing text.
- Make the `jq` example read the message with `jq -er '.message // empty'`. On a missing field it prints nothing and exits non-zero (4). Plain `jq -er '.message'` prints the string `null` and exits 1, so a script without `set -e` stores `"null"`.

**Escalate when**:
- The shipped behavior differs from the presence table (for example, an acceptance case fails or a new `single_preview` call site exists).

## 4. Decomposition

1. **Extend the README `--json` section**: after the existing example block (`README.md:280-284`), add a paragraph that states AC-1 to AC-4, then a lead-in sentence and one `jq` example that reads `.message` (AC-5). Files: `README.md`.

**Dependency order**: one sub-task.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | The section names `message` and its envelope (`status: "plan"`, `mode: "single"`) | Text present | Read `README.md` section diff |
| 2 | The four runs that carry `message` are listed, with `--dry-run` qualified as "without `--plan-only`" and the precedence stated | All four named, precedence stated | Compare with section 1 table |
| 3 | The absent cases are stated | `--plan-only` merge/fallback, grouped previews and non-`plan` envelopes named | Compare with section 1 table |
| 4 | `commit.message` is distinguished from `message` | One clause present | Read diff |
| 5 | The new `jq` example works | Prints `chore(test): mock commit message`, exit 0 | `printf '%s' '{"v":1,"status":"plan","mode":"single","plan":{"groups":[]},"changed_files":["a.txt"],"cached":false,"message":"chore(test): mock commit message"}'` piped to the example's `jq` filter (envelope shape from AC-493-10) |
| 6 | No other change | `git diff --stat` shows only `README.md` (plus workflow docs) and the section's old lines are intact | `git diff main -- README.md` |
| 7 | No regressions | The failure set on the README revision equals the failure set on the base commit (`b724366`). Known environment-dependent failures, such as AC-488-1 (CLO-944) and AC-493-7 (`OnboardingRequired` instead of `Provider` in an unconfigured environment), may appear in both runs | Run `scripts/acceptance.sh` twice with the same binary and environment, on the base and on the revision, and compare the `bad` lines |

**Edge cases to verify**:
- The new `jq` example exits non-zero with empty stdout on an envelope without `message`, such as a grouped `--plan-only --json` envelope.
- `--dry-run --plan-only --json` without `--all` during a clean merge returns the `plan` envelope without `message`. The README text must not imply that `--dry-run` alone decides.
- A clean tree with `--plan-only --all` returns the `noop` envelope and no `message` (CLO-940 spec, edge cases). The README text must not imply otherwise.
