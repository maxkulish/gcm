# Pre-PR Validation: clo-801

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Validated**: 2026-11-21
**Pipeline**: implement.md validation gate

---

## Verdict: PASS_WITH_NOTES

Reviewed `ab349ea` against `main` and the approved CLO-801 spec. The supplied diff matches the branch. The full offline test suite, Clippy with warnings denied, and the public-surface check passed. Loopback probes also confirmed that a truncated Anthropic reply triggers one fallback message request and that an index resolves to the original path after inventory redaction.

## Findings

- **MEDIUM — The Anthropic truncation fallback lacks the required integration test.** Evaluation row 16 calls for a CLI capture case. The Anthropic unit test checks `Deserialize`, while the existing CLI fallback case uses a Groq reply with an empty group. The loopback probe confirmed the current behavior, but the checked-in suite leaves it unguarded. [Spec](/Users/mk/Code/gcm--feat-clo-801-group-index/docs/specs/2026-11-21-clo-801-group-index.md:271) · [Anthropic test](/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/anthropic.rs:428) · [CLI test](/Users/mk/Code/gcm--feat-clo-801-group-index/tests/grouping_budget.rs:590)
- **LOW — Provider payload tests only partially cover the index contract.** All six builders use the updated prompt and schemas. Tests such as Ollama’s check that `groups` exists and that the user prompt contains “Changed files,” so they could pass with string items or incorrect grouping instructions. [Spec](/Users/mk/Code/gcm--feat-clo-801-group-index/docs/specs/2026-11-21-clo-801-group-index.md:272) · [Ollama payload test](/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/ollama.rs:263)
- **LOW — Project tracking shows conflicting status.** `PROJECT.md` lists CLO-801 in the Spec phase and says no WIP slot is in use. The task status records implementation with live checks open. [Project tracker](/Users/mk/Code/gcm--feat-clo-801-group-index/docs/PROJECT.md:99) · [Task status](/Users/mk/Code/gcm--feat-clo-801-group-index/docs/status/clo-801-group-index.md:10)

## Missing Items

- **AC10 and AC11 remain open for owner verification.** The 515-file Vertex timing and Anthropic live grouping results, including their `main` baselines, are pending. The Ollama cloud reference-count check is also pending. [Task status](/Users/mk/Code/gcm--feat-clo-801-group-index/docs/status/clo-801-group-index.md:24)

## Recommendations

- Add an Anthropic capture test that returns `stop_reason: "max_tokens"` and verifies one follow-up message request and a single-mode result.
- Assert integer item types in provider payload tests and pin the shared prompt’s `"i"` instructions.
- Sync the project tracker and include the exact owner-run commands in the PR.