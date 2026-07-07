# PRD: Fix `gcm resolve` splice when resolution lacks a trailing newline

| Field | Value |
|---|---|
| Author | Max Kulish |
| Status | Draft |
| Created | 2026-07-07 |
| Linear | [CLO-535](https://linear.app/cloud-ai/issue/CLO-535/fix-gcm-resolve-splice-resolution-missing-a-trailing-newline-joins-the) |
| Branch | `fix/clo-535-new-line` |
| Labels | Bug |
| Depends on | [CLO-531](https://linear.app/cloud-ai/issue/CLO-531/add-gcm-resolve-llm-assisted-merge-conflict-resolver-phase-1-local) (`gcm resolve` local markers) |

## 1. Overview

`gcm resolve` writes LLM-provided resolution text back into the original file by splicing it over each conflict hunk. When the provider returns a resolution that does not end with a newline, the next context line is appended directly to the last resolved line, producing mis-formatted output such as a closing brace pulled up onto a `format!` call.

This change defends `reconstruct` against missing trailing newlines in resolution text while preserving the existing line-ending behavior for CRLF files and files that originally had no final newline.

## 2. Problem & Objectives

### Problem

- `reconstruct` in `src/resolve/mod.rs` appends the resolution text verbatim and assumes it ends in `\n`.
- The following context line is then appended by the non-hunk branch, so a resolution without a trailing newline fuses with it.
- The bug is provider-independent (observed with both Groq and Gemini) and affects any conflict where the model omits the trailing newline.

### Objectives

- **O1:** Ensure a resolution without a trailing newline does not join with the following context line.
- **O2:** Keep CRLF files using CRLF at the splice point.
- **O3:** Preserve the existing behavior for files that originally had no final newline.
- **O4:** Add a unit test covering the missing-trailing-newline resolution case.

## 3. Scope

| # | Requirement |
|---|---|
| S1 | After appending resolution text in `reconstruct`, ensure it ends with exactly one line ending before the next context line. |
| S2 | The line ending used for the guard must match the file’s dominant line ending (`\r\n` for CRLF files, otherwise `\n`). |
| S3 | Do not introduce extra blank lines when the resolution already ends with a newline. |
| S4 | Preserve the final-trailing-newline trim that already handles files without a final newline. |
| S5 | Extend `reconstruct_replaces_hunk_with_resolution` (or add a new test) to assert the following context line stays on its own line when the resolution lacks a trailing newline. |

## 4. Functional Requirements

- **FR-1:** When `uses_crlf` is false, after pushing `text` (or the CRLF-normalized variant), if `out` does not end with `\n`, push one `\n`.
- **FR-2:** When `uses_crlf` is true, after pushing the normalized text, if `out` does not end with `\r\n`, push `\r\n`.
- **FR-3:** The existing `if !original.ends_with('\n') && !out.is_empty() { out.pop(); }` behavior at the end of `reconstruct` must remain unchanged.
- **FR-4:** A unit test must fail before the fix and pass after the fix.

## 5. Acceptance Criteria

- [ ] A resolution whose text lacks a trailing newline does not fuse with the following context line.
- [ ] CRLF files keep CRLF endings at the splice.
- [ ] Files with no final newline are unchanged in that respect (existing behavior preserved).
- [ ] Unit test covers the missing-trailing-newline resolution case.

## 6. Out of Scope

- Changing how the provider is prompted or constraining the response schema to require a trailing newline.
- Auto-staging or `--continue` behavior (unchanged from CLO-531).
- Any new configuration or CLI flags.
