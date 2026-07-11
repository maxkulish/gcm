# Pre-PR validation: clo-545

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-07-11
**Pipeline**: lok pre-pr-validation
---

I've now independently confirmed all three Codex findings and the production-code cleanliness both reviewers claimed. I have enough to adjudicate.

The core disagreement is AC5/AC8. My verification shows: production (live `src/`) code is 100% clean — the only `rg` hits are inside `#[cfg(test)]` blocks. But the implementation carries legacy model strings in **two** test functions (`openai.rs` helper test + `mod.rs` gate test) plus extra o-series IDs at `openai.rs:297`, while AC5/AC8 permit exactly **one** intentional fixture. Codex read the AC literally (correct on the facts); Gemini read "fixtures" as acceptable (missed the "single/sole" wording). The divergence is real but trivially fixable, and production behavior is unaffected.

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Verdict FAIL; 3 findings, all factually confirmed against the tree |
| Gemini | OK | Verdict PASS; 359/359 tests, fmt clean, clippy clean — confirmed on the substance, but missed the AC5/AC8 "single fixture" wording |
| Claude fallback | SKIPPED | At least one external reviewer succeeded |

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR
- **AC5/AC8 — legacy model strings live in two test fixtures, spec allows one.** `gpt-5.4-mini` appears in both `src/provider/openai.rs:292-295` (`validate_model_accepts_supported_rejects_others`) and `src/provider/mod.rs:835,840` (`select_openai_validates_gpt_5_6_family`), and `openai.rs:297` adds `gpt-4.1`/`gpt-4o`/`o3-mini`. AC5 permits "the single intentional legacy-rejection fixture" and AC8 calls it "the sole legacy string permitted in `src/` tests." Bounded fix (Codex's path): keep the `mod.rs` `select`-gate test as the single legacy fixture — it *is* the AC9 regression scenario (a saved/overridden `gpt-5.4-mini` rejected at construction with `provider == "OpenAI"`, `ErrorKind::Config`) — and retarget the `openai.rs` helper test to non-legacy invalid placeholders (e.g. `gpt-5.6-sol`, `unsupported-model`) so it proves helper rejection without carrying legacy strings. This preserves both coverage layers (helper unit + gate wiring) and satisfies the AC literally. Note: ST3 explicitly anticipated the helper living in `openai.rs`, so the fix is retargeting the strings, not deleting a test.
- **Trailing whitespace fails `git diff --check`.** 8 lines in `docs/reviews/clo-545-spec-review-gemini.md` (`:13,23,28,32,36,40,49,52` — the `**Strong**: ` / `**Aligned**: ` style labels). `git diff main...HEAD --check` fails, which commonly trips a pre-merge hook. Strip the trailing spaces in the same iteration.

## Out of Scope / Deferred
- **AC7 live smokes (grouping / message / resolve) pending.** By design owner-run with `OPENAI_API_KEY`; not runnable in a read-only sandbox. Not a code change — must be executed before final task completion, and per §3 Escalate a `temperature` HTTP 400 there would reopen scope (BS4). Track as a manual gate, not a PR-code blocker.
- **Stale round-1 wording in `docs/status/clo-545-workflow.yaml`.** Line 22 says "luna default" and line ~84 records the fallback assertion as `["gpt-5.6-luna","gpt-5.6-terra"]` (wrong order) — both contradict the approved **terra-default, terra-first** spec. Status-metadata only; doesn't affect the shipped change or any AC, but cheap to correct — fold into the same doc pass to avoid misleading later status review.
- **Gemini's "single-source-of-truth validation message" suggestion.** Forward-looking nicety for future provider-wide validators; no action for this PR.

## False Positives / Tooling Artifacts
- **Codex: `cargo test provider_defaults_and_tokens` "blocked by read-only access to target/debug/.cargo-lock."** Sandbox limitation, not a defect. Gemini's full run (359/359 pass, `clippy -D warnings` clean, `fmt --check` clean) covers this; re-run outside the sandbox as the standard pre-PR gate.
- **Codex verdict FAIL is over-weighted.** The finding is real but is test-fixture hygiene against an explicit-but-trivially-satisfiable AC, with production code fully clean — that is a one-iteration fix, not a FAIL-grade divergence.

## Recommendation
PROCEED_WITH_FIXES. One bounded iteration closes everything: (1) retarget the `openai.rs` `validate_model` test to non-legacy invalid placeholders so the `mod.rs` `select`-gate test is the sole legacy fixture (satisfies AC5/AC8); (2) strip trailing whitespace in `docs/reviews/clo-545-spec-review-gemini.md` so `git diff --check` passes; (3) optionally correct the stale "luna default"/fallback-order lines in the workflow YAML. Then re-run `cargo fmt --check && cargo clippy -- -D warnings && cargo test` outside the read-only sandbox to reconfirm green. The design is faithfully implemented — terra default, branch-free uniform GPT-5.6 payload, and the Design A validation gate in `provider::select` guarding both commit and resolve — with live `src/` code sweep-clean; no pivot or scope decision is needed. The AC7 live smokes remain an owner-run gate to clear before final task completion.
