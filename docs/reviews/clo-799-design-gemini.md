# Design Review: CLO-799 enabled-model-pinning

**Reviewer**: Gemini 3.5 Flash

**Reviewed**: 2026-09-16

**Pipeline**: direct opencode (lok design-review synthesis step failed - template bug on skipped claude_fallback)



---



# Design Review: CLO-799 — Fix the enabled-model set pinning gcm to one model

**Verdict:** `APPROVE_WITH_SUGGESTIONS`

---

### Key Findings

1. **Completeness & Design Quality (High):** The design document is thorough, addressing the self-perpetuating configuration trap with minimal architectural footprint. Keeping the rejection logic local and deterministic by reusing the static fallback catalog is excellent operational practice (no network latency or I/O failure risks on error paths).
2. **Codebase Alignment:** Reusing existing CLI structures (`cliclack`) and scoping changes to `pub(crate)` ensures we do not pollute the public API or break existing test contracts.
3. **Robustness & UX:** The introduction of helper warning labels and improved defaults successfully addresses the underlying usability trap (filtering vs. selection) without requiring complex upstream library rewrites.

---

### Prioritized Actionable Items

#### 1. Robustness: Handle Empty Selection Edge Case (Code Quality / Blind Spots)
* **Finding:** In Section C, `initial_default_model` is defined to fall back to `selected.first()`. If the user has disabled model pinning entirely or toggled off all selections (making the `selected` vector empty), calling `.first()` will return `None`.
* **Action:** Ensure `initial_default_model` returns an `Option<String>` (or handles empty/none gracefully) to prevent runtime panics when no models are selected.

#### 2. UX: Format/Limit Long Known-Model Output (Operational Readiness)
* **Finding:** For providers with extensive static catalogs (e.g., OpenAI or complex custom deployments), appending the entire un-enabled list to the rejection message could result in a massive, unreadable wall of text on standard 80-character terminal screens.
* **Action:** Wrap long model lists cleanly to the terminal width, or restrict the printed models to the first $N$ (e.g., 5-8) most common models followed by an ellipsis (`...`), directing users to run `gcm provider` to see the full list.

#### 3. Reliability: Standardize Canonical Model ID Normalization (Security Posture)
* **Finding:** The document assumes canonical comparisons are identical across membership, fallback catalogs, and live-fetched names. Subtle differences in model prefixing (e.g., `gemini-3.5-flash` vs `models/gemini-3.5-flash`) can bypass membership checks or cause double list entries.
* **Action:** Ensure that the same parser or string-normalization helper (case, whitespace, and known provider prefixes) is applied consistently during the `model_is_enabled` check and when deduping the wizard's fallback lists.
