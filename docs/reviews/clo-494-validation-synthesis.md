# Pre-PR validation: clo-494

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-06-22
**Pipeline**: lok pre-pr-validation
---

All gates green: `cargo fmt --check` clean, `cargo clippy --all-targets -- -D warnings` clean, 133 tests pass (15 Anthropic tests, 0 failures). This resolves Codex's read-only caveat and confirms Gemini's clippy claim. I now have everything needed to synthesize.

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | success=true. Thorough; correctly surfaced the project-tracking-doc collapse, plan-conformance test gaps, and README staleness. Read-only, so could not run clippy/test — I ran them and the gate is green. |
| Gemini | OK | success=true. Confirmed all 4 ACs + ST1–ST5 implemented and clippy clean; found the deserialization round-trip nit. Missed the plan-conformance test gaps Codex caught. |
| Claude fallback | SKIPPED | Both external reviewers succeeded. |

## Verdict
PASS_WITH_NOTES

The Anthropic provider code faithfully implements the design (forced tool-use, `x-api-key` + `anthropic-version` via `extra_headers`, direct-deserialize-first, `strip_think` backstop, env resolution). The pre-merge gate is green. The Must Fix items are all bundled-doc and cheap-test gaps addressable in one bounded iteration. No pivot or fundamental scope divergence.

## Must Fix Before PR
- **Project-tracking docs collapsed and staled (Codex MEDIUM, confirmed).** `docs/PROJECT.md` marks CLO-494 as phase **Discovery** (it is fully implemented), deletes the all-tasks master table + dependency tree + FR allocation, and drops CLO-488 (was In Progress/PR), CLO-490, CLO-493, CLO-495, CLO-496. `docs/ROADMAP.md` drops 8 tasks; `docs/DEPENDENCIES.md` wipes the blockers/ready tables. These are unrelated to the Anthropic feature and degrade the planning source of truth on merge. Fix: confirm whether this was an intended `/project:sync` regeneration — if not, drop the three tracking-doc changes from this PR; if yes, correct CLO-494's phase and restore the dropped tasks.
- **Plan-conformance test gaps (Codex MEDIUM, partially confirmed) — cheap, no network needed.** (a) No test proves the wire headers: add a unit test on `request()` asserting `auth == ("x-api-key", …)` and `extra_headers` contains `("anthropic-version", "2023-06-01")`. (b) Plan ST3 acceptance requires the unknown-provider error string to contain `"anthropic"` — `pick_provider_id_unknown_is_config_error` only asserts `"groq"`; add the `anthropic` assertion (the message already contains it, line 230). (c) Plan test #13 requires a `GCM_ANTHROPIC_BASE_URL` override test — only the default path is tested (`base_url_defaults_to_production`); add the override case.
- **README provider tables stale (Codex LOW, confirmed).** README still lists only Groq/Google/OpenAI and `GROQ/GEMINI/OPENAI_API_KEY`, but this PR's CLI help (`EGRESS_DISCLOSURE`) now exposes `anthropic`/`ANTHROPIC_API_KEY`/`GCM_ANTHROPIC_MODEL`. Add Anthropic to the provider, key, and env tables so the shipped docs match the shipped CLI.

## Out of Scope / Deferred
- **No acceptance.sh / mock-server header coverage (Codex).** The design framed end-to-end header verification as integration test 25 requiring a live mock server, and the implementation plan (ST1–ST5) did not include acceptance-harness additions. The cheap `request()` unit test above closes the practical risk; a full mock-server acceptance test is a reasonable follow-up.
- **Live-API assumptions unverified (Codex).** Valid typed plan, forced-tool-call behavior, model-ID validity, no reasoning leakage — cannot be checked without a real key. Design assumptions A1–A7 document these and they surface as clear `BadRequest`/`Deserialize` errors if wrong. Defer to manual integration run.
- **Deserialization round-trip redundancy (Gemini LOW).** `extract_tool_use_input` does `Value→Plan→String` then `generate_plan` does `String→Plan` — a harmless double parse on the happy path. The design itself specified "direct deserialization first" in both spots; functionally correct, negligible cost at plan-sized payloads. Optional cleanup, not blocking.

## False Positives / Tooling Artifacts
- **Codex's clippy/test caveat.** Codex declined to run `cargo clippy`/`cargo test` (read-only session). I ran the full gate: `cargo fmt --check` clean, `cargo clippy --all-targets -- -D warnings` clean, 133 tests pass (15 Anthropic, 0 failures). No outstanding tooling concern.
- **Gemini "Missing Items: None."** Slightly optimistic — the implementation is complete, but Gemini overlooked the plan-conformance test gaps Codex found. Not a false finding, just an omission.

## Recommendation
**PROCEED_WITH_FIXES.** The provider implementation is correct, design-faithful, and passes the full pre-merge gate; nothing blocks on code quality or correctness. Before opening the PR, apply one bounded fix iteration: (1) resolve the bundled project-tracking-doc collapse — either drop `PROJECT.md`/`ROADMAP.md`/`DEPENDENCIES.md` from this PR or correct CLO-494's stale "Discovery" status and restore the dropped tasks; (2) add three cheap tests (`request()` header assertion, `anthropic` in the unknown-provider error, `GCM_ANTHROPIC_BASE_URL` override); (3) refresh the README provider/key/env tables to include Anthropic. The deferred items (acceptance-harness mock coverage, live-API verification, the round-trip micro-optimization) can follow in a separate change and should not block this PR.
