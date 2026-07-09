# Design Review: CLO-537 - Add Vertex AI provider (keyless ADC)

**Reviewer**: Gemini 2.5 Pro (fallback model; `gemini-3.1-pro-preview` returned empty output twice)
**Reviewed**: 2026-07-08
**Design Document**: docs/designs/clo-537-vertex-provider.md
**Method**: Read the design doc plus `src/provider/{mod,gemini,http}.rs`, `src/config.rs`, `src/status.rs`, ADR-001 (code-reading review).

---

## 1. Completeness Check

The design document is largely complete. It correctly identifies the need for a new `ProviderId`, a new `vertex.rs` implementation, configuration changes in `config.rs`, and updates to the `gcm status` and `gcm provider` commands. It also specifies a testing strategy.

However, it is incomplete in one area: it overlooks a necessary logic change in `src/config.rs` within the `env_plan` function, which currently assumes any provider without a `key_env_var` is Ollama.

## 2. Architecture Assessment

**Strengths**:

*   **Code Reuse**: The decision to reuse the existing Gemini payload builders (`build_*_payload`) and response extractor (`extract_text`) is excellent. It minimizes code duplication and ensures that any future improvements or bug fixes to the Gemini-specific logic (like the CLO-534 schema fix) are automatically inherited by the Vertex provider.
*   **Keyless Auth Pattern**: Introducing ADC via a `gcloud` shell-out is a pragmatic approach. It aligns with the common developer workflow on GCP, avoids introducing new heavy dependencies for native ADC, and fits the project's established pattern of using external binaries (`git`, `gh`). The lazy token acquisition is also a good performance choice.
*   **Configuration**: The proposed changes to `config.toml` (adding optional `project` and `location` fields) are non-breaking, which is a key consideration for existing users. The plan to honor GCP-standard environment variables (`GOOGLE_CLOUD_PROJECT`) is also a thoughtful touch that improves the user experience.

**Concerns**:

*   **Shelling out to `gcloud`**: While pragmatic, this introduces a runtime dependency. The design correctly identifies the need for actionable error messages if `gcloud` isn't found or configured, but the performance implications of this shell-out on every request could be a concern. A slow `gcloud` command could significantly impact the tool's responsiveness. The design should consider and specify timeouts for this external command.
*   **Promotion of Gemini functions**: The proposal is to make the Gemini functions `pub(super)`. This is a reasonable "smaller diff" approach, but it creates a slightly awkward dependency where `vertex.rs` is coupled to implementation details inside `gemini.rs`. The design acknowledges this and suggests a `google_common.rs` as a future option; this is an acceptable trade-off for the MVP.

## 3. ADR Compliance

The design fully complies with `docs/adrs/001-foundational-architecture-decisions.md`.

*   **Decision 1 (Shell out to `git`)**: The design uses a shell-out for `gcloud`, which aligns with the established pattern of using external binaries.
*   **Decision 2 (Blocking HTTP client)**: The proposed implementation is synchronous and will use the existing blocking HTTP client in `http.rs`. There is no introduction of an async runtime.
*   **Decision 4 (Config format/location/precedence)**: The design correctly extends the TOML config, respects the established precedence rules, and handles secrets appropriately (the token is generated at runtime, not stored).

## 4. Security Review

The security posture is sound.

*   **Authentication**: Using short-lived ADC tokens is significantly more secure than long-lived, user-managed API keys. The token is acquired at runtime and held in memory, not persisted to disk.
*   **Command Execution**: The shell-out command (`gcloud auth application-default print-access-token`) is static and does not involve user-provided input, mitigating the risk of command injection.
*   **Secrets on Stdout**: The design for `gcm status` explicitly states it will report the *auth source* (`gcloud ADC`) but not the token itself, preventing accidental secret leakage.

## 5. Implementation Concerns

*   **Fragile Provider-Type Detection**: The analysis confirms the primary blind spot. Several places in `src/config.rs` use `id.key_env_var().is_none()` to mean "this is the Ollama provider."
    *   `src/config.rs` (`run_wizard`, `run_provider_wizard`): The interactive wizards will incorrectly try to configure an endpoint for Vertex. The design doc correctly specifies a three-way branch is needed, but this highlights the fragility of the existing code.
    *   `src/config.rs` (`env_plan`): This function, which bridges config values to environment variables, will incorrectly attempt to process the Vertex config section as if it were Ollama. The design document *missed* this location.
    *   This pattern should be refactored to be more explicit, for example by adding a method like `fn auth_method(&self) -> AuthMethod` to `ProviderId` that can return `ApiKey`, `KeylessEndpoint`, or `KeylessADC`.

*   **Error Handling for `gcloud`**: The design mentions typed errors for `gcloud` not being found or not being logged in. This needs to be comprehensive. What happens if `gcloud` hangs? What if it returns a malformed token? The implementation must be robust against various failure modes of the external command.

## 6. Blind Spots

*   **The `key_env_var().is_none()` issue**: As detailed above, this is the most significant blind spot. The current logic in `config.rs` directly couples "keyless" to "Ollama's endpoint config," which will break when Vertex is added.
*   **Wizard logic for Vertex-specific fields**: The design for `run_provider_wizard` mentions prompting for `project` and `location`, but the main `run_wizard` (which configures *all* providers) does not. The implementation must ensure these Vertex-specific prompts are included in the main wizard flow when Vertex is selected.
*   **Performance of Token Generation**: The design assumes the `gcloud` shell-out is fast enough. On a slow machine or a system with unusual `gcloud` configurations, this could add noticeable latency to every command. This should be acknowledged as a potential operational issue.

## 7. Verdict

**APPROVE_WITH_SUGGESTIONS**

The core architectural approach is sound, secure, and aligns well with project standards. The plan to reuse Gemini's payload logic is a major strength. The identified blind spot regarding keyless provider detection is significant but easily correctable.

## 8. Actionable Feedback

1.  **CRITICAL - Refactor Keyless Provider Logic**: Before implementation, modify the logic in `src/config.rs` that relies on `key_env_var().is_none()`. The `match id.key_env_var()` blocks in `env_plan`, `run_wizard`, and `run_provider_wizard` will handle `Vertex` incorrectly. Refactor to be explicit about `ProviderId` (e.g. `match id { Ollama => …, Vertex => …, _ => api_key }`). A similar explicit match is required in `env_plan` to bridge `project`/`location` for Vertex.
2.  **Add Timeout to `gcloud` Shell-out**: The token acquisition in `src/provider/vertex.rs` must include a reasonable timeout (e.g. 5-10 seconds) to prevent the CLI from hanging.
3.  **Update `commented_reference()`**: This function in `src/config.rs` needs a new arm to generate the commented reference for `project`/`location` for Vertex.
4.  **Update Main Wizard**: Ensure `run_wizard()` includes the `project`/`location` prompts when `ProviderId::Vertex` is selected. The design focuses more on the dedicated `gcm provider` wizard.

---

*This review was automatically generated. Human judgment should be applied when interpreting these suggestions.*
