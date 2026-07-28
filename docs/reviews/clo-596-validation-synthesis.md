# Pre-PR validation: clo-596

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-07-28
**Pipeline**: lok pre-pr-validation
---

I verified every claim against the code and ran the checks Codex could not.

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | 3 findings + 3 missing items; all substantive claims verified accurate. Could not run Cargo (read-only sandbox) — I ran the full matrix on its behalf. |
| Gemini | OK | Verdict PASS. Confirmed design conformance correctly, but missed both MEDIUM regressions Codex caught (see note in Recommendation). |
| Claude fallback | SKIPPED | Both external reviewers succeeded. |

**Checks I ran (Codex's tooling gap closed):** `cargo test` → **492 passed, 0 failed** (design baseline 489); `cargo test --no-default-features` → passes (17 lib + 2 + 1); `cargo fmt --check` → clean; `cargo clippy --all-targets -- -D warnings` → clean.

## Verdict

PASS_WITH_NOTES

## Must Fix Before PR

- **OpenAI model list is now two sources of truth — a regression this ticket exists to prevent.** On `main` there was exactly one list: `openai::SUPPORTED_MODELS`, consumed by the runtime gate (`openai.rs:134,142`), the registry filter (`models.rs:348,405`), and `default_model` (`mod.rs:376`). HEAD introduces `identity::OPENAI_SUPPORTED_MODELS` (`src/provider/identity.rs:148`) for the library side while `openai.rs:28` keeps its own copy. They agree today, but nothing ties them — and because the library const is `pub(crate)`, the binary cannot even see it, so a cross-check test is currently *unwritable*. Adding a model to one list silently desynchronizes the registry filter from the runtime gate. Fix: make it `#[doc(hidden)] pub` in the library, re-export through the facade, have `openai.rs` consume it, delete the local const.

- **Four runtime-selection tests deleted with no replacement; `select()` and `pick_provider_id()` now have zero coverage repo-wide.** Dropped: `pick_provider_id_precedence`, `pick_provider_id_unknown_is_config_error`, `select_ollama_is_key_free`, `select_openai_validates_gpt_5_6_family`. This is worse than Codex reported: that last test was CLO-545's *designated* AC9 breaking-change regression fixture (its own comment names it "the sole intentional legacy-string fixture... the AC5/AC8 sweep exemption"), and `openai.rs:290-291` still cites it as living in `mod.rs` — a dangling reference to a deleted guard. Both `pick_provider_id` (`facade.rs:290`) and `select()` survive with unchanged signatures, so the tests restore verbatim into the facade test module. Update the stale `openai.rs` comment to point at the new location.

- **`ProviderId::auth_method()` has no test, though ST6 and the design test plan both mandate one.** Implemented at `identity.rs:223` with five consumers across `config.rs` and `status.rs`; `rg auth_method` finds call sites only. Plan ST6 line: "`ProviderId::auth_method` returns the correct variant for each provider." Design test-plan item #8 repeats it. Roughly eight lines.

**Also blocking, procedurally:** `src/provider/identity.rs` and `docs/status/clo-596-workflow.yaml` are uncommitted. The `identity.rs` edits are the three `#[allow(dead_code)]` attributes Gemini reviewed — they must be committed or `--no-default-features` clippy breaks for anyone else.

## Out of Scope / Deferred

- **Trailing whitespace (`git diff main...HEAD --check` exits 2).** Confirmed: 7 lines across `docs/discovery/clo-596.md` and the PRD. All are two-space markdown hard line breaks in docs — intentional syntax, not code, and stripping them changes rendering. ST7's gate is `fmt && clippy && test`, all green. Strip only if CI enforces `--check`.
- **CLI-side alias acceptance untested.** `cli.rs` has `try_parse_from` tests but only `--provider ollama`. The clap `value(alias = ...)` attributes are present and correct (`identity.rs:124,130`), the library-side `parse` aliases are tested in both `identity.rs` and `tests/library_provider_api.rs`, and the plan's risk table deliberately assigned clap-side aliases to HITL items 19/20. Plan-compliant as-is — but it's ~5 lines and this diff is exactly the kind that could break it, so fold it in if touching the file anyway.
- **Gemini's `#[allow(dead_code)]` note and the `http`-visibility comment suggestion.** Both correct and both benign. The allows are the right pragmatic fix for lib-internal helpers with no `--no-default-features` callers; the design already prescribes `#[doc(hidden)] pub mod http` as the "internal" signal.

## False Positives / Tooling Artifacts

- **Codex: "I could not run Cargo checks."** Sandbox limitation, not a code defect. Full matrix run above is green: 492 tests (up from the 489 baseline), no-default-features passes, fmt and clippy `-D warnings` clean.
- **Gemini finding #2**, which frames the `OPENAI_SUPPORTED_MODELS` relocation as "graceful," is incorrect — it inspected the library side and did not notice `openai.rs` retained a duplicate. Codex's read is the accurate one.

## Recommendation

**PROCEED_WITH_FIXES.** The extraction itself is sound and matches the design closely: type identity is preserved through facade re-exports rather than re-declaration (lesson L1), the `clap` derive is correctly `cfg_attr`-gated with aliases intact, the injected-fetcher seam keeps the library hermetic, and the whole build matrix is green with three net-new tests. Nothing here is a pivot or a design divergence. Three bounded fixes stand between this and a PR, each confined to one file with no design change: unify the OpenAI model list behind a single library const that `openai.rs` consumes (making the two lists diff-visible and testable); restore the four deleted `select`/`pick_provider_id` tests into the facade test module and repair the now-dangling `openai.rs:290` comment; and add the ST6-mandated `auth_method` test. Commit the two dirty files as part of that iteration. Gemini's PASS is too generous — it validated structure without noticing that the split reintroduced the exact duplication CLO-596 was chartered to eliminate and dropped a prior ticket's regression guard, so weight Codex's report over Gemini's on this one.
