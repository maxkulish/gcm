# Design Review: CLO-496

**Reviewer**: Gemini 2.5 Pro (fallback model)
**Reviewed**: 2026-06-22
**Pipeline**: manual opencode invocation (primary model gemini-3.5-flash produced empty output)

---

I will now review the design document. First, I need to read the specified file for context.
### Verdict: **APPROVE_WITH_SUGGESTIONS**

This is an exceptionally well-researched and thorough design document. It demonstrates strong alignment with existing codebase patterns, a robust approach to error and edge-case handling (especially for non-TTY environments), and a comprehensive test plan. The proposed architecture is sound, minimally invasive, and maintains the existing configuration precedence.

The design is close to being fully approved, pending resolution of the following key findings.

---

### Key Findings

*   **Positive:**
    *   **Excellent Codebase Alignment:** The design consistently reuses existing patterns for configuration paths, atomic file writes, and CLI interactions, which significantly reduces risk.
    *   **Robustness:** The plan for handling non-TTY environments is a standout feature, preventing hangs in CI/scripted use and providing clear, actionable instructions.
    *   **High-Quality Abstractions:** The separation of the imperative wizard shell (`run_wizard`) from the pure, testable core logic (`build_config`) is a sign of a high-quality, maintainable design.
    *   **Comprehensive Planning:** The test, migration, and rollout plans are detailed and cover all major aspects of the change, from unit tests to manual verification.

*   **Needs Attention:**
    *   **Security Policy Conflict:** The most significant issue is the tension between the proposal to store inline API keys in a `0600` file and the guidance in `ADR-001 Decision 4` to *never* store them as plaintext. The design correctly identifies this but leaves it open. Storing secrets in plaintext, even with file permissions, is a security risk and violates a documented architectural decision.
    *   **Operational Risk:** The wizard plans to probe the Ollama daemon at a local endpoint. Without a specified connection timeout, this network call could cause the CLI to hang indefinitely if the local service is unresponsive.

---

### Actionable Items (Prioritized)

1.  **[High] Resolve ADR-001 Conflict on Secret Storage:**
    *   **Recommendation:** Strictly adhere to `ADR-001`. The wizard should guide the user to set the appropriate environment variables themselves. It should *not* capture and write secrets to the config file.
    *   **Justification:** This approach is more secure, avoids creating a "honeypot" file with secrets, and respects the established architectural principles. The wizard can still save which providers are enabled and which is the default, but it would always rely on the environment for the keys themselves.
    *   **Alternative:** If the UX of asking the user to set an env var is deemed unacceptable, `ADR-001` must be formally amended to allow for the `0600` file storage approach, with the rationale clearly documented.

2.  **[Medium] Add Network Timeout for Ollama Probe:**
    *   **Recommendation:** Specify and implement a short, reasonable connection timeout (e.g., 2-5 seconds) for the probe to the Ollama endpoint.
    *   **Justification:** This prevents a poor user experience where the CLI hangs during the first-run wizard because of a misconfigured or unresponsive local Ollama service.

3.  **[Low] Clarify Wizard Input Validation:**
    *   **Recommendation:** During implementation, ensure that the interactive prompts in the wizard are robust against empty or invalid input. For example, if a user selects a default provider that they have not enabled, the wizard should re-prompt with a clear error message rather than erroring out.

Once the secret storage strategy (Item 1) is finalized, this design can be considered fully approved.
