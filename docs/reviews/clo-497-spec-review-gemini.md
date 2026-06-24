# Spec Review: clo-497

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-06-24
**Pipeline**: lok spec-review

---

I have completed my review of the specification. Below is the detailed architectural assessment.

## 1. Problem Statement Assessment
The problem statement is clear, complete, and highly self-contained. It correctly maps the two major remaining gaps for the `v1` milestone (lack of a compiled release pipeline and legacy shell alias coupling) to their respective Linear tasks and PRD/ADR decisions. It makes no unstated assumptions, aligns perfectly with the target repository's current state, and respects the existing code-level and architecture-level boundaries.

## 2. Acceptance Criteria Review
**Strong**: 
* Specific, measurable, and highly testable criteria (AC-1 through AC-7).
* Covers the critical documentation requirements (AC-4, AC-5, AC-6) preserving rollback safety and removing hardcoded absolute personal paths.
* A robust and executable verification matrix is provided in Section 5 with concrete shell commands.

**Gaps**:
* **GHA Token Permissions**: Modern GitHub repositories default to read-only `GITHUB_TOKEN` permissions. The spec does not explicitly specify that the `.github/workflows/release.yml` must request `permissions: contents: write` to allow the release creation and asset upload.
* **Release Transactionality**: If one of the matrix build runners fails or experiences latency, creating releases on-the-fly per matrix target can result in broken, partial, or fragmented GitHub Releases.
* **Release Archive Contents**: The criteria (AC-2) only mentions packaging the executable `gcm` binary. It does not specify whether foundational project metadata files (e.g., `LICENSE` and a minimal `README.md`) should be bundled into each `.tar.gz`.

## 3. Constraints Check
**Aligned**:
* Native runners match the desire to avoid cross-compilers (`cross` / QEMU) and preserve high compilation speeds.
* Preserving the existing aggressive release profile in `Cargo.toml` is excellent for keeping binary size and cold starts minimized.
* Restricting changes from touching the `src/` codebase is a superb boundary constraint.

**Concerns**:
* **Linux GLIBC Mismatch**: Compiling for `x86_64-unknown-linux-gnu` or `aarch64-unknown-linux-gnu` on `ubuntu-latest` or `ubuntu-24.04-arm` will dynamically link the binary against that runner's specific version of `glibc` (often glibc 2.35 or 2.39). This will cause the binary to crash with a dynamic linker error on older systems (such as Debian Bullseye or Ubuntu 20.04).
* **Loose Cargo.toml Version Constraint**: Aligning `Cargo.toml` `version` with the git tag is currently listed only as a *Prefer* constraint. If they mismatch, `gcm --version` (which reads from `CARGO_PKG_VERSION`) will report the outdated/unbumped version string, even if the git short SHA is resolved.

## 4. Decomposition Quality
**Well-scoped**:
* All 4 sub-tasks are small, independent, and easily completed within the ~2-hour timeframe.
* Correctly sequences the overlapping edits to `README.md` to avoid git conflicts.

**Issues**:
* None. The scope of decomposition is exceptional.

## 5. Evaluation Coverage
**Covered**:
* Excellent 1:1 test matrix coverage in §5 mapping to all major acceptance criteria.
* Clear, executable CLI snippets make automated and manual smoke tests straightforward.

**Gaps**:
* No test scenario checking for tag-vs-Cargo.toml version parity.
* No validation step verifying the compatibility of the generated Linux binary on older `glibc` environments.

## 6. Codebase Alignment
**Violations**:
* None. The spec wisely chooses to perform no modifications to the Rust source code (`src/` behavior is unchanged).

**Alignment**:
* The spec correctly honors `build.rs` requirements for a complete Git history by mandating `fetch-depth: 0` in the CI checkout.
* The spec correctly bypasses potential errors with `anyhow` or `BackendErrorKind` by aligning to the existing custom `GcmError` and `ProviderError` structures (which are used instead of foreign error libraries in this codebase).

## 7. Blind Spots
* **Dynamic Linking on Linux**: Distributing pre-compiled `gnu` targets built on modern runners introduces host dependency issues. To guarantee maximum compatibility across Linux distributions, the spec should favor compiling against `musl` (`x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`) or enforcing compilation on older Ubuntu runners.
* **Rollback Shell Path Assumption**: The rollback documentation (AC-6) depends on `/opt/script/git-commit-ai.sh` being present. If a developer proactively deletes `/opt/script/` during cutover, rollback will fail. The guide should explicitly instruct users to leave the legacy directory intact.

## 8. Verdict
**APPROVE_WITH_SUGGESTIONS**

## 9. Actionable Feedback

1. **Mandate a Two-Stage Release Pipeline**: 
   Ensure `.github/workflows/release.yml` uses a two-stage approach to ensure transactionality. Step 1: Compile binaries on the matrix runners and upload them as workflow run artifacts using `actions/upload-artifact`. Step 2: A single `release` job with `needs: [build]` downloads all artifacts, computes checksums, and creates a single unified draft/published GitHub Release.
2. **Add Explicit GHA Permissions Block**: 
   Add a constraint that the release workflow must define:
   ```yaml
   permissions:
     contents: write
   ```
3. **Mitigate Linux GLIBC Compatibility Issues**: 
   To prevent target system dynamic linker crashes, compile the Linux binaries using the `musl` targets (`x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`). Since `gcm` has zero system library C-dependencies, a static musl binary is trivially compilable and will run anywhere.
4. **Bundle License and README**: 
   Add a constraint that each target `.tar.gz` archive must include the executable `gcm` along with the project's `LICENSE` and a minimal `README.md` or release note text file.
5. **Elevate Version Parity to a Must Constraint**: 
   Elevate "Align Cargo.toml version with the release tag" to a **Must** constraint, and add a quick assertion step in the release workflow to verify that `refs/tags/v${version}` matches the `version` defined in the checked-out `Cargo.toml` before proceeding with compilation.
