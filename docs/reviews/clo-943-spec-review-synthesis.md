# Spec Review Synthesis: clo-943

**Synthesized**: 2026-11-19
**Pipeline**: lok spec-review

---

## Review Sources

- **Gemini:** The review failed. The output is the `opencode run` usage text, not a review. The invocation was malformed: either the message argument was missing or a flag was rejected.
- **Ollama:** The review failed. The configured model `glm-5` was retired on 2026-07-15. The provider suggests `glm-5.3-flash:cloud` as the replacement.
- **Claude fallback:** This review is valid. Because it is the only valid source, the synthesis below comes from it alone. No findings were cross-checked, so the Agreement and Disagreement tables are empty.

## Agreement (High Confidence)

None. Only one reviewer returned a review.

## Disagreement (Needs Human Decision)

None. Only one reviewer returned a review.

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | The "Prefer" clause says that `jq -er '.message'` gives a script an error instead of `null`. That claim is false. The reviewer ran the filter on an envelope without `message`: it printed `null` and exited 1. Without `set -e` or `pipefail`, `msg=$(... \| jq -er .message)` stores the string `"null"`. Two alternatives work: `.message // empty` (verified to exit 4 with empty stdout) and `.message // error("no message")`. | Claude | High |
| 2 | The edge case for a missing `message` checks only for a non-zero exit. It must also require empty stdout, because of finding 1. | Claude | Medium |
| 3 | AC-2 and AC-3 describe the fallback as "when the grouping plan does not parse". Four paths actually lead to `run_fallback` and then `single_preview`: a provider error classed as fallback (`src/main.rs:689-695`), a plan that fails validation (`src/main.rs:699`), a prompt-budget refusal (`src/main.rs:621`), and an invalid cached plan (`src/main.rs:494-511`). With the current wording, the README will not list all the cases where `message` appears. | Claude | Medium |
| 4 | Evaluation step #5 cannot be run as written. The mock provider is a Python heredoc inside `scripts/acceptance.sh` with no standalone launcher, and later cases overwrite `/tmp/gcm-json.out`. A saved envelope built from the AC-493-10 assertions can replace it. | Claude | Medium |
| 5 | The table row for "`committed`, `fallback`, `noop`, `error` envelopes" cites CLO-940 AC-4 and AC-5. The error envelope without `message` is asserted in CLO-940-7. | Claude | Low |
| 6 | The README labels its one existing example with a singular "Example:". A second block under that label will read awkwardly. The new `message` paragraph needs its own lead-in, so that the existing lines stay unchanged as AC-6 requires. | Claude | Low |

The reviewer confirmed that the following parts of the spec are correct:

- every code and line reference
- the "zero requests" reading of merge `--plan-only` (CLO-940-4) compared with fallback `--plan-only` (CLO-940-5)
- the collision risk between the top-level `message` and `commit.message`
- the constraints on contractions, `v`, the single call site and the S estimate
- the AC-488-1 exclusion, which CLO-944 tracks

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

The only valid reviewer gave this verdict, and no reviewer returned NEEDS_REVISION.

## Priority Actions

1. **Fix the `jq` guidance.** In the "Prefer" clause, replace `jq -er '.message'` with `jq -er '.message // empty'`. Update the missing-`message` edge case so that it requires empty stdout as well as a non-zero exit. (Findings 1 and 2)
2. **Widen the fallback wording.** In AC-2, AC-3 and the presence table, write "when grouping falls back to a single commit". Cite `src/main.rs:494-511, 621, 689-699` as the triggers. (Finding 3)
3. **Make evaluation step #5 runnable.** Use this saved envelope: `{"v":1,"status":"plan","mode":"single","plan":{"groups":[]},"changed_files":["a.txt"],"cached":false,"message":"chore(test): mock commit message"}`. (Finding 4)
4. **Correct the citation.** In the non-`plan` envelope row, cite CLO-940-7 for the error envelope. (Finding 5)
5. **Give the new example its own lead-in.** Keep the existing "Example:" block as it is. (Finding 6)

**Follow-up outside this spec:** Two of the three review backends are broken. Gemini needs its `opencode run` invocation fixed. Ollama needs `glm-5` replaced with a current model, and the provider suggests `glm-5.3-flash:cloud`. Until both are fixed, every spec review falls back to the Claude reviewer alone, with no cross-check.
