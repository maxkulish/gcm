# CLO-490: Optional Secret Scanning and gcmignore Path Filtering

**Linear Issue**: https://linear.app/cloud-ai/issue/CLO-490/add-optional-secret-scanning-and-gcmignore-path-filtering
**Status**: Design Approved
**Author**: Team
**Created**: 2026-06-22

---

## Summary

CLO-490 adds an opt-in privacy layer before provider egress. Users can keep repo paths out of analysis with `.gcmignore`/`gcmignore`, and can enable a pre-send secret scan that either redacts detected credentials or aborts before any provider request.

---

## Background

FR-48 already excludes gitignored untracked files through `git ls-files --exclude-standard`, but tracked files and intentionally unignored paths can still be sent to the selected provider. FR-50 adds a second, gcm-specific guard for paths and credential-looking content.

### Prior Research

- ADR-001 Decision 1/3 keeps git CLI plumbing for exact status/diff/gitignore parity.
- ADR-001 Decision 6 requires later group messages to be generated from only that group's diff.
- CLO-491 already added `diff::gather_for_files`, proving provider-bound diffs can be path-scoped.

---

## Architecture

### Component Overview

Add a small `privacy` module between `git::changed_files` and provider calls. It owns ignore-pattern loading, changed-file filtering, secret-scan mode resolution, and payload sanitization. Provider backends stay unchanged.

### Affected Components

| Component | Change Type | Description |
|-----------|-------------|-------------|
| `src/privacy.rs` | New | `.gcmignore`/`gcmignore` parser, glob matcher, secret scanner/redactor |
| `src/cli.rs` | Modified | `--secret-scan=off|redact|abort` and help/privacy text |
| `src/diff.rs` | Modified | Build grouping/single diffs from filtered `ChangedFile`s |
| `src/main.rs` | Modified | Filter changed files before cache/validation/staging; sanitize payloads before provider calls |
| `src/error.rs`, `src/output.rs` | Modified | Secret-scan/config errors |
| `README.md`, `scripts/acceptance.sh` | Modified | User docs and end-to-end privacy checks |

### Dependencies

- **Internal**: `git::ChangedFile`, `diff::{GroupingContext,GatheredDiff}`, `GcmError`
- **External**: none

---

## Detailed Design

### Implementation Approach

1. Load ignore patterns from `.gcmignore` and `gcmignore` in the repo root. Blank lines and `#` comments are ignored; built-in patterns exclude the ignore files themselves from provider prompts.
2. Filter `ChangedFile`s before cache load/save, plan validation, display, and staging. A rename/copy is excluded if either the new path or original path matches.
3. Change provider-bound diff builders to use the filtered path set, including untracked content allow-lists, so ignored tracked paths cannot leak through whole-tree `git diff`.
4. Resolve secret scan mode from `--secret-scan` or `GCM_SECRET_SCAN` (`off`, `redact`, `abort`). In `redact`, credential-looking spans are replaced before the provider call; in `abort`, `gcm` exits with an error before egress.

### Code Structure

```rust
pub enum SecretScanMode { Off, Redact, Abort }

pub struct Privacy {
    filter: PathFilter,
    secret_scan: SecretScanMode,
}

impl Privacy {
    pub fn load(repo: &Repo, cli_mode: Option<SecretScanMode>) -> Result<Self, GcmError>;
    pub fn filter_changed(&self, changed: &[ChangedFile]) -> Vec<ChangedFile>;
    pub fn prepare_grouping(&self, ctx: GroupingContext) -> Result<GroupingContext, GcmError>;
    pub fn prepare_diff(&self, diff: GatheredDiff) -> Result<GatheredDiff, GcmError>;
}
```

---

## Testing Strategy

- **Unit tests**: glob matching, built-in ignore patterns, rename/original-path filtering, secret redaction, abort detection.
- **Integration tests**: mock-provider capture proves ignored paths and secrets are absent, and abort mode sends no request.
- **Regression checks**: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `scripts/acceptance.sh`.

### Completion Criteria

| Criterion | Target | How to Verify |
|-----------|--------|---------------|
| Ignored paths never reach provider requests | zero matching path/content in capture | `scripts/acceptance.sh` CLO-490 case |
| Redact mode strips detected credential values | request contains redaction marker, not secret | `scripts/acceptance.sh` CLO-490 case |
| Abort mode blocks egress | exit 1 and no mock request | `scripts/acceptance.sh` CLO-490 case |
| Existing behavior preserved | unit + acceptance pass | cargo + acceptance gates |

---

## Security Considerations

- The scan is best-effort, not a complete DLP engine.
- `abort` is the strictest mode and is recommended for high-sensitivity repos.
- `.gcmignore`/`gcmignore` files are never sent to the provider by default because their contents can reveal sensitive paths.

---

## Constraint Architecture

### Musts

- Filter provider-bound diffs, not just the model-facing file list.
- Apply the same filtering to grouped, single-commit, fallback, and cache-hit message calls.
- Avoid new dependencies for this slice.

### Must-Nots

- Do not alter provider wire formats.
- Do not send detected secret values in redact or abort mode.
- Do not let ignored paths invalidate or replay the plan cache.

### Preferences

- Prefer small, testable pure helpers over a large config subsystem.
- Prefer explicit `--secret-scan`/`GCM_SECRET_SCAN` over enabling noisy scanning by default.

### Escalation Triggers

- A future config-file format must be reconciled with CLO-496 onboarding/config decisions.
- A full gitignore-compatible pattern engine should be considered separately if users need negation or nested ignore-file semantics.

---

## Acceptance Criteria

- [ ] A path matched by the ignore list never appears in the provider request.
- [ ] With scan enabled, an embedded credential pattern triggers redaction or abort before egress.
- [ ] Documentation explains the feature and limits.
- [ ] Tests pass.

---

## Rollback Plan

Revert `privacy.rs` and the call-site wiring. With no privacy layer, the repo returns to FR-48-only behavior where gitignored untracked files are excluded but no gcm-specific filtering or secret scan runs.

---

## Open Questions

- [x] Pattern source for this slice: support repo-local `.gcmignore` and `gcmignore`; defer broader config-file integration.
- [x] Secret scan default: off to avoid false-positive interruption; users opt into `redact` or `abort`.

---

## References

- [PRD FR-50](../prds/prd-gcm.md)
- [ADR-001](../adrs/001-foundational-architecture-decisions.md)
- [CLO-491 design](2026-06-20-clo-491-plan-cache.md)
