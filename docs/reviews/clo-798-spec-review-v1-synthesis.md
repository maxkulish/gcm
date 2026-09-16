# Spec Review Synthesis: clo-798

**Synthesized**: 2026-09-16
**Pipeline**: lok spec-review

---

Only one reviewer returned output, so this is a single-source synthesis with the cross-reference sections noted as unavailable.

**Reviewer status**: Gemini succeeded. Ollama failed on infrastructure, not content (`Pulling model glm-5:cloud... pull model manifest: file does not exist`) - the model tag could not be pulled, so no review was produced. Claude fallback was skipped by the harness rule.

## Agreement (High Confidence)

Not applicable - a single valid reviewer means no finding has independent corroboration. Every item below carries single-source confidence. I verified each against the worktree; verification notes are in the tables.

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| 1 | `gcm resolve` in scope | Pull `ui::CallProgress` into `resolve_hunks` in the same change - same transport, same silence, trivial once the guard exists | REVIEW_FAILED | Keep it out. The spec already names the exclusion with a rationale, and the global scope rule says don't widen. Worth a follow-up ticket, not this one |

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | `gcm resolve` stays silent. `src/resolve/mod.rs:966` calls `resolve_hunks` inside a **batch loop**, so a resolve run makes several sequential blocking calls - the silence there is longer than on the commit path, not shorter. The spec's "later at no extra design cost" is accurate, but the UX gap is bigger than the one-line dismissal implies | Gemini | Medium |
| 2 | No numbered test covers the ticker's error-path join. The constraint exists ("always joined... including on the error path") and the `MissingKey` case appears under *Edge cases to verify*, but tests 1-11 never assert it. An un-joined or half-drawn ticker on an error return would ship green | Gemini (reframed) | Medium |
| 3 | AC-2's "approximate prompt size in bytes" does not state whether it means prompt text or serialized JSON payload. Sub-task 4 resolves it (section byte counts over `GroupingContext` / `GatheredDiff`), so this is a wording fix in the AC, not a design hole | Gemini | Low |
| 4 | Gemini flagged `std::process::exit` skipping destructors as a design risk. **Verified as a non-issue**: `src/main.rs:38` is `std::process::exit(run(&args))` - `run` returns normally, so every local in `run` (including a `CallProgress` guard) drops before `exit` is ever called. The residual risk is only a panic mid-call, which the spec's Ctrl-C edge case already addresses by not hiding the cursor | Gemini | Informational |

Gemini also flagged a `Backend` vs `Provider` naming mismatch. That was in the review prompt, not the spec - `src/provider/facade.rs:40` defines `pub trait Provider` and the spec uses it correctly. No action.

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

Gemini rated the problem statement, constraints, decomposition and evaluation as reference-grade, with no correctness violations and no NEEDS_REVISION findings. Nothing in the single-source result blocks implementation.

## Priority Actions

1. **Decide the `resolve` boundary** (Finding 1). My recommendation: keep it out of scope and file a follow-up. The batch loop at `src/resolve/mod.rs:966` makes it a real gap, but it is a second surface with its own call-shape (N calls, not 1) and its own progress semantics - folding it in now widens a 7-file change and weakens the "one ticket, one path" framing the spec already committed to.
2. **Add a numbered test for the error-path join** (Finding 2). Promote the `MissingKey` edge-case bullet into row 12 of the evaluation table: run with no API key, assert the process exits cleanly, stderr carries no ticker fragment, and no thread outlives the call. This is the one AC-adjacent constraint with no test behind it.
3. **Pin AC-2's unit** (Finding 3). One clause: "approximate prompt size in bytes (the byte count of the generated prompt text sections per sub-task 4, not tokens and not the serialized request body)."

━━━ CLO-798 · SPEC REVIEW - approve with three suggestions ━━━━━━━━

✅ DONE
   Synthesis complete from 1 of 2 external reviewers; verdict APPROVE_WITH_SUGGESTIONS
   Gemini's `std::process::exit` concern checked against `src/main.rs:38` and dismissed

⚠️ NOT DONE
   Ollama review unavailable - `glm-5:cloud` manifest pull failed, so no second opinion exists on any finding above
   Nothing here has cross-reviewer corroboration

👉 YOUR MOVE
1. [as-is]      Apply actions 2 and 3, keep `resolve` out of scope, proceed to implementation   ← recommended
2. [widen]      Fold `resolve_hunks` progress into this spec as a seventh sub-task
3. [re-review]  Fix the Ollama model tag and re-run for a second opinion before deciding
