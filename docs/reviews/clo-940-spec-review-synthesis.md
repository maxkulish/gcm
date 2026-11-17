# Spec Review Synthesis: clo-940

**Synthesized**: 2026-11-17
**Pipeline**: lok spec-review

---

# Spec review synthesis: CLO-940 `--plan-only --all` preview

## Reviewer status

| Reviewer | Status | Cause |
|---|---|---|
| Gemini | Failed | The output was the `opencode run --help` text, not a review. The invocation probably passed an argument that `opencode run` did not accept, or passed no message. |
| Ollama | Failed | The `glm-5` model was retired on 2026-07-15. The error suggests `glm-5.3-flash:cloud` as the replacement. |
| Claude (fallback) | Valid | It is the only source of findings below. |

With one valid source, the Agreement and Disagreement tables are empty. Every finding has one reviewer and no cross-check. Claude cited `file:line` evidence for its findings, but I did not re-verify those citations in this synthesis.

## Findings (single reviewer: Claude)

| # | Finding | Severity |
|---|---|---|
| 1 | Removing the early return at `src/main.rs:917-927` also changes `--plan-only` without `--all` on the merge-in-progress path and the grouping-fallback path (`run_fallback`, `src/main.rs:1127`). Those paths gain one provider call and a `message` field. This conflicts with the spec's Must-not, and no existing test catches it (AC-493-1/2 use a valid `PLAN_FILE`). | High |
| 2 | The spec needs a decision on Q1: should `--plan-only` on the merge and fallback paths preview the message? Claude recommends accepting it, so that the preview matches the real run. The cost is one extra provider call, the same as `--dry-run` today. This is out of scope per Linear, so the owner must decide. | High (decision) |
| 3 | AC-4 is unclear. "Grouped `--dry-run` previews" can mean the envelope mode or the way the user invoked gcm. A grouped `--dry-run` that falls back returns `mode: single` and will carry `message`. AC-4 also does not list `fallback` and `committed_group`. Both are safe through `Envelope::base`, but the AC must name them. | Medium |
| 4 | Evaluation row 4 does not check "one provider call" from AC-1. It must count request bodies in the mock's `$CAPTURE` file. | Medium |
| 5 | The edge case "existing plan cache stays byte-identical" has no Evaluation row. A per-test `GCM_CACHE_DIR` (as in `scripts/acceptance.sh:682`) makes this check cheap. | Medium |
| 6 | The comment at `src/main.rs:1245` ("the message is not part of the stable envelope") goes stale after the change. The spec does not list it. | Low |
| 7 | The second Escalate rule does not apply. No code outside `src/output.rs` builds `Envelope` with a struct literal. | Info |
| 8 | Consumers that reject unknown keys will break on `message`. The owner already accepted this, and `v` stays 1. | Low (accepted) |

Claude confirmed these points as correct: the diagnosis, the `--dry-run --all` precedent at `:977`, the `--help` text, the `noop_mode` edge case, the three-sub-task order, and the "3 files" scope (if the comment is fixed in sub-task 2).

## Consolidated verdict

**NEEDS_REVISION.** This verdict rests on one reviewer. The revision is small: one owner decision and four edits to the spec.

## Priority actions

1. Decide Q1, the scope of `--plan-only` on the merge and fallback paths. Then rewrite the Must-not to name exactly which `--plan-only` paths change. If the answer is accept, add an AC and an acceptance case: empty `PLAN_FILE`, `--plan-only --json`.
2. Clarify AC-4: state whether a grouped invocation that falls back to `mode: single` carries `message`. Add `fallback` and `committed_group` to the envelopes that must not carry it.
3. In Evaluation row 4, assert exactly one provider request from `$CAPTURE`.
4. Add an Evaluation row for the byte-identical plan cache, with a per-test `GCM_CACHE_DIR`.
5. Update the comment at `src/main.rs:1245` in sub-task 2.
6. Outside this spec: repair the review tooling before the next review. Change the Ollama model from `glm-5` to `glm-5.3-flash:cloud`, and fix the Gemini `opencode run` invocation so that it sends the message. Until then, every review has one reviewer.
