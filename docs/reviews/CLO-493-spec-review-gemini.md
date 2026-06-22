# Spec Review: CLO-493

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-06-22
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is clear, complete, and accurate. It perfectly captures why human-readable output makes automation brittle, and identifies the need for `--json` machine contracts, non-interactive safe options (`--yes`/`--no-input` and `--plan-only`), and structured, stderr-isolated logging. It is fully aligned with the Linear task goals.

## 2. Acceptance Criteria Review
**Strong**: Almost all criteria are specific, measurable, and highly actionable. The inclusion of explicit, testable shell commands (using `cargo run`, `jq`, and `git` asserts) for verification makes them exceptionally strong.
**Gaps**:
- **AC-6 Fallback Ambiguity (Critical)**: On a grouping failure with `--yes --json` active, `gcm` will "emit `status: "fallback"` and continue the single-commit behavior path." If it actually commits, how does the final JSON envelope represent both the fallback event *and* the successful commit outcome (such as the commit hash or status)? Does `status` change to `"committed"` containing a nested `fallback` block, or does `status` remain `"fallback"` but contain inner `commit` details? This schema design must be explicit.
- **Error JSON Schema Definition**: The specific JSON schema structures for each envelope type (`plan`, `noop`, `committed`, `error`, `fallback`) are not formally defined.
- **Universal Error Serialization**: The criteria does not explicitly mandate that *all* other runtime errors (e.g. `GcmError::NotARepo`, `GcmError::Git`, etc.) must be caught and returned as structured JSON error objects on `stdout` when `--json` is specified.

## 3. Constraints Check
**Aligned**:
- Aligns with the blocking, synchronous design of `Provider` traits and custom error taxonomy (`GcmError`, `ProviderError`).
- Aligns with the core isolation of all output streams (stdout for JSON payload, stderr for logs/warnings).
**Concerns**:
- **Missing Constraints Section**: The specification lacks a structured "Constraints" section using Must/Must-not/Prefer/Escalate categories.
- **Implicit Constraints**: Needs explicit constraints declaring that `--json` **Must Not** write anything other than a single, valid JSON object to stdout, and **Must** direct all diagnostic logging and warnings (including existing `curated_index_warning`) exclusively to stderr.

## 4. Decomposition Quality
**Well-scoped**: Sub-tasks ST1 to ST6 are extremely well-scoped, logical, and correctly sized (S/M). Dividing into JSON modeling, CLI flag extension, path routing, logging, and documentation makes the plan realistic and manageable.
**Issues**:
- ST1 (JSON model) and ST3 (Route paths) should explicitly include the requirement to catch and serialize the existing `GcmError` and `ProviderError` variants rather than allowing them to bubble up to raw stderr text under `--json`.
- Task dependencies are not explicitly declared (e.g. ST3 depends heavily on ST1 and ST2).

## 5. Evaluation Coverage
**Covered**: The evaluation table is exceptional. It maps 10 distinct, highly realistic testing scenarios to precise inputs, expected behaviors, and verification checks.
**Gaps**:
- Missing a scenario for verifying error serialization under `--json` when other runtime errors occur (e.g. missing API key, unmerged conflicts, git commit pre-commit hook failure).
- Missing a scenario for fresh plan validation failure under `--json` (detecting plan hallucinations/omissions and fallback serialization).

## 6. Codebase Alignment
**Violations**: None. However, be aware that the codebase does **not** use `anyhow` or `BackendErrorKind`. It relies on custom `GcmError` (in `src/error.rs`) and `ProviderError` (in `src/provider/mod.rs`). The spec should align with these existing types rather than assuming foreign libraries or patterns.
**Alignment**: Follows the synchronous execution model of the `Provider` trait and the index transactions (`snapshot_index` / `restore_index` / `clear_staged`) in `Repo` perfectly. `--plan-only` integrates natively with `needs_terminal_but_absent` to bypass non-TTY prompts safely.

## 7. Blind Spots
- **Log Level Precedence Collision**: The spec does not define which environment variable wins if both `DEBUG_GCM=1` and `GCM_LOG_LEVEL=warn` are set simultaneously (recommend `GCM_LOG_LEVEL` overriding legacy `DEBUG_GCM`).
- **Default Log Level**: If `GCM_LOG_LEVEL` is unset, what is the default log behavior? (e.g. `warn`, `error` or `off`).
- **Interaction with index safety warns**: Under `--json`, human-oriented warnings (such as the `curated_index_warning` for partial staging) **Must Not** be outputted on `stdout` and should instead be directed to `stderr`.

## 8. Verdict
**Verdict**: APPROVE_WITH_SUGGESTIONS

## 9. Actionable Feedback
1. **Define Unified JSON Schemas**: Explicitly document the JSON structure of every envelope variant to prevent developer implementation drift:
   - `plan`: `{ "status": "plan", "plan": Plan, "changed_files": Vec<String>, "provider": String, "model": String }`
   - `noop`: `{ "status": "noop" }`
   - `committed`: `{ "status": "committed", "commit": { "status": "ok", "hash": String, "message": String } }`
   - `error`: `{ "status": "error", "error": { "code": String, "message": String } }`
2. **Resolve AC-6 Fallback Schema**: Standardize how a fallback is represented on an unattended commit run. Recommend a merged structure: `{ "status": "fallback", "fallback": { "reason": String }, "commit": { "status": "ok", "hash": String, "message": String } }`.
3. **Add Constraints Section**: Incorporate a Constraints section into the document defining the following:
   - **Must**: Direct all CLI logging, trace information, and warnings (e.g., `curated_index_warning`) to `stderr` under `--json` mode.
   - **Must Not**: Use async runtimes; all JSON formatting, flag routing, and logging must remain synchronous.
4. **Harden Error Serialization**: Add a sub-task or explicitly specify in ST1/ST3 that all `GcmError` and `ProviderError` variants must serialize to stdout using the `status: "error"` schema under `--json` instead of printing raw strings to stderr.
5. **Establish Logging Precedence**: Specify that `GCM_LOG_LEVEL` has higher precedence than `DEBUG_GCM`, and set the default log level when unset to `off`.
