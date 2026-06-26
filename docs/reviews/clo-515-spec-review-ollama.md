# Spec Review: clo-515

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-06-26
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement is **clear and well-defined**. It correctly identifies the gap: users have no single command to understand "what will gcm do right now, and why." The need for source attribution (env vs config vs default) is properly motivated by the layered configuration system.

**Strengths:**
- Accurately describes the real user pain point
- Correctly identifies that the value alone isn't enough—users need provenance
- Explicitly states no network calls (purely local introspection)
- Mirrors Linear task description accurately
- Correctly identifies the hard part: source attribution with correct precedence

**Minor Gap:** The statement mentions "prints version, config paths, and per-provider activation state" but doesn't explicitly mention the **gcm version** source (which is `cli::VERSION` from build.rs). This is minor but worth confirming.

## 2. Acceptance Criteria Review

**Strong:**
- **AC-1** (no network/diff): Clear, testable, aligns with existing `Commands::Config` early-dispatch pattern
- **AC-2** (paths/version): Specific fields enumerated, testable via stdout inspection
- **AC-4** (secret masking): Strong security constraint, testable by substring assertion
- **AC-5** (JSON validity): Explicit jq round-trip test, `v: 1` schema version contract
- **AC-6** (no-config case): Covers edge case explicitly
- **AC-8** (help/clap validity): Standard CLI test

**Gaps:**

1. **AC-3** (provider output) is missing a crucial detail: **What is "activated"?** The spec says "whether it is activated" but the definition in AC-7 is split between two places. AC-7 should be cross-referenced from AC-3.

2. **AC-7** (activation semantics) states "a provider is 'activated' iff it is listed in the loaded config's `providers` **or** has a usable key/endpoint available" — but this needs clarification:
   - For cloud providers: "usable key" means the env var is set and non-blank?
   - For Ollama: "always reachable to attempt" — but status doesn't probe, so what does "activated" mean for Ollama? Just that it's in config OR... what?

3. **Missing criteria implied by Linear description:**
   - No criterion for **exit code** (should be 0 for success since it's a read-only introspection command)
   - No criterion for **performance** (should complete in < 100ms since no network/diff)
   - No criterion for **behavior when provider selection fails** (e.g., invalid `GCM_PROVIDER` value)

4. **AC-5** says stdout is pure JSON, but doesn't explicitly state that **human mode must not print JSON to stdout** — this is implicit but should be explicit.

5. **Missing criterion for error cases:**
   - What if `config_path()` returns `None` (no OS config dir available)?
   - What if the config file is readable but malformed (per `load()` returning `None`)?

## 3. Constraints Check

**Aligned with codebase patterns:**
- **Must: Dispatch `Status` early** — matches `Commands::Config` pattern in `main.rs:38-40`
- **Must: Report same precedence runtime uses** — correctly identifies `env > inline-config` for keys (already in `config::env_plan`) and `flag > env > default` for models (already in `pick_model`)
- **Must: Mask secrets** — aligns with FR-55 (secrets never world-readable)
- **Must: No network/Repo** — correctly avoids `provider::select` construction
- **Must: Pure functions for attribution** — matches existing `config_path_from` / `env_plan(is_set)` testable-pure-function pattern
- **Must-not: Call `apply_to_env`** — critical constraint correctly identified; this would corrupt attribution

**Concerns:**

1. **Missing constraint for test isolation:** The spec should explicitly require that tests use hermetic env (like `tests/onboarding.rs` pattern) — `GCM_CONFIG` override + clean env vars.

2. **Implicit constraint not captured:** The `ProviderId::default_model()` function is **private**. The spec correctly identifies this needs exposure, but doesn't specify the minimal API surface. Sub-task 2 mentions `pub fn resolve_model_with_source` but this may not be the minimal choice — `default_model()` and `model_env_vars()` could also be exposed separately.

3. **Escalate constraint unclear:** The "Escalate when: masked-suffix conflicts with policy" is reasonable, but there's no escalation path defined. Should this block shipping? Be an ADR discussion?

## 4. Decomposition Quality

**Well-scoped sub-tasks:**
- **ST1 (CLI surface):** Small, self-contained, testable via clap's `debug_assert()`
- **ST2 (Model-resolution introspection):** Focused on `provider/mod.rs` extension
- **ST3 (Attribution + report model):** Core logic in new `src/status.rs`
- **ST4 (JSON output):** Follows existing `output::emit` pattern
- **ST5 (Human rendering + dispatch):** Thin wiring in `main.rs`
- **ST6 (Tests):** Comprehensive acceptance test plan

**Dependency ordering is correct:** 1 & 2 independent, 3 depends on 2, 4 depends on 3, 5 depends on 3+4, 6 runs alongside each.

**Issues:**

1. **ST2 may be undersized for ~2 hours:** Exposing model resolution requires:
   - Adding `pub fn resolve_model_with_source`
   - Potentially exposing `default_model()` (currently private)
   - Potentially exposing `model_env_vars()` (currently private)
   - Refactoring `resolve_model` to delegate (no behavior change)
   - Unit tests for each precedence branch
   
   This could be 3-4 hours if the refactor is extensive.

2. **Missing sub-task:** Where is **version string** retrieval? `cli::VERSION` is `const`, but the spec says AC-2 shows "gcm version (`cli::VERSION`)". This is trivial but needs a task entry.

3. **Missing sub-task:** **Ollama endpoint source attribution** has special handling (three env vars: `GCM_OLLAMA_BASE_URL`, `OLLAMA_HOST`, then config `endpoint`, then default). Sub-task 3 mentions "Ollama -> endpoint_source" but doesn't detail the precedence chain. This is more complex than cloud provider key attribution.

4. **ST6 should specify file locations:** The acceptance tests belong in `tests/status.rs` (new file), but unit tests for attribution functions should be in `src/status.rs` as `#[cfg(test)]` module.

## 5. Evaluation Coverage

**Covered:**
- All happy-path scenarios have clear test approaches
- AC-4 (secret never printed) has explicit test case (#4)
- JSON validity has explicit jq round-trip test (#5)
- Edge cases (blank env values, Google dual model env vars) are enumerated

**Gaps:**

1. **Missing test for malformed config:** Evaluation table doesn't include a test where `config::load()` returns `None` due to malformed TOML or wrong version.

2. **Missing test for `GCM_PROVIDER` invalid value:** What happens if `GCM_PROVIDER=bogus`? The status command should report the error, but this isn't tested.

3. **Missing test for concurrent `GCM_CONFIG` changes:** The spec says status should report "whether the file exists" — but what if the file is deleted between path resolution and existence check? (This is an edge case but could be a race.)

4. **Missing test for stdin behavior:** Should status read stdin? (No, but should be explicit.)

5. **Test #7 (model flag source)** should also test **per-provider model flag attribution** — currently only tests selected/default provider.

## 6. Codebase Alignment

**Violations:** None found. The spec correctly:
- Follows the `Commands` enum pattern in `cli.rs`
- Mirrors early dispatch in `main.rs::run()` before `execute()`
- Proposes a new `StatusReport` struct following the `Envelope` pattern (without overloading `Envelope.status`)
- Correctly identifies that `apply_to_env` must not be called
- Proposes pure functions matching `config_path_from` / `env_plan` style

**Alignment observations:**

1. **Line references are accurate:**
   - `src/cli.rs:104` `Commands` enum — verified at line ~93
   - `src/main.rs:35` `run()` — verified
   - `src/config.rs:226` `apply_to_env` — verified
   - `src/provider/mod.rs:167` `key_env_var` — verified
   - `src/provider/mod.rs:271-293` `resolve_model`/`pick_model` — verified

2. **JSON output pattern follows existing `output.rs`:** The spec correctly proposes a separate `StatusReport` struct with its own emit function, rather than overloading `Envelope`. This maintains backward compatibility for existing `--json` consumers.

3. **Test pattern matches `tests/onboarding.rs`:** The proposed acceptance tests correctly use `CARGO_BIN_EXE_gcm` and hermetic environment isolation.

**Potential issue:** The spec says "status must NOT call `apply_to_env`" but doesn't explicitly state that **status should NOT hydrate any env vars at all**. The existing `ensure_configured()` calls `apply_to_env` for the commit flow, but status should bypass this entirely. The dispatch should happen before `ensure_configured()`.

## 7. Blind Spots

1. **Provider selection failure mode:** If `GCM_PROVIDER` is set to an invalid value, `pick_provider_id()` returns a `ProviderError`. But status is supposed to be read-only and non-failing. What does status show? This should be explicit: should it show an error field for provider selection, or show all providers?

2. **Config file permissions check:** The existing `load()` function checks for insecure permissions (mode 0644) and warns. Should status report this? The spec doesn't mention it.

3. **`GCM_CONFIG` directory existence:** If `GCM_CONFIG` points to a non-existent directory, `config_path()` still returns `Some(path)` (the would-be path). Should status report "directory doesn't exist" vs "file doesn't exist"?

4. **Provider ordering in output:** The spec says "For each `ProviderId`" but doesn't specify output order. Should it be alphabetical? The order from config? The canonical order from `cloud_then_ollama()`?

5. **Default provider attribution:** The spec shows "whether it is the default" but doesn't specify **which config source determines the default**. The default is `config.default`, but if there's no config, what determines it? (Answer: `ProviderId::Groq` built-in default.)

6. **Cross-cutting concern: debug logging:** The existing codebase uses `debug_log!` macro. Should status emit debug logs about attribution decisions? The spec is silent.

7. **Cross-cutting concern: `GCM_LOG_LEVEL`:** Status should respect log level settings, but logs go to stderr. This is implicit and should be explicit.

8. **Schema version compatibility:** If `v: 1` is the schema version, what happens when future versions add fields? The spec should state that unknown fields should be ignored for forward compatibility.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is fundamentally sound and well-aligned with existing codebase patterns. The decomposition is reasonable, acceptance criteria are largely testable, and the constraints correctly identify the critical "do not call `apply_to_env`" requirement. However, there are several gaps in edge case coverage and a few ambiguities that should be clarified before implementation.

## 9. Actionable Feedback

**Priority 1 (Block on implementation):**

1. **Clarify AC-7 activation semantics for Ollama:** Define what "activated" means for Ollama since there's no key to check. Suggest: "Ollama is 'activated' if it's in config OR (always, since it's key-free)." Or: "Ollama is always listed but shows endpoint status instead of key status."

2. **Add missing acceptance criterion for exit code:**
   > **AC-9:** `gcm status` exits 0 on success, exits 1 only if provider selection fails (invalid `GCM_PROVIDER` value) or config is malformed beyond recovery.

3. **Specify dispatch order relative to `ensure_configured()`:** Add a constraint clarifying that `Commands::Status` dispatch must happen **before** `ensure_configured()` is called (since that function calls `apply_to_env`).

4. **Add test case for malformed config:**
   > | 9 | Config file malformed | `GCM_CONFIG` points to invalid TOML | shows env-derived state, reports config not usable | `echo 'bad toml' > $GCM_CONFIG/config.toml && gcm status` |

**Priority 2 (Address during implementation):**

5. **Split ST2 into two sub-tasks:**
   - ST2a: Add `pub fn default_model(&self) -> &'static str` and `pub fn model_env_vars(&self) -> &[&'static str]` (trivial exposure)
   - ST2b: Add `pub fn resolve_model_with_source` helper (more complex)
   This improves granularity for time estimation.

6. **Add Ollama endpoint precedence detail to ST3:** Create a dedicated `ollama_endpoint_source()` function that checks `GCM_OLLAMA_BASE_URL` → `OLLAMA_HOST` (normalized) → config `endpoint` → `http://localhost:11434`. The spec mentions this but ST3 glosses over it.

7. **Specify provider output order:** Add to AC-3: "Providers are listed in canonical order: Groq, Google, OpenAI, Anthropic, Ollama (matching `cloud_then_ollama()`)."

8. **Add test for invalid `GCM_PROVIDER`:**
   > | 10 | Invalid GCM_PROVIDER | `GCM_PROVIDER=bogus gcm status` | exits 1 with error, reports unknown provider | `GCM_PROVIDER=bogus gcm status --json | jq '.status=="error"'` |

**Priority 3 (Consider for future revision):**

9. **Add a note about forward compatibility:** "JSON consumers should ignore unknown fields in `StatusReport` to maintain forward compatibility as the schema evolves."

10. **Document behavior for insecure config permissions:** Consider whether status should report `"config_permissions": "insecure"` or similar for 0644 files, matching the existing warning behavior.
