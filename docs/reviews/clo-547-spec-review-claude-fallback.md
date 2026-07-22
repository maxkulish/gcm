# Spec Review: clo-547

**Reviewer**: Claude (fallback)
**Reviewed**: 2026-07-22
**Pipeline**: lok spec-review
**Note**: Both external reviewers failed; this is the fallback review

---

All spec claims validated against the code. Here is the fallback review.

---

# Spec Review: CLO-547 Model Discovery Hardening

**Reviewer**: Claude (fallback, after Gemini + Ollama/Codex failures)
**Spec**: `docs/specs/2026-07-22-clo-547-model-discovery-hardening.md`
**Validation**: every code reference checked against the working tree at `8c98cab`

## 1. Problem Statement

**Accurate and code-verified.** All three gaps are real:

- `keep_chat_model` (`src/provider/models.rs:248`) is indeed name-exclusion only with `_ => true` for Google/Vertex (models.rs:269). The claim that every non-5.6 OpenAI id the picker offers is guaranteed to fail at commit time is confirmed: `openai::validate_model` (`src/provider/openai.rs:133`) rejects anything outside `SUPPORTED_MODELS = ["gpt-5.6-terra", "gpt-5.6-luna"]` (openai.rs:28), and it's called from `select`, so the picker-vs-gate contradiction is genuine and the severity ranking (High) is right.
- The unconditional `.extend(static_fallback_models(id))` after a successful fetch is at models.rs:75-77, exactly as cited.
- `fetch_live` (models.rs:107) calls `http::get_json` directly with no seam; the `resolved_base_url_with` injection idiom cited as the in-file precedent exists (models.rs:164).
- The CLO-564 pairing note is well-grounded: `HttpGet` (`src/provider/http.rs:62`) already carries `auth: Option<(&'static str, String)>` + `extra_headers`, so the proposed "given an `HttpGet`, return the body" seam does accommodate the Vertex ADC + `x-goog-user-project` shape without signature rework.

One framing strength worth noting: the problem statement correctly identifies that the Gemini `generateContent` structural filter is necessary-but-insufficient rather than proposing to replace it - the 41-model live-catalog evidence makes the name-policy layer defensible rather than speculative.

## 2. Acceptance Criteria

**Testable and mutually consistent.** AC1-AC8 each map to a row in the Evaluation table. Spot-checks:

- **AC2**: the exemplars match `SUPPORTED_MODELS` exactly; the "no second hardcoded list" clause is enforceable via the proposed iteration test (edge case 4). Visibility is fine - models.rs already reads `super::openai::SUPPORTED_MODELS` at line 283.
- **AC3**: pass/exclude exemplars are internally consistent - none of the pass exemplars (`gemini-3.5-flash`, `gemma-*`, etc.) contain any of the 12 exclude substrings.
- **AC5**: the wizard's `model_items` tuples are `(String, String, &'static str)` (config.rs:969-972) with hint currently `""` - a static `"not in live catalog"` literal fits the existing type with no signature change. The escalation clause for cliclack filter-mode hint rendering is a good pre-registered exit.
- **AC7**: correctly restates the module contract (models.rs doc comment, no-key short-circuit at models.rs:57-67).

**Two gaps** (detailed in Priority Actions): AC5's absence check must compare by canonical form, and AC6's timeout case has an unstated cost problem.

## 3. Constraints & Assumptions

**Sound, with one structural assumption that doesn't hold.**

- The must/must-not/prefer/escalate split is well-calibrated; keeping the `provider::select` gate and Vertex short-circuit out of scope correctly firewalls CLO-545/CLO-564 territory.
- The "no new dependencies" and loopback-only constraints match the repo's existing test style.
- **Faulty assumption**: Decomposition 4 and AC6 offer `tests/` "following the `tests/vertex.rs` stub pattern" as an alternative test location. `gcm` is a **binary-only crate** (no `[lib]` target in Cargo.toml), so integration tests in `tests/` cannot call `pub(crate)` items like `fetch_supported_models_with` - and `tests/vertex.rs` works by driving the *built binary* end-to-end, which is impossible for the interactive cliclack wizard. Only the `#[cfg(test)]` in-crate option is viable. Not a blocker since the spec lists it first, but the false alternative should be struck so the implementer doesn't burn time on it.
- The Anthropic pass-through assumption ("believed all-chat") is honestly flagged with an escalation trigger rather than asserted.

## 4. Decomposition / Phases

**Correct ordering, right granularity.** 1 → {2, 3} → 4 is the true dependency graph: the seam (1) is what makes 4 possible; 2 and 3 are genuinely independent (filter policy vs. merge/labeling live in different functions). File attribution is accurate (step 3 touches both `models.rs` and the config.rs step-4 wizard block at config.rs:958-994). The proposed seam signature `fetch: impl Fn(&HttpGet) -> Result<String, ProviderError>` matches what `get_json` already is, so the refactor is mechanical. Scope estimate M is realistic.

One unstated but convenient fact supporting the plan: `resolved_base_url_with` already honors an explicit `endpoint` for **all** providers (models.rs:169), so TcpListener tests can point any provider at `http://127.0.0.1:PORT` through the existing parameter - no env manipulation needed.

## 5. Risks & Open Questions

1. **Timeout test cost (unaddressed)**: `MODEL_FETCH_TIMEOUT` is a hardcoded 5s const (`http.rs:28`) with one retry - a real stub-side timeout test adds ~10s of wall clock to `cargo test` and there is no env override for the model-fetch timeout to shrink it. The spec should say the timeout case is simulated via the injected seam returning a timeout-shaped `Err` (fast), with TcpListener reserved for the 401/500 cases.
2. **Google false-positive exclusions have no fresh-selection escape**: the multiselect is filter-only over candidates - no free-text entry. A legitimate future text model matching an exclude substring (e.g. a hypothetical `gemini-4-audio-understanding` text model) becomes unselectable for new setups; the only mitigation is the enabled-set union for existing configs. The conservative policy is owner-approved, so this is accepted risk - but it should be listed as such in the spec so the trade-off is on the record.
3. **Stale module doc**: models.rs:7-8 documents the "static baselines merged in" behavior AC1 removes. Nothing in the ACs forces the doc update, and clippy won't catch it.
4. **AC5 canonical-form comparison**: `wizard_model_list` dedupes by `canonicalize_model` (config.rs:1056-1060); the hint fn's "absent from live" check must use the same comparison, or a migrated `llama3` would be labeled `not in live catalog` when the live list has `llama3:latest`. Test 8 should include this exemplar.

No open questions require owner input before implementation - both owner decisions (gate-family filter, CLO-564 seam shape) are already recorded in the spec.

## Verdict

**APPROVE_WITH_SUGGESTIONS**

The spec is accurate against the code (every line reference and exemplar checked out), the ACs are testable, and the decomposition order is right. Nothing blocks starting implementation; the suggestions below prevent predictable mid-flight friction.

## Priority Actions

1. **(Medium)** Strike the `tests/` integration-test alternative in Decomposition 4 / AC6 - binary-only crate, `pub(crate)` seam, interactive wizard: only `#[cfg(test)]` in-crate tests are viable. Commit to that location explicitly.
2. **(Medium)** Specify how the timeout case in test 5 is exercised: injected-seam `Err` (fast), not a real 5s+retry TcpListener stall - or the suite grows ~10s with no way to shrink it (no env override on `MODEL_FETCH_TIMEOUT`).
3. **(Low)** Require canonical-form comparison in the AC5 hint fn and add a `llama3` vs `llama3:latest` exemplar to test 8.
4. **(Low)** Add "update the models.rs module doc comment (D7 merge description)" to Decomposition 3 - AC1 makes it false and no gate catches it.
5. **(Low)** Record risk 2 (no free-text escape for Google false-positive exclusions) as accepted risk in the spec, with the enabled-set union named as the mitigation.
