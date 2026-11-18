# Spec Review Synthesis: clo-942

**Synthesized**: 2026-11-18
**Pipeline**: lok spec-review

---

# Spec Review Synthesis: CLO-942, the orchestrate pi extension on pi 1.5.x

## Reviewer Status

Only the Claude fallback produced a review, so this synthesis rests on that one source.

- **Gemini failed.** It returned the `opencode run` usage text instead of a review. The call that the review tooling sends to OpenCode is likely malformed, for example a wrong flag or a missing message argument.
- **Ollama failed.** The configured model `glm-5` was retired on 2026-07-15. The error message suggests `glm-5.3-flash:cloud` as the replacement.

Both failures come from the review tooling, not from the spec. Fix them separately from CLO-942.

## Agreement (High Confidence)

None. Only one reviewer produced a review, so no finding can be cross-checked.

## Disagreement (Needs Human Decision)

None.

## Findings (Claude Only)

| # | Finding | Section | Severity |
|---|---------|---------|----------|
| 1 | AC8 cannot be checked at PR time. Dependabot alerts only track the default branch. The spec has no baseline of open alerts. The `gh api` call returns only the first page. | AC | High |
| 2 | Pi 1.5.2 is installed locally, so the "if pi is not installed" branch does not apply. `tsc` passing does not prove that pi rejects an invalid phase at runtime, because pi-coding-agent bundles its own typebox (1.3.27 vs 1.3.36). | Risks / Assumptions | Medium |
| 3 | AC5 does not pin the TypeScript version, because neither lockfile contains `typescript`. The flags in AC5 differ from the flags in Evaluation #1. | AC | Medium |
| 4 | Evaluation #2 only tests the negative case. A `never` type also rejects `"bogus"`, so the check needs a valid assignment that compiles. | AC | Medium |
| 5 | Sub-task 3 (brace-expansion) is not in the ticket. It is low-risk, because `minimatch@11.0.3` already declares `brace-expansion ^5.0.12`. | Decomposition | Low |
| 6 | minimatch and undici can resolve to lower versions once the overrides are removed (11.0.9 to 11.0.3, 8.12.4 to 8.12.3). Both versions still meet AC7. | Decomposition | Low |
| 7 | `^1.5.2` is stricter than linear's manifest, which still declares `^1.1.0`. Only linear's lockfile resolves 1.5.2. | Problem Statement | Low |
| 8 | npm audit on the 1.5.2 tree can report advisories outside undici and minimatch. The existing escalation trigger covers this case. | Risks | Low |
| 9 | No CI workflow type-checks `.pi/extensions/`, so AC5 is checked only on the local machine. | Risks | Low (follow-up) |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

The only valid review gave this verdict. No reviewer returned NEEDS_REVISION.

## Priority Actions

1. **Rewrite AC8.** Record the current open alerts for `.pi/extensions/orchestrate/package-lock.json` as a baseline. Check them again after the merge to `main`, not after the push. Add `--paginate` to the `gh api` call.
2. **Make the pi load check mandatory.** Show that the extension loads and the orchestrate tools register. Then make one tool call with an invalid `phase` and confirm that pi rejects it.
3. **Pin TypeScript.** Use `npx -p typescript@<x> tsc ...` with the same flags in AC5 and in Evaluation #1.
4. **Add a positive case to Evaluation #2.** A valid phase must compile, so a `never` type cannot pass.
5. **Label sub-task 3 as an extra in the PR body.** Cite the `brace-expansion ^5.0.12` declaration in minimatch as the evidence.
6. **Update section 1.** State that `^1.5.2` is stricter than linear's manifest, and that the lower resolved versions of minimatch and undici are expected.
7. **Follow-up outside CLO-942:** add a CI type-check for `.pi/extensions/`, and fix the Gemini and Ollama reviewer setup.
