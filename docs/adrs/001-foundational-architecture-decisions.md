# ADR-001: Foundational Architecture Decisions for the gcm Rust Rewrite

**Status:** Accepted
**Date:** 2026-06-19
**Linear Task:** [CLO-485](https://linear.app/cloud-ai/issue/CLO-485) - Lock foundational architecture decisions and verify provider capabilities (ADR)
**Design Doc:** N/A (this ADR is the design artifact; driven by [PRD: gcm](../prds/prd-gcm.md) §8)
**Covers:** FR-52. **Unblocks:** FR-10, FR-27, FR-40, FR-45, FR-54 (and tasks CLO-486, CLO-489, CLO-491, CLO-494, CLO-496).

## Context

gcm is a greenfield Rust rewrite of the bash tool `docs/tmp/git-commit-ai.sh` (v2.7): an AI-assisted git commit helper that asks an LLM to partition working-tree changes into commit groups and writes conventional-commit messages. No Rust code exists yet. Several foundational choices gate implementation because they shape the HTTP client, the git layer, the config schema, and onboarding all at once, so the PRD (§8 Open Questions) deferred them to a single decision point. This ADR resolves every open question with a concrete value (not just a list of options) and records a provider-capability snapshot verified against live vendor docs, so downstream slices can cite it rather than re-litigate.

Two facts frame the whole set:

1. **The grouping plan is the central LLM↔tool contract** (PRD §"Data Model"). The tool must reliably get a typed JSON `Plan { groups: [{ files, summary, commit_message }] }` back, with no chain-of-thought leaking into stdout. Every provider exposes structured output and reasoning suppression *differently* - verified below - so the provider abstraction must accommodate three distinct shapes.
2. **This is a personal-cutover-first, open-source-second tool.** The primary user (Max) currently aliases `gcm` → Anthropic Haiku via the subscription `claude` CLI. The rewrite must serve a clean OSS default *and* let the primary user keep their workflow via config/onboarding.

## Decision Summary

| # | Question | Decision | FR |
|---|----------|----------|-----|
| 1 | Git access | **Shell out to `git`** (thin typed wrapper) | FR-4, FR-31, FR-58 |
| 2 | Runtime | **Blocking HTTP client** (no async runtime) | NFR cold-start |
| 3 | Anthropic auth | **Direct Messages API only** (`ANTHROPIC_API_KEY`); no `claude` CLI | FR-10, FR-41 |
| 4 | Config format / location / precedence | **TOML** in OS config dir; **flag > env > config > default** | FR-12, FR-40, FR-55 |
| 5 | Default provider (bare `gcm`) | **Groq** shipped default; onboarding sets the user's personal default | FR-13, FR-54 |
| 6 | Multi-group message contract | **Regenerate-per-group**; subsequent runs are message-only, scoped to the remaining group's diff | FR-2, FR-45, FR-27 |
| 7 | OpenAI model + alias | **`gpt-4o-mini`** (`gpt-4o-mini-2024-07-18`), alias **`gcmo`** | FR-14, FR-16 |
| 8 | Ollama in v1 | **Yes**; alias **`gcml`**; onboarding probes the daemon | FR-56 |
| 9 | Partial staging | **Reset curated index with a warning** (no hunk preservation in v1) | FR-46, FR-47 |
| 10 | Non-interactive defaults | **`--yes`/`--no-input` + `--plan-only` in v1**; non-TTY without them errors actionably | FR-37, FR-51, FR-53 |
| 11 | Onboarding parameters | **Minimal wizard + GPG-signing check**; no forced model selection | FR-53, FR-54 |
| 12 | Cache location | **OS cache dir**; drop bash `/tmp` compat | FR-25, FR-29, FR-30 |
| 13 | Cerebras in v1 | **Dropped entirely** (not deferred) | FR-13 |

---

## Decision 1 - Git access: shell out to `git`

**Decision:** Implement git operations as a thin typed wrapper that shells out to the `git` binary. Do **not** use `git2`/libgit2.

**Drivers:** byte-exact parity with bash behavior (FR-31); native GPG signing (FR-4); native pre-commit hook execution (FR-58); honor the user's full git config (signing key, `includeIf`, aliases, credential helpers); cross-platform install simplicity.

**Alternatives considered:**
- *`git2`/libgit2 (rejected):* typed errors and no subprocess, but historically weaker/divergent GPG-signing and hook support, and it forces us to reimplement git config resolution. The signing + hooks + parity requirements dominate the typed-error benefit.

**Rationale:** FR-31 specifies exact plumbing - `git status --porcelain=v1 -z`, `-c core.quotePath=false`, diff against the empty tree on an unborn branch, rename records read from porcelain `-z` NUL fields. Shelling out to the user's real `git` reproduces these and makes signing, hooks, and credential helpers "just work." Subprocess overhead is negligible for a per-invocation CLI, and careful NUL-delimited output parsing is already required regardless of the access method.

**Consequences:**
- (+) Signing/hooks/credential-helpers/config behave identically to the user's git.
- (+) No libgit2 system dependency or build complexity; smaller surface for cross-platform breakage (FR-42).
- (−) Must parse git porcelain/diff text carefully (NUL fields, binary detection, rename records) - mitigated by FR-31/FR-32/FR-33 ACs.
- (−) Errors are inferred from exit codes + stderr rather than typed libgit2 errors - wrap them in the typed error taxonomy (FR-21).

---

## Decision 2 - Runtime: blocking HTTP client

**Decision:** Use a blocking HTTP client (`ureq`, or `reqwest::blocking`). Do **not** adopt `tokio` or any async runtime. The provider trait method is synchronous: `fn generate(&self, req: PlanRequest) -> Result<Plan, ProviderError>`.

**Drivers:** fast cold start (NFR); a single LLM call per invocation; binary size and code simplicity.

**Alternatives considered:**
- *Async (`tokio`) (rejected):* warranted only for streaming responses or multi-provider concurrency, neither of which is a v1 goal. It adds runtime spin-up latency on every invocation and forces async coloring through the entire call path.

**Rationale:** gcm makes essentially one network call (the grouping plan, or a per-group message) then does local git work. There is no fan-out or streaming requirement (the tool wants the *complete* JSON plan, not a token stream). Blocking is simpler, starts faster, and produces a smaller binary.

**Consequences:**
- (+) Lower cold-start latency; simpler error handling and control flow; smaller binary (FR-41).
- (−) If a future phase wants to race multiple providers or stream, a runtime change is needed - contained because all HTTP lives behind the provider trait (FR-11).

---

## Decision 3 - Anthropic auth: direct Messages API only

**Decision:** Anthropic is called via the direct Messages API using `ANTHROPIC_API_KEY`. The `claude` CLI path is **not** retained.

**Drivers:** FR-10 (Must) - "no `mods`/`crush`/`claude` CLI in the runtime"; single self-contained binary (FR-41); uniform error taxonomy (FR-21) and retries (FR-22); config-driven models (FR-14).

**Alternatives considered:**
- *Keep the `claude` CLI path (rejected):* free for subscription users, but directly contradicts the FR-10 Must and the single-binary goal, and reintroduces the subprocess-LLM coupling the rewrite exists to remove.
- *Both (direct default + optional CLI) (rejected):* hedges the cost concern but still violates FR-10 in the runtime and roughly doubles the Anthropic integration surface for a single user's convenience.

**Rationale:** A subprocess LLM CLI is exactly what the rewrite is eliminating. Direct API gives the structured-output contract (forced tool-use, verified below), typed errors, and retries that the bash tool lacks. Structured output uses Anthropic forced tool-use (`tools` + `tool_choice: {type:"tool"}` + `input_schema`, optional `strict`) or `output_config.format` on current models - there is no generic `response_format` (capability matrix).

**Consequences:**
- (+) Anthropic behaves like every other provider behind the trait; no subprocess.
- (−) Requires a **paid** `ANTHROPIC_API_KEY` (the subscription `claude` CLI was effectively free) - this is real adoption friction.
- *Mitigation:* Anthropic is **not** the shipped default (Decision 5); it is optional in onboarding; the egress/auth model is documented (FR-49). The primary user sets Anthropic as their personal default via config.

---

## Decision 4 - Config format, location, and precedence

**Decision:**
- **Format:** TOML (`config.toml`).
- **Location:** OS-appropriate config dir via the `directories` crate - `~/.config/gcm/config.toml` (Linux, honoring `XDG_CONFIG_HOME`), `~/Library/Application Support/gcm/config.toml` (macOS). An explicit `GCM_CONFIG` env var overrides the path.
- **Precedence (highest → lowest):** CLI flag → env var → config file → built-in default.
- **Secrets:** API keys are referenced by env-var name, never stored as plaintext in config; if a value is ever written, the file is `0600`-equivalent (FR-55).

**Drivers:** FR-12 (flag > env > default), FR-40 (configurable, OS-appropriate, no hardcoded personal paths), FR-55 (safe secret handling).

**Alternatives considered:**
- *JSON (rejected):* no comments, less ergonomic to hand-edit.
- *A hardcoded dotfile path (rejected):* violates FR-40's "no personal paths hardcoded."

**Rationale:** TOML is the Rust-ecosystem standard (Cargo), serde-friendly, and comment-friendly for a hand-edited config. The `directories` crate gives correct per-OS paths. The precedence chain is the conventional, least-surprising extension of FR-12 (config slots between env and default).

**Consequences:**
- (+) Standard, portable, documented config story; clean override chain for scripts and CI.
- (+) Secrets stay in env vars / the OS keychain pattern, never world-readable.
- (−) Config schema becomes a compatibility surface - version it (the cache fingerprint already includes a prompt/schema version, FR-27).

---

## Decision 5 - Default provider for a bare `gcm`

**Decision:** The shipped default for a bare `gcm` (no flag, no config) is **Groq**. After onboarding (FR-54), the default is whatever provider the user marked.

**Drivers:** zero-friction first run for new adopters; cost; structured-output reliability for the grouping contract.

**Alternatives considered:**
- *Anthropic Haiku (bash continuity) (rejected as the shipped default):* matches today, but direct Anthropic now needs a paid key (Decision 3), so it is a high-friction out-of-the-box default for anyone but the primary user.
- *Ollama local (rejected as the shipped default):* zero-egress and key-free, but a bare `gcm` fails until the user installs Ollama and pulls a model - high first-run friction. It remains the recommended *privacy* choice, selectable in onboarding.

**Rationale:** Groq has a free tier, is fast, and has **verified `strict: true` json_schema** support (on `gpt-oss-120b`/`20b`) - the strongest structured-output guarantee among the no-cost options, which directly serves the grouping-plan contract. Because onboarding sets the personal default, this choice only governs the truly-unconfigured first run.

**Consequences:**
- (+) A freshly-installed gcm with a `GROQ_API_KEY` works immediately with a guaranteed-schema model.
- (−) Departs from the bash default (Anthropic Haiku); documented in the migration matrix below.
- *Note:* the Groq default model should be `openai/gpt-oss-120b` (strict json_schema). gpt-oss reasoning cannot be fully disabled - suppress it from output with `include_reasoning: false` (capability matrix).

---

## Decision 6 - Multi-group message contract and subsequent-run diff context

**Decision:** Use **regenerate-per-group**: the first run produces the grouping plan and a `commit_message` for `groups[0]` only; each subsequent run generates the next group's message on the run that commits it. These subsequent runs are **message-only** calls (no re-grouping, FR-2), and the model receives **only the remaining group's diff**, not the whole original diff.

**Drivers:** FR-45 (every group has a usable message at commit time), FR-27 (content-fingerprint cache freshness), FR-15 (per-provider diff budget), and the bash failure mode this must not reproduce.

**Alternatives considered:**
- *Generate all messages up front (rejected):* one call total, but it conflicts with content-aware cache freshness (FR-27) - editing a later group busts the cache and forces regeneration anyway - and risks stale messages. The bash tool implements a broken half of this (message only for `groups[0]`, then the advanced cache's new first group has `commit_message: null`, silently collapsing to single-commit-all).
- *Resend the whole original diff on each subsequent run (rejected for the sub-question):* wastes the diff budget and tempts the model to re-group; the file→group partition is already fixed in the cached plan.

**Rationale:** Regenerate-per-group is robust against edits between commits and structurally prevents the bash null-message bug. Scoping the subsequent-run prompt to the committing group's own diff keeps it cheap and focused; grouping is not re-decided (FR-2), only the message for that group's actual changes is written. Plan validation (FR-23) checks message placement per this contract.

**Consequences:**
- (+) No group ever reaches commit with a null/empty message; no silent mode switch.
- (+) Cheaper, focused per-group message prompts; resilient to inter-commit edits (caught by FR-27 content hashes).
- (−) N groups cost up to N message-generation calls (vs one upfront) - acceptable, and avoided entirely for single-group/`--all` flows.

---

## Decision 7 - OpenAI default model and alias

**Decision:** Default OpenAI model is **`gpt-4o-mini`**, pinned to the snapshot `gpt-4o-mini-2024-07-18` in shipped config. Alias **`gcmo`**.

**Drivers:** verified strict structured-output support; no reasoning to suppress; cost; alias convention (provider-initial).

**Alternatives considered:**
- *`gpt-4.1-mini` (rejected):* not in OpenAI's official strict-`json_schema` compatibility list, with documented failures on the unversioned alias - unreliable for the grouping contract.

**Rationale:** `gpt-4o-mini` is the only cheap OpenAI model officially confirmed for strict `json_schema` Structured Outputs **and** is a non-reasoning model, so there is zero chain-of-thought to suppress - it satisfies "no reasoning in stdout" with no extra parameters. Models are config-driven (FR-14), so users may override. `gcmo` follows the provider-initial alias convention and does not collide.

**Consequences:**
- (+) Guaranteed schema conformance via constrained decoding; clean default.
- (−) Pinned snapshot must be revisited as OpenAI deprecates models (models are config-driven, so this is a config bump, not code).

---

## Decision 8 - Ollama in v1

**Decision:** Ollama ships in v1. Alias **`gcml`**. Onboarding probes for a running daemon at `http://localhost:11434` (and honors `OLLAMA_HOST`) and surfaces a clear, actionable message when unreachable.

**Drivers:** FR-56 and the privacy/zero-egress story (FR-48–50); it is the only no-API-key option.

**Alternatives considered:**
- *Defer Ollama (rejected):* it is the entire privacy/local angle and a "Should" with high value for the shareable goal; the integration is an OpenAI-compatible-or-native-`format` call, low marginal cost given the provider trait.

**Rationale:** Verified: Ollama's native `/api/chat` accepts a JSON-Schema object in `format`, and `think` toggles reasoning (boolean for most models; `low`/`med`/`high` for gpt-oss). Structured-output fidelity is model-dependent, handled by defensive parsing (FR-20) plus schema validation + retry. `gcml` (l = local/llama) does not collide with the taken `o` (OpenAI).

**Consequences:**
- (+) Zero-egress option for sensitive repos; no key required.
- (−) Capability varies by pulled model and Ollama version - the capability matrix records the floor and the tool validates + repairs (FR-20). Structured output is local-only (not Ollama Cloud).
- *Note:* prefer the native `/api/chat` `format`=schema path (first-class, no OpenAI-envelope translation); read `message.content`, ignore `message.thinking`.

---

## Decision 9 - Partial (curated) staging

**Decision:** When files are already staged (including hunk-level `git add -p`), gcm **warns** the user, then resets the index as part of its scoped staging. v1 does **not** preserve partial/hunk-level staging.

**Drivers:** FR-46 (don't silently destroy a curated index), FR-47 (transactional: restore on abort), v1's whole-file grouping model.

**Alternatives considered:**
- *Preserve hunk-level staging (rejected for v1):* significant complexity (patch-level index manipulation) for a tool that groups whole files; out of scope for v1.

**Rationale:** v1 groups whole files, so a curated hunk-level index can't be honored faithfully anyway. Warning before reset (and restoring the original index on abort/failure per FR-47) avoids the silent-destruction failure the bash tool has. The limitation is documented, not hidden.

**Consequences:**
- (+) No silent loss of a curated index; clean transactional restore on decline/failure.
- (−) Users who curate with `add -p` must re-stage after gcm if they decline - documented limitation; revisit in a later phase if demand appears.

---

## Decision 10 - Non-interactive defaults

**Decision:** `--yes`/`--no-input` (auto-confirm) and `--plan-only` (emit plan, exit without committing) both land in v1. In a non-TTY context **without** one of these flags, gcm **errors** with the exact config/env needed and a non-zero exit - it does not silently preview or hang on a prompt.

**Drivers:** FR-51 (non-interactive operation), FR-37 (`--json` machine-readable), FR-53 (non-interactive onboarding prints exact config and exits non-zero).

**Alternatives considered:**
- *Default non-TTY to `--plan-only` (rejected):* silently changing behavior in automation hides intent; an explicit, loud failure is safer and matches FR-53's onboarding behavior.

**Rationale:** Agentic/CI use needs deterministic, non-blocking behavior. `--json` + `--plan-only` yields a machine-readable preview; `--json` + `--yes` commits unattended. A bare invocation in a non-TTY should fail actionably rather than block or guess.

**Consequences:**
- (+) Predictable automation surface; no hangs waiting on a prompt that can't be answered.
- (−) Callers must opt into a non-interactive flag explicitly - intended.

---

## Decision 11 - Onboarding parameters

**Decision:** The first-run wizard is minimal: present the v1 providers, let the user enable one or more, capture/locate each enabled provider's key (Ollama needs an endpoint, not a key), and record the default for a bare `gcm`. **Additionally**, the wizard checks GPG-signing configuration and warns if it is unset. It does **not** force a default-model selection.

**Drivers:** FR-53/FR-54 (guided setup reaching a working first commit), FR-4 (every commit is signed).

**Alternatives considered:**
- *Force model selection in the wizard (rejected):* adds friction; sensible per-provider default models already exist and advanced users edit config.
- *Skip the GPG check (rejected):* unconfigured signing is the single most likely first-commit failure (FR-4 makes signing mandatory); a cheap early warning prevents a confusing failure.

**Rationale:** Keep onboarding short (the FR-53 intent) but proactively surface the one environmental prerequisite - GPG signing - that would otherwise fail the very first commit.

**Consequences:**
- (+) New users reach a working, signed first commit without hand-editing config.
- (+) Re-runnable via `gcm config` / `--reconfigure` (FR-55).
- (−) The GPG check is advisory (warn, not block) - a user without signing configured is told early but not prevented from finishing setup.

---

## Decision 12 - Cache location

**Decision:** The per-repo plan cache lives in the OS-appropriate cache dir via the `directories` crate - `~/Library/Caches/gcm/` (macOS), `$XDG_CACHE_HOME/gcm/` or `~/.cache/gcm/` (Linux). Drop the bash `/tmp`-style path and the FR-30 backward-read of in-flight bash caches. Cache files/dir are restricted to the current user; the cache key is `sha256(repo-root-path)` (FR-25).

**Drivers:** FR-29 (OS-appropriate cache, not a hardcoded `/tmp`), FR-25 (per-repo key), FR-30 (bash cache compat - a "Could").

**Alternatives considered:**
- *Keep `/tmp` for bash-cache backward-read (FR-30) (rejected):* FR-30 is a "Could," and the personal cutover is a one-time event - not worth coupling the cache path to a legacy location, especially since `/tmp` is wrong on macOS.

**Rationale:** Correct per-OS cache directories with user-only permissions; the freshness model (content fingerprint, FR-27) does not depend on the path. The bash→Rust migration tolerates one cold re-analysis.

**Consequences:**
- (+) Correct, portable cache location with restricted permissions.
- (−) An in-flight bash cache is not read by the Rust tool - the next run re-analyzes once (acceptable for a personal one-time cutover).

---

## Decision 13 - Cerebras: dropped from v1

**Decision:** Cerebras is **dropped entirely** from the provider set (not deferred to a later phase). The `gcmc` alias is removed.

**Drivers:** catalog stability; adoption; maintenance surface.

**Rationale:** Verification (2026-06-19) found Cerebras had **removed the very Qwen model the PRD named** (`qwen-3-235b-a22b-instruct-2507`, deprecated 2026-05-27) from public access; the current public catalog is just `gpt-oss-120b` + `zai-glm-4.7` (preview). Combined with a free tier too tight for real CLI use (5 RPM) and thin adoption, the provider is not worth carrying. The *capability* picture is adequate (OpenAI-compatible, strict json_schema, suppressible reasoning) - this is a stability/adoption decision, not a capability gap. The verified snapshot is retained in the appendix for the record.

**Consequences:**
- (+) One fewer integration to maintain against a volatile catalog; reinforces the "no hardcoded model IDs" stance.
- (−) Loses Cerebras's very-high-throughput inference - revisit only if a paid tier is budgeted and the catalog stabilizes.

---

## Knock-on effects

**Provider trait shape (FR-11).** The trait must abstract three structured-output mechanisms behind one synchronous method:
- OpenAI-style `response_format: {type:"json_schema", json_schema:{…, strict:true}}` - Groq, OpenAI, Ollama (OpenAI-compat).
- Gemini `generationConfig.responseSchema` (OpenAPI-3.0 subset).
- Anthropic forced tool-use (`tools` + `tool_choice` + `input_schema`) or `output_config.format`.
Reasoning suppression is likewise per-provider (see matrix). Because some providers only *hide* rather than *disable* reasoning, the layered defensive parser and last-resort `<think>` strip (FR-20) remain mandatory.

**Alias & migration matrix update (PRD §9).** `gcmc` (Cerebras) is removed; `gcmo` (OpenAI → `gpt-4o-mini`) and `gcml` (Ollama, local) are confirmed. The bare-`gcm` default changes from Anthropic Haiku to Groq at the shipped level (personal default still settable).

**PRD updates this ADR triggers (applied alongside this ADR):**
- §8 Open Questions: all resolved → reference this ADR.
- Provider Capability Matrix: every "Pending" row filled with a verified-on date (2026-06-19) - see appendix.
- §9 alias matrix: drop `gcmc`; note the default-provider change.

---

## Appendix A - Provider Capability Matrix (verified 2026-06-19)

Structured-output and reasoning-control capabilities, verified against current vendor docs. Used to satisfy FR-52 and to fill the PRD's "Pending" rows.

| Provider | Structured output | Reasoning control | Known constraint | Verified |
|----------|-------------------|-------------------|------------------|----------|
| **Groq** | `response_format` `json_object` \| `json_schema`; `strict:true` only on `openai/gpt-oss-20b` & `gpt-oss-120b` | `reasoning_effort` (gpt-oss: low/med/high, no `none`), `reasoning_format` (raw/parsed/hidden), `include_reasoning` | `reasoning_format:raw` + JSON mode = HTTP 400; gpt-oss reasoning is hide-only (`include_reasoning:false`), only Qwen3 fully disables (`reasoning_effort:none`); **streaming + tools unsupported with json_schema** | 2026-06-19 |
| **Google Gemini** | `responseMimeType:"application/json"` + `responseSchema` (OpenAPI-3.0 subset; `$ref`/`allOf`/`oneOf`/`not` silently ignored) | 3.x `thinkingConfig.thinkingLevel` (MINIMAL/LOW/MED/HIGH); legacy `thinkingBudget` | **No hard "off" on 3.x** - floor is `minimal` (soft); `gemini-3.1-flash-lite` is current GA; `thinkingLevel`+`thinkingBudget` together = 400; only `gemini-2.5-flash-lite` `thinkingBudget:0` truly disables thinking | 2026-06-19 |
| **Anthropic** | **Forced tool-use** (`tools` + `tool_choice:{type:"tool"}` + `input_schema`, optional `strict`) or `output_config.format` (Opus 4.8 / Sonnet 4.6 / Haiku 4.5) | Adaptive thinking (`thinking:{type:"adaptive"}`); CoT `display` omitted by default | **No generic `response_format`** - structured output is a forced tool call; shapes the provider trait | 2026-06-19 |
| **OpenAI** | `response_format` `json_schema` + `strict:true` (constrained decoding) | Reasoning models hide CoT by default; `reasoning_effort` | **`gpt-4o-mini` officially supported & non-reasoning (zero CoT to suppress)**; `gpt-4.1-mini` not reliably supported for strict json_schema | 2026-06-19 |
| **Ollama (local)** | Native `/api/chat` `format` = JSON-Schema object (or `"json"`); OpenAI-compat `response_format` | `think` bool (gpt-oss: low/med/high, not fully off); thinking separated into `message.thinking` | Structured output local-only (not Ollama Cloud); fidelity **model-dependent** → validate + retry (FR-20); no API key, `OLLAMA_HOST` override | 2026-06-19 |
| **Cerebras** *(dropped - Decision 13)* | OpenAI-compat `json_schema`+`strict` (confirmed `gpt-oss-120b`) | `reasoning_effort` + `reasoning_format` (`raw` incompatible w/ JSON) | **Qwen family removed 2026-05-27** (catalog: `gpt-oss-120b`, `zai-glm-4.7`); free tier 5 RPM; `tools` + `response_format` mutually exclusive | 2026-06-19 |

### Sources (official docs, fetched/verified 2026-06-19)

- **Groq:** console.groq.com/docs - structured-outputs, reasoning, api-reference, models, changelog.
- **Gemini:** ai.google.dev/api/generate-content; /gemini-api/docs - structured-output, thinking, models/gemini-3.1-flash-lite, api-key.
- **Anthropic:** authoritative Anthropic API reference (structured outputs via forced tool-use / `output_config.format`; adaptive thinking).
- **OpenAI:** developers.openai.com/api/docs - guides/structured-outputs, guides/reasoning, models/gpt-4o-mini.
- **Ollama:** github.com/ollama/ollama docs - api.md, capabilities/structured-outputs, capabilities/thinking, api/openai-compatibility.
- **Cerebras:** inference-docs.cerebras.ai - structured-outputs, reasoning, models/overview, support/rate-limits, support/deprecation.

### Confidence caveats (carried for implementers)

- **Groq:** whether `reasoning_effort:"low"` is strictly required alongside `include_reasoning:false` for gpt-oss, and the exact default of `reasoning_format` under JSON mode, are MEDIUM/LOW - smoke-test one gpt-oss request before finalizing the Groq default config.
- **Gemini:** `responseSchema` advanced-keyword handling (`$ref` recursion) and whether `minimal` thinking ever degrades schema adherence are LOW - validate output app-side against the Rust struct regardless.
- **OpenAI:** exact JSON-Schema numeric limits and the `gpt-4.1-mini` negative are MEDIUM (partly community-sourced) - `gpt-4o-mini` strict support is HIGH/confirmed.
- **Ollama:** no formal JSON-Schema subset spec is published; treat advanced keywords as unverified and validate per pulled model.

---

## Compliance

- [ ] **Manual:** Code review verifies each downstream slice (CLO-486/489/491/494/496) cites this ADR for its foundational choices.
- [ ] **Automated:** the provider trait is synchronous (no `async fn`); no `tokio` dependency; no LLM-CLI subprocess invocation in the runtime (grep CI check for `Command::new("claude")`-style calls); no hardcoded model IDs (models resolved from config).
- [ ] **Documentation:** PRD §8/§9 and the capability matrix updated to reference this ADR with the 2026-06-19 verification date.

## Related

- **Supersedes:** N/A (first ADR).
- **Related ADRs:** None yet.
- **Related Tasks:** CLO-485 (this); unblocks CLO-486, CLO-489, CLO-491, CLO-494, CLO-496.
- **Affected components:** git layer, HTTP/provider trait, config schema, plan cache, onboarding wizard, CLI surface.

## Notes

- **Author:** Max Kulish
- **Reviewers:** Owner-approved at the CLO-485 design checkpoint (2026-06-19).
- **Approved:** 2026-06-19
