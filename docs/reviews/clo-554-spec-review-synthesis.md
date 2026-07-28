# Spec Review Synthesis: clo-554

**Synthesized**: 2026-07-28
**Pipeline**: lok spec-review

---

## Source Status

| Reviewer | Result |
|---|---|
| Gemini | ✅ Valid |
| Ollama (Codex / glm-5:cloud) | ❌ REVIEW_FAILED — empty output, process emitted only startup banner |
| Claude fallback | ⏭️ Skipped (external reviewer succeeded) |

Only one reviewer produced usable output, so there is no cross-reference. The sections below are synthesized from Gemini alone; confidence is correspondingly single-source.

## Agreement (High Confidence)

Not applicable — a single valid review. No finding was independently corroborated.

## Disagreement (Needs Human Decision)

None surfaced. With Ollama failing and the Claude fallback skipped, no contradicting positions exist to adjudicate.

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | `--dry-run` and `--no-finish` single-round behavior lives only in Constraints/Edge-cases prose, never pinned as an acceptance criterion | Gemini | Medium |
| 2 | No-progress termination (exit code, `loop.terminal == "no_progress"`, warning diagnostic) is described but not an AC | Gemini | Medium |
| 3 | Evaluation table has no case forcing the no-progress guard to fire | Gemini | Medium |
| 4 | Evaluation table has no case asserting `--no-finish` / `--dry-run` run exactly one round and omit the `loop` object | Gemini | Medium |
| 5 | Non-TTY runs without `--yes` / `--no-input` should be documented as rejected by `needs_terminal_but_absent` before round 1, not at the gate | Gemini | Low |
| 6 | Blind spot: the user can mutate the repo from another terminal while parked at the `[y/N]` gate — head SHA must be re-read immediately after the gate is accepted, not carried over from before it | Gemini | Low–Medium |
| 7 | Blind spot: SSH/GPG signing may trigger pinentry or a hardware-token touch on every `git rebase --continue`, once per round — an operational characteristic worth documenting | Gemini | Low |

Positive findings (no action required): problem statement accurate and aligned with the Linear issue; loop driver in `run_resolve` with `run_resolve_in_repo` kept single-round is correct separation and leaves remote resolution untouched; keying continuation on `FinishResult::StoppedOnConflict` makes rebase and cherry-pick loop transparently; mirroring the `max_rounds` default in both serde and the hand-written `Default` closes the known config-drift gap; the 7 sub-tasks are correctly ordered (1–4 parallel, 5 depends on 1–4, 6 on 3+5, 7 on 5+6) and each under ~2h; reuse of `parse_choice` / `PROMPT_ATTEMPTS` and the `GcmError::Config` / `GcmError::FinishFailed` mapping match existing codebase patterns. Gemini reported zero constraint violations, zero decomposition issues, and zero codebase-alignment violations.

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

The single valid reviewer returned `APPROVE_WITH_SUGGESTIONS`. Every finding is a documentation-completeness gap — behavior that the spec already defines in prose but does not pin as testable criteria. No design flaw, no constraint violation, no rework of the decomposition. The spec is implementable as written; the suggestions harden its verifiability.

Caveat: with two of three reviewers unavailable, this verdict rests on one model's pass. If the spec touches cost-sensitive LLM spend logic (it does — that is the core risk it mitigates), consider a second opinion before treating "zero violations" as settled.

## Priority Actions

1. **Add AC9 / AC10 for `--dry-run` and `--no-finish`** — state that each executes exactly one round, performs no looping, and omits the `loop` block from the JSON report. Currently only implied by Sections 3 and 5.
2. **Promote the no-progress guard to an acceptance criterion** — pin exit 0, `loop.terminal == "no_progress"`, and a clear warning diagnostic on stderr.
3. **Add two evaluation cases** — one asserting `--no-finish` runs a single round without looping, and one that forces no-progress (a finish returning `StoppedOnNextConflict` without advancing `REBASE_HEAD`) and asserts the loop terminates as `no_progress`.
4. **Re-read the head SHA immediately after the round gate is accepted**, not before the prompt. The existing no-progress and external-edit checks will catch out-of-band changes, but only if the post-gate read is the one that counts. Worth an explicit note in the loop-driver sub-task.
5. **Add a constraint clarifying non-TTY handling** — a run without `--yes` or `--no-input` in a non-TTY is rejected by `needs_terminal_but_absent` before round 1 begins, so the gate never faces an unanswerable prompt.
6. **Document the signing-prompt characteristic** — with SSH/GPG signing enabled, each round's `git rebase --continue` may require a passphrase or token touch. Expected, but users running long interactive loops should know.

If you want the failed reviewer covered, re-running the Ollama pass is worthwhile — its failure was a harness-level empty-output condition (Codex printed only its banner), not a substantive abstention.
