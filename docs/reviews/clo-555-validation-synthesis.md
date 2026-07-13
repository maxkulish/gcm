# Validation Synthesis: CLO-555

**Date**: 2026-07-13
**Inputs**: `clo-555-codex-validation.md` (Codex GPT-5.4, verdict FAIL), `clo-555-gemini-validation.md` (Gemini 3.1 Pro, verdict PASS)

## Verdict: PASS_WITH_FIXES_APPLIED (proceed to PR)

Gemini passed the branch outright. Codex returned FAIL with 5 findings; every
finding was validated against the code, all were real (one overstated), and
all were fixed in one iteration (`1db180e`). Post-fix: 378 unit + 76
integration tests green, clippy/fmt clean, acceptance suite 254 PASS / 0 FAIL.

## Codex findings - validation and disposition

| # | Severity | Finding | Validation | Fix |
|---|---|---|---|---|
| 1 | CRITICAL | Provider errors bubble out of phase A as run aborts, violating "tool escalation is not rejection" | **Valid per owner decision 1** (its text lists provider failure as escalation). The "loses confirmed work" claim was overstated - phase A precedes all confirmations - but the contract point stands | `resolve_hunks` errors escalate the file (markers kept, actionable error printed, other files proceed, Partial). ConflictMarkers-retry failure now escalates like ValidateCmdFailed (was inconsistent) |
| 2 | HIGH | Snapshot + zdiff3 run before the non-TTY guard and provider/privacy setup, so early failures mutate the tree with no restore | **Valid** - carried over the pre-CLO-555 ordering | All failure-prone preconditions moved ahead of the first mutation; new test proves a NonInteractive exit leaves merge-style markers byte-identical |
| 3 | HIGH | Edited proposal failing validation aborts the run (mid-phase-B error skips restore, drops earlier confirmations) | **Valid** - and worse under the transaction than under the old per-file flow | Edit-validation failure escalates the file (`FileAction::Escalated`, markers kept, run continues to Partial) |
| 4 | HIGH | Remote comment failure downgrades a committed (possibly pushed) run to Partial, breaking "Partial = not committed" | **Valid** - pre-existing CLO-533 behavior colliding with the new gate | Downgrade removed; comment failure surfaces via stderr warning + `commented: false` only |
| 5 | MEDIUM | Engine stages in Remote mode; spec requires write-only engine remotely | **Valid** (conformance; wrapper's `add -A` made it harmless in practice) | `stage_paths` + the `staged` report field are Local-only |

## Test impact

- New: `provider_error_escalates_file_and_reports_partial`,
  `early_failure_before_any_mutation_leaves_tree_untouched`.
- Contract updates: `resolve_validation_retry_then_escalate` (now asserts
  Partial + exit 0 + kept markers, was exit 1) and the vertex missing-token
  test (provider failure escalates with the actionable gcloud/ADC message
  still surfaced, was a hard error).

## Gemini findings

None (PASS; all 12 ACs confirmed, zero regressions reported).

## Disagreement resolution

Codex FAIL vs Gemini PASS: not weighted against each other - each Codex
finding was independently verified against source before any fix. All five
were genuine; the CRITICAL rating on #1 was reduced in practice to a contract
violation without the claimed data-loss component.
