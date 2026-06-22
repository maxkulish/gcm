# Design Review: CLO-496

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-06-22
**Pipeline**: manual ollama launch codex invocation

---

## Design Review: CLO-496 Onboarding Wizard

### Verdict: **APPROVE_WITH_SUGGESTIONS**

The design is thorough, well-structured, and demonstrates strong architectural thinking. The document follows the codebase's existing patterns (mirroring `cache.rs`, reusing `ui.rs` shell-out idiom) and makes appropriate tradeoffs with clear reasoning. A few operational gaps and edge cases need attention before implementation.

---

## Key Findings

### ✅ Strengths

1. **Excellent codebase alignment** - The design mirrors `cache.rs` patterns (`write_atomic`, `open_private`, `config_dir()`) and reuses the shell-out idiom from `ui.rs`, ensuring consistency.

2. **Clear precedence preservation** - The `apply_to_env` approach (hydrate env vars that aren't already set) elegantly preserves the existing `flag > env > default` precedence by construction.

3. **Security-conscious defaults** - `0600` file permissions, atomic writes, echo-suppressed key entry, and storing `key: None` for env-sourced keys all demonstrate good security hygiene.

4. **Comprehensive test coverage** - The test matrix is thorough, including per-provider behavior and both unit and integration tests.

5. **Graceful non-TTY degradation** - The design properly handles CI/automation contexts with printed instructions + non-zero exit rather than hanging.

6. **Well-documented open questions** - All six open questions are genuine tensions with clear tradeoffs articulated.

---

## ⚠️ Actionable Items (Prioritized)

### P0 - Critical (Address before implementation)

1. **Ollama probe timeout missing** — The design mentions probing `http://localhost:11434` but specifies no timeout. A hanging network call blocks the wizard indefinitely.
   ```rust
   // Add explicit timeout (suggest 2-5 seconds)
   let response = reqwest::blocking::Client::builder()
       .timeout(Duration::from_secs(3))
       .build()?
       .get(&endpoint)
       .send();
   ```

2. **First-run race condition** — If two `gcm` processes start simultaneously with no config, both may attempt onboarding. The atomic write helps for the file, but the wizard interaction could collide. Consider:
   - File-locking the config path during `needs_onboarding` check
   - Or documenting this as acceptable (first-to-write wins, second sees config exists)

### P1 - Important (Address during implementation)

3. **Empty key input validation** — What happens if the user presses Enter without typing a key? Currently unspecified. Should be treated as cancel/opt-out, not `Some("")`. Add:
   ```rust
   // In read_secret or wizard loop
   if key.trim().is_empty() {
       // Either re-prompt or mark as env-only (key: None)
   }
   ```

4. **Malformed config file recovery** — The design treats wrong-version as "no usable config" but doesn't address TOML parse errors. Should malformed configs trigger onboarding, or exit with a specific error pointing to `~/.config/gcm/config.toml`?

5. **Config file permission check on load** — If someone manually creates `config.toml` with `0644`, the wizard should warn or refuse to use it. Consider:
   ```rust
   fn load() -> Option<Config> {
       // Check permissions on existing file, warn if not 0600
   }
   ```

6. **Ctrl+C during wizard** — If the user interrupts the wizard, the terminal echo state (`stty -echo`) must be restored. Ensure `read_secret` uses a RAII guard or `ctrlc` handler to restore echo even on panic/interrupt.

### P2 - Nice to have (Consider for follow-up)

7. **Key rotation UX** — The design doesn't mention how users update stored keys. Add a brief note in README or `--help` that `gcm config` re-runs setup and overwrites.

8. **Ollama endpoint URL validation** — The endpoint is stored without format validation. Consider basic URL parse before persisting.

9. **Default vs enabled providers validation** — The `build_config` rejects mismatched defaults, but what if someone hand-edits `config.toml` to have `default = "openai"` with only `[[providers]] id = "groq"`? Should `load()` validate, or defer to runtime `MissingKey` error?

---

## Minor Observations

- The `toml` crate dependency is appropriate for v1; if "no new deps" is a hard constraint, JSON is viable but less user-friendly for hand-editing.

- The `edition = 2021` → `unsafe set_var` in edition 2024 note is forward-thinking; consider adding a comment in code for future maintainers.

- The test plan's `needs_onboarding_matrix` covers the important cases; ensure it includes the edge case of `GCM_PROVIDER=""` (blank env var).

---

## Recommendation

Proceed with implementation after addressing P0 items (Ollama timeout, race condition decision). The P1 items can be resolved during implementation. This is a solid design that will significantly improve first-run UX.
