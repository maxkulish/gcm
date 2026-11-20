# Spec Review (Round 3): clo-945

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Reviewed spec sha256**: 1fdf4ba2d04b
**Date**: 2026-11-19

---

## 1. Problem Statement Assessment

The fourth revision matches the supplied Linear context. It identifies the three gates, pins Codex `gpt-6.2-sol` at high effort and Gemini `google-vertex/gemini-4-hydrogen#high`, and traces how CLI failures can become review artifacts. The remaining issues concern how the gates preserve failure evidence, publish reports, and hand results to Pi. I reviewed the current spec, scanned the repository Markdown and Rust sources, and traced the active callers and local lok source. Live provider behavior remains unverified in this review. [Current specification](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md)

## 2. Acceptance Criteria Review

**Strong**: AC6–AC8 define bounded diagnostics, prefix-based failure detection, and a post-validation usability decision. AC14 and AC23 distinguish unavailable reviews, synthesis failure, and input failure. AC17 requires two live reviews grounded in the actual spec.

**Gaps**:

- **A validator can turn a CLI failure into an apparent review.** The guard prints `REVIEW_FAILED:` and exits 0. A failure line with its stderr excerpt can exceed the 200-character minimum. If Haiku returns a structurally valid replacement with `status: pass`, lok exposes that replacement as successful output and AC8 loses the original failure. AC9 cannot recover it from the template context. Preserve the guard outcome independently and make it authoritative. [AC6–AC9](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:64) · [lok replacement logic](/Users/mk/Code/orchestrator/lok/src/workflow.rs:2093)
- **AC9 promises rollback that per-file renames cannot provide.** If the first rename succeeds and the second fails, the run exits non-zero with a mixture of old and new reports. AC14 and AC23 also intentionally publish current diagnostics before exiting non-zero. Define which failures preserve the previous checkpoint and what callers can trust after publication starts. [AC9](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:67) · [AC14](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:72) · [AC23](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:81)
- **The read-only pre-PR fallback lacks an explicit diff handoff.** AC23’s literal replacement matches the shared external-review prompt. The fallback has a separate instruction to run Git, while AC26 removes its shell access. Require every assembled pre-PR reviewer prompt to name the prepared diff and the applicable task documents. [Shared prompt](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/prompts/pre-pr-validation-prompt.md:3) · [Fallback prompt](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml:162)

## 3. Constraints Check

**Aligned**: POSIX shell, guarded references to skipped steps, and a dependent verdict step fit lok’s execution model. Keeping gcm’s Rust provider code outside the change is appropriate. [lok dependency handling](/Users/mk/Code/orchestrator/lok/src/workflow.rs:1642) · [lok shell runner](/Users/mk/Code/orchestrator/lok/src/workflow.rs:3410)

**Concerns**: AC26 restricts the fallback, but Haiku validation and synthesis still receive model-authored text through lok’s unrestricted Claude CLI launch. The current pre-PR synthesis also explicitly asks for repository grounding and a Git diff. Checking stdout happens after the model has had access to tools, and the repository’s Claude settings allow Git writes and Linear mutations. Specify and test tool restrictions for every Claude call that consumes reviewer output. [lok Claude launch](/Users/mk/Code/orchestrator/lok/src/backend/claude.rs:255) · [Pre-PR synthesis prompt](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml:203) · [Claude permissions](/Users/mk/Code/gcm--fix-clo-945-review-gates/.claude/settings.json:3)

## 4. Decomposition Quality

**Well-scoped**: The shared guard gives the three workflows and the inline gate a common dependency. The workflows can be wired independently once its input, output, and status contract is settled.

**Issues**: The pre-PR subtask combines input preparation, reviewer execution, fallback, synthesis, publication, and exit propagation. It is unlikely to fit two hours with failure-path verification. Split it into input and dispatch, then publication and caller behavior. Phase integration depends on verdict formats, fallback paths, and exit semantics as well as filenames. [Decomposition](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:117)

## 5. Evaluation Coverage

**Covered**: The table pairs stub failures with live grounding checks and correctly calls for asserting the outer lok exit code. It covers validator outage, a validator stripping the verdict, skipped fallback, and bad diff input.

**Gaps**: Test a failed CLI whose long diagnostic is replaced by a valid-looking validator response, a write failure after the first rename, and a pre-PR fallback that cites a known changed hunk. Make validator rejection deterministic with a stub. Exercise the actual Pi pre-PR decision with each reviewer missing in turn and with fallback as the sole review. A spec-to-implement transition using prepared YAML only checks state-schema compatibility. [Evaluation table](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:132)

## 6. Codebase Alignment

**Violations**: AC22 overstates Pi compatibility. Pi records spec and design report paths that differ from the workflow outputs. The extension accepts the state because those paths are optional. For pre-PR, the phase instructions check report contents and `codex_validated`, while the extension checks the presence of state fields and history. AC14’s reliance on Pi blocking a partial-review run therefore depends on the caller following the phase instructions. [Pi spec handoff](/Users/mk/Code/gcm--fix-clo-945-review-gates/.pi/orchestrator/phases/spec.md:122) · [Pi design handoff](/Users/mk/Code/gcm--fix-clo-945-review-gates/.pi/orchestrator/phases/design.md:248) · [Pi pre-PR checklist](/Users/mk/Code/gcm--fix-clo-945-review-gates/.pi/orchestrator/phases/implement.md:353) · [Transition validator](/Users/mk/Code/gcm--fix-clo-945-review-gates/.pi/extensions/orchestrate/index.ts:545)

**Alignment**: gcm uses a synchronous `Provider` trait with typed `ProviderError`. lok owns the asynchronous `Backend` trait and `BackendErrorKind`. The proposed shell steps sit at lok’s process boundary, where exit status and diagnostics are the relevant contract. No Rust trait change is needed. [gcm Provider](/Users/mk/Code/gcm--fix-clo-945-review-gates/src/provider/facade.rs:43) · [lok Backend](/Users/mk/Code/orchestrator/lok/src/backend/mod.rs:318) · [lok error classification](/Users/mk/Code/orchestrator/lok/src/utils.rs:51)

## 7. Blind Spots

- Concurrent runs for the same slug share stderr, input-failure state, and the diff path. A second run can remove or overwrite inputs the first still needs. Specify per-invocation storage or serialization.
- AC25 accepts model-generated `NO_REVIEWS_AVAILABLE` even when AC14 counts usable reviews. Reserve that verdict for the deterministic zero-review path.
- AC13’s 60-second minimum probe exceeds the current 30-second health-step deadlines. Specify an outer budget that also covers input preparation, and preserve the probe’s exit status and diagnostic when design review short-circuits. [Status and timeout criteria](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:64) · [Synthesis criteria](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:83) · [Current health-step budget](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml:14)

## 8. Verdict

NEEDS_REVISION

## 9. Actionable Feedback

1. Make the original guard outcome authoritative and test that validator replacement cannot promote a failed CLI.
2. Define run isolation, publication guarantees, and which artifacts a caller may use after each non-zero exit. Test failure after publication begins.
3. Give the pre-PR fallback the same prepared inputs as the external reviewers. Enforce tool restrictions on Claude validation and synthesis, then test the assembled prompts and actual launch paths.
4. Resolve the Pi compatibility claim. If `.pi/` stays out of scope, state the narrower guarantee and prove where partial-review runs block. Cover both development and specification tasks.
5. Reserve the zero-review verdict for the writer, specify the health-step deadline and diagnostic handoff, and split the pre-PR work around its input and publication contracts.


