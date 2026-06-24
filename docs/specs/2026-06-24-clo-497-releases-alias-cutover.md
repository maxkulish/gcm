# Spec: Cross-platform releases + bash→Rust alias cutover (CLO-497)

**Created**: 2026-06-24
**Linear**: [CLO-497](https://linear.app/cloud-ai/issue/CLO-497) · covers FR-42, FR-43, FR-44
**Estimated scope**: M (1 new CI workflow, README edits, 1 new guide doc, optional Cargo.toml/build.rs touch — ~4 files, 4 sub-tasks)

## 1. Problem Statement

`gcm` is feature-complete (CLO-485…CLO-496 + CLO-514 all merged) but is still only installable by building from source on the developer's own machine, and the primary user's shell still drives the legacy bash tool. The bash→Rust migration cannot be declared done until two things exist:

1. **Installable cross-platform binaries.** There is no release pipeline. `.github/workflows/ci.yml` builds and tests on `ubuntu-latest` + `macos-latest` but produces no published artifact. FR-42 requires release builds for **macOS + Linux on x86_64 + arm64** (4 target triples). Today an adopter on any machine must clone the repo and run `cargo build --release` (README "Install" section, lines 43-49).

2. **A documented, reversible cutover.** The user's `~/.zshrc` aliases (`gcmq`, `gcmq20`, `gcmq27`, `gcmg`, plus the commented `gcm`/`gcmc`) still point at `/opt/script/git-commit-ai.sh` (confirmed in `~/.zshrc:142-148`). FR-44 requires that repointing each alias to the Rust binary is documented and that rollback is a **one-line alias revert with the bash script left intact**. FR-43 requires the README to document install via a release binary and via `cargo install`, with **no assumption of `/opt/script`**.

The CLI already supports every flag the aliases need (`--provider`, `--model`, `--version`; confirmed in README "Usage" and `gcm --version` build-stamped via `build.rs`). The alias→invocation mapping is **already fully resolved** in `docs/prds/prd-gcm.md` "Alias Parity & Migration Matrix" (lines 370-384) and `docs/adrs/001-foundational-architecture-decisions.md` (#5 bare-`gcm` ships Groq default; #7 `gcmo`→OpenAI `gpt-4o-mini-2024-07-18`; #8 `gcml`→Ollama; #13 `gcmc`/Cerebras dropped). **This spec transcribes that decision into shippable artifacts; it does not re-open it.**

So the work is pure release-engineering + docs: a tag-triggered GitHub Actions release workflow that produces and publishes 4 verified binaries, README install instructions for binary + `cargo install`, and a cutover guide carrying the alias matrix and rollback steps. No `src/` behavior changes.

## 2. Acceptance Criteria

- [ ] **AC-1 (FR-42):** A tag-triggered release workflow `.github/workflows/release.yml` builds release binaries for all four targets: `x86_64-apple-darwin`, `aarch64-apple-darwin`, **`x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`** (static Linux builds — see Constraints for the GLIBC rationale).
- [ ] **AC-2 (FR-42):** Each build is packaged as a `gcm-<version>-<target>.tar.gz` archive **containing the `gcm` binary plus the `LICENSE` file** (root of the tarball, no nested dir); each archive has a matching `.sha256` checksum; and all archives + checksums are attached to a **single** GitHub Release for the pushed tag.
- [ ] **AC-3 (FR-42):** The released binary runs on macOS and Linux: the build job executes the freshly-built binary (`gcm --version`, `gcm --help`) on each target's native runner **before** packaging/publishing, so a broken native artifact fails the release. (The cross-OS human install is the checkpoint in §5.)
- [ ] **AC-4 (FR-43):** README documents **two** install paths — (a) download a release binary + verify checksum (showing **both** `shasum -a 256 -c` for macOS and `sha256sum -c` for Linux) + place on PATH + the macOS Gatekeeper unquarantine step, and (b) `cargo install --git https://github.com/maxkulish/gcm --locked` — with **no hardcoded `/opt/script` install assumption in the README**. (The cutover guide legitimately names `/opt/script/git-commit-ai.sh` as the legacy/rollback target — that reference is required, not a violation.)
- [ ] **AC-5 (FR-44):** A cutover guide (`docs/guides/cutover-from-bash.md`) carries the complete alias migration matrix (`gcm`, `gcmq`, `gcmq20`, `gcmq27`, `gcmg`, `gcmo`, `gcml`; `gcmc` dropped; `gcms` unchanged/out-of-scope) mapping each old `/opt/script/git-commit-ai.sh` invocation to its `gcm …` equivalent, **and the README links to this guide.**
- [ ] **AC-6 (FR-44):** The cutover keeps the OLD alias block in `~/.zshrc` (commented, not deleted) so rollback is a **single revert** — uncomment the OLD block / comment the NEW block (or, in the optional `source`-file variant, toggle one line). The guide explicitly instructs the user to **leave `/opt/script/git-commit-ai.sh` in place** so the revert target still exists.
- [ ] **AC-7:** The existing `.github/workflows/ci.yml` is unchanged and still green (release is a separate workflow).
- [ ] **AC-8 (version parity):** `gcm --version` from a released artifact reports the tag's version number; the workflow **asserts** `Cargo.toml` `version` matches the pushed tag (minus the `v`) and fails the release on mismatch.
- [ ] **AC-9 (idempotency):** Re-running the release on an already-existing tag **updates** the release assets in place rather than creating duplicates or a second release.

**Verification method:** trigger the release workflow on a pre-release tag (`v0.1.0-rc.1`) or `workflow_dispatch` and inspect that 4 archives + 4 checksums land on **one** Release (`gh release view`); download the runner-native archive and run `--version`/`--help`; verify `--version` matches the tag; re-run the workflow on the same tag and confirm no duplicate assets; `cargo install --git --locked` into a temp root; grep docs for `/opt/script`; render the alias matrix against the PRD table for 1:1 parity. Full test table in §5.

## 3. Constraints

**Must:**
- **Static Linux builds via musl.** Use `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`, not `-gnu`. A `-gnu` binary built on a modern Ubuntu runner dynamically links that runner's glibc and crashes on older distros (Ubuntu 20.04, Debian Bullseye). gcm has **no C dependencies** (TLS is pure-Rust `rustls`, confirmed in `Cargo.lock`), so a fully static musl binary is clean and portable. Install the musl toolchain (`sudo apt-get install -y musl-tools`) **and** point cargo at it via `CARGO_TARGET_<TARGET>_LINKER=musl-gcc` — without the explicit linker, the static link can fail with `cannot find crti.o`.
- Build all four targets using **native runners per target** (no QEMU): `x86_64-unknown-linux-musl`→`ubuntu-latest`; `aarch64-unknown-linux-musl`→`ubuntu-24.04-arm`; `aarch64-apple-darwin`→`macos-latest`; `x86_64-apple-darwin`→`macos-15-intel` (the `macos-13` Intel image was retired; `macos-15-intel` is the current Intel runner).
- **Two-stage pipeline for release transactionality.** Stage 1: a build matrix produces per-target artifacts and uploads them via `actions/upload-artifact`. Stage 2: a single `release` job with `needs:` on the matrix downloads all artifacts and assembles **one** GitHub Release. This prevents a single runner failure from fragmenting into a partial/broken release and makes re-runs naturally idempotent.
- **Grant `permissions: contents: write`** to the release job. The default `GITHUB_TOKEN` is read-only on modern repos; without this, release creation/asset upload fails outright.
- **Version parity (hard).** Assert in the workflow that `Cargo.toml` `version` equals the pushed tag without its leading `v` (e.g. tag `v0.1.0` ⇒ `version = "0.1.0"`); fail the release on mismatch. `gcm --version` reads `CARGO_PKG_VERSION`, so a mismatch would ship a stale version string.
- Keep the release workflow **separate** from `ci.yml`; trigger on tag push matching `v*` plus `workflow_dispatch` (the dry-run rehearsal path).
- Use the existing release profile in `Cargo.toml` (`strip=true, lto=true, codegen-units=1, panic="abort"`) — do not weaken it.
- Checkout with `fetch-depth: 0` in the build jobs so `build.rs`'s `git rev-parse --short HEAD` resolves a real SHA (it falls back to `"unknown"` without git history; see `build.rs`).
- Each tarball contains the `gcm` binary **and the `LICENSE` file** at its root (no nested directory).
- The alias matrix in the guide must match `docs/prds/prd-gcm.md` lines 376-384 exactly (provider, model, target invocation). Transcribe, don't invent.
- The cutover guide must instruct the user to **keep `/opt/script/git-commit-ai.sh` intact** (rollback target) and keep the bash script (source `docs/tmp/git-commit-ai.sh`) untouched.
- README install steps must include the macOS Gatekeeper unquarantine step (`xattr -d com.apple.quarantine ./gcm`) and **both** checksum-verify commands (`shasum -a 256 -c` for macOS, `sha256sum -c` for Linux).

**Must-not:**
- Must-not publish to crates.io in this task (needs an account/token decision — see Escalate).
- Must-not modify any `src/**` provider/CLI behavior, flags, or defaults.
- Must-not hardcode `/opt/script` (or any absolute personal path) in README or the guide.
- Must-not edit `ci.yml` beyond leaving it as-is.
- Must-not add macOS code-signing/notarization in this task (see Escalate).

**Prefer:**
- The widely-used `softprops/action-gh-release` for the Stage-2 upload, **pinned to a version tag** (e.g. `@v2`), with `generate_release_notes: true` so notes are auto-populated from merged PRs.
- A portable archive/checksum step that branches on the runner OS for the right `shasum`/`sha256sum` tool.

**Escalate when:**
- crates.io publishing is wanted (requires the crates.io account + `CARGO_REGISTRY_TOKEN` secret) — confirm before adding a publish job.
- macOS signing/notarization becomes a requirement (needs an Apple Developer ID cert + secrets).
- `ubuntu-24.04-arm` native arm64 runner is unavailable on the repo's plan — then decide between `cross`/`cargo-zigbuild` for `aarch64-unknown-linux-musl` or dropping arm64-linux from v1 (FR-42 is a "Should"); `log` the chosen fallback.

## 4. Decomposition

1. **Release workflow (two-stage)** — author `.github/workflows/release.yml`: `on: push: tags: ['v*']` + `workflow_dispatch`. **Stage 1 `build` (matrix, 4 entries runner↔target):** checkout (`fetch-depth: 0`) → assert `Cargo.toml` version == tag (AC-8) → install musl toolchain on Linux runners (`apt-get install -y musl-tools`) → `rustup target add <target>` → `cargo build --release --locked --target <target>` → tar.gz `gcm` + `LICENSE` as `gcm-<tag>-<target>.tar.gz` → produce `.sha256` (branch on OS for `shasum`/`sha256sum`) → `actions/upload-artifact`. **Stage 2 `release` (`needs: build`, `permissions: contents: write`):** `actions/download-artifact` (all) → `softprops/action-gh-release@v2` with `generate_release_notes: true` attaching every archive + checksum to one Release (idempotent on re-run, AC-9). Files: `.github/workflows/release.yml`.
2. **Version hygiene** — bump `Cargo.toml` `version` to the intended tag (e.g. `0.1.0`) and confirm `build.rs` SHA stamping works under a `fetch-depth: 0` checkout; no logic change. Files: `Cargo.toml` (version line only). *(Independent of #1.)*
3. **README install** — replace/extend the "Install" section with (a) release-binary download + checksum verify + PATH placement (incl. the macOS quarantine note) and (b) `cargo install --git …`; ensure no `/opt/script` assumption. Files: `README.md`. *(Independent of #1/#2.)*
4. **Cutover guide** — create `docs/guides/cutover-from-bash.md`: the full alias matrix transcribed from the PRD, step-by-step repoint of `~/.zshrc`, side-by-side validation note, and the one-line rollback (bash untouched). Link it from README. Files: `docs/guides/cutover-from-bash.md`, `README.md` (one link). *(Independent of #1/#2; shares the README link with #3 — sequence #3 before the link edit or do both README edits together.)*

**Dependency order:** all four are largely independent. Only coupling: #3 and #4 both edit `README.md` (do the two README edits in one pass to avoid a conflict). Verification (§5) requires #1 + #2 merged and a test tag pushed.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Release workflow dry-run on a pre-release tag | Workflow succeeds; exactly **one** Release with 4 archives + 4 `.sha256` | `git tag v0.1.0-rc.1 && git push origin v0.1.0-rc.1`; then `gh release view v0.1.0-rc.1` |
| 2 | Archive contents (AC-2) | Each `gcm-*-<target>.tar.gz` contains executable `gcm` + `LICENSE` at root | `gh release download v0.1.0-rc.1 -p '*darwin*'`; `tar tzf` then `tar xzf` |
| 3 | Runner-native binary smoke (AC-3) | `gcm --version` prints `gcm 0.1.0+<sha>`; `gcm --help` exits 0 | On dev mac: extract `aarch64-apple-darwin` archive, run both |
| 4 | Version parity vs tag (AC-8) | `--version` number equals the tag; mismatched tag fails the workflow | Inspect test-3 output; verify the workflow's assert step on a deliberately mismatched local tag |
| 5 | Idempotent re-run (AC-9) | Re-running on the same tag updates assets, no duplicates / second release | Re-run via `workflow_dispatch` (or re-push tag); `gh release view` shows 8 assets, not 16 |
| 6 | Checksum integrity | Recomputed SHA-256 matches the published `.sha256` | `shasum -a 256 -c gcm-*-aarch64-apple-darwin.tar.gz.sha256` |
| 7 | Linux static binary portability (musl) | Linux archive is a static binary that runs with no glibc dependency | `file gcm` shows "statically linked"; run on a Linux box / container (e.g. `alpine`) |
| 8 | `cargo install` path (AC-4b) | Installs a runnable `gcm` into an isolated root | `cargo install --git https://github.com/maxkulish/gcm --root /tmp/gcm-it --locked && /tmp/gcm-it/bin/gcm --version` |
| 9 | No `/opt/script` install assumption in README (AC-4) | Zero matches in README (guide references are intentional) | `rg -n '/opt/script' README.md` → no output |
| 10 | README links cutover guide (AC-5) | README contains a link to `docs/guides/cutover-from-bash.md` | `rg -n 'cutover-from-bash' README.md` |
| 11 | Alias matrix parity (AC-5) | Guide rows match PRD lines 376-384 for all 7 live aliases | Diff the guide table against `sed -n '376,384p' docs/prds/prd-gcm.md` |
| 12 | One-line rollback documented (AC-6) | Guide states rollback = single alias-line revert, and to keep `/opt/script` intact | Manual read of `docs/guides/cutover-from-bash.md` |
| 13 | CI untouched (AC-7) | `ci.yml` byte-identical to `main`; CI green on the PR | `git diff main -- .github/workflows/ci.yml` → empty |

**Edge cases to verify:**
- `build.rs` falls back to `GCM_GIT_SHA="unknown"` outside a git checkout — confirm the release checkout (`fetch-depth: 0`) yields a real SHA, and note that a binary rebuilt from a source tarball will show `unknown` (acceptable).
- macOS Gatekeeper quarantines a downloaded unsigned binary — the install doc must give the `xattr -d com.apple.quarantine ./gcm` escape hatch (now a Must, in README steps).
- `Cargo.toml` `version` vs. the git tag mismatch — the workflow assert step (AC-8) fails the release rather than shipping a stale `--version`.
- `ubuntu-24.04-arm` runner unavailable on the plan — fall back per the Escalate clause (`cross`/`cargo-zigbuild` or drop arm64-linux), and `log` the decision.
- aarch64-musl link step needs the musl cross-linker on the arm runner — verify `musl-tools` (or the target's linker) is present; the build fails loudly if not.
- `cargo install --git` without `--locked` could resolve newer deps — `--locked` honors `Cargo.lock`.
- Re-running the release on an existing tag — the two-stage `needs:` design + `action-gh-release` update assets in place (AC-9).
