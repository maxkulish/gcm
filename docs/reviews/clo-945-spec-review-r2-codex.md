# Spec Review (Round 2): clo-945

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Reviewed spec sha256**: 5c434bc8f048
**Date**: 2026-11-19

---

## 1. Problem Statement Assessment

The third revision clearly identifies the spec, design, and pre-PR review gates, explains how CLI failures became review artifacts, and matches the supplied Linear brief and model pins. The main gap is the definition of gate success when only some stages succeed. I reviewed the current spec (`7bd2141`) against the repository and lok 20261107.0.2 source. Live provider and end-to-end Pi runs remain unverified. [Current specification](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md)

## 2. Acceptance Criteria Review

**Strong**  
AC6–AC8 define concrete checks for CLI failure, help text, and post-validation output. AC14 makes zero reviews a deterministic outcome. AC17 requires both pinned models to produce substantive reviews of the spec.

**Gaps**

- **High — Pre-PR can return success without a usable synthesis.** AC14 lets a failed synthesis reach the writer, but only defines a process failure when the usable-review count is zero. With one valid review, a Claude error string or malformed synthesis can satisfy the existing non-empty-file check while leaving Pi without its binding verdict. AC8 also classifies only the external reviewers, so the writer can reject a fallback report after synthesis has already seen it. [AC8–AC14](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:59) · [Pre-PR writer](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml:360) · [Pi verdict contract](/Users/mk/Code/gcm--fix-clo-945-review-gates/.pi/orchestrator/phases/implement.md:263)
- **Medium — A report can pass the gate and still lack the section its consumer reads.** AC18 accepts any three other `##` headings for design reviews. AC24 asks for `## 8. Actionable Feedback`, but the gate can accept a report that omits it or a validator replacement that removes it. The design phase extracts feedback from that exact section. [AC18](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:69) · [Design phase](/Users/mk/Code/gcm--fix-clo-945-review-gates/.claude/commands/task/phases/design.md:92)

## 3. Constraints Check

**Aligned**  
POSIX shell syntax, lok’s `shell_escape` filter, and the gcm Rust boundary fit the current architecture. The scope escalation rule gives the implementer a clear path when the fixed file list conflicts with a requirement. [lok shell runner](/Users/mk/Code/orchestrator/lok/src/workflow.rs:3410) · [lok template filter](/Users/mk/Code/orchestrator/lok/src/template/filters.rs:16) · [Constraints](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:79)

**Concerns**  
AC9’s startup deletion conflicts with preserving historical reviews. Normal task slugs already have tracked reports at the target paths, including CLO-533 pre-PR validation and CLO-942 spec review. A rerun can remove those files before any replacement is ready. The spec needs a publication rule that preserves checkpoint artifacts while preventing a previous run from satisfying the current gate. [AC9](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:60) · [Historical-review constraint](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:95) · [Tracked pre-PR report](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/reviews/clo-533-validation-synthesis.md) · [Tracked spec report](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/reviews/clo-942-spec-review-gemini.md)

## 4. Decomposition Quality

**Well-scoped**  
The shared guard is a sensible first task. The three workflows can then be wired independently once the status and report contracts are fixed.

**Issues**  
Tasks 2–4 each combine reviewer migration, status handling, publication, and integration tests. Their two-hour scope is optimistic. Give the shared task ownership of the status schema and failure-report format. Give pre-PR publication and Pi handoff explicit verification tasks. Task 6 also depends on the shared format because first-line failure detection must agree with the writers. [Decomposition](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:108)

## 5. Evaluation Coverage

**Covered**  
The matrix covers bad CLI output, child timeout, skipped fallback, validator replacement, zero reviews, model overrides, and live spec and design review.

**Gaps**  
Test 8c needs a case with a usable reviewer and failed or malformed synthesis. Test 8a should use a structurally valid report and a deterministic validator rejection, exercising the production guard without a bypass. Test 17 checks workflow invocation and a spec-to-implement transition, but never exercises Pi’s actual PR checklist with a missing reviewer or a specification task. Inline test 11 also needs the zero-review override and pause paths. [Evaluation matrix](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:123)

## 6. Codebase Alignment

**Violations**  
AC9 needs an explicit channel for failure details. lok templates expose `output`, `success`, and parsed output fields. They do not expose the engine’s `stderr` or validation metadata, so the writer cannot retrieve those fields directly. Also, test 4 expects a one-line failure file while AC10 requires a reviewer header. Specify that the first line carries the failure sentinel and define where the header and bounded diagnostics follow. [lok template context](/Users/mk/Code/orchestrator/lok/src/template/context.rs:45) · [AC8–AC10](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:59) · [Test 4](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:128)

**Alignment**  
The external shell steps preserve the existing boundary. gcm uses a synchronous `Provider` with `ProviderError` and `GcmError`. lok’s async `Backend` returns typed `BackendError`, while its workflow runner handles orchestration errors through `anyhow`. CLO-945 needs no Rust error-type changes. [gcm Provider](/Users/mk/Code/gcm--fix-clo-945-review-gates/src/provider/facade.rs:43) · [gcm errors](/Users/mk/Code/gcm--fix-clo-945-review-gates/src/error.rs:6) · [lok Backend](/Users/mk/Code/orchestrator/lok/src/backend/mod.rs:313) · [lok workflow error path](/Users/mk/Code/orchestrator/lok/src/workflow.rs:1729)

## 7. Blind Spots

- **High — Pi’s real PR handoff is stricter than the proposed workflow.** AC22 can pass with two healthy stub reviewers while Pi still blocks a run with one usable review because its checklist requires `codex_validated: true` and a verdict in both external reports. For specification tasks, the documented Pi invocation also requests design and plan files even though the spec is the plan. Define the supported outcome for each case and verify it through Pi’s actual entry point. [AC22](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:73) · [Pi invocation](/Users/mk/Code/gcm--fix-clo-945-review-gates/.pi/orchestrator/phases/implement.md:214) · [Pi checklist](/Users/mk/Code/gcm--fix-clo-945-review-gates/.pi/orchestrator/phases/implement.md:353) · [Specification transition](/Users/mk/Code/gcm--fix-clo-945-review-gates/.claude/commands/task/phases/spec.md:162)
- **Medium — Diff preparation can fail before review starts.** AC23 requires a readable branch diff but leaves missing `main`, failed diff generation, and failed writes without a defined outcome. The pre-PR health step currently allows errors to continue. Require a named input failure and stop dispatch if the current run cannot prepare its diff. [AC23](/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md:74) · [Pre-PR health step](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml:14)

## 8. Verdict

NEEDS_REVISION

## 9. Actionable Feedback

1. Define the gate outcome for zero reviews, usable reviews with failed synthesis, and usable reviews with a valid synthesis verdict. Classify Claude fallback before synthesis consumes it. Assert the outer lok exit code in each case.
2. Reconcile AC22 with Pi’s actual PR checklist and specification-task inputs. If Pi intentionally requires both external reviews, specify a named blocked outcome for partial success and test it.
3. Stage artifacts per run, check that diff preparation succeeded before dispatch, and preserve tracked checkpoint files during publication. Test a failed rerun against an existing tracked report.
4. Specify the report and diagnostic format before wiring the workflows. Require the design feedback section after validation and use deterministic stubs for validator rejection, synthesis failure, and inline override behavior.
