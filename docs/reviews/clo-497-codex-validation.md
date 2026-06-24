## Verdict: FAIL

The release workflow is mostly sound on the release-engineering basics: the four runner labels are currently valid (`ubuntu-24.04-arm`, `macos-15-intel`, `macos-latest`/`macos-15`), with `ubuntu-24.04-arm` still marked public preview in GitHub’s docs ([GitHub Docs](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)); `contents: write` is present; upload/download naming is consistent; and the tag/version assert does accept both `v0.1.0` and `v0.1.0-rc.1` in [release.yml](/Users/mk/Code/gcm/.github/workflows/release.yml:50). Reruns should also update assets in place because `softprops/action-gh-release` defaults `overwrite_files` to `true` ([action docs](https://github.com/softprops/action-gh-release)).

I did not find a material Actions injection or secret-exposure issue in this diff. The branch still fails the spec because AC-3 and AC-6 are not actually met.

## Findings

- HIGH: AC-3 is missing from the workflow. [release.yml](/Users/mk/Code/gcm/.github/workflows/release.yml:69) builds, packages, and uploads artifacts, and [the release job](/Users/mk/Code/gcm/.github/workflows/release.yml:109) publishes them, but nowhere does the workflow execute the built binary with `gcm --version` and `gcm --help`. That means a tagged run can still publish a broken native artifact or malformed archive without CI catching it first.

- MEDIUM: The cutover docs do not provide the promised one-line rollback. The guide tells the user to [replace the old alias block](/Users/mk/Code/gcm/docs/guides/cutover-from-bash.md:50), then later claims rollback is “one line” in [the rollback section](/Users/mk/Code/gcm/docs/guides/cutover-from-bash.md:87), but the example only restores `gcmq` and says “and the rest.” That is a multi-line manual restore, not a one-line revert, and it makes the [README claim](/Users/mk/Code/gcm/README.md:86) inaccurate.

## Missing Items

- AC-3: Add runner-native smoke checks to the release workflow before publishing, covering at least `gcm --version` and `gcm --help` for each built target on its native runner.
- AC-6: Rewrite the cutover so rollback is truly one line after following the guide, while keeping `/opt/script/git-commit-ai.sh` intact.

## Recommendations

- Add a smoke-test step in the build job after `cargo build` and before artifact upload: run the built binary directly from `target/<triple>/release/gcm`, then optionally untar the packaged archive and re-run the same checks on the extracted binary.
- Change the cutover procedure so the old bash block stays in place, commented, and the user flips a single `source` line or a single alias-file include. As written, “replace the old block” destroys the one-line rollback property.
- Make two small hardenings explicit even though they are not the failing issues: set `overwrite_files: true` in the release action instead of relying on the default, and validate `workflow_dispatch.inputs.tag` as a `v*` tag before continuing.

