# Spec: Harden `gcm provider` model discovery (capability filtering, no-inject-after-live, transport tests)

**Created**: 2026-07-22
**Task**: [CLO-547](https://linear.app/cloud-ai/issue/CLO-547)
**Estimated scope**: M (2-3 files, 3 sub-tasks)

## 1. Problem Statement

The interactive `gcm provider` wizard discovers each provider's models via
`fetch_supported_models` (`src/provider/models.rs`, introduced in CLO-516). Three
correctness gaps - confirmed against code during the CLO-545 review, with fresh
live-catalog evidence from 2026-07-22 (CLO-564 investigation) - let the wizard
present selectable-but-broken or unavailable models:

1. **The capability filter is name-exclusion only, and provider list endpoints are
   not capability catalogs.** `keep_chat_model` (models.rs:248) excludes a fixed
   substring list for OpenAI/Groq and passes everything else through. OpenAI's
   `/v1/models` returns only `id`/`created`/`object`/`owned_by` - no
   chat/structured-output signal - so realtime, search, Codex, deep-research, and
   deprecated o-series ids pass the filter. Worse, since CLO-545 the runtime gate in
   `provider::select` rejects any OpenAI model outside `openai::SUPPORTED_MODELS`
   (`gpt-5.6-terra`, `gpt-5.6-luna`), so every other id the picker offers is
   *guaranteed* to produce a config error at commit time. On the Gemini side,
   `parse_models` filters to `supportedGenerationMethods` containing
   `generateContent` - necessary but not sufficient: the 2026-07-22 live AI Studio
   catalog passes 41 models through that filter, including `lyria-3-*` (music),
   `nano-banana-pro-preview` / `*-image` (image), `*-tts-preview`,
   `gemini-robotics-er-*`, `gemini-2.5-computer-use-*`, `deep-research-max-*`, and
   `antigravity-preview-*` - none usable for gcm's structured-output text calls.
   `keep_chat_model` has no Google/Vertex arm at all (`_ => true`).

2. **Live discovery injects unavailable models.** After a *successful*
   account-specific fetch, models.rs:73-77 always `.extend(static_fallback_models(id))`
   then dedupes - so baseline IDs are presented alongside live results even if the
   account or a compatible proxy did not return them. Static models are treated as
   proof of availability rather than an offline fallback.

3. **Discovery HTTP is untestable in isolation.** `fetch_live` (models.rs:107)
   constructs the per-provider `HttpGet` and calls `http::get_json` directly - no
   injectable seam - so `/v1/models` auth headers, parsing, filtering, live-success,
   and fallback behavior have no transport-level tests. (The file already models the
   right pattern: `resolved_base_url_with` injects the env lookup for hermetic tests.)

Affected users: anyone running `gcm provider` against a cloud provider - they can
select a model that either errors at commit time (OpenAI gate; non-text Gemini
variants) or was never available on their account (injected baselines).

**Pairing note (CLO-564)**: implemented immediately after this task, CLO-564
replaces the Vertex static short-circuit (models.rs:47) with a live
publisher-models fetch that needs an `Authorization: Bearer <ADC token>` header
plus an `x-goog-user-project: <project>` extra header. The transport seam built
here must accept that shape without rework - `HttpGet` already carries
`auth` + `extra_headers`, so a seam that injects "given an `HttpGet`, return the
body" is sufficient. The Vertex short-circuit itself stays untouched in this task.

## 2. Acceptance Criteria

- [ ] **AC1 (no-inject-after-live)**: When the live fetch succeeds with >=1
      capability-matched id, `fetch_supported_models` returns only live ids -
      `static_fallback_models` is NOT appended. Fetch failure / no key / empty
      result still degrades to the static fallback with the existing warning
      wording (unchanged behavior).
- [ ] **AC2 (OpenAI gate-family filter)**: On a successful OpenAI live fetch, only
      ids that are members of `openai::SUPPORTED_MODELS` are surfaced (owner
      decision 2026-07-22: gate family only). The membership check reads
      `SUPPORTED_MODELS` itself - no second hardcoded list.
- [ ] **AC3 (Gemini name policy)**: Google/Vertex ids passing the structural
      `generateContent` filter are additionally name-filtered; at minimum the
      families `image`, `tts`, `lyria`, `robotics`, `computer-use`,
      `deep-research`, `nano-banana`, `antigravity`, `omni`, `audio`, `veo`,
      `imagen` are excluded (case-insensitive substring, mirroring the OpenAI/Groq
      exclude style). `gemini-3.5-flash`, `gemini-3.5-flash-lite`,
      `gemini-3.6-flash`, `gemini-3.1-*` text models, and `gemma-*` chat models
      pass.
- [ ] **AC4 (Groq unchanged)**: Groq keeps the existing exclude-list behavior
      (no gate exists for Groq; its catalog is open-weights chat + whisper/guard
      already excluded).
- [ ] **AC5 (unverified labeling)**: When the wizard-side union (`wizard_model_list`,
      D7.3: current enabled set + default stay selectable) adds an id that a
      *successful* live fetch did not return, the multiselect renders it with a
      non-empty hint (e.g. `not in live catalog`) instead of the empty hint. The
      absent-from-live check compares by canonical form (`canonicalize_model`,
      config.rs) so a migrated `llama3` is not falsely labeled when live has
      `llama3:latest`. On fallback-source lists, no hints (nothing is
      live-verified, warning already shown).
- [ ] **AC6 (transport seam + tests)**: The live-fetch path is injectable
      (`fetch_supported_models_with`-style, mirroring `resolved_base_url_with`),
      and transport tests using a local `TcpListener` HTTP stub cover:
      per-provider auth headers (`Authorization: Bearer` for OpenAI/Groq,
      `x-api-key` + `anthropic-version` for Anthropic, `x-goog-api-key` for
      Google), response parsing, capability filtering,
      live-success-without-static-injection, and fallback on 401/500. The
      timeout case is simulated via the injected seam returning a timeout-shaped
      `Err` (no real stall: `MODEL_FETCH_TIMEOUT` is a hardcoded 5s const with
      one retry and no env override, so a real stall would add ~10s to
      `cargo test`). Tests live in-crate under `#[cfg(test)]`: gcm is a
      binary-only crate, so `tests/` integration tests cannot reach `pub(crate)`
      seams (the `tests/vertex.rs` TcpListener stub is the *pattern* reference
      only - it drives the built binary, which cannot exercise the interactive
      wizard).
- [ ] **AC7 (fallback-safety preserved)**: `fetch_supported_models` still never
      errors, always returns a non-empty list, and the no-key short-circuit does
      not touch the network.
- [ ] **AC8**: `cargo test`, `cargo fmt --check`, `cargo clippy -- -D warnings` all
      clean.

**Verification method**: unit + transport tests per the Evaluation table; manual
spot-check `gcm provider` -> Google (with `GEMINI_API_KEY`) shows no
`lyria`/`tts`/`image` ids, and -> OpenAI shows only the 5.6 family.

## 3. Constraints

**Must**:
- Preserve the module's contract: never errors, always a usable list, no-key
  short-circuit skips the network (models.rs doc comment).
- Source the OpenAI membership filter from `openai::SUPPORTED_MODELS` (single
  source of truth established by CLO-545).
- Keep the Gemini structural `generateContent` filter in `parse_models`; the new
  name policy layers on top (in `keep_chat_model`'s Google/Vertex arm), not
  instead of it.
- Keep the D7.3 wizard-side union in `wizard_model_list` (current enabled +
  default remain selectable) - AC5 labels it, does not remove it.
- The transport seam must accept an `HttpGet` with arbitrary `auth` +
  `extra_headers` so CLO-564's Vertex fetch (Bearer ADC token +
  `x-goog-user-project`) plugs in without signature changes.
- Leave the Vertex short-circuit (models.rs:47) and `static_fallback_models`
  contents untouched (both are CLO-564 scope).
- No new dependencies.

**Must-not**:
- Do not change runtime model resolution or the `provider::select` gate.
- Do not make tests hit the real network (loopback `TcpListener` only).
- Do not add a `pub` API surface beyond what the wizard needs (`pub(crate)`/
  `pub(super)` like the existing seams).

**Prefer**:
- Express the name policies as `const` slices with focused unit tests (existing
  `EXCLUDE` style).
- Follow the `resolved_base_url_with(lookup)` injection idiom for the fetch seam.
- Keep hint text short and lowercase, matching existing wizard copy style.

**Accepted risk** (reviewer finding 5): a Google/Vertex name-exclusion false
positive (a future chat-capable id matching an excluded substring) cannot be
selected via free text - the multiselect is filter-only. Mitigation: the D7.3
enabled-set union keeps any previously-enabled id selectable, and the exclude
list is a `const` amendable in a release. Accepted as the conservative
trade-off this task exists to make.

**Escalate when**:
- The Anthropic live list turns out to include non-chat ids that need a policy
  (currently pass-through and believed all-chat) - would widen scope.
- cliclack's multiselect hint (third tuple element) cannot render per-item hints
  in filter mode - AC5 would need a different labeling mechanism.

## 4. Decomposition

1. **Transport seam**: extract the body of `fetch_supported_models` into
   `fetch_supported_models_with(id, key, endpoint, fetch)` where
   `fetch: impl Fn(&HttpGet) -> Result<String, ProviderError>`; the public
   wrapper passes `http::get_json`. Build the per-provider `HttpGet` in a
   helper the tests can also call. - files: `src/provider/models.rs`
2. **Capability policy**: OpenAI arm of `keep_chat_model` becomes membership in
   `openai::SUPPORTED_MODELS`; add Google/Vertex arm with the AC3 exclude list;
   Groq keeps the current exclude list; Anthropic/Ollama pass through. Unit
   tests per family (kept + excluded exemplars from the 2026-07-22 live
   catalogs). - files: `src/provider/models.rs`
3. **No-inject-after-live + wizard labeling**: on `Ok` with >=1 filtered id,
   return live-only (drop the `.extend`); empty-after-filter keeps today's
   fallback+warning path. Update the models.rs module doc (lines 7-8), which
   documents the static-baseline merge this removes. In the wizard (config.rs
   step 4), pass a hint for candidates absent (by canonical form) from a
   `Live`-source `outcome.models`. - files: `src/provider/models.rs`,
   `src/config.rs`
4. **Transport tests**: in-crate `#[cfg(test)]` `TcpListener` stub serving
   canned responses; assert request headers per provider, parse/filter results,
   AC1 no-inject, and 401/500 fallback; timeout fallback via an injected-seam
   `Err`. - files: `src/provider/models.rs` (cfg(test)).

**Dependency order**: 1 -> {2, 3} independent -> 4 (tests exercise all prior).

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | OpenAI live body with `gpt-5.6-terra`, `gpt-4.1`, `o3-mini`, `gpt-realtime`, `codex-mini-latest`, `deep-research` | only `gpt-5.6-terra` surfaced | `cargo test` (seam-injected body) |
| 2 | Gemini live body with `gemini-3.6-flash`, `lyria-3-pro-preview`, `gemini-3.1-flash-image`, `gemini-2.5-flash-preview-tts`, `gemini-robotics-er-1.6-preview`, `nano-banana-pro-preview` (all with `generateContent`) | only `gemini-3.6-flash` surfaced | `cargo test` |
| 3 | Successful live fetch returning only `gpt-5.6-luna` | result is exactly `["gpt-5.6-luna"]` - no `gpt-5.6-terra` injected from baselines | `cargo test` |
| 4 | Live fetch returns 200 but every id is filtered out | fallback list + "no usable models" warning (today's empty-live path) | `cargo test` |
| 5 | Live fetch 401 / 500 (TcpListener stub); timeout as an injected-seam timeout-shaped `Err` | static fallback + "could not fetch" warning; no panic; no real 5s stall in the suite | `cargo test` |
| 6 | No key for a cloud provider | fallback + env-var warning, zero network calls (stub asserts no connection) | `cargo test` (existing test extended) |
| 7 | Auth headers: OpenAI/Groq `Authorization: Bearer`, Anthropic `x-api-key`+`anthropic-version`, Google `x-goog-api-key` | stub sees the exact header per provider | `cargo test` |
| 8 | Wizard union labeling | candidate in enabled-set but absent from Live outcome gets hint `not in live catalog`; Live-returned ids get empty hint; canonical-form match: enabled `llama3` with live `llama3:latest` gets NO hint | `cargo test` (unit on the hint fn) |
| 9 | Full gates | all green | `cargo test && cargo fmt --check && cargo clippy -- -D warnings` |

**Edge cases to verify**:
- Live list returns duplicates -> dedupe still first-occurrence stable.
- Filter empties an OpenAI live list that contained only gate-rejected ids
  (e.g. proxy serving `gpt-4o`) -> falls back with warning (test 4 path).
- Case sensitivity: `Gemini-3.1-Flash-IMAGE` style ids still excluded
  (case-insensitive matching).
- `SUPPORTED_MODELS` growth: adding a model there automatically widens both the
  gate and the discovery filter (assert via a test iterating `SUPPORTED_MODELS`).
