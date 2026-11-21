# Spec Review: clo-801

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Reviewed**: 2026-11-21
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The spec matches CLO-801. It explains why echoing paths makes grouping slow and identifies the preflight output-cap check that CLO-797 added. The investigation’s 85.5s result is measured wall time. The ~36K output-token figure is an estimate based on the input inventory. The current CLI announces fallback on stderr at the default log level, so “silently loses the grouping” overstates the default behavior. [Investigation](/Users/mk/Code/gcm--feat-clo-801-group-index/docs/investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md:144) · [Fallback path](/Users/mk/Code/gcm--feat-clo-801-group-index/src/main.rs:1115)

## 2. Acceptance Criteria Review

**Strong.** AC1–AC5 define zero-based indices, mixed integer and path replies, numeric filenames, and rejection of invalid references. AC9 gives “well inside 60s” a measurable threshold of 30s.

**Gaps.** AC9 and AC10 need an exact success predicate. `--plan-only` can return `status: "plan"` after fallback, and it can also return a cached grouping plan without a provider call. Require `mode: "plan_only"`, `cached: false`, and a validated partition of all 515 expected paths. For AC10, require the Anthropic request to use `max_tokens: 4096`. AC11 establishes parser compatibility but needs evidence that an actual Ollama cloud model follows the new integer prompt. [Acceptance criteria](/Users/mk/Code/gcm--feat-clo-801-group-index/docs/specs/2026-11-21-clo-801-group-index.md:99) · [Cache lookup](/Users/mk/Code/gcm--feat-clo-801-group-index/src/main.rs:492) · [Fallback preview](/Users/mk/Code/gcm--feat-clo-801-group-index/src/main.rs:917) · [CLO-517 live criterion](/Users/mk/Code/gcm--feat-clo-801-group-index/docs/hotfix/2026-06-29-ollama-cloud-plan-parse-failure.md:89)

## 3. Constraints Check

**Aligned.** Resolve against the same ordered, privacy-filtered `changed` slice used to build the inventory. Persist resolved paths, keep the cache format version, and bump the fingerprint version. The spec also correctly keeps the message path and provider limits outside the change. [Inventory construction](/Users/mk/Code/gcm--feat-clo-801-group-index/src/diff.rs:263) · [Cache versioning](/Users/mk/Code/gcm--feat-clo-801-group-index/src/cache.rs:19)

**Concerns.** AC6 requires cache tests to pass unmodified, but the existing golden test pins both fingerprint version 3 and its digest. Allow those expectations to change and preserve the cache-format and validator assertions. Likewise, preserve the path-string parser fixtures and assertions while allowing them to resolve the new wire type before comparing paths. [Cache golden](/Users/mk/Code/gcm--feat-clo-801-group-index/src/cache.rs:372) · [Parser tests](/Users/mk/Code/gcm--feat-clo-801-group-index/src/plan.rs:676)

## 4. Decomposition Quality

**Well-scoped.** Inventory indexing is an independent, small change. The spec correctly identifies the dependency from the wire type through the six providers to `build_plan`.

**Issues.** Treat the wire-type cutover as one compiling integration step, with separately bounded work for budget policy and regression tests. Add `tests/grouping_budget.rs` explicitly. Its existing 515-file Anthropic test expects a preflight refusal and only a message request, which directly conflicts with AC7. Give live verification its own owner and credential prerequisites so the two-hour estimate applies to implementation work. [Decomposition](/Users/mk/Code/gcm--feat-clo-801-group-index/docs/specs/2026-11-21-clo-801-group-index.md:161) · [Existing Anthropic integration test](/Users/mk/Code/gcm--feat-clo-801-group-index/tests/grouping_budget.rs:423)

## 5. Evaluation Coverage

**Covered.** The table gives practical unit tests for schema shape, mixed references, numeric filenames, out-of-range and duplicate indices, and `commits` recovery.

**Gaps.** The proposed timing source cannot verify AC9. JSON mode disables the ticker, and `finish()` emits no completion duration. Measure around `generate_plan` or explicitly bound total CLI wall time. For a comparable before-and-after result, use the same provider and model on both builds. The historical run used `google`, while AC9 selects `vertex`. Promote the redacted and renamed path cases from the edge-case list into a CLI test that checks the transmitted inventory and resulting path-based JSON. [Progress implementation](/Users/mk/Code/gcm--feat-clo-801-group-index/src/ui.rs:353) · [Historical provider](/Users/mk/Code/gcm--feat-clo-801-group-index/docs/investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md:198) · [Edge cases](/Users/mk/Code/gcm--feat-clo-801-group-index/docs/specs/2026-11-21-clo-801-group-index.md:214)

## 6. Codebase Alignment

**Violations.** The proposed return type changes the internal `Provider::generate_plan` contract. Elevate that signature change from **Prefer** to an explicit contract and update the trait documentation. The applicable error types are `ProviderError`, `ErrorKind`, and `BuildError`. Keep malformed wire data on `ErrorKind::Deserialize` and assign `PlanValidation` directly to resolution and partition failures. The current substring classifier can mislabel a `DuplicateFile` if the filename itself contains `plan parse error`. [Provider trait](/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/facade.rs:43) · [Error mapping](/Users/mk/Code/gcm--feat-clo-801-group-index/src/main.rs:675) · [Substring classifier](/Users/mk/Code/gcm--feat-clo-801-group-index/src/output.rs:301)

**Alignment.** The trait and plan types are binary-internal. The proposed wire type fits that boundary, and the existing path-based validation, cache, and commit flow can consume the resolved `Plan`. [Library boundary](/Users/mk/Code/gcm--feat-clo-801-group-index/docs/adrs/002-library-boundary.md:238)

## 7. Blind Spots

- **The output-cap check covers only the index echo.** At 1,500 files, the proposed formula estimates 3,195 tokens for the indices. Adding the existing 1,024-token overhead gives a 4,219-token completion reserve, but the echo-only check admits it under Anthropic’s 4,096 cap. Define whether the guard estimates full completion or is intentionally a heuristic, and test the boundary and truncation behavior. [Budget implementation](/Users/mk/Code/gcm--feat-clo-801-group-index/src/budget.rs:423)
- **A valid partition can still group the wrong files.** A permutation of in-range indices passes validation. Cover a deliberately nonalphabetical inventory with known group membership, and preserve CLO-517’s first-group-only `message` alias so recovered replies continue to regenerate later commit messages. [Partition validator](/Users/mk/Code/gcm--feat-clo-801-group-index/src/plan.rs:421) · [Recovery rules](/Users/mk/Code/gcm--feat-clo-801-group-index/src/plan.rs:238)

## 8. Verdict

**NEEDS_REVISION**

The design fits the codebase. The spec needs a runnable latency check, a real Ollama cloud verification step, and a test plan that reconciles the existing Anthropic and cache expectations. This was a read-only review against source at `4c04ce0` and the working-tree spec. I verified that `f2e2ad0` has 513 additions and two modifications. I did not run builds or live provider calls.

## 9. Actionable Feedback

1. **Make AC9–AC11 executable.** Specify a fresh fixture, explicit provider and model, effective timeout, elapsed-time measurement, and exact JSON assertions. Add an owner-run Ollama cloud smoke test that confirms integer assignments produce a valid plan.
2. **Update the integration and cache tasks.** Turn the 515-file Anthropic refusal test into a successful `tool_use` test. Keep a separate over-cap case that checks zero grouping requests, and update the fingerprint golden alongside the version bump.
3. **Pin the boundary behavior.** State the output-cap policy, require checked index conversion and lookup, and add tests for overflow, duplicates within a group and across mixed references, and the `Deserialize` versus `PlanValidation` split.
