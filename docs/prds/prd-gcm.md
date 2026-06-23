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

**UC-5: First-run onboarding**
- Trigger: A new user installs the binary and runs `gcm` for the first time with no config and no provider set up.
- Steps:
  1. Tool detects no config and no usable provider, and starts an interactive wizard.
  2. Wizard offers the v1 providers (Groq, Gemini, Anthropic, OpenAI, and local Ollama) and lets the user enable one or more.
  3. For each enabled cloud provider, the wizard uses the API key from the environment if present, otherwise prompts for it; for Ollama it confirms a reachable local endpoint instead.
  4. User picks which enabled provider is the default for a bare `gcm`.
  5. Tool writes the config (secrets with user-only permissions or referenced by env var) and proceeds to the normal commit flow.
- Outcome: A first-time user reaches a working commit without hand-editing config or guessing env-var names; re-running `gcm config` adjusts choices later.

## 4. Functional Requirements

### FR Group: Core Commit Workflow

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-1 | Generate a grouping plan from working-tree changes in a single LLM call | Must | Given changed files, the tool produces a plan with `groups[]`, each having `files`, `summary`, and (group 1 only) `commit_message` |
| FR-2 | Commit exactly one group per invocation, advancing through groups on re-run | Must | After confirming, only group 1's files are staged and committed; the next run commits the next group, which must already carry a usable commit message (FR-45). No new grouping call is required; the message-generation strategy is defined by FR-45 |
| FR-3 | Stage scoped to the committed group | Must | Tool runs the equivalent of `git reset` then stages only group 1's paths; no unrelated files enter the commit |
| FR-4 | GPG-sign every commit | Must | All commits are created with signing enabled (`-S` equivalent); a repo without signing configured produces a clear error |
| FR-5 | Interactive confirmation with edit option | Must | Prompt offers `[Y/n/e]`; `e` opens `$EDITOR` (default `vim`) on the message; `n` aborts with exit 0 |
| FR-6 | `--all` single-commit mode | Should | Stages all changes, requests one Conventional Commits-formatted message (FR-59), commits once, clears the cache |
| FR-7 | `--dry-run` preview | Should | Prints the grouping plan, commits nothing, does not advance the cache |
| FR-8 | `--reset` forces re-analysis | Should | Deletes the cached plan before running and always calls the LLM |
| FR-9 | Abort is not an error | Must | User declining the prompt exits 0; only genuine failures exit non-zero |
| FR-45 | Every group has a usable commit message when committed | Must | No group reaches the commit step with a null/empty message. The tool either generates messages for all groups up front or regenerates the next group's message on its commit run (strategy chosen in Open Questions). The bash failure mode, where an advanced cache's first group has `commit_message: null` and silently falls back to single-commit-all, must not recur |
| FR-58 | Handle commit failure (e.g. rejecting pre-commit hook) | Must | If `git commit` fails (a pre-commit hook rejects it, signing fails, etc.), the tool does not advance the plan cache (FR-26), leaves the group's files staged so the user can fix and retry, and surfaces the underlying error. A hook that reformats and re-stages the committed group is acceptable; remaining groups stay pending |
| FR-59 | Generate Conventional Commits-formatted messages | Must | Every generated `commit_message` follows the Conventional Commits v1.0.0 shape: a `<type>[optional scope]: <description>` header, optional body, and optional footers (including `BREAKING CHANGE:`). The system prompt directs the model to pick a type/scope; the tool runs a lightweight format check on the header of its own generated output and, on a malformed header, treats it as a recoverable generation issue (regenerate/repair, then fall back per FR-24). This is message *generation* plus a self-check of gcm's own output, not linting of existing commits or pre-commit hook enforcement (out of scope). A correctly-formatted history is what lets downstream tools (e.g. release-please) compute semver bumps and changelogs; gcm produces the format but does not run or configure those tools, and reliable parsing under squash-merge depends on the repo's merge strategy (PR title vs per-commit) |

### FR Group: Multi-Provider LLM Backend (Direct HTTP)

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-10 | Call provider REST APIs directly; no `mods`/`crush`/`claude` CLI in the runtime | Must | Network calls go to provider endpoints; no subprocess LLM CLI is invoked |
| FR-11 | Provider abstraction behind a trait | Must | Adding a provider requires implementing one trait and registering it; core flow is unchanged |
| FR-12 | Provider selection via flag, env var, and alias | Must | `--provider=<name>`, `GCM_PROVIDER`, and per-provider aliases all select the backend; precedence is flag > env > default |
| FR-13 | Support the active provider matrix | Should | Groq, Google (Gemini), Anthropic, and OpenAI are callable via direct HTTP; Ollama is callable via a local endpoint (FR-56). Cerebras is dropped from v1 (ADR-001 #13: unstable public catalog, thin adoption) |
| FR-14 | Per-invocation model override | Should | A `--model` flag and a `GCM_GROQ_MODEL`-equivalent select the model; `gcmq20`/`gcmq27` map to their models |
| FR-15 | Per-provider diff budget with per-file truncation | Should | The diff is fit to the provider's char budget by omitting or truncating the largest files' diffs first, each replaced with an explicit `[diff omitted: N bytes]` placeholder, rather than tail-chopping the concatenated diff. Tail-chopping leaves files at the end of the diff with zero context while the file list still requires them to be grouped (FR-23), forcing the model to fabricate their summaries and messages. Starting budgets: Anthropic/Haiku 80k, Groq 350k, OpenAI per-model, Google 500k |
| FR-16 | Request JSON via structured outputs / JSON mode where supported | Must | Groq requests `response_format` json_schema; Gemini uses `responseSchema`; Anthropic uses tool-use or JSON instruction; the response deserializes without heuristic extraction |
| FR-17 | Control reasoning output per model | Must | For reasoning models the request sets reasoning suppression (`reasoning_effort: none` / `reasoning_format: hidden`/`parsed` as the provider supports), so no chain-of-thought reaches stdout |
| FR-18 | Resolve API keys from environment | Must | Keys read from `GROQ_API_KEY`, `GEMINI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `CEREBRAS_API_KEY`; a missing key for the selected provider produces a clear, actionable error. Local providers (Ollama, FR-56) need no key but a reachable endpoint |
| FR-52 | Verify provider capability before building its integration | Should | A provider capability matrix (structured-output support and its JSON-Schema subset, reasoning controls, and their interaction constraints) is verified against current provider docs and recorded with a verification date before that provider's integration ships. FR-16/FR-17 are implemented per the verified capability, with FR-20 defensive parsing where a capability is absent |
| FR-56 | Local / self-hosted provider support (Ollama) | Should | A local provider talks to an OpenAI-compatible endpoint (default `http://localhost:11434`, overridable via `OLLAMA_HOST`/config), requires no API key, and uses a user-selected local model. It is the zero-egress option (no diff leaves the machine), reinforcing Privacy (FR-48 to FR-50). The tool surfaces a clear error when the endpoint is unreachable |

#### Provider Capability Matrix (verified - FR-52)

Verified against live vendor docs on 2026-06-19; full sources and confidence caveats in [ADR-001](../adrs/001-foundational-architecture-decisions.md) Appendix A.

| Provider | Structured output | Reasoning control | Known constraint | Verified |
|----------|-------------------|-------------------|------------------|----------|
| Groq | `response_format` json_object / json_schema; `strict:true` only on `openai/gpt-oss-20b` & `gpt-oss-120b` | `reasoning_effort` (gpt-oss: low/med/high, no `none`), `reasoning_format`, `include_reasoning` | `reasoning_format:raw` + JSON mode = HTTP 400 (use parsed/hidden); gpt-oss reasoning is hide-only (`include_reasoning:false`), only Qwen3 fully disables; streaming + tools unsupported with json_schema | 2026-06-19 |
| Google Gemini | `responseMimeType:"application/json"` + `responseSchema` (OpenAPI-3.0 subset; `$ref`/`allOf`/`oneOf` ignored) | 3.x `thinkingLevel` (MINIMAL/LOW/MED/HIGH); legacy `thinkingBudget` | No hard "off" on 3.x (floor `minimal`); `gemini-3.1-flash-lite` is current GA; `thinkingLevel`+`thinkingBudget` together = 400; only `gemini-2.5-flash-lite` `thinkingBudget:0` truly disables | 2026-06-19 |
| Anthropic | Forced tool-use (`tools` + `tool_choice` + `input_schema`, optional `strict`) or `output_config.format` (Opus 4.8 / Sonnet 4.6 / Haiku 4.5) | adaptive thinking; CoT omitted by default | No generic `response_format`; structured output is via a forced tool call | 2026-06-19 |
| OpenAI | Structured Outputs (`response_format` json_schema, `strict:true`) | reasoning models hide CoT by default; `reasoning_effort` | `gpt-4o-mini` supported & non-reasoning (zero CoT to suppress); `gpt-4.1-mini` NOT reliably supported for strict json_schema | 2026-06-19 |
| Ollama (local) | native `/api/chat` `format`=JSON-Schema object (or `"json"`); OpenAI-compat `response_format` | `think` bool (gpt-oss: low/med/high, not fully off); thinking separated into `message.thinking` | structured output local-only (not Ollama Cloud); fidelity model-dependent → validate + retry (FR-20); no key, `OLLAMA_HOST` override | 2026-06-19 |
| Cerebras (dropped) | OpenAI-compat json_schema + `strict` (confirmed `gpt-oss-120b`) | `reasoning_effort` + `reasoning_format` (`raw` incompatible w/ JSON) | DROPPED from v1 (ADR-001 Decision 13): Qwen family removed 2026-05-27 (catalog: `gpt-oss-120b`, `zai-glm-4.7`); free tier 5 RPM; unstable public catalog | 2026-06-19 |

### FR Group: Robust Parsing & Error Handling

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-19 | Typed deserialization of the plan | Must | Response maps to `Plan`/`Group` structs via serde; type mismatch is a recoverable, logged error |
| FR-20 | Defensive fallback parsing | Should | If structured output is unavailable, a layered extractor (direct -> wrapper-key unwrap -> generic `groups` key) recovers the plan; a residual `<think>` strip remains only as last-resort defense |
| FR-21 | Typed error taxonomy | Must | The tool distinguishes rate limit (429), bad request (400), server error (5xx), timeout, auth failure, and parse failure as separate error types |
| FR-22 | Retry with backoff on transient errors | Must | 429 and 5xx are retried with exponential backoff up to a bounded attempt count; 400 and auth errors are not retried |
| FR-23 | Validate the plan against the real change set | Must | The plan is rejected to fallback if it (a) names a file absent from `git status` (hallucination), (b) omits any changed file, (c) lists a file in more than one group, or (d) contains an empty group. Commit-message placement is validated per the chosen contract (FR-45). The bash validator only checks (a), so omissions silently drop files from history |
| FR-24 | Single-commit fallback on grouping failure | Must | After retries are exhausted (FR-22), an empty/unparseable/invalid plan or missing message degrades to a single-commit flow. The fallback warns that grouping failed and why, stages changes only after the user confirms the lumped commit, and on decline restores the pre-run index rather than leaving everything staged. It never silently switches modes mid-stream or blocks the commit workflow |

### FR Group: Plan Cache

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-25 | Per-repo plan cache keyed by repo root | Must | Cache key is `sha256(repo-root-path)`; cache is shared across providers for the same repo |
| FR-26 | Advance cache on commit | Must | After committing group 1, the cache holds only the remaining groups; the cache is deleted when the last group is committed |
| FR-27 | Stale-cache detection by content, re-stamped on advance | Must | The cache stores a fingerprint of (sorted pending-file set + per-file content hash of the not-yet-committed files + provider/model + prompt/schema version). Reuse requires a match; any mismatch re-analyzes. The fingerprint covers only pending files and is recomputed when the cache advances (FR-26), so committing a group does not self-invalidate the remainder. Edits and external commits are caught via the content hashes; the fingerprint must NOT naively pin the analysis-time `HEAD`, or every post-commit run would invalidate and defeat FR-26. Filename-set equality alone (the bash behavior) is insufficient: content can change after a `--dry-run` while names stay identical |
| FR-28 | Cache invalidation paths | Must | `--reset` deletes the cache up front; `--all` and any fallback clear it |
| FR-29 | Cross-platform cache location | Should | Cache lives in an OS-appropriate temp/cache directory (not a hardcoded `/tmp` path on all platforms); file permissions restrict it to the current user |
| FR-30 | Backward-readable bash cache format | Could | During personal cutover, an in-flight bash cache file is still readable by the Rust tool |

### FR Group: Diff Gathering & Safety

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-31 | Gather changes without staging, using NUL-delimited paths | Must | Context is built from `git status --porcelain=v1 -z` (or v2 `-z`) and the diff, all run with `-c core.quotePath=false` so path representation is identical between the file list and the diff. Otherwise C-escaped diff paths (git's default for non-ASCII) mismatch the raw `-z` paths and trip strict validation (FR-23) into silent fallback. Paths with spaces, quotes, newlines, unicode, or literal `->` substrings are parsed from NUL fields, not line/awk heuristics. On an unborn branch / empty repo (no `HEAD`), the diff is taken against the empty tree (or `--cached`) instead of `HEAD`. Nothing is staged during analysis |
| FR-32 | Binary-file detection and elision | Must | Files detected as binary are replaced with a placeholder; non-UTF-8 bytes never corrupt the prompt or the tool |
| FR-33 | Rename and delete handling | Must | Renames are read from porcelain `-z` rename records (old and new as separate NUL fields, not a greedy ` -> ` split); the new path is used; deletes are guarded so staging does not error |
| FR-34 | Untracked file inclusion | Should | Untracked text files contribute their content (capped per file) to the analysis context; collapsed untracked directories are expanded to individual files, subject to the bounds in FR-57 |
| FR-57 | Bound untracked expansion to avoid runaway I/O | Must | Untracked-directory expansion and content reading are capped by file count and total bytes (default e.g. 50 files / a few hundred KB). Beyond the cap, the tool includes names only (no content) or aborts with a warning to update `.gitignore`, rather than expanding, stat-ing, and reading thousands of files (e.g. an un-ignored `node_modules`/`target`), which would freeze the CLI and exhaust the context window before truncation runs |

### FR Group: Index & Repo Safety

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-46 | Do not silently destroy a curated index | Should | If files are already staged (especially partial/hunk-level staging via `git add -p`) when the tool runs, it warns before resetting the index. v1 does not preserve partial staging (it groups whole files); this limitation is documented, not silent as in bash |
| FR-47 | Commit is transactional: generate before mutating, restore on failure or abort | Must | The commit message is generated before any staging. If generation fails, or the user declines the commit, the index is restored to its pre-run state rather than left staged. The bash flows mutate the index before confirmation (`git add -A` in fallback, `reset`+`add` in grouped), so a failure or abort leaves a mutated index; the rewrite captures the original index/worktree state and restores it on any non-commit exit |

### FR Group: CLI, Configuration & Observability

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-35 | Preserve all current flags | Must | `--provider`, `--all`, `--dry-run`, `--reset` behave as in bash |
| FR-36 | `--version` reporting | Should | `gcm --version` prints a build-stamped version string |
| FR-37 | `--json` machine-readable output (preview-oriented) | Should | `--json` emits the structured plan and outcome; it does not by itself bypass the interactive prompt. Automation combines it with a non-interactive flag (FR-51) to either commit unattended or only preview |
| FR-38 | Structured logging with level control | Should | `GCM_LOG_LEVEL=debug` (and `DEBUG_GCM=1` as a synonym) emit structured debug logs to stderr; default output stays clean |
| FR-39 | Distinct usage exit code | Should | Invalid CLI usage exits 2; runtime errors exit 1; success and user-abort exit 0 |
| FR-40 | Config file for providers and models | Should | Providers, models, endpoints, and diff caps are configurable via a documented config file in an OS-appropriate location, with env/flag overrides; no personal paths are hardcoded |
| FR-51 | Non-interactive operation for scripts and agents | Should | A `--yes`/`--no-input` flag auto-confirms the commit (for agentic/CI use); a `--plan-only` flag emits the grouping plan and exits without committing. Their interaction with `--json` is defined so a caller can fetch a machine-readable plan and either commit unattended or only preview |

### FR Group: First-Run Onboarding & Configuration

A new user with no configuration must reach a working first commit without hand-editing config files or guessing env-var names. On first run with no config and no usable provider, the tool runs a short interactive setup rather than failing.

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-53 | Detect first run and launch interactive onboarding | Must | When no config file exists and no provider is configured, the tool starts a guided setup instead of erroring. In a non-interactive context (no TTY, or `--no-input`), it instead prints the exact config/env needed and exits with a clear, non-zero status |
| FR-54 | Onboarding activates providers and sets the default | Must | The wizard presents the v1 providers (Groq, Gemini, Anthropic, OpenAI, and local Ollama), lets the user enable one or more, captures or locates each enabled provider's API key (reads the env var if already set, else prompts and stores it; Ollama needs an endpoint, not a key), and records which enabled provider is the default for a bare `gcm` |
| FR-55 | Onboarding persists config safely and is re-runnable | Should | Choices are written to the config file (FR-40); a `gcm config` / `--reconfigure` entry re-runs the wizard idempotently; secrets are referenced by env var or stored with user-only (0600-equivalent) permissions, never world-readable, never committed |

### FR Group: Distribution & Cross-Platform

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-41 | Single self-contained binary | Must | The tool ships as one binary with no runtime dependency on an LLM CLI |
| FR-42 | macOS and Linux support | Should | Release builds pass on macOS and Linux for x86_64 and arm64 |
| FR-43 | Documented install path | Should | README documents install via release binary and/or `cargo install`; no assumption of `/opt/script` |
| FR-44 | Reversible personal cutover | Should | Repointing the shell alias from the bash script to the Rust binary is documented, and rollback is a one-line alias change with the bash script left intact |

### FR Group: Privacy & Data Egress

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|---------------------|
| FR-48 | Respect gitignore on untracked content | Must | Untracked files are gathered with `--exclude-standard` so gitignored files (e.g. `.env`) are never sent to a provider; this preserves the bash behavior and is the primary secret-leak guard |
| FR-49 | Disclose third-party data egress | Must | The README and `--help` state plainly that diffs and untracked file content are sent to the selected external LLM provider, and link each provider's data-retention / training policy |
| FR-50 | Optional secret scanning and path ignore | Should | A user can exclude paths from analysis (a `gcmignore` or config glob) and opt into a pre-send scan that redacts or aborts on detected credentials in the diff. This FR covers the modes (`off`/`redact`/`abort`), path-ignore, and exit behavior; detection *quality* (rule pack + entropy) is specified in FR-60 |
| FR-60 | Real secret detection engine | Should | The pre-send scan (FR-50) detects credentials with a maintained, data-driven rule pack, not a hand-coded handful of patterns. A vendored ruleset (TOML, derived from the MIT gitleaks / Apache-2.0 Kingfisher corpora with attribution, embedded at build time via `include_str!`) of provider-specific regexes is executed by a real regex engine (the pure-Rust `regex` crate), combined with: (a) known-prefix/format rules, (b) keyword/context-proximity on assignments, and (c) a Shannon-entropy gate with charset-aware thresholds (base64 ~4.5, hex ~3.0, generic ~3.5) and a minimum length, so prefix-less / generically-named secrets (e.g. `GITLAB="<random>"`) are caught. False positives are controlled by structured-value suppression (UUIDs, MD5/SHA/git-SHA hex), an inline `# gcm:allow` pragma, and the FR-50 path exclusions. Out of scope: network/live validation (it would transmit the very secret being withheld) and any heavyweight engine (Hyperscan/`vectorscan`, ML/BPE tokenization) - detection stays pure-Rust and dependency-light (one new crate, `regex`; `toml`/`serde` already present). The `off`/`redact`/`abort` modes and exit behavior are unchanged (FR-50) |

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
- Every group must have a non-empty `commit_message` available at the moment it is committed (FR-45). The contract is **regenerate-per-group** ([ADR-001](../adrs/001-foundational-architecture-decisions.md) #6): only `groups[0]` carries a message from the initial plan, and each later group's message is generated on the run that commits it, scoped to that group's diff. The bash script implements neither valid strategy correctly: it requests a message only for `groups[0]`, then after advancing the cache the new first group has `commit_message: null`, which trips the single-commit-all fallback and collapses grouping for groups 2+. The rewrite closes this.
- The plan must partition the change set: every `files` entry exists in the current change set, every changed file appears in exactly one group, and no group is empty (FR-23). Violations reject the plan to fallback (FR-24).
- The cache holds only the not-yet-committed groups; committing `groups[0]` rewrites the cache as `groups[1..]` (FR-26), with freshness validated by content fingerprint (FR-27), not file names alone.
- Every `commit_message` is Conventional Commits v1.0.0-formatted (FR-59): a `<type>(scope): description` header, optional body, optional footers (e.g. `BREAKING CHANGE:`). The field carries the full message verbatim; gcm generates this format and self-checks its own header, but does not lint or enforce CC on existing commits.

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
| Privacy | Third-party data egress | The tool transmits source diffs and untracked file content to external LLM providers. Gitignored files are excluded (FR-48), egress is disclosed to the user (FR-49), and path-ignore plus optional secret redaction are available (FR-50). No PII is stored locally beyond the plan cache |
| Security | Secret handling on egress | A user-configurable pre-send scan can redact or abort on detected credentials (FR-50), backed by a real rule-pack + entropy detection engine (FR-60); API keys themselves are never placed in the prompt |
| Compliance | Regulatory standards | Not applicable as a local CLI: no regulated-data storage. The relevant exposure is voluntary source-code egress to the chosen provider, addressed under Privacy above |

## 6. Scope & Phasing

### In Scope (v1)

- Rust binary that talks to provider HTTP APIs directly. **v1 onboarding ships with five providers: Groq, Gemini, Anthropic, OpenAI, and local Ollama** (FR-56). Cerebras stays in the abstraction but off the menu (Phase 3).
- First-run interactive onboarding that activates providers, captures API keys (or a local endpoint for Ollama), and sets the default.
- Full grouped-commit workflow with plan cache, signed commits, and single-commit fallback - including the multi-group message contract (FR-45) the bash script never implemented correctly.
- Structured outputs / JSON mode and per-model reasoning control, gated by a verified provider-capability matrix (FR-52).
- Typed errors with retry/backoff; transactional commit that generates before staging (FR-47).
- Stronger plan validation (no omissions/duplicates/empty groups) and content-aware cache freshness (FR-23, FR-27).
- NUL-delimited git parsing for path safety (FR-31).
- Privacy: gitignore-respecting egress, disclosure, optional redaction (FR-48 to FR-50).
- Cross-platform builds (macOS + Linux, x86_64 + arm64) and documented install.
- `--version`, `--json`, non-interactive `--yes`/`--plan-only`, structured logging, distinct usage exit code.

### Out of Scope

- **Hunk-level (sub-file) grouping** - reason: file-level grouping covers ~90% of cases; sub-file splitting is a large addition deferred to a later phase.
- **Interactive group reordering / commit-all-groups-in-one-run** - reason: the one-group-per-run model is intentional and matches current muscle memory; batch modes are a future enhancement.
- **Windows support** - reason: not a current target platform; revisit on demand.
- **GUI/TUI beyond the confirm prompt** - reason: scope control; the CLI prompt is sufficient for v1.
- **Conventional-commit *linting/enforcement* of existing commits and pre-commit hook integration** - reason: separate concern from message generation. Note: *generating* Conventional Commits-formatted messages, including a lightweight self-check of gcm's own output, IS in scope (FR-59); what stays out is linting arbitrary/existing commits and wiring CC checks into git hooks.
- **Preserving partial/hunk-level staging** - reason: v1 groups whole files; a pre-existing curated index is reset (with a warning per FR-46), not preserved. Hunk-level work is deferred with hunk-level grouping to Phase 2.

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
| Provider HTTP APIs (Groq, Gemini, Anthropic, OpenAI) | External | Available | API/schema changes break a provider |
| Ollama runtime (local, optional) | User | Optional | Local provider (FR-56) unavailable if not installed/running; cloud providers unaffected |
| Provider API keys / accounts | Owner / adopter | Owner has keys | Adopters supply their own (Ollama needs none); documented in README |
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
| Source diffs / untracked content sent to a provider include sensitive code or secrets | M | H | Gitignore exclusion (FR-48), egress disclosure (FR-49), optional pre-send redaction and path ignore (FR-50), real rule-pack + entropy detection (FR-60) |
| Multi-group message contract chosen wrong: stale messages, or extra per-group LLM cost | M | M | Decide the strategy in design (Open Questions); content-fingerprint cache (FR-27) bounds staleness either way |
| Onboarding stores secrets insecurely or in a non-portable location | M | M | Prefer env-var references; 0600 file permissions; never committed (FR-55) |
| Five providers multiply integration, structured-output, and reasoning-control surface area | M | M | Capability matrix gate (FR-52) before each integration; provider trait isolates differences; Groq/OpenAI share an OpenAI-compatible shape |
| Ollama capability varies by pulled model / version (may lack JSON-schema support) | M | L | FR-20 defensive parsing; capability matrix records the verified floor; Ollama is optional, not the default |

### Open Questions

All resolved 2026-06-19 in [ADR-001](../adrs/001-foundational-architecture-decisions.md); decision numbers in brackets.

- [x] **Anthropic backend** [ADR-001 #3]: direct Messages API with `ANTHROPIC_API_KEY` only; no `claude` CLI path (FR-10).
- [x] **git access** [ADR-001 #1]: shell out to `git` (thin typed wrapper) for FR-31 parity + native signing/hooks; not `git2`.
- [x] **Default provider for a bare `gcm`** [ADR-001 #5]: **Groq** shipped default (free tier, fast, verified strict json_schema); onboarding sets the user's personal default.
- [x] **OpenAI model and alias** [ADR-001 #7]: `gpt-4o-mini` (pin `gpt-4o-mini-2024-07-18`), alias `gcmo`. `gpt-4.1-mini` rejected (unreliable strict support).
- [x] **Ollama in v1 (FR-56)** [ADR-001 #8]: firm yes; alias `gcml`; onboarding probes `localhost:11434`/`OLLAMA_HOST`.
- [x] **Multi-group message contract (FR-45)** [ADR-001 #6]: regenerate-per-group on its commit run; subsequent runs are message-only calls scoped to **only the remaining group's diff**.
- [x] **Partial staging (FR-46)** [ADR-001 #9]: reset a pre-existing curated index with a warning; no hunk-level preservation in v1 (documented limitation).
- [x] **Non-interactive defaults (FR-51)** [ADR-001 #10]: `--yes`/`--no-input` + `--plan-only` both in v1; non-TTY without them errors with exact config/env + non-zero exit.
- [x] **Onboarding parameters (FR-53/54)** [ADR-001 #11]: minimal wizard + a GPG-signing config check; no forced model selection.
- [x] **Config file format and location** [ADR-001 #4]: TOML in the OS config dir (`directories` crate); precedence flag > env > config > default; `GCM_CONFIG` override.
- [x] **Cache location cross-platform** [ADR-001 #12]: OS cache dir (`directories` crate); drop FR-30 bash `/tmp` compat (a one-time cold re-analysis is acceptable).
- [x] **Cerebras** [ADR-001 #13]: **dropped entirely** from v1 (not deferred) - unstable public catalog (Qwen pulled 2026-05-27) + thin adoption.
- [x] **Async vs sync runtime** [ADR-001 #2]: blocking HTTP client; no `tokio` (single-call CLI, faster cold start).

## 9. Rollout & Measurement

### Release Plan

- **Personal cutover (primary user)**: Build and install the binary, repoint the `~/.zshrc` aliases from `/opt/script/git-commit-ai.sh` to the Rust binary, and run side-by-side for a short observation window. Rollback is a one-line alias revert; the bash script stays in place untouched.
- **Open-source release**: Tag `v0.1.0`, publish cross-platform release binaries (macOS + Linux, x86_64 + arm64), document install and provider setup in the README, and keep models/providers config-driven so adopters can adjust without recompiling.
- **Migration safety**: Preserve cache semantics so an in-flight session survives the swap; validate grouping parity against the bash version on a scratch repo before cutover (LLM output is non-deterministic, so message text need not be byte-identical, but group structure should match).

### Alias Parity & Migration Matrix

Exact mapping from the current shell aliases (`~/.zshrc`, all pointing at `/opt/script/git-commit-ai.sh`) to the Rust invocation. Rollback is repointing each alias back to the bash script.

Resolved per [ADR-001](../adrs/001-foundational-architecture-decisions.md): bare-`gcm` **shipped** default is now **Groq** (#5); the primary user keeps Anthropic Haiku as their personal default via onboarding/config. `gcmc` (Cerebras) dropped (#13); `gcmo`/`gcml` aliases confirmed.

| Alias | Provider | Model | Current (bash) | Target (Rust) | v1 |
|-------|----------|-------|----------------|---------------|----|
| `gcm` | Anthropic (personal default) | haiku | `git-commit-ai.sh` | `gcm` | Yes (direct Messages API; shipped OSS default is Groq per ADR-001 #5; Max sets Anthropic via onboarding) |
| `gcmq` | Groq | openai/gpt-oss-120b | `--provider=groq` | `gcm --provider=groq` | Yes (strict json_schema) |
| `gcmq20` | Groq | openai/gpt-oss-20b | `GCM_GROQ_MODEL=... --provider=groq` | `gcm --provider=groq --model=openai/gpt-oss-20b` | Yes (FR-14; strict json_schema) |
| `gcmq27` | Groq | qwen/qwen3.6-27b | `GCM_GROQ_MODEL=... --provider=groq` | `gcm --provider=groq --model=qwen/qwen3.6-27b` | Yes (FR-14, FR-17; best-effort json_schema only - strict is gpt-oss-only; reasoning fully disableable via `reasoning_effort:none`) |
| `gcmg` | Google | gemini-3.1-flash-lite | `--provider=google` | `gcm --provider=google` | Yes (responseSchema; thinking floor `minimal`) |
| `gcmo` (new) | OpenAI | gpt-4o-mini-2024-07-18 (configurable) | n/a (new provider) | `gcm --provider=openai` | Yes (ADR-001 #7) |
| `gcml` (new) | Ollama (local) | user-pulled model | n/a (new provider) | `gcm --provider=ollama` | Yes (ADR-001 #8; FR-56) |
| `gcmc` | ~~Cerebras~~ | n/a | `--provider=cerebras` (commented out) | dropped | No (ADR-001 #13 - dropped entirely) |
| `gcms` | none | n/a | `git commit -S -m` | unchanged | Not part of gcm |

### Measurement Plan

- **Capture bash baselines first**: add lightweight `DEBUG_GCM` timing and parse-outcome logging to the bash script for a short window before cutover, so the "Current" baselines in Section 2 (parse success, fallback rate, latency) are real numbers and the O1/O2 improvements are provable rather than asserted.

- Instrument debug logs to record: plan parse success/failure, fallback invocations and their cause, error types encountered, and timing (cold start, total overhead).
- First measurement after the first 100 real commit sessions post-cutover.
- Decision criteria:
  - **Continue/expand** if parse success >= 99% and fallback rate < 5%.
  - **Investigate** if any provider shows parse success < 95% or repeated unexplained fallbacks.
  - **Pivot** a provider integration to defensive parsing (or drop the provider from defaults) if its structured-output support proves unreliable.
