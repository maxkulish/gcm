# Pre-PR validation: clo-493

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-06-22
**Pipeline**: lok pre-pr-validation
---

I've independently verified every finding against the code. Confirmed:
- `ui::confirm(msg, false, true)` (json, no `--yes`) skips the preview but still `print!`s the prompt to **stdout** (ui.rs:28) — reachable via `gcm --json` in a TTY without `--yes`.
- Early `output::error(None, None, None, …)` calls pass `mode = None`, which `skip_serializing_if` omits — violating §5 "Must include `mode` in all envelopes."
- `commit_first_group` hardcodes `cached: false` (main.rs:320/334) even when `plan` came from `cache::load`'s hit arm.
- `--reset` has no JSON handling and README has zero `reset` content; acceptance.sh tests `--reset` only in non-JSON mode.
- `last_commit_hash()` returns untrimmed `rev-parse` stdout → trailing `\n` in `commit.hash`.

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | success=true; verdict FAIL; 5 findings, all independently reproduced against the diff |
| Gemini | OK | success=true; verdict PASS_WITH_NOTES; 1 LOW + 1 missing-item (reset), reproduced |
| Claude fallback | SKIPPED | At least one external reviewer succeeded |

## Verdict
PASS_WITH_NOTES

All confirmed defects are localized and fixable in one bounded iteration. The core design (typed `Envelope`, exhaustive `GcmError`/`ProviderError` code mapping, stdout/stderr separation, mode markers) matches the design doc. No pivot, no material divergence.

## Must Fix Before PR
- **stdout prompt pollution under `--json` (AC-1 / §5 "exactly one JSON object on stdout").** `gcm --json` in a TTY without `--yes`/`--plan-only`/`--dry-run` reaches `ui::confirm(msg, false, true)`: `quiet` suppresses the preview but `print!("Commit with this message?…")` still writes to stdout before the envelope (src/ui.rs:28, called from src/main.rs:405 and src/main.rs:536). Trigger is narrow (interactive `--json`; piped stdin hits the NonInteractive guard first), but it is a real stream-purity violation. Fix: route the prompt to stderr, or require a non-interactive flag in json mode.
- **`mode` omitted on early error envelopes (§5 "Must: include `mode` in all envelopes").** `output::error(None, None, None, …)` at src/main.rs:56, 58, 68, 78, 87, 90 passes `mode = None`, which `skip_serializing_if = Option::is_none` drops (src/output.rs:33). NotARepo, Git, NonInteractive, and UnmergedConflicts envelopes therefore ship without `mode`. `mode` is computable from `args` at each site (see `grouped_mode`/`noop_mode`). Note: the clean-repo `noop` at src/main.rs:71 *does* carry `mode` via `noop_mode`, so Codex's "noop omits mode" is imprecise — only the error envelopes are affected.
- **`cached` always `false` on cache hits (stable v1 contract data).** `commit_first_group` passes literal `false` to `output::plan` at src/main.rs:320 and src/main.rs:334 even when `plan` came from the `cache::load(...) => Some(plan)` hit arm (src/main.rs:140). A consumer reading `cached` always sees `false`. Fix: thread a `cached` bool out of the load/build match into `commit_first_group`.
- **`--reset --json` behavior undefined, undocumented, and untested (ST6 + §5 explicit requirement).** src/main.rs:63 clears the cache then falls through to noop/plan/commit; there is no `status: "reset"` envelope, README contains no `reset` text, and acceptance.sh exercises `--reset` only in non-JSON mode (scripts/acceptance.sh:885). The spec mandates this be "explicitly defined and covered by acceptance (either reset status envelope or documented non-output)." Lowest-effort compliant path (per Gemini): document the fall-through behavior in README and add one acceptance assertion. This is a documented either/or in the spec, so no user decision is needed.
- **Commit hash carries a trailing newline (LOW, but dirties the v1 contract).** `last_commit_hash()` returns `capture(["rev-parse","HEAD"])` untrimmed (src/git.rs:68, src/git.rs:64), so `commit.hash` is `"<sha>\n"`. AC-6's `type=="string"` check still passes, masking it. One-line fix: `.trim()`.

## Out of Scope / Deferred
- **`provider`/`model` absent on pre-selection envelopes (NotARepo, clean-repo noop, NonInteractive).** The spec's `noop`/`error` shapes list `provider`/`model`, but the provider is selected only after the no-changes/merge guards (src/main.rs:97), so these are genuinely unknown at those return points. Defensible omission; not a blocker. Worth a one-line spec note that identity fields are best-effort and present only once a provider is resolved.

## False Positives / Tooling Artifacts
- **Gemini LOW — `emit` fallback string instead of panic (src/output.rs:175).** Gemini itself rates this reasonable. Emitting a guaranteed-valid minimal error envelope rather than panicking is the correct choice for an automation surface, and the type is `#[derive(Serialize)]` over infallible fields so the branch is effectively unreachable. No change needed.
- **Neither external reviewer ran `cargo test` or the acceptance matrix** (both state static-only review). Not a finding against the code, but it means runtime/AC verification is unproven by the reviewers. I reproduced each accepted finding by reading the diff; the fix iteration should run `cargo test` + `scripts/acceptance.sh` to close AC-1/AC-7/AC-13 empirically.

## Recommendation
PROCEED_WITH_FIXES. Apply one bounded iteration: (1) send the confirm prompt to stderr (or gate prompting off in json mode) so stdout stays pure; (2) populate `mode` on the early error envelopes from `args`; (3) thread the real `cached` flag from the cache-load arm into the plan envelopes; (4) resolve `--reset --json` by documenting the fall-through in README and adding an acceptance assertion (simplest spec-compliant option); (5) `.trim()` the commit hash. Then run `cargo test` and `scripts/acceptance.sh` to confirm AC-1, AC-7, and AC-13 before opening the PR. None of these require a user decision, so no STOP is warranted.
