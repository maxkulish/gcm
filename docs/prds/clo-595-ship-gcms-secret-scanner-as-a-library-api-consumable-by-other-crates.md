# PRD: Ship gcm's secret scanner as a library API consumable by other crates

| Field | Value |
|---|---|
| Author | Max Kulish |
| Status | Draft |
| Created | 2026-07-27 |
| Linear | [CLO-595](https://linear.app/cloud-ai/issue/CLO-595/ship-gcms-secret-scanner-as-a-library-api-consumable-by-other-crates) |
| Branch | `feat/clo-595-ship-gcms-secret-scanner-as-a-library-api-consumable-by` |
| Labels | AFK |
| Depends on | [CLO-594](https://linear.app/cloud-ai/issue/CLO-594/lock-the-gcm-library-boundary-the-sync-async-seam-and-the-config-shape-adr) |

## 1. Overview

CLO-595 must make gcm's secret scanner callable by downstream crates (including `lok` and other in-org consumers) without forcing them to depend on CLI/runtime-only types from the main binary. The scanner currently lives entirely behind `crate::`-scoped modules and returns a CLI-oriented `GcmError`, so consumers cannot build a `RuleEngine` and scan text safely in downstream code.

The task is to define a new external API in gcm that includes the secret scanner surface (`RuleEngine`, `secret_ranges`, `redact_secrets`, `merge_ranges`, entropy helpers, `SecretScanMode`) while keeping CLI parsing and transport concerns in the binary. The API should let callers configure scan mode via caller-supplied environment lookup and support an abort-only path without reading process env.

The binary should keep scanning paths (`.gcmignore`/`gcmignore` filtering, diff preparation, path-aware behavior) and egress control, but expose a clean testable core that is safe for non-CLI consumers.

## 2. Problem & Objectives

### Problem
- gcm has no `[lib]` target, so everything compiles under a single binary crate.
- Secret scanner types and functions are not directly consumable by external crates without pulling binary dependencies.
- `SecretScanMode::resolve` currently reads process environment directly and `Privacy::scan_text` returns `GcmError`, which is domain-specific to CLI execution.

### Objectives
- Expose a stable scanner API that external crates can call directly to build a `RuleEngine` and run detection on arbitrary text.
- Preserve existing scanner behavior for redact/abort modes where relevant.
- Preserve CLI defaults and behavior for gcm itself; do not change user-facing scanning defaults in this task.
- Keep library dependencies free of CLI-only heavy dependencies where possible.

## 3. Scope

| # | Requirement |
|---|---|
| S1 | Add library-surface API for secret scanning (`RuleEngine`, `CompiledRule`, `vendored`, `secret_ranges`, `redact_secrets`, `merge_ranges`, `Charset`, entropy functions, and `SecretScanMode`). |
| S2 | Replace scanner path resolution with a caller-supplied env lookup closure (no direct process-env read on the shared path). |
| S3 | Define/return a library-local scanner error for abort mode and map it in the binary. |
| S4 | Keep `.gcmignore` filtering (`Privacy` facade, path matching, changed-file filtering) in the binary layer. |
| S5 | Preserve `--secret-scan=redact` and `--secret-scan=abort` behavior for CLI flows, including `gcm resolve` and commit flow parity. |
| S6 | Create/update docs and tests so an external crate can compile a rule engine and scan text with no gcm domain types in signatures. |

## 4. Functional requirements

- **FR1:** `gcm --secret-scan=redact` and `gcm --secret-scan=abort` keep current behavior on diff/hunk egress paths.
- **FR2:** `Cargo test` remains green (current target remains 475 passing tests).
- **FR3:** Library API supports external usage through vendored rules (`src/privacy/rules.toml`) and allows callers to scan text without binary internals.
- **FR4:** Abort mode path is available to callers via a scanner error type that does not require importing `GcmError`.
- **FR5:** The library API avoids mandatory `clap` dependency; CLI-specific parsing remains in the `main` crate path.

## 5. Acceptance Criteria

- [ ] 475 tests still pass.
- [ ] A test consumes the scanner as an external crate would, including `RuleEngine` construction and text scanning with no gcm domain types in any signature touched by caller code.
- [ ] Selecting abort mode from a caller-supplied env map (empty process env) works and is tested.
- [ ] `gcm --secret-scan=redact` and `gcm --secret-scan=abort` behave identically on the same input as v0.5.2.
- [ ] Scanner extraction path does not pull in `clap` or `cliclack` for the library build.

## 6. Out of scope

- Full `gcm` transport refactors.
- Provider trait extraction or async migration.
- External crates.io publish workflow (path dependency-first for in-org consumers).
- New provider or rule syntax features beyond current FR-60 surface.