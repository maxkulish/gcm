# Spec: Live Vertex model discovery + Gemini catalog refresh (default → gemini-3.5-flash-lite)

**Created**: 2026-07-22
**Task**: [CLO-564](https://linear.app/cloud-ai/issue/CLO-564)
**Estimated scope**: M (4 files, 4 sub-tasks)

## 1. Problem Statement

`gcm provider` → Vertex still shows only the static built-in `gemini-3.1-*` trio
("Using the built-in model list") because `fetch_supported_models` short-circuits
Vertex before any network call (CLO-537 design D4 deferred the live catalog). A
Vertex-selected install - the owner's setup - cannot see `gemini-3.5-flash`,
`gemini-3.5-flash-lite`, or `gemini-3.6-flash` in the picker at all; the Google
(AI Studio) provider has no such gap. The static side is equally stale: the
offline fallback catalog and the built-in Gemini default (`gemini-3.1-flash-lite`
in `ProviderId::default_model`) are frozen at the 3.1 generation.

Feasibility verified live 2026-07-22 (CLO-564 investigation comment):
`GET https://aiplatform.googleapis.com/v1beta1/publishers/google/models?pageSize=200`
with `Authorization: Bearer $(gcloud auth application-default print-access-token)`
returns 403 "requires a quota project" alone, and 200 with
`x-goog-user-project: <project>` - 22 publisher models including all three new
Geminis, entries shaped `{"publisherModels":[{"name":"publishers/google/models/<id>", ...}]}`.

Everything this needs already exists post-CLO-547: the injectable
`fetch_supported_models_with` seam takes any `HttpGet` (auth + extra headers);
`vertex.rs` owns ADC token acquisition (`GCM_VERTEX_TOKEN` else bounded `gcloud`
shell-out) and the `GCM_VERTEX_BASE_URL` test seam; the wizard has
project/location in hand and runs the ADC probe *before* the model fetch; the
Google/Vertex name exclude-list already covers the non-chat publisher entries
(imagen/veo/etc.).

## 2. Acceptance Criteria

- [ ] **AC1**: `gcm provider` → Vertex with working ADC lists the live publisher
      catalog - including `gemini-3.5-flash`, `gemini-3.5-flash-lite`,
      `gemini-3.6-flash` - with the "Fetched N models" spinner message (Live
      source), name-filtered by the existing Google/Vertex exclude-list.
- [ ] **AC2**: Without a resolvable ADC token (no `GCM_VERTEX_TOKEN`, gcloud
      absent/unready), the Vertex picker degrades to the refreshed static list
      with a warning naming the remedy - never an error, no gcloud shell-out
      from `models.rs` itself (the wizard resolves the token and passes it in;
      a `None` token short-circuits without any network call, mirroring the
      no-key path).
- [ ] **AC3**: The static Gemini fallback (Google + Vertex) becomes
      `gemini-3.5-flash-lite`, `gemini-3.5-flash`, `gemini-3.6-flash`,
      `gemini-3.1-flash-lite`, `gemini-3.1-flash`, `gemini-3.1-pro` (3.1 entries
      retained - still published).
- [ ] **AC4**: `ProviderId::default_model` returns `gemini-3.5-flash-lite` for
      Google and Vertex (owner decision); `gcm status` reflects it on a fresh
      setup; the fallback-contains-default invariant test still passes.
- [ ] **AC5**: Vertex discovery honors `GCM_VERTEX_BASE_URL` (test seam) and
      sends `Authorization: Bearer <token>` + `x-goog-user-project: <project>`;
      the parser reads the `publisherModels` key and de-prefixes
      `publishers/google/models/`; a missing/empty/malformed `publisherModels`
      shape yields an empty vec (→ static fallback via the empty-live path,
      never a panic). `resolved_base_url_with` gets a dedicated Vertex arm
      (`GCM_VERTEX_BASE_URL` / global `aiplatform` default), splitting the
      previously shared Google arm.
- [ ] **AC6**: Existing config whitelists keep working unchanged (an old
      `models = ["gemini-3.1-flash-lite"]` still restricts until the wizard is
      re-run); no config version bump.
- [ ] **AC7**: `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` clean.

**Verification method**: unit + transport tests below; live end-to-end
`gcm provider` → Vertex on the owner's machine after merge (the original
complaint's screen must show the 3.5/3.6 models).

## 3. Constraints

**Must**:
- Keep `models.rs` free of subprocess calls - token resolution stays in
  `vertex.rs`, exposed as a `pub(crate)` helper the wizard calls; discovery
  receives the token as its `key` argument.
- Preserve the never-errors / always-non-empty / no-network-without-credentials
  contract, including the CLO-547 blank-id guard and no-inject-after-live.
- Reuse the existing Google/Vertex name exclude-list as the capability filter
  for publisher entries (no new policy).
- Global `aiplatform.googleapis.com` host for discovery regardless of location
  (list is not region-scoped for the wizard's purposes); `GCM_VERTEX_BASE_URL`
  override wins.
- Sweep remaining `gemini-3.1-flash-lite` default references in comments,
  config template text, and README (CLO-545 sweep pattern).

**Must-not**:
- No runtime commit-path changes in `vertex.rs` beyond exposing the token
  helper; no config schema/version changes; no per-commit model fetch.
- No real network or gcloud in tests.

**Prefer**:
- Mirror CLO-547's TcpListener + injected-seam test styles.

**Escalate when**:
- The publisher-models response requires pagination for the models gcm needs
  (pageSize=200 returns 22 today - if a `nextPageToken` ever matters, stop and
  ask).

## 4. Decomposition

1. **Token helper**: extract/expose `pub(crate) vertex_access_token()` in
   `vertex.rs` (env-else-gcloud, reusing `gcloud_token`); wizard resolves it
   for Vertex after the ADC probe and passes it as the fetch key plus the
   configured project. - files: `src/provider/vertex.rs`, `src/provider/mod.rs`,
   `src/config.rs`
2. **Discovery arm**: replace the Vertex short-circuit with: `None` token →
   fallback + ADC warning; else build `HttpGet` (global host or
   `GCM_VERTEX_BASE_URL`, `/v1beta1/publishers/google/models?pageSize=200`,
   Bearer auth, `x-goog-user-project` when project is known) via a new
   `project` parameter threaded through `fetch_supported_models[_with]`;
   new `parse_models` Vertex arm for the `publisherModels` shape. - files:
   `src/provider/models.rs`, `src/config.rs`
3. **Catalog + default refresh**: `static_fallback_models` Google/Vertex set per
   AC3; `default_model` → `gemini-3.5-flash-lite` (Google + Vertex); update the
   asserting tests and sweep stale references (comments, config template,
   README). - files: `src/provider/mod.rs`, `src/provider/models.rs`,
   `src/config.rs`, `README.md`
4. **Tests**: parse fixture for `publisherModels` (de-prefix, name filter);
   TcpListener transport test asserting Bearer + `x-goog-user-project` headers
   and Live outcome; no-token fallback (no network); default/fallback invariant
   updates. - files: `src/provider/models.rs` (cfg(test)), `src/config.rs` tests
   if the wizard call site changes signatures.

**Dependency order**: 1 → 2 → {3, 4}.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Vertex fetch with token+project against TcpListener stub serving `publisherModels` incl. `gemini-3.6-flash`, `imagen-4`, `veo-3` | Live source; `["gemini-3.6-flash"]` (imagen/veo name-filtered); request carried `Authorization: Bearer` + `x-goog-user-project` | `cargo test` |
| 2 | Vertex fetch with `None` token | Fallback + warning mentioning ADC/`GCM_VERTEX_TOKEN`; zero network | `cargo test` |
| 3 | Vertex fetch transport error (injected `Err`) | Fallback + "could not fetch" warning | `cargo test` |
| 4 | Parse fixture `{"publisherModels":[{"name":"publishers/google/models/gemini-3.5-flash"}]}` | `["gemini-3.5-flash"]` | `cargo test` |
| 5 | `static_fallback_models(Google/Vertex)` | exactly the AC3 six, default first | `cargo test` |
| 6 | `default_model()` Google + Vertex | `gemini-3.5-flash-lite`; fallback-contains-default invariant green | `cargo test` |
| 7 | Full gates | all green | `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings` |
| 8 | Live e2e (owner, post-merge) | Vertex picker shows 3.5/3.6 models | `gcm provider` → Google (Vertex AI) |

**Edge cases to verify**:
- Blank `name` in a publisher entry → dropped by the CLO-547 blank-id guard.
- Project id absent (config missing) → header omitted; 403 → fallback+warning.
- `GCM_VERTEX_BASE_URL` set → discovery uses it (hermetic tests rely on this).
