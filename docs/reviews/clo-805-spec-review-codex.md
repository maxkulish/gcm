# Spec Review: clo-805

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Reviewed**: 2026-11-21
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The spec matches CLO-805. Filtering changes which rows are visible, SPACE changes the checked set, and ENTER submits that set. The proposed confirmation addresses the reported failure at the point where the user can still correct it. The spec also correctly distinguishes this from CLO-802, which governs the subsequent Default model prompt. This was a static review of the working-tree spec against source at `4c04ce0`. I scanned all 315 Markdown files under `docs/` and inspected the relevant docs, source, and pinned cliclack implementation. I did not run the test suite or PTY cases. [CLO-805 spec](/Users/mk/Code/gcm--fix-clo-805-enable-models/specs/2026-11-21-clo-805-enable-models-unchanged-confirm.md:7) · [Incident history](/Users/mk/Code/gcm--fix-clo-805-enable-models/docs/DEPENDENCIES.md:46)

## 2. Acceptance Criteria Review

**Strong**. AC1–AC6 define observable behavior for canonical set equality, Yes, No, changed selections, first-time setup, and cancellation.

**Gaps**. AC9 presses SPACE on the filtered row immediately after No. A fresh cliclack picker starts with an empty filter and its cursor on the first row. That SPACE can toggle a different model, including the incumbent. Specify that No reopens an unfiltered picker and that the test retypes the query before SPACE. Require the exact persisted set `{A, B}` and default `B`. Checking only that `B` is present would let a replacement regression pass. [AC9](/Users/mk/Code/gcm--fix-clo-805-enable-models/specs/2026-11-21-clo-805-enable-models-unchanged-confirm.md:34) · [Picker initialization](/Users/mk/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cliclack-0.5.4/src/multiselect.rs:46)

## 3. Constraints Check

**Aligned**. The public API restriction, canonical comparison, binary-only placement, and preservation of SPACE behavior fit the existing wizard and library boundary. [Library boundary ADR](/Users/mk/Code/gcm--fix-clo-805-enable-models/docs/adrs/002-library-boundary.md:87)

**Concerns**. The PTY fixture needs a valid `config.toml` with mode `0600`. The loader ignores a seed with group or other permissions, so the confirmation branch would never be reached. The existing no-key fallback provides a deterministic offline catalog without a mock request. Seed Groq without a key, unset `GROQ_API_KEY` for the child, and skip the key prompt. [Config loader](/Users/mk/Code/gcm--fix-clo-805-enable-models/src/config/mod.rs:227) · [No-key fallback](/Users/mk/Code/gcm--fix-clo-805-enable-models/src/provider/models.rs:86)

## 4. Decomposition Quality

**Well-scoped**. The confirmation and rendering helpers can be developed independently. Each looks feasible within two hours once the retry behavior is specified.

**Issues**. Both tasks change the same `interact()` call. Assign integration of that call site to one task and require the PTY task to validate the combined flow. Include fixture setup and a baseline failure check in the PTY estimate. [Decomposition](/Users/mk/Code/gcm--fix-clo-805-enable-models/specs/2026-11-21-clo-805-enable-models-unchanged-confirm.md:66)

## 5. Evaluation Coverage

**Covered**. Unit tests suit set comparison and message formatting. PTY tests suit keyboard handling, prompt ordering, and persistence.

**Gaps**. The hint helper tests cannot establish that the hint actually renders in Active and Error states or disappears on Submit and Cancel. Compare the real renderer with cliclack’s default output. The proposed radio comparison also cannot detect a leaked checkbox override because radios use a different renderer. Verify theme cleanup at the picker boundary and exercise both Esc and Ctrl-C with unchanged config. Schedule a non-skipped PTY run against a freshly built binary. The acceptance script reuses an existing release binary, and `make check` and CI omit the PTY script. [Evaluation table](/Users/mk/Code/gcm--fix-clo-805-enable-models/specs/2026-11-21-clo-805-enable-models-unchanged-confirm.md:74) · [Acceptance runner](/Users/mk/Code/gcm--fix-clo-805-enable-models/scripts/acceptance.sh:12) · [Makefile](/Users/mk/Code/gcm--fix-clo-805-enable-models/Makefile:121) · [CI](/Users/mk/Code/gcm--fix-clo-805-enable-models/.github/workflows/ci.yml:38)

## 6. Codebase Alignment

**Violations**. None at the provider boundary. `Backend` is lok’s contract. gcm uses a synchronous `Provider` trait with `ProviderError` and `ErrorKind`, and the wizard uses `GcmError`. [Provider trait](/Users/mk/Code/gcm--fix-clo-805-enable-models/src/provider/facade.rs:43) · [Architecture ADR](/Users/mk/Code/gcm--fix-clo-805-enable-models/docs/adrs/002-library-boundary.md:63)

**Alignment**. The save remains after Default model selection, and No can reuse the fetched catalog. The spec currently routes every confirmation I/O error through cancellation. `confirm` can also return terminal errors, so use `wizard_cancelled()` for `Interrupted` and `wizard_io()` for other errors. [Wizard save path](/Users/mk/Code/gcm--fix-clo-805-enable-models/src/config/facade.rs:250) · [Prompt error handling](/Users/mk/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cliclack-0.5.4/src/prompt/interaction.rs:171)

## 7. Blind Spots

After scrolling past the first 15 rows, narrowing the filter can reset the cursor while leaving the page offset past the filtered results. The renderer then skips the highlighted row, so the theme hook cannot display its hint. This is a pre-existing cliclack limitation identified from source. Scope AC7 to rendered rows and document the limitation if pagination stays outside CLO-805. [Wizard row limit](/Users/mk/Code/gcm--fix-clo-805-enable-models/src/config/facade.rs:238) · [Cursor and paging logic](/Users/mk/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cliclack-0.5.4/src/multiselect.rs:130) · [Render loop](/Users/mk/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cliclack-0.5.4/src/multiselect.rs:201)

## 8. Verdict

**NEEDS_REVISION**

## 9. Actionable Feedback

1. **Required.** Fix AC2, AC9, and evaluation case 6 together. After No, wait for the reopened picker, retype the filter, verify B is highlighted and unchecked, then press SPACE and ENTER. Assert arrival at Default model without another confirmation and verify exactly `{A, B}` with `B` as default.
2. **Required.** Specify the private offline fixture and require a non-skipped run against a fresh build. Confirm that the pre-fix binary fails at the missing confirmation.
3. **Recommended.** Record the confirmation default as a product decision. If Yes is selected, document that a second ENTER accepts the old set and test that behavior explicitly. Add renderer coverage and narrow the hint guarantee to rows cliclack actually renders.
