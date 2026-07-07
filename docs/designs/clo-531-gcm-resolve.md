# Design: CLO-531 — `gcm resolve` LLM-assisted merge conflict resolver (Phase 1: local markers)

**Linear:** [CLO-531](https://linear.app/cloud-ai/issue/CLO-531/add-gcm-resolve-llm-assisted-merge-conflict-resolver-phase-1-local)
**Branch:** `feat/clo-531-resolve`
**PRD:** `docs/prds/clo-531-gcm-resolve.md`
**Discovery:** `docs/discovery/clo-531.md`
**Date:** 2026-07-06

---

## 1. Problem

When a merge, rebase, or cherry-pick produces conflicts, the user must hand-edit every `<<<<<<<` marker, decide keep-left / keep-right / combine per hunk, validate syntax, and stage/continue manually. For multi-file conflicts this is tedious and error-prone; for non-trivial code merges it is easy to drop one branch's intent or introduce a syntax error. The discovery phase (§Discovery Report) confirmed that the foundational gcm rewrite pieces — the `Provider` trait (CLO-489), structured output + defensive parsing (CLO-487/517), config/onboarding (CLO-496/516), and privacy filtering (CLO-490/514) — are all in place and reusable. The chosen approach is a **layered local pipeline** where deterministic stages shrink what the LLM must reason about before the model is invoked as the last resort.

## 2. Goals / Non-goals

### Goals

- **G1:** New `gcm resolve` subcommand that detects an in-progress merge/rebase/cherry-pick and enumerates conflicted files.
- **G2:** Re-checkout conflicted files with `zdiff3` markers and parse each hunk into labeled `base`/`ours`/`theirs`.
- **G3:** Optional `mergiraf` pre-stage: detect on `PATH`, run per file, forward only unresolved hunks onward.
- **G4:** Reuse the existing `Provider` trait to resolve only the hard hunks via a 3-way, function-granularity prompt with anti-hallucination rules.
- **G5:** Validation gate: syntax-safe default; optional `conflict.validate_cmd`; exactly one bounded LLM retry on failure, then escalate to human.
- **G6:** New `[conflict]` config block + CLI flags with the same `flag > env > config > default` precedence as the rest of gcm.
- **G7:** Per-file `[Y/n/e]` preview loop; write resolved files only on accept; never auto `git add` or `--continue`.
- **G8:** Honor `--dry-run` (preview, no write), `--json` (machine envelope), `--yes` (non-interactive), `.gcmignore` matcher, `--secret-scan` for provider egress.

### Non-goals

- Remote MR/PR orchestration (fetch a GitHub PR / GitLab MR branch, run the merge, push to a resolution branch) — Phase 2.
- RAG-over-git-history exemplars (LLMinus style) — Phase 2+ reliability add-on.
- Bundling `mergiraf` or any tree-sitter grammar as a Rust dependency (ADR-001 pattern: shell out to tools on `PATH`).
- Running build/test as the default validation (gcm is language-agnostic; the user opts in via `validate_cmd`).
- Auto-continuing the merge/rebase after resolution (non-negotiable safety: the human decides).

## 3. Architecture

### Module overview

```
src/
├── cli.rs              — add Commands::Resolve variant + conflict flags
├── main.rs             — add Commands::Resolve dispatch branch (early return)
├── resolve/
│   ├── mod.rs          — orchestration: pipeline stages, per-file loop
│   ├── markers.rs      — zdiff3 marker parser → ConflictFile { hunks: Vec<Hunk> }
│   ├── classify.rs     — hunk classification (trivial / moderate / complex)
│   ├── mergiraf.rs     — optional external pre-resolution stage
│   ├── prompt.rs       — 3-way resolution prompt + JSON schema
│   ├── validate.rs     — validation gate (syntax-safe default + user cmd)
│   └── report.rs       — ResolveReport envelope for --json
├── provider/mod.rs     — add resolve_hunks() to Provider trait
├── config.rs           — add [conflict] section to Config
├── output.rs           — add resolve-specific envelope variants
├── error.rs            — add resolve-specific error variants
└── git.rs              — add unmerged_files(), checkout_conflict_zdiff3()
```

### Data flow

```
gcm resolve
  │
  ├─ 1. Detect conflict state (git.rs: is_merging(), unmerged_files())
  │     └─ Abort if no merge in progress or no unmerged files
  │
  ├─ 2. Re-checkout conflicted files with zdiff3 markers
  │     └─ git checkout --conflict=zdiff3 -- <paths>
  │
  ├─ 3. Binary file detection (git.rs)
  │     └─ Skip binary files (git diff --numstat shows "-" for binary)
  │     └─ Print a warning, leave them conflicted for manual resolution
  │
  ├─ 4. Read each text conflicted file → markers.rs parser
  │     └─ Parse zdiff3 markers → ConflictFile { path, hunks: Vec<Hunk> }
  │        Hunk { base: Option<String>, ours: String, theirs: String,
  │               start_line: usize, end_line: usize }
  │
  ├─ 5. Optional: mergiraf pre-stage (if on PATH and not --no-mergiraf)
  │     └─ mergiraf.rs: run `mergiraf --ast <file>` per file
  │        └─ If file has no remaining markers → fully resolved, skip LLM
  │        └─ If file still has markers → re-parse remaining hunks
  │
  ├─ 6. Classify remaining hunks (classify.rs)
  │     ├─ Trivial: ours == theirs (identical edit) → auto-resolve
  │     ├─ Trivial: base == ours (one side unchanged) → take theirs
  │     ├─ Trivial: base == theirs (one side unchanged) → take ours
  │     ├─ Trivial: one side is empty (add/delete conflict) → take non-empty
  │     └─ Complex: both sides diverge → send to provider
  │
  ├─ 7. Privacy filter (privacy/mod.rs)
  │     └─ Apply .gcmignore matcher to filter files
  │     └─ Apply --secret-scan to hunk text before provider egress
  │
  ├─ 8. Provider resolution (provider/mod.rs: resolve_hunks)
  │     └─ For each complex hunk: send base + ours + theirs + style context
  │     └─ Provider returns Resolution { replacement: String }
  │     └─ strip_think + defensive parse (reuse existing)
  │     └─ Context window management: if total complex hunk text exceeds
  │        the provider's diff_budget, batch hunks into multiple calls
  │
  ├─ 9. Validation gate (validate.rs)
  │     ├─ Syntax-safe default: no conflict markers in output
  │     ├─ Optional validate_cmd: run user-configured command
  │     └─ On failure: exactly one bounded LLM retry, then escalate
  │
  ├─ 10. Per-file preview loop (ui.rs: confirm per file)
  │     ├─ Show resolved file diff/preview
  │     ├─ [Y/n/e] prompt (e opens $EDITOR on the resolved file)
  │     ├─ Y → write resolved file to working tree
  │     ├─ n → skip (leave conflicted)
  │     └─ NEVER git add, NEVER git merge --continue
  │
  └─ 11. Report (report.rs → output.rs)
        └─ --json: emit ResolveReport envelope on stdout
        └─ Human prose on stderr
```

### Concrete types

```rust
// src/resolve/markers.rs

/// One conflict hunk parsed from zdiff3 markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// 1-based start line of the conflict in the working-tree file.
    pub start_line: usize,
    /// 1-based end line (inclusive).
    pub end_line: usize,
    /// The common ancestor text (zdiff3 `|||||||` block). `None` if absent
    /// (diff3 without base, though zdiff3 always includes it).
    pub base: Option<String>,
    /// The "ours" / current branch text (`<<<<<<<` block).
    pub ours: String,
    /// The "theirs" / incoming branch text (`>>>>>>>` block).
    pub theirs: String,
}

/// A conflicted file parsed into its hunks.
#[derive(Debug, Clone)]
pub struct ConflictFile {
    pub path: String,
    pub hunks: Vec<Hunk>,
    /// Lines outside any conflict hunk (context, carried verbatim).
    pub context_lines: Vec<String>,
}

/// The resolution strategy applied to a hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkResolution {
    /// Deterministic: identical sides, one side unchanged, or one side empty.
    Auto { text: String, reason: AutoReason },
    /// LLM-resolved: both sides diverge, provider returned a replacement.
    Llm { text: String },
    /// Escalated: LLM output failed validation after retry, left conflicted.
    Escalated,
    /// Skipped by user in the preview loop.
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoReason {
    IdenticalSides,
    OursUnchanged,
    TheirsUnchanged,
    OneSideEmpty,
}

/// Result of resolving one file.
#[derive(Debug, Clone)]
pub struct FileResolution {
    pub path: String,
    pub hunks_total: usize,
    pub hunks_auto: usize,
    pub hunks_llm: usize,
    pub hunks_escalated: usize,
    pub action: FileAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Accepted,
    Skipped,
    Edited,
    Escalated,
    DryRun,
}
```

```rust
// src/resolve/classify.rs

/// Classify a hunk for resolution strategy.
pub fn classify(hunk: &Hunk) -> HunkClass {
    if hunk.ours == hunk.theirs {
        return HunkClass::Trivial(AutoReason::IdenticalSides);
    }
    if let Some(base) = &hunk.base {
        if base == &hunk.ours {
            return HunkClass::Trivial(AutoReason::OursUnchanged);
        }
        if base == &hunk.theirs {
            return HunkClass::Trivial(AutoReason::TheirsUnchanged);
        }
    }
    if hunk.ours.is_empty() || hunk.theirs.is_empty() {
        return HunkClass::Trivial(AutoReason::OneSideEmpty);
    }
    HunkClass::Complex
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkClass {
    Trivial(AutoReason),
    Complex,
}
```

```rust
// src/resolve/mergiraf.rs

/// Detect whether `mergiraf` is on PATH.
pub fn is_available() -> bool { /* which mergiraf */ }

/// Run `mergiraf` on a single conflicted file. Returns true if the file
/// has no remaining conflict markers after the run (fully resolved).
pub fn try_resolve(repo: &Repo, path: &str) -> Result<bool, GcmError> { /* ... */ }
```

```rust
// src/resolve/validate.rs

/// Validate a proposed resolution. Returns Ok(()) if it passes.
pub fn validate(
    resolved_text: &str,
    validate_cmd: Option<&str>,
    repo: &Repo,
    path: &str,
) -> Result<(), ValidationError> {
    // 1. Syntax-safe default: no conflict markers in output
    if has_conflict_markers(resolved_text) {
        return Err(ValidationError::ConflictMarkers);
    }
    // 2. Optional user-configured command
    if let Some(cmd) = validate_cmd {
        // Write resolved text to a temp file, run cmd, check exit code
        run_validate_cmd(cmd, resolved_text, repo, path)?;
    }
    Ok(())
}

#[derive(Debug)]
pub enum ValidationError {
    ConflictMarkers,
    ValidateCmdFailed { stdout: String, stderr: String },
}
```

```rust
// src/resolve/report.rs

/// The --json envelope for `gcm resolve`.
#[derive(Debug, Serialize)]
pub struct ResolveReport {
    pub v: i32,
    pub status: ResolveStatus,
    pub files: Vec<FileReport>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResolveStatus {
    Resolved,   // all files accepted
    Partial,    // some accepted, some skipped/escalated
    Noop,       // no conflicts found
    Error,
}

#[derive(Debug, Serialize)]
pub struct FileReport {
    pub path: String,
    pub hunks_total: usize,
    pub hunks_auto: usize,
    pub hunks_llm: usize,
    pub hunks_escalated: usize,
    pub action: FileAction,
}
```

```rust
// src/provider/mod.rs — new trait method

pub trait Provider {
    // ... existing methods ...

    /// Resolve conflict hunks that could not be resolved deterministically.
    /// Sends base/ours/theirs at function granularity with a 3-way prompt.
    /// Returns the resolved text for each hunk, in order.
    fn resolve_hunks(&self, ctx: &ResolveContext) -> Result<Vec<HunkResolution>, ProviderError>;
}

/// Context for conflict resolution (analogous to GroupingContext).
#[derive(Debug, Clone)]
pub struct ResolveContext {
    pub path: String,
    pub hunks: Vec<Hunk>,
    /// A short style excerpt from the file (context lines around the conflict).
    pub style_context: String,
    /// Temperature override from [conflict] config (default 0.1).
    pub temperature: f64,
}
```

```rust
// src/config.rs — new [conflict] section

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    // ... existing fields ...
    #[serde(default)]
    pub conflict: ConflictConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ConflictConfig {
    /// LLM temperature for resolution (default 0.1).
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    /// Optional validation command (e.g. `cargo check`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate_cmd: Option<String>,
    /// Glob patterns for paths that always require manual review.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sensitive_paths: Vec<String>,
    /// Auto-resolution policy: which hunk classes to auto-resolve.
    #[serde(default = "default_auto_policy")]
    pub auto_policy: AutoPolicy,
    /// Whether to use mergiraf if on PATH (default true).
    #[serde(default = "default_mergiraf")]
    pub mergiraf: bool,
}

fn default_temperature() -> f64 { 0.1 }
fn default_auto_policy() -> AutoPolicy { AutoPolicy::Trivial }
fn default_mergiraf() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AutoPolicy {
    /// Auto-resolve only trivial hunks (identical, one-side-unchanged, one-side-empty).
    Trivial,
    /// Also auto-resolve moderate hunks (simple structural merges).
    Moderate,
    /// Send everything to the LLM (no auto-resolution).
    Complex,
}
```

```rust
// src/cli.rs — new subcommand

#[derive(clap::Subcommand, Debug)]
pub enum Commands {
    Config,
    Status,
    Provider,
    /// Resolve in-progress merge/rebase/cherry-pick conflicts using the LLM provider.
    Resolve {
        /// Conflict resolution temperature (overrides [conflict].temperature).
        #[arg(long)]
        conflict_temperature: Option<f64>,

        /// Validation command for resolved files (overrides [conflict].validate_cmd).
        #[arg(long)]
        conflict_validate_cmd: Option<String>,

        /// Auto-resolution policy (overrides [conflict].auto_policy).
        #[arg(long)]
        conflict_auto_policy: Option<AutoPolicy>,

        /// Glob patterns for paths that require manual review.
        #[arg(long)]
        conflict_sensitive_paths: Option<Vec<String>>,

        /// Skip the optional mergiraf pre-resolution stage.
        #[arg(long)]
        no_mergiraf: bool,
    },
}
```

```rust
// src/git.rs — new methods

impl Repo {
    /// Enumerate unmerged (conflicted) file paths via `git diff --name-only --diff-filter=U`.
    /// NUL-delimited so unicode/space paths survive.
    pub fn unmerged_files(&self) -> Result<Vec<String>, GcmError> { /* ... */ }

    /// Re-checkout conflicted files with zdiff3 conflict markers.
    /// `git checkout --conflict=zdiff3 -- <paths>`
    pub fn checkout_conflict_zdiff3(&self, paths: &[&str]) -> Result<(), GcmError> { /* ... */ }

    /// Read a file's content from the working tree.
    pub fn read_file(&self, path: &str) -> Result<String, GcmError> { /* ... */ }

    /// Write content to a file in the working tree.
    pub fn write_file(&self, path: &str, content: &str) -> Result<(), GcmError> { /* ... */ }
}
```

```rust
// src/error.rs — new variants

#[derive(Debug)]
pub enum GcmError {
    // ... existing variants ...

    /// `gcm resolve` was called but no merge/rebase/cherry-pick is in progress.
    NoConflictInProgress,

    /// `gcm resolve` was called but no unmerged files were found.
    NoConflicts,

    /// A conflict resolution failed validation and was left conflicted.
    ResolutionEscalated { path: String, reason: String },
}
```

### Provider integration design

The new `resolve_hunks` trait method follows the same pattern as `generate_plan`:
- Each backend implements it using its native structured-output mechanism (OpenAI `response_format`, Gemini `responseSchema`, Anthropic forced tool-use, Ollama `format`).
- The resolution schema is a simple array of `{ hunk_index: usize, replacement: String }`.
- The prompt includes labeled `base`, `ours`, `theirs` at the smallest syntactic unit, a short file excerpt for style, and anti-hallucination rules ("combine both branches' intent; only use symbols that already exist; no new modules/deps").
- `strip_think` and `parse_defensive` are reused on the response.
- Retry/backoff via `http::post_json` is reused.

The default trait implementation can be a blanket implementation that wraps `generate_message` with a custom prompt, but since each backend needs its own structured-output shape, it will likely be implemented per-backend. The design phase defers this to the implementation phase — the key decision is that `resolve_hunks` takes a `ResolveContext` and returns `Vec<HunkResolution>`.

### Context window management (review S1)

If the total size of complex hunks for a single file exceeds the provider's `diff_budget`, the orchestrator batches hunks into multiple provider calls:

1. Estimate the token cost of all complex hunks (base + ours + theirs + style context).
2. If the total exceeds `provider.diff_budget().max_input_tokens * 0.75` (75% safety margin), split into batches.
3. Each batch is a separate `resolve_hunks` call; results are merged in hunk order.
4. If a file has more complex hunks than can fit in `max_batch_size` (default 10), the file is escalated with a clear message ("too many complex hunks for automatic resolution; resolve manually").

### Binary file detection (review S2)

Before marker parsing, the pipeline detects binary conflicted files and skips them:

- `git diff --numstat --diff-filter=U` shows `-	-	<path>` for binary files.
- Binary files are listed with a warning ("binary file, resolve manually") and left conflicted.
- They appear in the `ResolveReport` with `action: "escalated"` and `hunks_total: 0`.

### `validate_cmd` execution mechanics (review S3)

When `conflict.validate_cmd` is set:

1. The resolved file content is written to a temporary file (via `tempfile::NamedTempFile`).
2. The `validate_cmd` string is run via `sh -c "$cmd $FILE"` where `$FILE` is the temp file path, rooted at the repo root (`cwd = repo.root()`).
3. Exit code 0 → pass; non-zero → `ValidationError::ValidateCmdFailed` with stdout/stderr.
4. The temp file is removed on every exit path (Drop).
5. Example: `validate_cmd = "cargo check"` runs `cargo check $FILE` — though for Rust this is imperfect (`cargo check` checks the whole crate, not a single file). A more realistic usage is `validate_cmd = "rustfmt --check"` or `validate_cmd = "node --check"`.

### Editor integration flow (review S4)

When the user answers `e` in the per-file preview loop:

1. The resolved file content is written to a temp file (via `tempfile::NamedTempFile`).
2. `$EDITOR` (default `vim`) is launched on the temp file via `sh -c "$EDITOR \"$1\""` — same pattern as `ui::edit_in_editor`.
3. `gcm` blocks until the editor process exits.
4. The edited content is read back and replaces the proposed resolution.
5. The edited content still passes through the validation gate before being written to the working tree.
6. If the edited content fails validation, the user is warned and the file is left conflicted (no retry for human-edited content — the human is responsible for their edits).

## 4. Public API surface

### CLI

```
gcm resolve [--conflict-temperature <f>] [--conflict-validate-cmd <cmd>]
            [--conflict-auto-policy <trivial|moderate|complex>]
            [--conflict-sensitive-paths <glob>...]
            [--no-mergiraf]
            [--dry-run] [--json] [--yes] [--provider <id>] [--model <id>]
            [--secret-scan <off|redact|abort>]
```

Global flags (`--dry-run`, `--json`, `--yes`, `--provider`, `--model`, `--secret-scan`) are already on `Cli` and available to all subcommands.

### Config (TOML)

```toml
[conflict]
temperature = 0.1
validate_cmd = "cargo check"
sensitive_paths = ["secrets/**", "*.env"]
auto_policy = "trivial"
mergiraf = true
```

### JSON envelope (`--json`)

```json
{
  "v": 1,
  "status": "resolved",
  "files": [
    {
      "path": "src/lib.rs",
      "hunks_total": 3,
      "hunks_auto": 1,
      "hunks_llm": 1,
      "hunks_escalated": 1,
      "action": "accepted"
    }
  ]
}
```

Human-oriented prose goes to stderr; stdout contains a single JSON object (lesson `clo-493 §L1`).

## 5. Assumptions

| # | Assumption | Confidence | Verification |
|---|---|---|---|
| A1 | `git checkout --conflict=zdiff3` re-materializes conflict markers for all unmerged paths without losing the merge state. | high | Integration test in a real temp repo with a conflicted merge. |
| A2 | `mergiraf` is an optional external tool; its absence degrades gracefully to the pure-LLM path. | high | Acceptance test with `mergiraf` absent (uninstalled). |
| A3 | `mergiraf` exits 0 when it fully resolves a file and non-zero when hunks remain; residual markers are detected by re-parsing. | medium | Design-phase debt: verify exact `mergiraf` CLI semantics; documented in discovery debt. |
| A4 | The existing `Provider` trait can accommodate a new `resolve_hunks` method without breaking existing backends. | high | Each backend adds its own structured-output shape; existing methods unchanged. |
| A5 | `zdiff3` markers always include a `|||||||` base block (unlike plain `diff3` which omits it). | high | Verified against git docs and the existing `is_merging()` integration test. |
| A6 | The `--json` stdout discipline (stderr for human prose, stdout for JSON only) applies to `gcm resolve` the same as the commit flow. | high | Lesson `clo-493 §L1`; enforced by acceptance test. |
| A7 | `--secret-scan` and `.gcmignore` filtering apply to hunk text sent to the provider the same way they apply to diff text. | high | Lesson `clo-514 §L1/L2`; reuse existing `Privacy` API. |
| A8 | Provider HTTP timeouts for the resolve path should not preempt the outer resolve-command timeout. | high | Lesson `timeout-layering §L1`; reuse existing `http::post_json` retry/backoff. |
| A9 | The default syntax-safe validation (no conflict markers in output) is sufficient to catch most LLM hallucinations. | medium | Acceptance test with deliberately malformed LLM output. |
| A10 | A temperature of 0.1 is low enough for reproducible conflict resolution and high enough to avoid degenerate outputs. | medium | Manual testing across providers during implementation. |

## 6. Test plan

### Unit tests

| Test | Module | Description |
|---|---|---|
| `parse_zdiff3_single_hunk` | `markers.rs` | Parse a file with one conflict hunk into `ConflictFile { hunks: [Hunk] }`. |
| `parse_zdiff3_multiple_hunks` | `markers.rs` | Parse a file with N conflict hunks; verify start/end lines, base/ours/theirs. |
| `parse_zdiff3_no_base` | `markers.rs` | Handle a hunk without `|||||||` base block gracefully. |
| `parse_no_conflicts` | `markers.rs` | A file with no markers returns an empty `hunks` vec. |
| `classify_identical_sides` | `classify.rs` | `ours == theirs` → `Trivial(IdenticalSides)`. |
| `classify_ours_unchanged` | `classify.rs` | `base == ours` → `Trivial(OursUnchanged)`. |
| `classify_theirs_unchanged` | `classify.rs` | `base == theirs` → `Trivial(TheirsUnchanged)`. |
| `classify_one_side_empty` | `classify.rs` | `ours` or `theirs` is empty → `Trivial(OneSideEmpty)`. |
| `classify_complex` | `classify.rs` | Both sides diverge → `Complex`. |
| `validate_rejects_markers` | `validate.rs` | Resolution containing `<<<<<<<` → `ValidationError::ConflictMarkers`. |
| `validate_passes_clean` | `validate.rs` | Clean resolution → `Ok(())`. |
| `validate_cmd_passes` | `validate.rs` | `validate_cmd` exits 0 → `Ok(())`. |
| `validate_cmd_fails` | `validate.rs` | `validate_cmd` exits non-zero → `Err(ValidateCmdFailed)`. |
| `mergiraf_not_found` | `mergiraf.rs` | `is_available()` returns false when `mergiraf` not on `PATH`. |
| `resolve_report_serializes` | `report.rs` | `ResolveReport` serializes to the documented JSON shape. |
| `cli_resolve_parses` | `cli.rs` | `gcm resolve --dry-run --json` parses to `Commands::Resolve`. |

### Integration tests

| Test | Description |
|---|---|
| `resolve_trivial_conflict` | Two branches both add the same line → auto-resolved without LLM call. |
| `resolve_one_side_unchanged` | `base == ours` → takes `theirs` without LLM call. |
| `resolve_complex_conflict` | Both branches diverge → provider called, resolution validated, file written on accept. |
| `resolve_with_mergiraf_absent` | `mergiraf` not on `PATH` → pure-LLM path works. |
| `resolve_dry_run_no_write` | `--dry-run` → preview only, no files written. |
| `resolve_json_envelope` | `--json` → stdout is a single JSON object, stderr has human prose. |
| `resolve_yes_non_interactive` | `--yes` → all proposed resolutions accepted without prompt. |
| `resolve_skip_leaves_conflicted` | User answers `n` → file left conflicted, not written. |
| `resolve_edit_opens_editor` | User answers `e` → `$EDITOR` opens on the resolved file. |
| `resolve_validation_retry_then_escalate` | First LLM output fails validation, retry also fails → file escalated (left conflicted). |
| `resolve_no_merge_in_progress` | No merge/rebase/cherry-pick → `GcmError::NoConflictInProgress`. |
| `resolve_no_unmerged_files` | Merge in progress but all conflicts resolved → `GcmError::NoConflicts`. |
| `resolve_binary_file_skipped` | Binary conflicted file → skipped with warning, left conflicted, reported as escalated. |
| `resolve_secret_scan_aborts` | `--secret-scan=abort` with a credential in a hunk → `GcmError::SecretDetected`. |
| `resolve_gcmignore_excludes` | File matching `.gcmignore` → excluded from resolution. |

### Manual tests

| Test | Description |
|---|---|
| Multi-file conflict | 3+ conflicted files, mixed trivial/complex hunks; verify per-file preview loop. |
| `mergiraf` present | Install `mergiraf`, run `gcm resolve` on a structurally resolvable conflict; verify it resolves without LLM. |
| `validate_cmd` with `cargo check` | Set `conflict.validate_cmd = "cargo check"`; verify a resolution that breaks compilation triggers retry then escalation. |
| Provider matrix | Run `gcm resolve --provider {groq,google,openai,anthropic,ollama}` on the same conflict; verify all backends produce a valid resolution. |
| `sensitive_paths` | Set `conflict.sensitive_paths = ["secrets/**"]`; verify matching files are forced to manual review. |

## 7. Migration / rollout

- **No migration needed.** `gcm resolve` is a new subcommand; existing `gcm` behavior is unchanged.
- The `[conflict]` config section defaults to conservative values and is `#[serde(default)]`, so existing `config.toml` files without it parse unchanged.
- The `Provider` trait gains a new required method `resolve_hunks`. All five backends implement it in the same PR (Q1 resolved: a default impl would mask missing implementations).
- No new Cargo dependencies are introduced (no `mergiraf` crate, no tree-sitter; `mergiraf` is an external binary on `PATH`).

## 8. Open questions

| # | Question | Resolution |
|---|---|---|
| Q1 | Should `resolve_hunks` be a required `Provider` trait method or have a default impl? | **Resolved:** Required method, all five backends implement it in the same PR. A default impl would mask missing implementations. |
| Q2 | Should the resolution schema be per-hunk (array of replacements) or per-file (full file text)? | **Resolved:** Per-hunk. Smaller token budget, easier validation, aligns with the layered pipeline (only complex hunks are sent). |
| Q3 | Should `gcm resolve` support `--rebase` vs `--merge` mode detection? | **Resolved:** Auto-detect via `MERGE_HEAD` (merge), `REBASE_HEAD` (rebase), or `CHERRY_PICK_HEAD` (cherry-pick). No flag needed; the marker-recheckout step is the same for all three. |
| Q4 | Should the resolved file preserve the original file's line endings and encoding? | **Resolved:** Yes. Read and write as bytes; the marker parser operates on bytes with lossy UTF-8 for display only. |
| Q5 | Should `--yes` also auto-accept escalated files (validation failed)? | **Resolved:** No. Escalated files are always left conflicted regardless of `--yes`. Safety invariant: unvalidated output never lands in the working tree. |

---

### Lessons applied

| Lesson | How applied |
|---|---|
| `clo-493 §L1` | `--json` stdout discipline: human prose to stderr, JSON envelope only on stdout. Test: `resolve_json_envelope`. |
| `clo-514 §L1/L2` | `--secret-scan` and `.gcmignore` apply to hunk text before provider egress. Test: `resolve_secret_scan_aborts`, `resolve_gcmignore_excludes`. |
| `timeout-layering §L1` | Provider HTTP calls for resolve reuse existing `http::post_json` with its retry/backoff; no independent shorter timeout preempting the outer resolve command. |
| `pr-review-failures §L7` | Noted: merge conflicts are the first PR blocker. `gcm resolve` addresses local conflict resolution; Phase 2 remote orchestration will use this core. |