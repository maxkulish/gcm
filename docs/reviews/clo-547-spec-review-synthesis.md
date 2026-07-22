# Spec Review Synthesis: clo-547

**Synthesized**: 2026-07-22
**Pipeline**: lok spec-review

---

# Spec Review Synthesis: CLO-547 Model Discovery Hardening

## Reviewer Status

| Reviewer | Status |
|----------|--------|
| Gemini (opencode) | **FAILED** - empty output, CLI invocation error ("You must provide a message or a command") |
| Ollama/Codex (glm-5:cloud) | **FAILED** - empty output after model banner |
| Claude (fallback) | **SUCCESS** - all code references validated against working tree at `8c98cab` |

Only one valid source. No cross-referencing possible; agreement/disagreement tables omitted. All findings below are single-reviewer (Claude fallback), which validated every spec claim against the code before reporting.

## Findings (Single Reviewer: Claude)

| # | Finding | Severity |
|---|---------|----------|
| 1 | Decomposition 4 / AC6 offer a `tests/` integration-test alternative that cannot work: gcm is a binary-only crate, so `tests/` cannot reach `pub(crate)` seams, and `tests/vertex.rs` drives the built binary - impossible for the interactive wizard. Only in-crate `#[cfg(test)]` is viable | Medium |
| 2 | AC6 timeout case is unspecified in cost terms: `MODEL_FETCH_TIMEOUT` is a hardcoded 5s const with one retry and no env override, so a real TcpListener stall adds ~10s to `cargo test`. Timeout case should be simulated via the injected seam returning a timeout-shaped `Err` | Medium |
| 3 | AC5's "absent from live catalog" check must compare by canonical form (`canonicalize_model`, config.rs:1056-1060), or migrated `llama3` gets falsely labeled when live has `llama3:latest`. Test 8 needs this exemplar | Low |
| 4 | models.rs:7-8 module doc documents the static-baseline merge behavior AC1 removes; no AC or gate forces the doc update | Low |
| 5 | Google false-positive exclusions have no fresh-selection escape (multiselect is filter-only, no free text); accepted risk should be recorded in the spec with the enabled-set union named as mitigation | Low |

**Validated strengths** (no action needed): problem statement's three gaps all code-confirmed; AC exemplars internally consistent; decomposition ordering 1 → {2,3} → 4 matches the true dependency graph; proposed seam signature matches `get_json` exactly; both owner decisions already recorded, so no open questions block implementation.

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

(Sole valid reviewer returned APPROVE_WITH_SUGGESTIONS; nothing blocks starting implementation.)

## Priority Actions

1. **(Medium)** Strike the `tests/` integration-test alternative in Decomposition 4 / AC6; commit explicitly to in-crate `#[cfg(test)]` tests.
2. **(Medium)** Specify the timeout case in test 5 as an injected-seam `Err` (fast), not a real TcpListener stall - no env override exists to shrink the 5s+retry cost.
3. **(Low)** Require canonical-form comparison in the AC5 hint fn; add `llama3` vs `llama3:latest` to test 8's exemplars.
4. **(Low)** Add the models.rs module doc update (D7 merge description) to Decomposition 3.
5. **(Low)** Record the no-free-text-escape limitation for Google exclusions as accepted risk, with the enabled-set union as mitigation.

**Caveat**: with two of three reviewers failed, this synthesis rests on a single perspective. The Claude review is code-validated, but consider re-running Gemini (fix the CLI invocation - it received no message argument) and Ollama if independent confirmation matters for this spec.
