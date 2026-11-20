# Spec Review: clo-945

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Reviewed**: 2026-11-19
**Pipeline**: manual run of the fixed codex exec command (lok spec-review not used, it is the subject of this task)

---

## 1. Problem Statement Assessment

The model and CLI changes match the supplied CLO-945 context. The incident is concrete: CLO-942’s Gemini report contains OpenCode help, and its Ollama report records a retired model. The spec needs a clearer definition of success across the full gate, from reviewer launch through report writing and the decision to proceed. [Specification](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:7) · [Gemini incident](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/reviews/clo-942-spec-review-gemini.md:9) · [Ollama incident](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/reviews/clo-942-spec-review-ollama.md:9)

## 2. Acceptance Criteria Review

**Strong** — AC1–AC5 and AC8 specify the reviewer pair, provider, reasoning effort, supported flags, and removal of the second Gemini attempt. AC7 and AC9 make report attribution and filenames observable.

**Gaps** — AC6 covers failures detected by the shell guard. It leaves validator rejection, validator outage, and lok’s outer timeout without a defined report outcome. In the lok source matching the installed version, a validator can mark a step unsuccessful while retaining its raw output. The current writers can then publish that rejected output under a normal review header. Require one authoritative reviewer outcome that the fallback, synthesis, and writer all consume. Define the minimum valid review shape, including a phase-appropriate verdict, for the validator-outage path. [AC6](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:34) · [lok shell result handling](/Users/mk/Code/orchestrator/lok/src/workflow.rs:2070) · [Pre-PR report writer](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml:293)

AC1, AC2, and AC10 allow literal models in the inline gate, although the spec promises per-run overrides and matching labels. Make override behavior consistent across entry points, and distinguish the **attempted** model on a failure report from the model that produced a successful review. AC6’s broad text matches also need one precise rule. A valid review of this very spec could quote `Unrecognized flag` or `REVIEW_FAILED`. [Model decisions and acceptance criteria](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:19) · [Fallback condition](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/spec-review.toml:137)

## 3. Constraints Check

**Aligned** — Parallel reviewers, Claude fallback, and a shell-level guard fit the existing workflow. Keeping gcm’s Rust providers outside the change is appropriate.

**Concerns** — The Must to preserve synthesis as written conflicts with the healthy design-review path. When Claude fallback is skipped, the design synthesis and writer still interpolate its output unconditionally. The repo documents a prior run that lost a successful review this way. Allow the narrow template fix. Also limit the `--dir` ban to OpenCode, explicitly allow a bounded stderr excerpt in **failure** reports, and scope Gemini’s unattended permissions to review-only operations. The archived help defines `--auto` as approving permissions that are not explicitly denied. [Constraints](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:43) · [Design workflow](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/design-review.toml:221) · [Prior incident](/Users/mk/Code/gcm--fix-clo-945-review-gates/.pi/lessons/lok-workflow-templates.md:3) · [OpenCode help artifact](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/reviews/clo-942-spec-review-gemini.md:31)

## 4. Decomposition Quality

**Well-scoped** — The shared guard is a reasonable first slice. The workflow changes can proceed independently once the result contract and fixtures are defined.

**Issues** — Subtask 2 bundles two workflows with different synthesis and writer behavior. Split it by workflow to make the two-hour target credible. Separate the inline gate from the lok backend configuration, and assign explicit ownership for the gate’s final verdict and its consumers. Section 4 lists nine existing files plus an optional helper. Correct the eight-file estimate and the escalation boundary. [Decomposition](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:67)

## 5. Evaluation Coverage

**Covered** — The proposed fixtures exercise nonzero exit, short output, and exit-zero help. The strict-config smoke checks the Codex effort key. The installed lok exposes `lok workflow validate`, and all three existing workflow files passed it during this review.

**Gaps** — Tests #4–#6 can miss the intended branch. A single-behavior OpenCode stub may fail the preflight or accidentally pass its loose `grep -qi "ok"` because the help text contains `opencode`. Give probe and review calls distinct stub responses and assert that the reviewer was invoked. Test all three workflows and the inline gate with controlled Claude responses. Cover both-success, one-failure, both-failure with fallback success and failure, timeout, validator rejection, whitespace-only output, and retry with existing reports. Test both model overrides in argv and headers. Separate live success from degradation: AC11 currently passes when both live reviewers fail, and #9 never runs the inline gate. Test #1 also rejects lok’s valid `--dir`, while #3 counts any one of several required strings. [Evaluation table](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:77) · [Gemini preflight](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/spec-review.toml:16)

## 6. Codebase Alignment

**Violations** — AC9 renames the shared reviewer field and artifacts on the Claude side while Pi still records `review_ollama` and documents old paths. That breaks the documented handoff between runtimes. Preserve compatibility for existing and in-flight tasks, or include a coordinated consumer update. The Claude workflow-state and synthesis templates also still name Ollama. For `.lok/lok.toml`, use the backend’s `model` field as the single model source. Lok appends `--model` itself, so repeating it in `args` creates two model flags. [Pi handoff contract](/Users/mk/Code/gcm--fix-clo-945-review-gates/.pi/AGENTS.md:14) · [Pi spec consumer](/Users/mk/Code/gcm--fix-clo-945-review-gates/.pi/orchestrator/phases/spec.md:108) · [Claude state template](/Users/mk/Code/gcm--fix-clo-945-review-gates/.claude/templates/workflow-state.yaml:43) · [Claude synthesis template](/Users/mk/Code/gcm--fix-clo-945-review-gates/.claude/templates/review-synthesis.md:14) · [lok Gemini argv](/Users/mk/Code/orchestrator/lok/src/backend/gemini.rs:546)

**Alignment** — The reviewer shell steps use lok’s subprocess path. gcm’s Rust contract is `Provider` with `ProviderError`. Lok’s `Backend::query` returns typed `BackendError`, while its shell runner uses contextual errors and captures process status and stderr separately. The spec can stay at that shell boundary. [gcm provider contract](/Users/mk/Code/gcm--fix-clo-945-review-gates/src/provider/facade.rs:43) · [lok backend contract](/Users/mk/Code/orchestrator/lok/src/backend/mod.rs:313) · [lok shell runner](/Users/mk/Code/orchestrator/lok/src/workflow.rs:3410)

## 7. Blind Spots

- **Zero usable reviews can look like a passing gate.** Pre-PR synthesis allows `PASS` when there are no must-fix findings and has no explicit rule for zero usable inputs. The writer checks only whether required files are nonempty, so an old report can satisfy a rerun. The inline gate also offers ordinary `proceed` when both reviewers are unavailable and unconditionally records `codex_validated: true`. Define a failed gate outcome for zero usable current-run reviews and preserve any explicit override as a distinct, recorded decision. [Pre-PR verdict rules](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml:244) · [Pre-PR file checks](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml:365) · [Inline gate state handling](/Users/mk/Code/gcm--fix-clo-945-review-gates/.claude/commands/task/phases/implement.md:114)
- **Timeouts and parallel execution can erase or misattribute the reason.** The design and spec workflows pair a five-minute lok deadline with a 300-second child deadline, leaving the shell guard no reliable time to publish a failure. The inline gate’s bare `wait` loses individual exit statuses, which I confirmed locally. Fixed stderr paths can mix diagnostics between runs. Require time for failure handling, per-process status capture, and per-run temporary files. [Design reviewer deadlines](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/design-review.toml:24) · [Inline parallel execution](/Users/mk/Code/gcm--fix-clo-945-review-gates/.claude/commands/task/phases/implement.md:67)

## 8. Verdict

**NEEDS_REVISION**

The reviewer choices are clear. The blocking gaps are failure propagation, the design workflow’s skipped-fallback path, and the pre-PR decision when no current review is usable. This was a read-only review. I scanned all 285 Markdown files under `docs/` for relevant context, traced the gate code, and validated the existing workflow definitions locally. Live provider execution remains unverified in this review.

## 9. Actionable Feedback

1. Define one current-run result per reviewer and carry it through validation, fallback, synthesis, report writing, and the PR decision. Cover rejected output, outer timeout, and zero usable reviews.
2. Fix the design workflow’s skipped-fallback references and reconcile report paths and state fields across Claude and Pi, including resume from existing task state.
3. Use the same resolved model for invocation and attribution everywhere. Give the inline gate consistent override behavior and use a single model source in the lok backend config.
4. Replace the broad grep checks with invocation-specific assertions. Add controlled end-to-end cases for every gate and require a separate live success check for both configured reviewers.

