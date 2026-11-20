# Spec Review: clo-947

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Reviewed**: 2026-11-20
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The spec matches CLO-947. I verified all 17 moves across the four gate files, including both diff publishers and the pre-PR branch that publishes an input-failure synthesis. The failure mechanism is clear. The spec overstates what lok displays when a writer fails. [CLO-947 spec](/Users/mk/Code/gcm--fix-clo-947-report-publish/docs/specs/2026-11-20-clo-947-report-publish.md:7)

## 2. Acceptance Criteria Review

**Strong** criteria define the helper’s exit status, output channels, and filesystem effects for directory targets, directory symlinks, and normal publication.

**Gaps** remain at two caller boundaries.

- **AC5 and AC10 promise a diagnostic lok can hide.** On `20261107.0.2`, a two-step probe exited 1 and displayed the CLO-947 synthesis error as `…: target is ...`. A valid `clo-429` destination turned the entire diagnostic into `rate limited (try again later or use fewer backends)`. Lok classifies failed-step stderr and truncates unclassified summaries to 80 characters. Sending the line to stderr alone cannot guarantee the prefix, full path, and reason reach the caller. [Lok error summary](/Users/mk/Code/orchestrator/lok/src/utils.rs:185)
- **AC6 and AC7 leave the inline exit boundary ambiguous.** In a probe of the existing wrapper, child exits 1 and 3 printed `gate exit 1` and `gate exit 3`, and the outer shell returned 0 in both cases. Specify the child status, printed status, absence of success lines, and unchanged directory. The wrapper’s outer status is tracked in CLO-949. [Inline wrapper](/Users/mk/Code/gcm--fix-clo-947-report-publish/.claude/commands/task/phases/implement.md:141) · [CLO-949](https://linear.app/cloud-ai/issue/CLO-949/fix-the-minor-review-gate-defects-left-open-after-clo-945)

## 3. Constraints Check

**Aligned** constraints include POSIX `sh`, reuse of the shared guard, portable directory detection, and preservation of report order and the dependent verdict gates. The existing CI matrix supports the requested Linux and macOS coverage. [Shared guard](/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/lib/review-guard.sh:1) · [CI matrix](/Users/mk/Code/gcm--fix-clo-947-report-publish/.github/workflows/ci.yml:12)

**Concerns** center on the diagnostic contract. The spec’s escalation condition applies because lok can replace the named stderr line. The diff path can preserve its `REVIEW_INPUT_FAILED` convention and structured health-check stdout while defining a reliable way for the caller to see the reason. [Constraints](/Users/mk/Code/gcm--fix-clo-947-report-publish/docs/specs/2026-11-20-clo-947-report-publish.md:54)

## 4. Decomposition Quality

**Well-scoped** slices cover the helper, CI, workflow wiring, inline gate, and reader docs. Each looks feasible in roughly two hours once the diagnostic approach is settled.

**Issues** include two understated dependencies. The wiring tests can be authored alongside the workflow changes, but they pass only after both wiring slices land. The reader-doc slice shares `pre-pr-validation.toml` with the workflow slice and depends on the revised diagnostic contract. [Decomposition](/Users/mk/Code/gcm--fix-clo-947-report-publish/docs/specs/2026-11-20-clo-947-report-publish.md:84)

## 5. Evaluation Coverage

**Covered** scenarios include missing and existing regular-file targets, directory and symlink refusal, an `mv` failure, all four gate entry points, and both CI runners.

**Gaps** include AC3’s post-move failure branch and tests that distinguish the writer’s `usable=` output from the earlier `review_status` output. Specify valid stub review and synthesis responses, log model invocations for the diff failure, and run the documented `lok workflow run pre-pr-validation` command. Assert that the rejected directory gains no file and the final `reports published` marker is absent. [Evaluation table](/Users/mk/Code/gcm--fix-clo-947-report-publish/docs/specs/2026-11-20-clo-947-report-publish.md:100) · [Documented pre-PR entry point](/Users/mk/Code/gcm--fix-clo-947-report-publish/.pi/orchestrator/phases/implement.md:214)

## 6. Codebase Alignment

**Violations** are at the shell boundary. The pre-PR writer is named `write_reports`, and lok summarizes its stderr before showing it. [Pre-PR writer](/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/pre-pr-validation.toml:338) · [Lok shell failure handling](/Users/mk/Code/orchestrator/lok/src/workflow.rs:2122)

**Alignment** is sound on the Rust side. The change operates through shell output and exit status. Gcm’s `Provider`, `ProviderError`, and `GcmError`, and lok’s `Backend` and `BackendErrorKind`, require no changes for the publication helper. [Gcm provider contract](/Users/mk/Code/gcm--fix-clo-947-report-publish/src/provider/facade.rs:43) · [Gcm errors](/Users/mk/Code/gcm--fix-clo-947-report-publish/src/error.rs:10) · [Lok backend contract](/Users/mk/Code/orchestrator/lok/src/backend/mod.rs:313)

## 7. Blind Spots

AC6 stores the lok diff failure in the synthesis file. The existing failed writer’s `REVIEW_INPUT_FAILED: see ...` message goes to stdout, which lok dropped in the probe. If the synthesis destination is also a directory, the diff reason remains in the run’s internal `input-failed` marker. Define where the caller can retrieve the reason and test this combined failure. [Input-failure marker](/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/pre-pr-validation.toml:63) · [Failure synthesis publication](/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/pre-pr-validation.toml:396)

## 8. Verdict

**NEEDS_REVISION.** The shared helper is a reasonable fix for existing directory targets. The spec needs a caller-visible diagnostic that survives lok’s error handling and tests that verify it. This review used read-only source inspection, two-step lok probes, and a stubbed inline wrapper. Full gate runs remain implementation verification.

## 9. Actionable Feedback

1. **Revise AC5, AC6, and AC10** around a verified diagnostic route. Test the full path and reason for a normal report, a longer path, and a slug containing `429`. Define how the lok diff failure is surfaced when the synthesis cannot be published.
2. **Clarify AC6 and AC7 for the inline gate.** Assert the child’s failure status and the wrapper’s printed status, plus the filesystem and output effects. Reference CLO-949 for the outer exit status.
3. **Complete the test matrix.** Add the AC3 post-move check and the blocked failure-synthesis case. Scope `usable=` checks to the writer and demonstrate both creation and replacement on successful publication.
