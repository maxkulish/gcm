# Spec Review Synthesis: clo-497

**Synthesized**: 2026-06-24
**Pipeline**: lok spec-review

---

Both external reviewers returned `APPROVE_WITH_SUGGESTIONS`. Note: Ollama was flagged `success=false` by the harness but returned a complete, coherent review, so I cross-referenced it while marking the status. Claude fallback was correctly skipped.

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | **Archive contents underspecified.** AC-2 names only the `gcm` binary; neither the `.tar.gz` structure nor whether `LICENSE`/`README` are bundled is defined. README extraction instructions depend on this. (`license = "MIT"` in Cargo.toml implies a LICENSE file exists to include.) | Medium |
| 2 | **Tag ↔ Cargo.toml version parity is too weak.** Currently a *Prefer*. Since `gcm --version` reads `CARGO_PKG_VERSION`, a mismatch ships a stale version string. Both want it elevated to **Must** plus a test/assertion that the tag matches `Cargo.toml`. | Medium |
| 3 | **No explicit version-accuracy test.** §5 lacks a check that `gcm --version` output matches the release tag (e.g. `gcm 0.1.0 (abc1234)`). | Low-Med |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| - | None substantive | Reviews complement rather than conflict | Same | Skipped |

The two reviews are additive; no direct contradictions. The only one-sided strong claim is GLIBC/musl (Gemini), which Ollama simply did not raise rather than oppose - see Novel Insights.

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | **Linux GLIBC dynamic-linking trap.** `*-linux-gnu` built on modern Ubuntu runners links against that runner's glibc and crashes on older distros (Ubuntu 20.04, Debian Bullseye). Recommends static `musl` targets (`x86_64`/`aarch64-unknown-linux-musl`) - trivial since gcm has no C deps. | Gemini | **High** |
| 2 | **Missing `permissions: contents: write`.** Modern repos default `GITHUB_TOKEN` to read-only; release creation/asset upload will fail without this block. | Gemini | High |
| 3 | **Release transactionality.** Per-matrix-target release creation can fragment into partial/broken releases if a runner fails. Recommends two-stage pipeline: matrix builds → `upload-artifact`, then single `release` job with `needs:` that assembles one unified release. | Gemini | Medium |
| 4 | **Rollback depends on `/opt/script/git-commit-ai.sh` existing.** Cutover guide (AC-6) must explicitly tell users to leave the legacy dir intact or rollback breaks. | Gemini | Med |
| 5 | **Idempotent re-run not an AC.** Re-running release on an existing tag should update, not duplicate, assets - mentioned in edge cases but not testable as an AC. | Ollama | Medium |
| 6 | **No dry-run validation path.** First release should use `workflow_dispatch` on a pre-release tag before cutting official `v0.1.0`. | Ollama | Medium |
| 7 | **README must link the cutover guide.** AC-5 creates the guide but no AC verifies README links to it. | Ollama | Low-Med |
| 8 | **Checksum command portability.** macOS `shasum -a 256 -c` vs Linux `sha256sum -c`; README should document both. | Ollama | Low |
| 9 | **macOS Gatekeeper note** should move from *Prefer* into README install steps (Must). | Ollama | Low |
| 10 | **Release notes content** (empty vs CHANGELOG link vs auto-generated) is unaddressed. | Ollama | Low |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** (both reviewers concur; no blocking issues)

## Priority Actions

Ordered by severity, agreement items prioritized:

1. **Decide Linux target strategy (GLIBC).** Switch Linux builds to `musl` static targets, or pin to an old Ubuntu runner. Highest-severity item - silently ships binaries that crash on common distros. *(Gemini)*
2. **Add `permissions: contents: write`** to `release.yml` - without it the first release run fails outright. *(Gemini)*
3. **Specify archive contents in AC-2** - exact tarball layout (`gcm` + `LICENSE`, optional `README`) and document extraction/PATH placement in README. *(Both)*
4. **Elevate version parity to Must** and add a workflow assertion + a §5 test that `gcm --version` matches the tag. *(Both)*
5. **Adopt two-stage release pipeline** (matrix build → artifacts → single assembling release job) for transactionality + natural idempotency. *(Gemini + Ollama idempotency)*
6. **Add idempotency AC** and a **`workflow_dispatch` dry-run step** for the first release. *(Ollama)*
7. **Cutover guide hardening:** instruct users to keep `/opt/script/` intact for rollback; add README→cutover-guide link AC; portable checksum commands; move Gatekeeper note into install steps. *(Gemini + Ollama)*
8. **Minor:** define GitHub Release notes content; pin `softprops/action-gh-release` to a minimum version. *(Ollama)*

Items 1 and 2 are the only ones I'd treat as near-blocking despite the approve verdict - both produce a release pipeline that fails or ships broken artifacts on first real use.
