# CLO-493 Add automation surface: --json, non-interactive flags, logging

**Status:** draft
**Type:** specification
**Linear:** https://linear.app/cloud-ai/issue/CLO-493/add-automation-surface-json-non-interactive-flags-structured-logging
**Design context:** docs/adrs/001-foundational-architecture-decisions.md §§ 10, 49, 50; docs/prds/prd-gcm.md FR-49/50/51

## 1. Problem and goal

`gcm` currently emits only human-oriented text. Automation (CI/agents) therefore cannot reliably inspect outcome, errors, or planned groups without brittle text parsing. CLO-493 requires a stable machine contract and deterministic non-interactive behavior.

The change introduces `--json` (machine contract), `--plan-only` (explicit no-mutation preview), and `--yes`/`--no-input` for unattended commits, while preserving existing interactive defaults. It also formalizes logging output policy so JSON consumers can distinguish actionable machine output from diagnostics.

## 2. Acceptance criteria

### JSON envelope contracts (must be stable)

All `--json` responses must include `v: 1` and a top-level `status` field:

- `plan`: `{ "v": 1, "status": "plan", "mode": "grouped"|"single", "provider": "<provider>", "model": "<model>", "plan": <Plan>, "changed_files": ["<paths>"], "cached": bool }`
- `noop`: `{ "v": 1, "status": "noop", "mode": "plan_only"|"dry_run", "provider": "<provider>", "model": "<model>" }`
- `committed`: `{ "v": 1, "status": "committed", "mode": "grouped"|"single", "provider": "<provider>", "model": "<model>", "commit": { "status": "ok", "hash": "<sha>", "message": "<msg>", "changed_files": ["<paths>"] } }`
- `fallback`: `{ "v": 1, "status": "fallback", "mode": "grouped", "provider": "<provider>", "model": "<model>", "fallback": { "reason": "<text>", "raw_code": "<provider_or_git_code>" }, "commit": { "status": "ok", "hash": "<sha>", "message": "<msg>", "changed_files": ["<paths>"] } }`
- `error`: `{ "v": 1, "status": "error", "mode": "grouped"|"single"|"dry_run"|"plan_only", "error": { "code": "NotARepo|Git|Provider|NonInteractive|Editor|EmptyMessage|UnmergedConflicts|CommitFailed", "message": "<human message>" }, "provider": "<provider>", "model": "<model>" }`

`error.code` must map all `GcmError` and `ProviderError` variants that reach CLI output.

- [ ] **AC-1:** `--json` is stream-pure: all success/error envelopes are emitted as a JSON object on stdout, and all logs are emitted on stderr.
  - Verifiable: `cargo run -- --plan-only --json | jq -e '.v == 1 and .status == "plan"'`

- [ ] **AC-2:** `gcm --plan-only --json` on a dirty repo emits status `plan`, `mode: "plan_only"`, and the machine payload includes `changed_files` and `plan.groups`.
  - Verifiable: `cargo run -- --plan-only --json >/tmp/gcm.out; jq -e '.status=="plan" and .mode=="plan_only" and (.changed_files|type=="array") and (.plan.groups|type=="array")' /tmp/gcm.out`

- [ ] **AC-3:** `gcm --dry-run --json` emits status `plan` and `mode: "dry_run"` (same semantic payload as plan-only, with cache-affecting semantics preserved).
  - Verifiable: `cargo run -- --dry-run --json >/tmp/gcm.out; jq -e '.status=="plan" and .mode=="dry_run"' /tmp/gcm.out`

- [ ] **AC-4:** On a clean repo, `gcm --plan-only --json` returns `status: "noop"` and exits 0.
  - Verifiable: `cargo run -- --plan-only --json >/tmp/gcm.out; jq -e '.status=="noop"' /tmp/gcm.out`

- [ ] **AC-5:** `gcm --plan-only --json` is non-destructive: staging, write-tree, and HEAD are unchanged.
  - Verifiable: capture pre/post `git diff --cached`, `git write-tree` in a scratch repo and assert no mutation.

- [ ] **AC-6:** `gcm --yes --json` on dirty repo emits `status: "committed"`, and returns exit 0 for successful commit.
  - Verifiable: `cargo run -- --yes --json >/tmp/gcm.out; jq -e '.status=="committed" and .commit.status=="ok" and .commit.hash|type=="string"' /tmp/gcm.out`

- [ ] **AC-7:** Non-TTY without `--yes`/`--plan-only`/`--dry-run` emits `status: "error"`, `error.code == "NonInteractive"`, and non-zero exit.
  - Verifiable: `printf '' | cargo run -- --json >/tmp/gcm.out; test $? -ne 0 && jq -e '.status=="error" and .error.code=="NonInteractive"' /tmp/gcm.out`

- [ ] **AC-8:** Grouping path errors (provider parse/auth/missing-key/transport etc.) under `--json` map to a single `status: "error"` envelope, never raw multiline prose on stdout.
  - Verifiable: simulate provider/missing-key failure and assert JSON parse with `.status=="error"`.

- [ ] **AC-9:** Grouping fallback under `--yes --json` emits `status: "fallback"` and includes both `fallback.reason` and nested committed `commit` summary.
  - Verifiable: trigger a fallback test fixture and check: `jq -e '.status=="fallback" and .fallback.reason and .commit.hash' /tmp/gcm.out`

- [ ] **AC-10:** `--all --yes --json` emits single-commit payload (`mode: "single"`) and no grouped-plan details.
  - Verifiable: `cargo run -- --all --yes --json >/tmp/gcm.out; jq -e '.status=="committed" and .mode=="single" and .plan==null' /tmp/gcm.out`

- [ ] **AC-11:** `--all --plan-only --json` emits `status: "plan"`, `mode: "single"`, and remains non-destructive.
  - Verifiable: `cargo run -- --all --plan-only --json >/tmp/gcm.out; jq -e '.status=="plan" and .mode=="single" and .changed_files|type=="array"' /tmp/gcm.out`

- [ ] **AC-12:** `GCM_LOG_LEVEL` governs logging level with precedence over `GCM_DEBUG` (`off|error|warn|info|debug|trace`; default `off`), and all logs remain on stderr.
  - Verifiable: `GCM_LOG_LEVEL=warn GCM_DEBUG=1 cargo run -- --plan-only --json >/tmp/gcm.out 2>/tmp/gcm.err && jq -e '.status == "plan"' /tmp/gcm.out`

- [ ] **AC-13:** Exit codes in json mode are deterministic:
  - 0: `plan`, `noop`, `committed`, `fallback`
  - 1: `error`
  - Verifiable: assert exit status for one positive and one negative JSON scenario.

## 3. Sub-tasks

### ST1 Define JSON contract, envelope mapping, and error-code mapping
**Files:** src/output.rs (new), src/main.rs, src/error.rs, src/provider/mod.rs
**Tests:** `cargo test output::tests`
**Estimate:** M
**Dependencies:** none

Create one typed contract for statuses above and map all `GcmError` + `ProviderError` variants to `error.code` and message.

### ST2 Extend CLI machine-mode flags and mode-state model
**Files:** src/cli.rs, src/main.rs
**Tests:** `cargo test cli::tests`
**Estimate:** S
**Dependencies:** ST1

Confirm/add `--json`, `--plan-only`, and explicit mode markers for JSON output (`plan_only`, `dry_run`, `single`, `grouped`). Keep `--dry-run` semantics unchanged.

### ST3 Route execution outcomes through contract serializer
**Files:** src/main.rs, src/ui.rs
**Tests:** targeted execution path tests + `cargo test main::tests`
**Estimate:** L
**Dependencies:** ST1, ST2

Every runtime exit point must emit exactly one of `{plan,noop,committed,fallback,error}`; update grouped commit, single commit, merge guard, fallback, and `--all` paths.

### ST4 Logging policy and stream control
**Files:** src/debug.rs, src/main.rs, providers, ui code
**Tests:** `cargo test debug::tests`, stderr assertions in acceptance
**Estimate:** M
**Dependencies:** ST1

Add `GCM_LOG_LEVEL` and implement level precedence (`GCM_LOG_LEVEL` over `GCM_DEBUG`). Keep default `off`; preserve existing `GCM_DEBUG` behavior for legacy callers.

### ST5 Acceptance + automation test updates
**Files:** scripts/acceptance.sh, docs/README.md, scripts/test wrappers
**Tests:** acceptance matrix in this spec
**Estimate:** M

Add coverage for AC-1 through AC-13, including `--all --plan-only --json`, provider-failure-to-error, fallback+commit, non-tty error, and stdout/stderr separation.

### ST6 Document machine-mode and reset behavior
**Files:** src/cli.rs (help), README.md
**Tests:** human review + smoke `cargo run -- --help`
**Estimate:** S

Document JSON envelopes and logging policy. `--reset` should emit a human response (unless `--json` is on, in which case emit `{v:1,status:"reset",...}` or explicitly document non-emit behavior before implement.

## 4. Evaluation

| # | Scenario | Input | Expected | Verification |
|---|---|---|---|---|
| 1 | JSON stream purity | dirty repo + `--plan-only --json` | valid single JSON object only | `cargo run -- --plan-only --json \\| jq -e '.status'`
| 2 | Noop behavior | clean repo + `--plan-only --json` | `status: noop`, exit 0 | `cargo run -- --plan-only --json >/tmp/gcm.out; jq -e '.status=="noop"' /tmp/gcm.out`
| 3 | Non-interactive guard | non-TTY + `--json` | `error` with `NonInteractive` | `printf '' \| cargo run -- --json \\| jq -e '.error.code=="NonInteractive"'`
| 4 | Plain mode output unchanged | dirty repo + `--plan-only` | non-json human preview and no commit | manual/`scripts/acceptance.sh` smoke
| 5 | Unattended grouped commit | dirty repo + `--yes --json` | `status: committed` | `cargo run -- --yes --json \\| jq -e '.status=="committed"'`
| 6 | Grouping fallback with json | provider parse failure + `--yes --json` | `status: fallback` plus `fallback.reason` and `commit` | `... /tmp fixture ... /tmp/gcm.out; jq -e '.status=="fallback" and .fallback.reason and .commit'`
| 7 | All-path preview | dirty repo + `--all --plan-only --json` | `status: plan`, `mode: single`, no mutation | `cargo run -- --all --plan-only --json \\| jq -e '.status=="plan" and .mode=="single"'`
| 8 | All-path execute | dirty repo + `--all --yes --json` | `status: committed`, single mode, no `plan.groups` | `cargo run -- --all --yes --json \\| jq -e '.status=="committed" and .mode=="single" and .plan==null'`
| 9 | Error serialization on missing key | run without provider key + `--yes --json` | `status: error`, `error.code == "Provider"` | `... \\| jq -e '.status=="error" and .error.code=="Provider"'`
| 10 | Logging level and stream policy | `GCM_LOG_LEVEL=warn` + `--plan-only --json` | stderr only for logs, valid json on stdout | check stderr pattern and `jq -e '.status'`
| 11 | `--dry-run` vs `--plan-only` schema | both flags with `--json` | both `plan` with modes set (`dry_run`, `plan_only`) | `jq -e '.status=="plan" and (.mode=="dry_run" or .mode=="plan_only")'`
| 12 | Exit code contract | JSON error/no-error cases | exit codes match AC-13 | assertion scripts in acceptance

## 5. Edge cases and constraints

#### Constraints
- **Must:** emit exactly one JSON object on stdout per json-mode invocation (`v:1` contract).
- **Must:** send all logs/warnings (including `curated_index_warning`, merge warnings, and provider/commit diagnostics) to `stderr`.
- **Must:** include `mode` in all envelopes for routing clarity.
- **Must-not:** use async runtimes (`main` flow remains synchronous).
- **Must-not:** change existing `Plan` persistence schema.

#### Edge cases
- `gcm --plan-only --all` and `gcm --dry-run --all` are both non-mutation and should produce `mode: "single"` preview payloads.
- Legacy `GCM_DEBUG` should remain effective; if both are set, `GCM_LOG_LEVEL` controls level granularity.
- Merge detection paths (`repo.is_merging()`) should use deterministic json status/mode and avoid mixed stdout diagnostics.
- Provider errors with `RateLimit/Timeout/Transport/Deserialize/MissingKey` etc. must be serialized with deterministic `error.code` mapping.
- Partial staging should be preserved under JSON-mode for existing safety warnings; warnings themselves remain on `stderr`.
- Unknown/unsupported provider names and `--plan-only` edge paths should emit `error` status with clear machine-readable message and non-zero exit.
- `--reset` behavior under `--json` must be explicitly defined and covered by acceptance (either reset status envelope or documented non-output).