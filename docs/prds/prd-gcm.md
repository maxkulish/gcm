# PRD: gcm - AI Git Commit Tool (Rust Rewrite)

| Field | Value |
|-------|-------|
| Author | Max Kulish |
| Status | Draft |
| Created | 2026-06-18 |
| Last Updated | 2026-06-18 |
| Stakeholders | Max Kulish (owner, primary user); future open-source contributors |

## 1. Overview

`gcm` generates well-structured git commits from your working tree. Instead of producing one message for everything, it asks an LLM to split the changes into logical groups and commits them one group at a time, so history stays clean and conventional. It exists today as a ~490-line bash script (v2.7) that has outgrown its shape: every hard problem (JSON parsing, reasoning suppression, error handling) is forced through text manipulation because it shells out to a now-archived CLI (`mods`).

This PRD defines a ground-up rewrite in **Rust** that talks to provider HTTP APIs **directly**. The rewrite preserves every behavior users depend on (aliases, flags, grouping, plan cache, signed commits) and fixes the structural problems that the bash version could not reach: exact JSON parsing via structured outputs, native reasoning control, and typed errors with retries. It is built to be a **shareable, cross-platform** open-source tool, not just a personal script.

## 2. Problem & Objectives

### Problem Statement

The bash implementation works but has hit a structural ceiling. Because it pipes prompts through a CLI middleman, three problems are unsolvable in place:

1. **Parsing is heuristic and fragile.** A three-stage `sed` -> `perl -0777` -> `jq` pipeline extracts the grouping JSON. Any reasoning text, stray brace, or fenced block can corrupt the capture, at which point the tool silently falls back to a single lumped commit, defeating its entire purpose. Reasoning models such as Groq `qwen/qwen3.6-27b` emit inline `<think>` blocks on stdout that have already caused this failure and, worse, risked pasting chain-of-thought into commit history.
2. **Native model controls are unreachable.** JSON mode / structured outputs, `reasoning_effort`, and `reasoning_format` would eliminate the parsing problem at the source, but the CLI does not pass them through.
3. **Errors collapse to one bucket.** Every failure (HTTP 429 rate limit, HTTP 400 bad parameter, truncated stream, missing binary) surfaces as "empty response," which routes to the single-commit fallback. There is no way to retry, back off, or diagnose.

Compounding this, the underlying CLI (`mods`) was archived by its maintainer on 2026-03-09 and receives no further fixes.

The person affected is the tool's daily user (and any future adopter), every time they commit. The cost is silent quality degradation: commits that should have been cleanly grouped get lumped, and transient provider errors look like tool bugs.

### Objectives

- **O1**: Eliminate the parser bug class. Reach a grouping-plan parse success rate of >= 99% across all supported providers and models, including reasoning models, measured over real commit sessions.
- **O2**: Replace the archived CLI dependency with direct provider HTTP integrations for all active providers, removing `mods` and `crush` from the runtime entirely.
- **O3**: Preserve behavioral parity. A user who repoints their alias notices no regression in commands, flags, grouping behavior, cache semantics, or signed commits.
- **O4**: Ship as an installable, cross-platform (macOS + Linux), open-source binary with no hardcoded personal paths.

### Success Metrics (KPIs)

| Metric | Current (bash v2.7) | Target | How Measured |
|--------|---------------------|--------|--------------|
| Grouping-plan parse success rate | Unmeasured; known failures on reasoning models | >= 99% | Count plan parses vs fallback-due-to-parse events in debug logs over 100+ sessions |
| Fallback-to-single-commit rate (non-`--all`) | Unmeasured | < 5% | Ratio of fallback invocations to grouped runs in logs |
| Reasoning leakage into commit messages | Possible (regex-strip only) | 0 occurrences | Structured-output contract + grep audit of generated messages |
| Distinguishable error types surfaced | 1 (empty response) | >= 4 (429, 400, 5xx, timeout) | Error taxonomy present in logs and exit behavior |
| Cold-start latency (binary start to first git call) | TBD | < 50 ms p95 | Local benchmark |
| End-to-end run latency (excluding LLM time) | TBD | < 200 ms p95 | Instrumented timing minus provider round-trip |
| Supported platforms with passing release build | 1 (macOS) | 2 (macOS, Linux), x86_64 + arm64 | CI release matrix |

## 3. Users & Use Cases

### Personas

| Persona | Role | Need | Pain Point |
|---------|------|------|------------|
| Max (owner) | Senior engineer, heavy daily committer across many repos | Fast, clean, logically-grouped, signed commits with minimal friction | Bash version silently lumps commits when parsing fails; provider breakage is opaque |
| Adopter | Engineer who finds the OSS repo | `brew install` / `cargo install`, point an env var at their API key, and go | Personal scripts assume macOS, `/opt/script` paths, and pre-set provider config |
| Contributor | OSS developer extending providers | Add a provider behind a stable trait without touching core flow | Bash has no module boundaries; logic is one 490-line file |

### Key Use Cases

**UC-1: Grouped commit of a mixed working tree**
- Trigger: User has multiple unrelated edits staged and unstaged, runs `gcm`.
- Steps:
  1. Tool gathers `git status` + `git diff HEAD` + untracked file content, applies binary-safe elision and per-provider truncation.
  2. Tool makes one direct API call requesting a JSON-schema-constrained grouping plan.
  3. Tool deserializes the plan into typed `Plan`/`Group` values, validates every filename against the real change set.
  4. Tool displays the groups, marks group 1 "committing now."
  5. User confirms `[Y/n/e]`; tool stages only group 1's files and runs a GPG-signed commit.
  6. Tool advances the per-repo plan cache to the remaining groups.
- Outcome: One logical, signed commit; re-running `gcm` commits the next group with no new LLM call.

**UC-2: Provider/model selection**
- Trigger: User runs `gcmq` (Groq), `gcmg` (Gemini), or `gcmq27` (Groq reasoning model).
- Steps: Alias passes `--provider` and optionally a model override; tool dispatches to the matching direct-API provider implementation with that provider's diff cap, JSON-mode, and reasoning settings.
- Outcome: Identical grouping UX regardless of provider; reasoning output never reaches the parser or the commit message.

**UC-3: Single-commit and preview modes**
- Trigger: `gcm --all` (one commit for everything) or `gcm --dry-run` (preview grouping, commit nothing) or `gcm --reset` (discard cached plan, re-analyze).
- Steps: Tool follows the corresponding path; `--dry-run` uses/saves but does not advance the cache; `--all` and any fallback clear it.
- Outcome: User controls grouping granularity and can preview before committing.

**UC-4: Transient provider error**
- Trigger: Provider returns HTTP 429 or 5xx mid-run.
- Steps: Tool recognizes the typed error, retries with exponential backoff up to a bounded limit, and surfaces a clear message if it ultimately fails (distinct from a parse failure).
- Outcome: Rate limits and blips self-heal instead of masquerading as a tool bug or a silent fallback.

## 4. Functional Requirements

### FR Group: Core Commit Workflow

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-1 | Generate a grouping plan from working-tree changes in a single LLM call | Must | Given changed files, the tool produces a plan with `groups[]`, each having `files`, `summary`, and (group 1 only) `commit_message` |
| FR-2 | Commit exactly one group per invocation, advancing through groups on re-run | Must | After confirming, only group 1's files are staged and committed; the next run commits the next group without a new LLM call |
| FR-3 | Stage scoped to the committed group | Must | Tool runs the equivalent of `git reset` then stages only group 1's paths; no unrelated files enter the commit |
| FR-4 | GPG-sign every commit | Must | All commits are created with signing enabled (`-S` equivalent); a repo without signing configured produces a clear error |
| FR-5 | Interactive confirmation with edit option | Must | Prompt offers `[Y/n/e]`; `e` opens `$EDITOR` (default `vim`) on the message; `n` aborts with exit 0 |
| FR-6 | `--all` single-commit mode | Should | Stages all changes, requests one conventional-commit message, commits once, clears the cache |
| FR-7 | `--dry-run` preview | Should | Prints the grouping plan, commits nothing, does not advance the cache |
| FR-8 | `--reset` forces re-analysis | Should | Deletes the cached plan before running and always calls the LLM |
| FR-9 | Abort is not an error | Must | User declining the prompt exits 0; only genuine failures exit non-zero |

### FR Group: Multi-Provider LLM Backend (Direct HTTP)

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-10 | Call provider REST APIs directly; no `mods`/`crush`/`claude` CLI in the runtime | Must | Network calls go to provider endpoints; no subprocess LLM CLI is invoked |
| FR-11 | Provider abstraction behind a trait | Must | Adding a provider requires implementing one trait and registering it; core flow is unchanged |
| FR-12 | Provider selection via flag, env var, and alias | Must | `--provider=<name>`, `GCM_PROVIDER`, and per-provider aliases all select the backend; precedence is flag > env > default |
| FR-13 | Support the active provider matrix | Should | Groq, Google (Gemini), and Anthropic are callable; Cerebras is supported in config but may be disabled by default |
| FR-14 | Per-invocation model override | Should | A `--model` flag and a `GCM_GROQ_MODEL`-equivalent select the model; `gcmq20`/`gcmq27` map to their models |
| FR-15 | Per-provider diff size caps | Should | Diff is truncated to the provider's configured char budget before the call (Anthropic/Haiku 80k, Groq 350k, Cerebras 400k, Google 500k as starting values) |
| FR-16 | Request JSON via structured outputs / JSON mode where supported | Must | Groq requests `response_format` json_schema; Gemini uses `responseSchema`; Anthropic uses tool-use or JSON instruction; the response deserializes without heuristic extraction |
| FR-17 | Control reasoning output per model | Must | For reasoning models the request sets reasoning suppression (`reasoning_effort: none` / `reasoning_format: hidden`/`parsed` as the provider supports), so no chain-of-thought reaches stdout |
| FR-18 | Resolve API keys from environment | Must | Keys read from `GROQ_API_KEY`, `GEMINI_API_KEY`, `ANTHROPIC_API_KEY`, `CEREBRAS_API_KEY`; a missing key for the selected provider produces a clear, actionable error |

### FR Group: Robust Parsing & Error Handling

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-19 | Typed deserialization of the plan | Must | Response maps to `Plan`/`Group` structs via serde; type mismatch is a recoverable, logged error |
| FR-20 | Defensive fallback parsing | Should | If structured output is unavailable, a layered extractor (direct -> wrapper-key unwrap -> generic `groups` key) recovers the plan; a residual `<think>` strip remains only as last-resort defense |
| FR-21 | Typed error taxonomy | Must | The tool distinguishes rate limit (429), bad request (400), server error (5xx), timeout, auth failure, and parse failure as separate error types |
| FR-22 | Retry with backoff on transient errors | Must | 429 and 5xx are retried with exponential backoff up to a bounded attempt count; 400 and auth errors are not retried |
| FR-23 | Validate filenames against the real change set | Must | Any file in the plan not present in `git status` triggers fallback rather than a bad `git add` |
| FR-24 | Single-commit fallback on grouping failure | Must | Empty response, unparseable plan, hallucinated files, or missing group-1 message all degrade to a single-commit flow that never blocks the user |

### FR Group: Plan Cache

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-25 | Per-repo plan cache keyed by repo root | Must | Cache key is `sha256(repo-root-path)`; cache is shared across providers for the same repo |
| FR-26 | Advance cache on commit | Must | After committing group 1, the cache holds only the remaining groups; the cache is deleted when the last group is committed |
| FR-27 | Stale-cache detection | Must | Before reuse, the cached file set is compared to the current `git status` set; a mismatch discards the cache and re-calls the LLM |
| FR-28 | Cache invalidation paths | Must | `--reset` deletes the cache up front; `--all` and any fallback clear it |
| FR-29 | Cross-platform cache location | Should | Cache lives in an OS-appropriate temp/cache directory (not a hardcoded `/tmp` path on all platforms); file permissions restrict it to the current user |
| FR-30 | Backward-readable bash cache format | Could | During personal cutover, an in-flight bash cache file is still readable by the Rust tool |

### FR Group: Diff Gathering & Safety

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-31 | Gather staged + unstaged diff without staging up front | Must | Context is built from `git status --porcelain` and `git diff HEAD`; nothing is staged during analysis |
| FR-32 | Binary-file detection and elision | Must | Files detected as binary are replaced with a placeholder; non-UTF-8 bytes never corrupt the prompt or the tool |
| FR-33 | Rename and delete handling | Must | Renames use the new path; deletes are guarded so staging does not error |
| FR-34 | Untracked file inclusion | Should | Untracked text files contribute their content (capped per file) to the analysis context; collapsed untracked directories are expanded to individual files |

### FR Group: CLI, Configuration & Observability

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-35 | Preserve all current flags | Must | `--provider`, `--all`, `--dry-run`, `--reset` behave as in bash |
| FR-36 | `--version` reporting | Should | `gcm --version` prints a build-stamped version string |
| FR-37 | `--json` machine-readable output | Should | A global `--json` flag emits structured output suitable for scripting |
| FR-38 | Structured logging with level control | Should | `GCM_LOG_LEVEL=debug` (and `DEBUG_GCM=1` as a synonym) emit structured debug logs to stderr; default output stays clean |
| FR-39 | Distinct usage exit code | Should | Invalid CLI usage exits 2; runtime errors exit 1; success and user-abort exit 0 |
| FR-40 | Config file for providers and models | Should | Providers, models, endpoints, and diff caps are configurable via a documented config file in an OS-appropriate location, with env/flag overrides; no personal paths are hardcoded |

### FR Group: Distribution & Cross-Platform

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-41 | Single self-contained binary | Must | The tool ships as one binary with no runtime dependency on an LLM CLI |
| FR-42 | macOS and Linux support | Should | Release builds pass on macOS and Linux for x86_64 and arm64 |
| FR-43 | Documented install path | Should | README documents install via release binary and/or `cargo install`; no assumption of `/opt/script` |
| FR-44 | Reversible personal cutover | Should | Repointing the shell alias from the bash script to the Rust binary is documented, and rollback is a one-line alias change with the bash script left intact |

### Data Model: Grouping Plan

The grouping plan is the central contract between the LLM and the tool. It is requested via structured outputs (FR-16), deserialized into typed values (FR-19), and persisted verbatim as the per-repo cache (FR-25). The schema is preserved from bash v2.7.

```jsonc
// LLM response and on-disk cache share this shape
{
  "groups": [
    {
      "files": ["src/parse.rs", "src/parse_test.rs"], // paths from the real change set only
      "summary": "Group rationale, one line",           // human-readable grouping reason
      "commit_message": "fix(parse): handle nested braces\n\nbody..." // ONLY group[0]; null for the rest
    }
  ]
}
```

Conceptual Rust types (final shape decided in the design doc):

```rust
struct Plan { groups: Vec<Group> }
struct Group {
    files: Vec<String>,           // validated against `git status` (FR-23)
    summary: String,
    commit_message: Option<String>, // Some only for groups[0]
}
```

Invariants:
- Only `groups[0]` carries a `commit_message`; later groups get their message on the run that commits them.
- Every `files` entry must exist in the current change set, or the plan is rejected to fallback (FR-23, FR-24).
- The cache holds only the not-yet-committed groups; committing `groups[0]` rewrites the cache as `groups[1..]` (FR-26).

## 5. Non-Functional Requirements

| Category | Requirement | Target |
|----------|-------------|--------|
| Performance | Cold start to first git call | < 50 ms p95 |
| Performance | End-to-end overhead excluding provider round-trip | < 200 ms p95 |
| Reliability | Grouping failure never blocks committing | 100% of failures degrade to single-commit fallback |
| Reliability | Transient provider errors are retried | 429/5xx retried with bounded exponential backoff |
| Security | API keys are read from env/config and never logged or written to the cache | No key material in logs, cache, or commit content |
| Security | Cache file is user-only readable | Mode 0600-equivalent on the plan cache |
| Security | Commits are GPG-signed | All commits signed; signing misconfiguration surfaces clearly |
| Portability | Operating systems | macOS + Linux; x86_64 + arm64 |
| Maintainability | Module boundaries | Provider, parse, cache, git, ui, cli, config as separate units; provider is a trait |
| Maintainability | Test coverage of the parser | Parser tested against clean, fenced, preamble, nested, refusal, and reasoning-polluted fixtures |
| Compatibility | Behavioral parity with bash v2.7 | Same aliases, flags, env vars, grouping semantics, signing, exit-code intent |
| Availability | Uptime / MTTR SLA | Not applicable: local single-process CLI with no long-running service; availability is bounded by the user's machine and provider API |
| Scalability | Concurrent users / data volume | Not applicable: one invocation per user shell; the only scale axis is diff size, bounded by per-provider caps (FR-15) |
| Compliance | Regulatory standards | Not applicable: no PII storage or processing; the only sensitive data is API keys, handled under Security above |

## 6. Scope & Phasing

### In Scope (v1)

- Rust binary that talks to provider HTTP APIs directly for Groq, Google (Gemini), and Anthropic.
- Full grouped-commit workflow with plan cache, signed commits, and single-commit fallback.
- Structured outputs / JSON mode and per-model reasoning control.
- Typed errors with retry/backoff.
- Cross-platform builds (macOS + Linux, x86_64 + arm64) and documented install.
- Configurable providers/models with no hardcoded personal paths.
- `--version`, `--json`, structured logging, distinct usage exit code.

### Out of Scope

- **Hunk-level (sub-file) grouping** - reason: file-level grouping covers ~90% of cases; sub-file splitting is a large addition deferred to a later phase.
- **Interactive group reordering / commit-all-groups-in-one-run** - reason: the one-group-per-run model is intentional and matches current muscle memory; batch modes are a future enhancement.
- **Windows support** - reason: not a current target platform; revisit on demand.
- **GUI/TUI beyond the confirm prompt** - reason: scope control; the CLI prompt is sufficient for v1.
- **Conventional-commit linting/enforcement and pre-commit hook integration** - reason: separate concern from message generation.

### Future Phases

| Phase | Features | Depends On |
|-------|----------|------------|
| Phase 2 | Hunk-level grouping; commit-all-groups mode; richer config UX | v1 stable |
| Phase 3 | Additional providers (local models, OpenAI, others); plugin-style provider discovery | Provider trait stable |
| Phase 4 | Windows support; packaging for Homebrew/AUR; optional TUI | Cross-platform CI mature |

## 7. Dependencies

| Dependency | Owner | Status | Risk if Delayed |
|------------|-------|--------|-----------------|
| Rust toolchain (1.x, async runtime) | Owner | Available | None |
| Provider HTTP APIs (Groq, Gemini, Anthropic) | External | Available | API/schema changes break a provider |
| Provider API keys / accounts | Owner / adopter | Owner has keys | Adopters must supply their own; documented in README |
| git with GPG signing configured | User environment | Required | Unsigned commits / errors if missing |
| git access method (libgit2 via `git2` vs shelling to `git`) | Owner | Decision pending (see Open Questions) | Affects portability and parity |
| Structured-output / reasoning-control support per provider | External | Varies by provider | Falls back to defensive parsing where unsupported |
| Release/cross-compile tooling (e.g. `cargo-dist` / `cross`) | Owner | To be set up | Slower multi-platform releases |

## 8. Risks & Open Questions

### Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Each provider's request/response and structured-output schema differs, multiplying integration work | H | M | Provider trait isolates differences; start with Groq (OpenAI-compatible), then Gemini, then Anthropic |
| Reasoning-control parameters differ by provider/model and change over time | M | M | Centralize per-model reasoning config; keep `<think>` strip as defensive fallback |
| Anthropic direct API requires a paid API key, unlike the subscription-based `claude` CLI used today | M | M | Document the auth model; allow Anthropic to be optional; consider a free-tier-friendly default provider for adopters |
| Structured outputs unsupported or buggy for a given model | M | M | Layered defensive parser (FR-20) recovers the plan |
| GPG signing behaves differently across platforms / CI | M | M | Test signing on macOS and Linux; clear error when unconfigured |
| Scope creep from "shareable" requirements (config UX, docs, cross-platform) delays v1 | M | M | Keep config additions to Should/Could; ship core parity + fixes first |
| Provider deprecates a hardcoded model (as Kimi K2 was) | M | L | Models are config-driven, not compiled in |

### Open Questions

- [ ] **Anthropic backend**: direct Messages API with `ANTHROPIC_API_KEY`, or retain an optional `claude` CLI path for subscription users? - owner: Max, deadline: before design doc
- [ ] **git access**: use `git2`/libgit2 for portability and typed errors, or shell out to `git` for exact parity with the bash behavior? - owner: Max, deadline: before design doc
- [ ] **Default provider for adopters**: which provider should a fresh install default to, given Anthropic needs a paid key and Groq/Gemini offer free tiers? - owner: Max, deadline: before design doc
- [ ] **Config file format and location**: TOML/JSON under XDG config dir? What is the precedence chain with env vars and flags? - owner: Max, deadline: design
- [ ] **Cache location cross-platform**: keep `/tmp`-style temp dir or move to an OS cache dir; does FR-30 (bash cache compat) justify keeping the old path on macOS? - owner: Max, deadline: design
- [ ] **Cerebras**: include in v1 (currently paused for rate limits) or defer to Phase 3? - owner: Max, deadline: before design doc
- [ ] **Async vs sync runtime**: is `tokio` warranted for a single-call CLI, or is a blocking HTTP client simpler and faster to start? - owner: Max, deadline: design

## 9. Rollout & Measurement

### Release Plan

- **Personal cutover (primary user)**: Build and install the binary, repoint the `~/.zshrc` aliases from `/opt/script/git-commit-ai.sh` to the Rust binary, and run side-by-side for a short observation window. Rollback is a one-line alias revert; the bash script stays in place untouched.
- **Open-source release**: Tag `v0.1.0`, publish cross-platform release binaries (macOS + Linux, x86_64 + arm64), document install and provider setup in the README, and keep models/providers config-driven so adopters can adjust without recompiling.
- **Migration safety**: Preserve cache semantics so an in-flight session survives the swap; validate grouping parity against the bash version on a scratch repo before cutover (LLM output is non-deterministic, so message text need not be byte-identical, but group structure should match).

### Measurement Plan

- Instrument debug logs to record: plan parse success/failure, fallback invocations and their cause, error types encountered, and timing (cold start, total overhead).
- First measurement after the first 100 real commit sessions post-cutover.
- Decision criteria:
  - **Continue/expand** if parse success >= 99% and fallback rate < 5%.
  - **Investigate** if any provider shows parse success < 95% or repeated unexplained fallbacks.
  - **Pivot** a provider integration to defensive parsing (or drop the provider from defaults) if its structured-output support proves unreliable.
