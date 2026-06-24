# Spec Review: clo-497

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-06-24
**Pipeline**: lok spec-review

---

Now I have all the context needed. Let me provide my comprehensive specification review.

---

## 1. Problem Statement Assessment

**The problem statement is clear, complete, and accurate.**

The spec correctly identifies two distinct gaps:
1. **Missing release pipeline** - No cross-platform binary distribution exists; current README install requires building from source
2. **Missing cutover documentation** - Users need guidance to migrate from the legacy bash aliases to the Rust binary

The spec correctly references the pre-resolved alias matrix from the PRD (lines 370-384, confirmed in my reading) and ADR-001 decisions (#5, #7, #8, #13), explicitly stating it **transcribes existing decisions rather than re-opening them**. This is good - it prevents scope creep.

**Alignment with Linear task:** The task description mentions FR-42/43/44 and deliverables (`release.yml`, README install, `cutover-from-bash.md`). The spec covers all three.

**Self-contained:** Yes. The problem is stated without requiring external context, though it appropriately references the PRD for the alias matrix.

## 2. Acceptance Criteria Review

**Strong:**
- **AC-1 through AC-7 are specific and measurable** - Each can be verified with a concrete action (workflow inspection, `gh release view`, running binaries, grep for `/opt/script`)
- **FR traceability is explicit** - Each AC maps to FR-42/43/44
- **The test table (§5) maps directly to ACs** - Clear pass/fail criteria

**Gaps:**

1. **AC-2 lacks archive structure specification** - It says "packaged as `gcm-<version>-<target>.tar.gz`" but doesn't specify the archive structure (single binary? docs? LICENSE?). The README install will need to tell users to extract and place the binary.

2. **AC-3 "runs on macOS and Linux" is underspecified for cross-verification** - The runner-native test can only verify one target per CI run. The spec acknowledges this ("CI-provable for the runner's own target; the cross-OS human install is the checkpoint in §5"), but could clarify that **Test #3 only covers the runner's architecture**, not all four targets.

3. **Missing AC for release workflow idempotency** - Edge case §5 mentions "Re-running the release on an existing tag — `action-gh-release` should update, not duplicate, assets" but this isn't an explicit AC. It should be testable.

4. **No AC for README accuracy about the alias matrix** - The README must link to the cutover guide, but there's no AC verifying the link exists and points to the right place.

## 3. Constraints Check

**Aligned with codebase patterns:**

1. **Must: native runners per target** - Correctly maps runner types to targets (`macos-13` for x86_64-apple-darwin, etc.). GitHub-hosted runner availability for `ubuntu-24.04-arm` is noted as needing escalation if unavailable.

2. **Must: use existing release profile** - Confirmed in `Cargo.toml`:
   ```toml
   [profile.release]
   strip = true
   lto = true
   codegen-units = 1
   panic = "abort"
   ```

3. **Must: `fetch-depth: 0`** - Confirmed necessary in `build.rs` which runs `git rev-parse --short HEAD` and falls back to `"unknown"` without git history. The existing `ci.yml` already uses `fetch-depth: 0`.

4. **Must-not: modify any `src/**`** - Correctly scoped as release-engineering + docs only.

5. **Must-not: hardcode `/opt/script`** - Verified that current README has no `/opt/script` references (install section says `cargo build --release`).

**Concerns:**

1. **`ubuntu-24.04-arm` runner availability** - The Escalate clause correctly identifies this. However, the spec should note that `ubuntu-latest` can run `x86_64-unknown-linux-gnu` builds directly, but arm64 Linux cross-compilation or native builds may need verification. The "Escalate" correctly routes this decision.

2. **`softprops/action-gh-release` preference** - Good choice, but the spec should note the minimum version to use for reproducibility.

3. **Missing constraint: GitHub Release creation timing** - The workflow should create the Release (if not exists) before or concurrent with asset upload. The action handles this, but it's worth noting.

## 4. Decomposition Quality

**Well-scoped:**

- All four sub-tasks are **independent** (can be implemented in parallel except for the README link coupling)
- Each sub-task is **well-sized** - release workflow is the largest but is still bounded
- Dependencies are clearly stated (README edits from #3 and #4 should be combined)

**Issues:**

1. **Sub-task #2 (Version hygiene) is underspecified** - "bump `Cargo.toml` `version` to the intended tag" needs clarification:
   - Who decides the version number? The spec says "e.g. `0.1.0`" but doesn't specify the release process for version selection
   - Is this a manual edit before tagging, or should it be automated?

2. **Missing sub-task: Pre-release validation** - The spec mentions `workflow_dispatch` for dry-run, but there's no sub-task for **creating a test tag** or **validating the workflow locally** before the first real release.

3. **Sub-task #4 missing explicit link to PRD lines** - While the spec says "transcribed from the PRD lines 376-384", the cutover guide should also include the **invocation examples** (`gcm --provider=groq` vs `--provider=groq --model=...`).

## 5. Evaluation Coverage

**Covered:**

- All 9 tests map to specific ACs
- Test #1-4 cover the release workflow mechanics
- Test #5 covers `cargo install --git`
- Test #6-8 cover documentation accuracy
- Test #9 covers CI non-modification

**Gaps:**

1. **Missing test for checksum verification on downloaded artifacts** - Test #4 verifies checksum calculation, but not that a user can verify the downloaded archive matches the published checksum.

2. **Missing test for version stamp accuracy** - `gcm --version` output should match the git tag. This is implied but not explicit.

3. **No test for archive extraction** - The user experience of extracting `gcm-*-darwin.tar.gz` and placing it on PATH should be documented and tested (README verification).

4. **Edge cases partially covered but missing one** - "Re-running on existing tag" is mentioned but not in the test table.

## 6. Codebase Alignment

**Violations found: None.** The spec correctly avoids touching `src/` and follows established patterns.

**Alignment with established patterns:**

1. **Release profile already defined** - `Cargo.toml` has the correct `[profile.release]` section with `strip`, `lto`, `codegen-units=1`, `panic="abort"`.

2. **`build.rs` already handles version stamping** - The spec correctly notes `fetch-depth: 0` is needed for SHA resolution.

3. **CI workflow exists and is correct** - `.github/workflows/ci.yml` already tests on `ubuntu-latest` and `macos-latest` with `fetch-depth: 0`.

4. **Error handling patterns** - The spec doesn't need to modify `error.rs` (no new error types needed for release engineering).

5. **Documentation structure** - `docs/guides/` directory exists (with `.gitkeep`), ready for the cutover guide.

**Verified references:**

- **PRD alias matrix (lines 370-384)** - Confirmed correct reference. The matrix is complete and accurate.
- **ADR-001 decisions** - Confirmed #5 (Groq default), #7 (`gcmo`→OpenAI), #8 (`gcml`→Ollama), #13 (Cerebras dropped) are correctly referenced.

## 7. Blind Spots

**What the specification misses:**

1. **No `.github/workflows/release.yml` bootstrap consideration** - The first run of this workflow will have no prior workflow to reference. Need to ensure the `softprops/action-gh-release` action is correctly configured for first-time use.

2. **Missing LICENSE file consideration** - Release archives typically include a LICENSE file. The spec doesn't mention whether to include it in the tarball or verify its presence. (Cargo.toml shows `license = "MIT"` so a LICENSE file should exist.)

3. **No consideration of binary stripping for debug symbols** - The release profile has `strip = true`, but this could be an issue for users wanting to debug. Not critical, but worth noting.

4. **Missing consideration for GitHub Release notes content** - The spec doesn't address what goes in the Release body/notes. Should it be auto-generated from CHANGELOG, manually written, or left empty?

5. **No rollback plan for a bad release** - If a released binary is broken, what's the remediation? (Tag deletion? New patch release? `yank` doesn't exist for GitHub Releases.)

6. **macOS Gatekeeper note in Prefer is good** - But should be in the README install instructions, not just "Prefer".

7. **Archive content consistency** - The spec doesn't specify whether to include a README/LICENSE in the archive itself, or just the binary.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is fundamentally sound and ready for implementation. The problem is clear, the acceptance criteria are mostly specific and testable, constraints align with codebase patterns, and decomposition is appropriate. The blind spots are minor and addressable without blocking work.

## 9. Actionable Feedback

**Priority 1 - Must address before implementation:**

1. **AC-2: Specify archive structure** - Add that each archive contains exactly the `gcm` binary (and optionally LICENSE/README). Document the extraction path in README.

2. **Add Test #10 for version accuracy** - Verify `gcm --version` output matches the tag (e.g., `gcm 0.1.0 (abc1234)`).

3. **Sub-task #2: Clarify version bump process** - Specify whether this is:
   - Manual edit before tagging (simplest)
   - Automated in the release workflow
   - Part of a separate release preparation step

**Priority 2 - Should address for robustness:**

4. **Add AC-8 for idempotent releases** - "Re-running the release workflow on an existing tag updates (not duplicates) assets."

5. **Specify archive contents explicitly** - Add to AC-2: "Each archive contains: the `gcm` binary, optionally LICENSE (if present in repo)."

6. **README must include Gatekeeper workaround** - Move from "Prefer" to "Must" or include in the install path documentation.

7. **Test the `workflow_dispatch` flow** - Add a note that the first release should use `workflow_dispatch` on a pre-release tag to validate before creating the official `v0.1.0` tag.

**Priority 3 - Nice to have:**

8. **Release notes template** - Consider what content appears on the GitHub Release page. Even a simple "See CHANGELOG.md" link would be better than empty.

9. **Link the cutover guide from README's alias documentation** - AC-5 creates the guide, but there's no AC for linking it from the README's existing content (currently README has no alias section).

10. **Consider checksum verification command** - The test table shows `shasum -a 256 -c` but Linux uses `sha256sum -c`. The README should document both or use a portable approach.
