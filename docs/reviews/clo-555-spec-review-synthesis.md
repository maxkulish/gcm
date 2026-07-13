# Spec Review Synthesis: clo-555

**Synthesized**: 2026-07-12
**Pipeline**: lok spec-review

---

Both external reviewers succeeded (Gemini and Ollama); Claude fallback was skipped, so its position is marked N/A below. Both reviewers independently reached **APPROVE_WITH_SUGGESTIONS**.

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | **SIGINT/Ctrl-C mid-run breaks the transaction guarantee**: interrupting during phase B or C leaves the working tree mutated (zdiff3/mergiraf output) without triggering the byte-exact snapshot restore; the spec's test matrix and safety guarantees don't address it | High |
| 2 | **`--yes` semantics underspecified**: the spec doesn't explicitly state that `--yes` auto-confirms all non-escalated files (Gemini), nor how `--yes` interacts with `--no-finish` or with escalation on the first file before any confirmation (Ollama) | Medium |
| 3 | **Codebase alignment is solid** - both reviewers found zero pattern violations: snapshot/restore, `GIT_LITERAL_PATHSPECS=1`, `commit_signed` inheritance, JSON envelope with `skip_serializing_if`, and `GcmError` conventions all correctly reused | Positive - no action |
| 4 | **Problem statement and AC measurability are strong** - file:line evidence matches the Linear task; AC1-AC12 are objective and postcondition-driven | Positive - no action |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| 1 | ST4 (three-phase transaction) sizing | Decomposition has "no major issues"; ST1-ST8 sized perfectly for 1-2 hour windows | ST4 is significantly larger than the others (~200 lines of restructured core flow); split into ST4a (proposal collection), ST4b (abort/restore), ST4c (central staging) | N/A (skipped) |
| 2 | Remedy for the SIGINT gap | Document it as a known limitation in README safety guarantees | Go further: consider a SIGINT handler that reports "staged but not finished" state, plus an mtime check before restore | N/A (skipped) |

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | Spec omits mandating `leaves_staged()` in `src/error.rs` return `true` for the new `GcmError::FinishFailed`; without it, standard error paths could destructively clean the index, violating AC7 | Gemini | High |
| 2 | Internal contradiction: constraint says "no change to `--dry-run` behavior" but ST4 says "dead dry-run arm removed" - needs reconciliation | Ollama | High |
| 3 | AC3 makes Enter/EOF abort in the `gcm commit` message confirmation prompt - a breaking behavioral change for existing users (Enter previously accepted); must be prominently flagged in README/release notes | Gemini | Medium |
| 4 | Concurrent edits in another terminal during phase B would be silently overwritten by the abort restore; spec should add an mtime/hash check or document the limitation | Ollama | Medium |
| 5 | Missing test for the "no operation ref" case (`git checkout -m` style conflict with no MERGE_HEAD/REBASE_HEAD/CHERRY_PICK_HEAD) verifying `NothingToFinish` | Ollama | Medium |
| 6 | Validation-retry and provider-timeout behavior is undefined inside the three-phase structure - does retry happen in phase A, or does failure escalate immediately? | Ollama | Medium |
| 7 | No explicit sub-task covers the `FileAction::Rejected` enum variant or the interactive `--json` stderr preview (AC9/AC12) | Ollama | Medium |
| 8 | GPG pinentry failures in non-interactive contexts will fail the forced-sign continue; manual-continue output must be clearly surfaced for recovery | Gemini | Low |
| 9 | Older Git versions may not support `-c commit.gpgsign=true` combined with `rebase --continue`/`cherry-pick --continue`; verify across versions | Gemini | Low |
| 10 | AC9 doesn't specify `restored` field serialization when status is not `aborted` (false vs omitted) | Ollama | Low |
| 11 | AC11 references README line numbers, which drift - use section anchors instead | Ollama | Low |
| 12 | `scripts/acceptance.sh probe_signing` should be verified to exist or created in ST8; remote scratch-repo cleanup on abort is unspecified | Ollama | Low |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** (Gemini: APPROVE_WITH_SUGGESTIONS, Ollama: APPROVE_WITH_SUGGESTIONS)

## Priority Actions

1. **Document (or handle) SIGINT mid-run** [Agreement, High] - add a known-limitation note to the spec constraints and README safety guarantees stating that Ctrl-C during phases B/C bypasses snapshot restore; decide whether a signal handler is in scope (Disagreement #2).
2. **Mandate the `leaves_staged()` update for `FinishFailed`** [Gemini, High] - add it explicitly to ST3 so error paths preserve the staged index per AC7.
3. **Resolve the `--dry-run` contradiction** [Ollama, High] - reconcile the "no change to `--dry-run`" constraint with ST4's "dead dry-run arm removed" wording.
4. **Clarify `--yes` semantics** [Agreement, Medium] - spell out auto-confirmation of non-escalated files, the `--yes --no-finish` combination, and first-file escalation before any confirmation.
5. **Flag the `gcm commit` Enter-aborts breaking change** [Gemini, Medium] - add prominent warnings to ST8 README updates and release notes.
6. **Decide on ST4 split** [Disagreement #1, Medium] - human call on whether to break ST4 into ST4a/b/c or keep it whole.
7. **Add the `NothingToFinish` no-operation-ref test** [Ollama, Medium] and specify validation-retry/provider-timeout behavior within phase A.
8. **Sweep the low-severity items** [Low] - `FileAction::Rejected` sub-task placement, `restored` field serialization rule, README section anchors instead of line numbers, `probe_signing` script existence, concurrent-modification limitation note, remote scratch cleanup on abort.
