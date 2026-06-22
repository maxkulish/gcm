# Spec Review Synthesis: clo-495

**Synthesized**: 2026-06-22
**Pipeline**: lok spec-review

---

## Synthesis: CLO-495 Ollama Provider Spec Review

Two external reviewers succeeded (Gemini, Ollama). Claude fallback was correctly skipped. Both returned **APPROVE_WITH_SUGGESTIONS** with no blocking violations and strong codebase alignment.

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | **OLLAMA_HOST port defaulting.** Scheme-less/port-less hosts (e.g. `localhost`, `127.0.0.1`, `my-server.local`) must append `:11434`, or normalization yields port 80 and the connection fails. Needs an explicit AC + unit test; neither currently asserts it. | High |
| 2 | **`HttpRequest.auth_env_var` for Ollama.** The struct field is mandatory but Ollama has no auth. Pass `""` as a placeholder; `send_once` should only reference `auth_env_var` when `auth` is `Some(...)`. Spec must state the chosen value. | Medium |
| 3 | **Actionable error remapping stays inside `ollama.rs`.** Unreachable-daemon (Transport) and missing-model (404) must be remapped to setup-oriented messages within the provider, without widening the shared `ErrorKind` taxonomy. | Medium |
| 4 | Strong overall: clear problem statement, sound 6-task decomposition, realistic mock-server eval approach, correct `gemini.rs` template and `Option<auth>` transport change. | Info |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| 1 | **Which `ErrorKind` carries actionable messages** | Remap unreachable-daemon and 404 to `ErrorKind::Config(String)` — it displays verbatim with no fixed prefix/suffix, keeping `ollama pull`/setup instructions pristine. Warns standard `BadRequest`/`Transport` prefixes undermine custom text. | Keep `Transport(String)` and `Http(404)`; enrich the `String` message in place by inspecting the underlying `ureq::Error`. Stays within the existing taxonomy. | Skipped (fallback not run) |

*This is the one substantive divergence. Recommendation: verify the actual `Display` impls for `Transport`/`Http`/`Config` in `error.rs` before deciding — if Transport/Http prepend a prefix that mangles the message, Gemini's `Config` route is the cleaner fix; if they display the String cleanly, Ollama's in-place enrichment is the lower-blast-radius choice.*

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | **Parse 404 error body.** Ollama returns `{"error":"model '...' not found, try pulling it first"}`; spec should parse this body for the AC-7 actionable message rather than inferring from status alone. | Ollama | Medium |
| 2 | **Timeout for local inference.** Local MLX models are slower than cloud APIs; document that `GCM_HTTP_TIMEOUT_SECS` applies to Ollama and may need raising for large diffs. | Ollama | Medium |
| 3 | **"No Authorization header" test.** Add an acceptance assertion that no `Authorization` header is sent for `--provider=ollama` (the core zero-auth guarantee is currently untested). | Ollama | Medium |
| 4 | **Missing `message` key.** Specify mapping a response lacking `message` entirely → `ErrorKind::Deserialize("...missing 'message' key")`, beyond the invalid-JSON-content case. | Ollama | Medium |
| 5 | **Error-prefix concern (basis of Disagreement #1).** Standard `ErrorKind` variants display with fixed prefixes that can blunt AC-2/AC-7 custom messages. | Gemini | Medium |
| 6 | **`:cloud` egress documentation surface.** Spec notes `:cloud` is not zero-egress but doesn't say where it's disclosed (CLI help / README) or whether a warning fires on selection. | Ollama | Low |
| 7 | **`stream:false` criticality.** Omitting it makes `/api/chat` stream NDJSON and break the parser. Already required by spec — flag as a critical, must-not-regress detail. | Ollama | Low |
| 8 | **Missing eval rows:** `thinking` field present (still parses), trailing-slash host, empty/whitespace `OLLAMA_HOST`, case-insensitive `ProviderId::parse("OLLAMA")`. | Ollama | Low |
| 9 | **Effort estimates.** Task 3 (core backend) and Task 6 (mock-server routes) likely 3-4h, not ~2h. | Ollama | Low |
| 10 | **Ollama version/CI notes.** State minimum tested Ollama version and make explicit that CI has no daemon so AC-O* run against the mock only. | Ollama | Low |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** — both reviewers approved; suggestions are improvements, not blockers.

## Priority Actions

**Must address before implementation**
1. Add OLLAMA_HOST **port defaulting** to `:11434` for scheme-less/port-less hosts, with an explicit AC and unit test (Agreement #1).
2. Specify **`auth_env_var: ""`** for Ollama and gate its use in `send_once` on `auth.is_some()` (Agreement #2).
3. Resolve **Disagreement #1** (`Config` vs enriched `Transport`/`Http`) by inspecting the real `Display` impls, then lock the error-mapping mechanism in Task 4 (Agreement #3 + Novel #5).
4. Specify **parsing the 404 error body** for the AC-7 missing-model message (Novel #1).
5. Add the **"no Authorization header" acceptance test** (Novel #3).

**Should address**
6. Document **timeout behavior** for local inference (Novel #2).
7. Specify **missing-`message`-key → Deserialize** handling (Novel #4).
8. Document the **`:cloud` non-zero-egress disclosure** surface (Novel #6).

**Minor**
9. Add the missing **eval rows** (Novel #8) and reaffirm `stream:false` as critical (Novel #7).
10. Revise **Task 3/Task 6 estimates** and add the **Ollama version / CI-mock** notes (Novel #9, #10).
