# Spec Review: clo-515

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-06-26
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is clear, complete, and accurate. It precisely defines the core issue: users cannot easily introspect active configurations and precedence rules without manual inspection. It matches the Linear task description exactly. There are no unstated assumptions, and it explicitly covers the requirement of working outside a git repository without invoking network or LLM dependencies.

## 2. Acceptance Criteria Review
**Strong**:
- **AC-1** clearly establishes that `gcm status` must be read-only, local-only, and function outside a git repository.
- **AC-4** ensures robust secret masking, preventing accidental key exposure.
- **AC-5** guarantees clean stdout/stderr segregation for automation and scripts, ensuring robust integration with JSON parsers like `jq`.
- **AC-8** ensures zero CLI conflicts and verifies the command line definitions.

**Gaps**:
- **Ollama Activation Clarity (AC-7)**: The current definition states Ollama is "always reachable to attempt, but report endpoint", which implies Ollama is always "activated" even on a vanilla machine without Ollama or any config. This is misleading. Ollama should only be considered "activated" if (a) it is explicitly enabled in the config file, or (b) `OLLAMA_HOST` or `GCM_OLLAMA_BASE_URL` is explicitly set in the environment.
- **Secret Masking Specificity (AC-4)**: The mention of an optional masked suffix (`…<last4>`) is a security concern. High-entropy key suffixes can leak critical key space or trigger security scanners. It is safer to strictly display "set" or "not set" (or source attribution) without printing any partial key suffix.

## 3. Constraints Check
**Aligned**:
- Preventing the invocation of `config::apply_to_env` in the status path is an excellent, high-value constraint. Doing so would corrupt env/config attribution by writing inline keys to the environment.
- Strictly enforcing no network calls, no Ollama daemon pinging, and no `Repo` interactions in the main path matches the read-only, non-network requirement perfectly.
- Leveraging existing source-of-truth helpers (`ProviderId::key_env_var`) avoids duplicate configuration code and ensures consistency.

**Concerns**:
- None. The constraints are exceptionally well-aligned with the codebase's existing blocking client, OS-specific paths, and precedence rules.

## 4. Decomposition Quality
**Well-scoped**:
- The sub-tasks are highly independent, modular, and scoped to under 2 hours of implementation.
- Separating CLI parsing (1), model-resolution helpers (2), and pure attribution logic (3) facilitates rigorous unit testing and test-driven development.
- Splitting the human view (5) and JSON serialization (4) keeps the main module thin and maintainable.

**Issues**:
- None. The sequence of sub-tasks is logical, cleanly handling dependencies and concluding with a dedicated acceptance test suite.

## 5. Evaluation Coverage
**Covered**:
- The evaluation matrix (Section 5) is comprehensive, defining clean, reproducible commands for all key outcomes (env-key, inline-config, secret masking, JSON structure, Ollama host, and model flag).
- The integration test strategy is realistic, building on the existing subprocess-driven testing pattern established in `tests/onboarding.rs`.

**Gaps**:
- **Bogus/Invalid GCM_PROVIDER**: There is no test case for handling a malformed or unknown `GCM_PROVIDER` env variable (e.g. `GCM_PROVIDER=invalid`). The status command should gracefully report this configuration error rather than crashing or swallowing it.
- **Gemini Precedence Edge Case**: Testing the exact resolution order of Google's dual env vars (`GCM_GEMINI_MODEL` > `GCM_GOOGLE_MODEL`) is omitted from the table but crucial for validation.

## 6. Codebase Alignment
**Violations**:
- **Onboarding Interception**: In `src/main.rs:run()`, the normal flow will trigger onboarding if `config::load()` returns `None` and `needs_onboarding()` is true. `gcm status` is a read-only subcommand that must *never* prompt or fail with `OnboardingRequired`. The `run()` dispatch loop must intercept the `Commands::Status` subcommand *before* any onboarding or configuration verification, mirroring how `Commands::Config` is dispatched.

**Alignment**:
- The spec strictly respects the `directories` config and cache path conventions.
- The proposed `StatusReport` payload cleanly mirrors the output-module and serialization patterns used in `src/output.rs`.

## 7. Blind Spots
- **Ollama Cloud Egress Warning**: Ollama models ending in `:cloud` are not zero-egress because the local daemon proxies them off-machine. The status command should detect and explicitly report if the active Ollama configuration is non-zero-egress (e.g. adding a `zero_egress` boolean field).
- **Zero-Config First Run**: The behavior of `gcm status` on a completely unconfigured system must be explicitly defined. It must output the status report showing that nothing is configured and exit 0, without launching the onboarding wizard or printing the long non-TTY setup instructions.
- **Config Path Resolution Failure**: If `config_path()` returns `None` (e.g., if OS-appropriate config directories cannot be resolved), the status command must handle this gracefully instead of panicking.

## 8. Verdict
`APPROVE_WITH_SUGGESTIONS`

## 9. Actionable Feedback
1. **Refine Ollama Activation Rule (AC-7)**: Only consider Ollama "activated" if it is explicitly listed in `config.toml` or if `OLLAMA_HOST` / `GCM_OLLAMA_BASE_URL` are set. Do not consider it active by default on a completely unconfigured machine.
2. **Remove Key Suffix Masking (AC-4)**: Eliminate the `…<last4>` masked suffix option entirely. Display only `set (env <NAME>)` or `set (config)` to prevent security scanning false positives and key-space leakage.
3. **Dispatch Subcommand Prior to Configuration Check**: Explicitly document that the `Commands::Status` subcommand is intercepted early in `src/main.rs:run()`, bypassing `Repo::discover()` and `ensure_configured()`. This guarantees the status command never blocks on onboarding or fails outside of a git repository.
4. **Incorporate Ollama Cloud Egress Introspection**: Introspect Ollama models and explicitly flag if they end in `:cloud` (not zero-egress) in both human and JSON outputs.
5. **Add Test Cases for Bogus Provider and Gemini Precedence**: Update the Evaluation table and the test suite to cover an invalid `GCM_PROVIDER` env variable and Google's dual env var resolution (`GCM_GEMINI_MODEL` vs `GCM_GOOGLE_MODEL`).
