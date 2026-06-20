# Spec: Semantic grouping - plan via structured output, commit the first group

**Created**: 2026-06-20
**Task**: [CLO-487](https://linear.app/cloud-ai/issue/CLO-487) (slice S2)
**Estimated scope**: M (~7 files touched, ~6 sub-tasks)
**Extends**: CLO-486 single-commit tracer ([spec](2026-06-19-clo-486-single-commit-tracer.md)); architecture locked by [ADR-001](../adrs/001-foundational-architecture-decisions.md) Decisions 1 (grouping = central LLM contract), 5 (Groq `gpt-oss-120b` strict json_schema), 6 (regenerate-per-group).
**Covers FR**: 1, 2 (partial), 3, 7, 15, 16, 19, 23 (basic), 24 (partial), 31 (full), 33

---

## 1. Problem Statement

The merged tracer (CLO-486) sends the whole working-tree diff to Groq, gets back **one** plain-text conventional-commit message, and commits **all** changes as a single commit (`src/main.rs:69-85` `commit_flow` -> `repo.stage_all()` -> `repo.commit_signed()`). That is the headline behavior gap the rewrite exists to close: real working trees mix unrelated changes that belong in separate commits.

CLO-487 makes gcm split the change set into logical groups and commit only the **first** group, leaving the rest for the next run. The split comes from the LLM as a **typed JSON plan**, not a scraped text blob - that typed-deserialization contract (ADR-001 Decision 1) is precisely what the bash tool got wrong (`docs/tmp/git-commit-ai.sh:352` scrapes JSON with `sed -> perl -> jq`). gcm instead requests Groq structured outputs (`response_format: json_schema, strict: true` on `gpt-oss-120b`, ADR-001 Decision 5) and deserializes into typed `Plan { groups: [Group { files, summary, commit_message }] }`.

There is **no cache** in this slice (that is CLO-491). Multi-group progression is achieved purely by **re-analysis**: each run gathers the *current* change set, asks for a fresh plan, commits group 1, and exits. The next run sees a smaller change set (group 1's files are now committed) and produces the next group. This already delivers "commit groups one per run" without any persisted plan.

Three concrete correctness traps this slice must not fall into:

1. **Path agreement.** The file list the model groups by, and the paths gcm stages, must be byte-identical to git's real paths. The tracer already passes `-c core.quotePath=false` (`src/git.rs:45`) and NUL-splits untracked files (`src/git.rs:123-140`), but it has no porcelain *status* parse. Grouping needs the full changed-file list (modified, added, deleted, **renamed**) with new-vs-orig paths resolved. A filename literally containing ` -> ` (or a space, newline, or unicode) must survive - which means parsing `git status --porcelain=v1 -z` (NUL-delimited), never the human `R  old -> new` text form.

2. **Diff truncation that preserves structure.** The tracer truncates the *whole assembled body* at a byte cap with a tail-chop (`src/diff.rs:91-100` `body.truncate(end)`), which can sever the final file's diff mid-hunk and confuse the grouping model. CLO-487 must truncate **per file** and leave a `[diff omitted: N bytes]` placeholder for that file, so every file the model groups by still appears with an intact (if elided) entry.

3. **Safe degradation.** If structured output is unavailable, the JSON fails to parse, or the plan references files that are not in the real change set, gcm must fall back to the proven CLO-486 single-commit path rather than stage a wrong subset. The fallback must be explicit and announced, never silent.

**Who is affected**: every gcm user with more than one logical change in flight (the common case). **What triggers it**: running `gcm` in a dirty repo. **Why it matters**: clean, atomic, reviewable commit history is the product's reason to exist.

---

## 2. Acceptance Criteria

Mapped from the Linear task's acceptance criteria, made testable:

- [ ] **AC-1 (split + commit group 1)**: In a repo with a mixed change set (e.g. a source change + an unrelated docs change), gcm requests a plan, displays >=1 group, and commits **only group 1's files** with group 1's own conventional-commit message. Files in later groups remain uncommitted and unstaged.
- [ ] **AC-2 (multi-group progression, no cache)**: Re-running gcm on the remainder produces a new plan over the now-smaller change set and commits the next logical group. Two runs on a two-group change set yield two commits, each scoped to its group's files, with the working tree clean at the end.
- [ ] **AC-3 (typed structured-output plan)**: The plan is requested via the provider's structured-output mode (`response_format` json_schema, `strict: true`) and deserialized into typed `Plan`/`Group` - no regex/`sed`/`perl` scraping. A malformed/empty model response does not panic.
- [ ] **AC-4 (NUL-safe status, renames/deletes, `->`-in-name)**: A change set containing a renamed file, a deleted file, and a file whose name contains a space and the literal substring ` -> ` (and a unicode name) is parsed correctly from `git status --porcelain=v1 -z`; each such file is attributable to exactly one group and stages by its real path. A renamed file in group 1 stages **both** its new and original path so the resulting commit completes the rename (does not leave a stray copy of the old path). This case does **not** trip into the single-commit fallback.
- [ ] **AC-5 (per-file diff placeholders)**: When an individual file's diff exceeds the per-file cap, the prompt contains that file's header plus a `[diff omitted: N bytes]` placeholder (N = the omitted byte count), not a tail-chopped body. No file is dropped from the prompt by truncation.
- [ ] **AC-6 (basic plan validation)**: If the plan references any file not present in the real change set, or group 1 is empty, or group 1 has no usable commit message, gcm rejects the plan and falls back to the single-commit (CLO-486) path with a printed reason.
- [ ] **AC-7 (parse-failure fallback)**: If the structured-output call fails (HTTP/timeout/empty) or the JSON cannot be deserialized into `Plan`, gcm falls back to the single-commit path; the index is left in its pre-run state on any error exit (FR-47 transaction preserved).
- [ ] **AC-8 (`--dry-run` previews plan, commits nothing)**: `gcm --dry-run` prints the grouped plan (groups, their files, group 1's message) and exits 0 with **zero** staging and **zero** commits; `git status` is byte-identical before and after.
- [ ] **AC-9 (`--all` bypasses grouping)**: `gcm --all` skips the grouping call entirely and commits all changes as one signed commit via the CLO-486 path (forward-compat flag from `src/cli.rs:24-27` now wired).
- [ ] **AC-10 (signed commit, index transaction intact)**: Group 1 is committed with `git commit -S`; abort at the prompt and any post-snapshot failure restore the index to its pre-run tree (reuse `snapshot_index`/`restore_index`, `src/git.rs:143-151`).
- [ ] **AC-12 (unresolved-merge abort)**: Running gcm in a repo with unmerged entries (a conflicted `git merge`, status `UU`/`AA`/etc.) exits 1 with an actionable message and stages/commits nothing - it must **not** clear the index and bake `<<<<<<<` markers into a commit (verified hazard). A *clean* merge-in-progress (`MERGE_HEAD`, no conflicts) bypasses grouping and completes via a single merge commit.
- [ ] **AC-13 (glob/ARG_MAX-safe staging)**: A change set containing a file whose name contains a `*` (or `?`) stages **only** that literal file, never glob-siblings (`GIT_LITERAL_PATHSPECS=1`); staging uses `--pathspec-from-file=- --pathspec-file-nul` via stdin so a very large group does not overflow `ARG_MAX`.
- [ ] **AC-11 (quality gates)**: `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test` are clean; `acceptance.sh` passes including the new grouping cases driven by a mock-Groq server.

**Verification method**: unit tests (`cargo test`) for pure logic (status parsing, plan validation, per-file truncation); `acceptance.sh` integration cases driven by a mock-Groq HTTP server returning canned JSON plans (same mechanism CLO-486 established) for AC-1/2/4/6/7/8/9; manual TTY check for the interactive `[Y/n/e]` edit path (carried from CLO-486, may remain SKIP).

---

## 3. Constraints

**Must**:
- Request the plan via Groq structured outputs: `response_format = { type: "json_schema", json_schema: { name, schema, strict: true } }` on the default `openai/gpt-oss-120b` (ADR-001 Decision 5). Keep `include_reasoning: false` and the `strip_think` backstop (`src/groq.rs:132-168`).
- Deserialize into typed `Plan { groups: Vec<Group> }`, `Group { files: Vec<String>, summary: String, commit_message: Option<String> }` via serde. No text scraping of the JSON.
- Parse the changed-file list from `git status --porcelain=v1 -uall -z` with `-c core.quotePath=false`. **`-uall` (`--untracked-files=all`) is required (review-2 point #1, verified):** without it, git collapses a fully-untracked directory to a single `?? dir/` entry, but the diff gatherer (`git ls-files --others`, `src/git.rs:123`) lists the individual nested files - so the model would see `dir/` in the file list and `dir/file.txt` in the diff, then group by `dir/file.txt`, which is absent from the change set and trips `validate_basic` into a spurious fallback. `-uall` expands directories to individual files so the list and the diff paths are byte-identical. NUL-delimited; never split on ` -> `. For rename/copy (`R`/`C`) entries, use the **new** path in the change set (and keep the orig path for staging, per the rename-staging Must above). **Pin the new-vs-orig NUL field order with a test against real `git mv` output and document it in a code comment** (do not guess from memory). The two AI reviewers gave *contradictory* orderings for this format (Gemini's own text stated both new-first and orig-first; Ollama left it unspecified) - so the empirical temp-repo `git mv` test is the single source of truth, not any cited example.
- **Abort on an unresolved merge / unmerged index (review-2 point #2, verified - this can bake conflict markers into a commit).** Before any clearing or staging, detect unmerged entries in the porcelain status: any entry whose status pair contains `U`, or the codes `DD`/`AA` (i.e. `DD`, `AU`, `UD`, `UA`, `DU`, `AA`, `UU`). If any exist, **abort with exit 1** and an actionable message (e.g. `repository has unresolved merge conflicts; resolve them and stage your resolution before running gcm`). Do **not** fall back to single-commit here: the single-commit path (`git add -A`) stages the same marker-laden working tree, so it bakes `<<<<<<<` markers in too (verified: after `read-tree HEAD` + `git add -A`, the staged blob still contained the conflict markers). Aborting is the only safe response.
- Stage **only** group 1's files so the commit is exactly group 1. Implemented as: (a) snapshot the index up front (`write-tree`, the FR-47 restore point); (b) **clear staging to the committed state** - `git read-tree HEAD` when HEAD resolves, `git read-tree --empty` on an unborn branch (no HEAD - `read-tree HEAD` would fail; review-1 finding #2); (c) stage the group-1 pathspec (mechanism below). Clearing to HEAD (not emptying) leaves tracked group-2 files at their HEAD version in the index so they are **not** recorded as deletions in the commit; on an unborn branch there are no tracked files to lose, so `--empty` is safe.
- **Stage via NUL stdin with literal pathspecs (review-2 points #3+#4, both verified).** Do not pass paths as `git add -- <path> <path> ...` argv: (i) git's *internal* pathspec engine globs `*`/`?`/`[` even though `Command::new` bypasses the shell - verified: `git add -- 'a*.txt'` staged `ab.txt`+`ac.txt`, a different-files leak; (ii) thousands of changed files would overflow `ARG_MAX` (`E2BIG`). Instead run `git add -A --pathspec-from-file=- --pathspec-file-nul` (git >= 2.25; local git is 2.54) and write the group-1 paths **NUL-separated to the child's stdin**, with **`GIT_LITERAL_PATHSPECS=1` set in the command's environment** so no path is glob-expanded (verified: `GIT_LITERAL_PATHSPECS=1` staged only the literal `a*.txt`). This reuses the NUL bytes we already parsed from `-z` status and handles every special-character path uniformly.
- **Renames must stage both paths (review-1 finding #1, Critical).** A rename (`R`) is an unstaged deletion of the original path plus an addition of the new path. The plan groups by the **new** path; when that new path is in group 1, the NUL-stdin pathspec must include **both** the new and the original path, or the commit adds the new file without deleting the old one and splits the rename across two commits. Build the group-1 pathspec from the typed `ChangedFile` entries (not raw strings), expanding any `R`/`C` entry to both paths.
- Truncate diffs **per file** with a `[diff omitted: N bytes]` placeholder (N = omitted byte count); keep every changed file's header in the prompt. For tracked diffs this means splitting `git diff` output on `diff --git ` boundaries and capping each file's section individually (reuse the section-splitting in `elide_binary_diff`, `src/diff.rs:135-147`); for untracked content reuse the existing `PER_FILE_BYTES` cap. Per-file caps apply **during assembly**; the existing `MAX_TOTAL_BYTES` whole-body cap remains as an additional final safeguard on the assembled prompt (it does not replace the per-file caps). Preserve the existing binary-elision and untracked-content caps (FR-57).
- On any grouping failure (structured-output error, deserialize error, basic-validation failure), fall back to the CLO-486 single-commit path (`generate_commit_message` + `stage_all` + `commit_signed`) with a printed reason. Fallback must never be silent.
- Preserve the FR-47 index transaction: capture `snapshot_index()` before any staging; restore on abort or any post-snapshot error.
- Preserve the non-TTY guard (`ui::needs_terminal_but_absent`, `src/main.rs:42-44`) and exit-code contract (0 ok/abort, 1 runtime error, 2 usage).

**Must-not**:
- Must not introduce a plan cache, per-repo state file, or content fingerprint - that is CLO-491 (FR-25-30). Each run re-analyzes from scratch.
- Must not implement full bijective plan validation (every real file covered exactly once, message-placement-per-contract) - that is CLO-492 (FR-23 full). This slice does **basic** validation only (no unknown files; group 1 present + has a message).
- Must not add a provider trait or any non-Groq backend - that is CLO-489.
- Must not add an async runtime; keep the blocking `ureq` client (ADR-001 Decision 3).
- Must not split paths on whitespace or ` -> `, or read paths from non-`-z` porcelain.
- Must not generate messages for groups other than group 1 in this slice (regenerate-per-group's message-only subsequent calls are CLO-491; here, re-analysis regenerates everything).

**Prefer**:
- Request `commit_message` only for `groups[0]` (null/empty for later groups), matching the bash contract (`docs/tmp/git-commit-ai.sh:318`) - cheaper and we re-analyze anyway. The schema still types `commit_message` as nullable on every group.
- Prefer fewer groups (1-3) in the system prompt unless changes are truly unrelated (bash rule, `docs/tmp/git-commit-ai.sh:317`).
- Keep new logic in a dedicated `src/plan.rs` module (types + schema + basic validation); keep status parsing in `src/git.rs`; keep the structured-output call in `src/groq.rs`.
- Reuse the existing mock-Groq acceptance harness rather than introduce a new test mechanism.
- **Clean merge-in-progress (`.git/MERGE_HEAD` present, no unmerged `U` entries): bypass grouping and use the single-commit path** so a normal `git commit` finalizes the merge as a proper two-parent merge commit. Splitting a merge across grouped commits would drop the second parent. (The dangerous *conflicted* merge is already a hard Must-abort above; this Prefer covers only the already-resolved merge ready to commit.)
- **Disclose the staging model (review-2 point #5).** gcm groups at file granularity over the whole working tree (`diff HEAD`), so it overrides any manual hunk-level (`git add -p`) staging: group-1 files are staged whole, group-2 files are left unstaged (their changes stay in the working tree, never lost). Note this in `--help`/README so users do not expect partial-staging to be preserved. Not a bug - an inherent consequence of file-level grouping (same as the bash tool's `git reset HEAD` + `git add <files>`).

**Escalate when**:
- Groq's OpenAI-compatible endpoint rejects `strict: true` json_schema for `gpt-oss-120b` at implementation time (capability drift vs ADR-001) - stop and surface, do not silently downgrade to non-strict.
- The new-vs-orig NUL field order for renames cannot be determined from real git output (it can; this is a backstop).
- Implementing the spec would require touching the provider abstraction, cache, or full-validation surfaces owned by other slices (scope creep).

---

## 3b. Provider Contract (structured-output schema + grouping prompt)

*Added from review P1 items 1-2: the structured-output contract must be concrete in the spec.*

**JSON Schema** sent as `response_format = { "type": "json_schema", "json_schema": { "name": "commit_plan", "strict": true, "schema": <below> } }`. Groq strict mode (OpenAI-compatible) requires every property listed in `required` and `additionalProperties: false` on every object; `commit_message` is nullable so later groups can carry `null`:

```json
{
  "type": "object",
  "properties": {
    "groups": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "files":          { "type": "array", "items": { "type": "string" } },
          "summary":        { "type": "string" },
          "commit_message": { "type": ["string", "null"] }
        },
        "required": ["files", "summary", "commit_message"],
        "additionalProperties": false
      }
    }
  },
  "required": ["groups"],
  "additionalProperties": false
}
```

**Grouping system prompt** (adapted from bash `docs/tmp/git-commit-ai.sh:305-322`; the `response_format` enforces shape, so the prompt carries only the rules, not a JSON example):

```
Analyze these git changes. Group related files into logical commits by semantic relevance.

Rules:
- Every file from the file list must appear in exactly one group.
- Prefer fewer groups (1-3) unless changes are truly unrelated.
- commit_message: a full conventional-commit message for groups[0] ONLY; null for every other group.
- Conventional format <type>(<scope>): <desc>, first line under 72 chars; add a blank line and
  bullet points for details when there are multiple significant changes.
- For renamed files, use the NEW path in your file list.
- summary: a one-line description of each group.
```

**User message** carries (matching bash prompt inputs, `docs/tmp/git-commit-ai.sh:323-333`): the **file list** (new paths), the **porcelain status** (so the model sees R/D/M/A/?? codes), the **diff `--stat`**, and the **per-file full diff**. Keep `include_reasoning: false` and the `strip_think` backstop. We request a message for `groups[0]` only (we re-analyze each run, so later-group messages are never used here).

## 4. Decomposition

1. **Plan model + schema + basic validation** - new `src/plan.rs`: `Plan`/`Group` serde structs; the embedded JSON Schema string for the structured-output request (objects, `groups` array, `files` string array, `summary` string, `commit_message` nullable string; `strict`-compatible: all properties required, `additionalProperties: false`); `validate_basic(plan, &change_set) -> Result<(), PlanError>` (no unknown files; >=1 group; group 1 non-empty with a non-empty message). Unit-tested in isolation. - files: `src/plan.rs`, `src/main.rs` (module decl)

2. **NUL-delimited status parse + path-scoped staging** - `src/git.rs`: `changed_files() -> Result<Vec<ChangedFile>, _>` from `git status --porcelain=v1 -uall -z` (`-uall` expands untracked dirs - review-2 #1; fields: status pair, new path, optional orig_path; new-path resolution for R/C; deletions included); a `ChangedFile` struct + status enum; `has_unmerged()` (any `U`/`DD`/`AA`-family entry) so the orchestrator can abort (review-2 #2); `stage_group(&[&ChangedFile])` that builds the path set (expanding `R`/`C` to **both** new+orig) and stages via `git add -A --pathspec-from-file=- --pathspec-file-nul` with NUL-separated paths on stdin and `GIT_LITERAL_PATHSPECS=1` in the env (review-2 #3+#4); `clear_staged()` = `read-tree HEAD` (has HEAD) / `read-tree --empty` (unborn). Test: rename NUL order, ` -> `-in-name path, delete-only entry, a `*`-in-name path (must stage only the literal file), and an unmerged-status parse - all against a real temp-repo (`git mv`, `git rm`, conflict). - files: `src/git.rs`

3. **Per-file diff gathering for the grouping prompt** - `src/diff.rs`: add `gather_for_grouping(...) -> GroupingContext` (a **new** struct, not an extension of `GatheredDiff`, to keep the tracer's concerns separate) that emits a per-file structure where each file's diff is independently capped with `[diff omitted: N bytes]` - tracked diffs split on `diff --git ` boundaries (reuse `flush_section`/`elide_binary_diff` section logic) and capped per section; untracked content reuses `PER_FILE_BYTES`. Assemble the grouping context = file list + porcelain status + diff `--stat` + per-file full diff (bash inputs `docs/tmp/git-commit-ai.sh:323-333`). Keep `elide_binary_diff`, the FR-57 untracked caps, and the `MAX_TOTAL_BYTES` final safeguard. - files: `src/diff.rs`

4. **Groq structured-output plan call** - `src/groq.rs`: `generate_plan(context: &GroupingContext) -> Result<Plan, GroqError>` building the payload with `response_format` json_schema (`strict: true`) + the grouping system prompt; reuse the existing agent/timeout/`map_ureq_error`/`strip_think` machinery; defensive deserialize into `Plan` with a `GroqError::Deserialize` on failure. Keep `generate_commit_message` intact for the fallback. - files: `src/groq.rs`

5. **Orchestration: display, dry-run, stage group 1, fallback, `--all`** - new grouping flow in `execute`: first, **guard the repo state** - if `changed_files()` reports unmerged entries, abort (exit 1, actionable message) before any clearing/staging; if `.git/MERGE_HEAD` exists with no unmerged entries (clean merge ready), bypass grouping to the single-commit path so `git commit` completes the merge. Otherwise: gather grouping context -> `generate_plan` -> `validate_basic`; on success display groups (mirror bash `docs/tmp/git-commit-ai.sh:399-432`: "Group 1 (committing now)" / "Group N (next run)" + files + group 1 message); `--dry-run` previews and exits; else `[Y/n/e]` confirm (reuse `ui::confirm`; the `e` edit applies only to **group 1's message for this run** - later groups are re-analyzed next run), snapshot index, clear staged, stage group 1 (`stage_group`), `commit_signed`. On any grouping error -> announced fallback to the CLO-486 single-commit path with a concrete reason, e.g. `Plan validation failed: group 1 references unknown file 'foo.txt'. Falling back to single-commit mode.` or `Could not parse a grouping plan from Groq. Falling back to single-commit mode.`. `--all` -> straight to single-commit path, skipping grouping; `--all --dry-run` -> print the single-commit message and exit (no grouping/staging/commit). The change set is captured once at the start; paths that go stale before staging surface as a `validate_basic` mismatch -> fallback. Preserve non-TTY guard + index transaction. - files: `src/main.rs`, `src/ui.rs`, `src/cli.rs` (help text), and calls into `src/plan.rs` (`validate_basic`) + `src/groq.rs` (`generate_plan`)

6. **Tests + acceptance.sh extension + docs** - unit tests for sub-tasks 1-3 (incl. the `*`-in-name literal-staging and unmerged-status cases); add the staging-model disclosure (review-2 #5) to `--help`/README; extend `acceptance.sh` with mock-Groq plan responses: AC-1 (two-group split commits group 1 only), AC-2 (re-run commits group 2; tree clean), AC-4 (rename + delete + ` -> `/space/unicode/`*` names, no fallback), AC-6 (unknown-file plan -> fallback), AC-7 (malformed JSON / 500 -> fallback), AC-8 (`--dry-run` leaves tree untouched), AC-9 (`--all` single commit), AC-12 (unmerged repo -> abort, nothing staged), plus the untracked-dir `-uall` path-agreement case. - files: `scripts/acceptance.sh` (extend the existing mock-Groq harness: add JSON-plan responses and routes for the multi-group/bad-plan/unborn cases), `src/*.rs` test modules

**Dependency order**: 1 and 2 are independent and can land first. 3 is independent of 1/2. 4 depends on 1 (needs `Plan` + schema). 5 depends on 1-4. 6 depends on 1-5. Suggested sequence: 1 -> 2 -> 3 -> 4 -> 5 -> 6.

---

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Two-group change set (src change + unrelated docs change); mock-Groq returns a 2-group plan | One commit created containing only group 1's files; group 2's files still dirty | `acceptance.sh` AC-1 (mock server) |
| 2 | Run #1 then run #2 on the same starting tree | Two commits, each scoped to its group; `git status --porcelain` empty after run #2 | `acceptance.sh` AC-2 |
| 3 | Plan deserialization from a valid structured-output JSON string | `Plan` with N groups, group 1 has files+message | `cargo test plan::` (unit) |
| 4 | Malformed JSON / empty content from mock-Groq | No panic; `GroqError::Deserialize`/`EmptyResponse`; fallback to single commit; reason printed | `acceptance.sh` AC-7 + `cargo test` |
| 5 | `git status --porcelain=v1 -z` with a `git mv` rename | New path extracted; orig path not treated as a separate changed file; order documented in comment | `cargo test git::` against a temp repo |
| 6 | File named `a -> b.txt` (literal arrow) + a spaced + a unicode name in the change set | All parsed to correct single paths; grouped; staged by real path; **no** fallback | `acceptance.sh` AC-4 |
| 7 | Group references a file not in the change set | `validate_basic` returns error; fallback path taken; reason printed | `cargo test plan::validate` + `acceptance.sh` AC-6 |
| 8 | A single file's diff exceeds the per-file cap | Prompt contains that file's header + `[diff omitted: N bytes]`; file not dropped; other files intact | `cargo test diff::` (unit) |
| 9 | `gcm --dry-run` on a multi-group tree | Plan printed (groups + group 1 message); exit 0; `git status` identical before/after; no commit | `acceptance.sh` AC-8 |
| 10 | `gcm --all` on a multi-group tree | Single signed commit of everything; no grouping call made | `acceptance.sh` AC-9 |
| 11 | Abort (`n`) at the `[Y/n/e]` prompt after a plan is shown | Index restored to pre-run tree; nothing committed; exit 0 | `acceptance.sh` (PTY) / manual |
| 12 | Quality gates | `cargo fmt --check` clean; `cargo clippy -D warnings` clean; `cargo test` all pass | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` |
| 13 | Single-group plan (model puts everything in one group) | One commit of the whole change set via the grouping path; behaves like the tracer but typed | `acceptance.sh` (mock returns 1-group plan) |
| 14 | Plan with `groups: []` (empty array) | `validate_basic` rejects (no group 1); fallback to single-commit; reason printed | `cargo test plan::validate` + `acceptance.sh` |
| 15 | `commit_message: null` in group 1 (the exact bash null-message bug) | Rejected by `validate_basic`; fallback, **not** silent single-commit | `cargo test plan::validate` + `acceptance.sh` |
| 16 | Delete-only group 1 (`git rm`'d file is group 1) | `git add -A -- <deleted>` stages the deletion; commit succeeds with only that deletion | `acceptance.sh` AC-4 variant |
| 17 | Request payload for a plan call includes `response_format` with `strict: true` | The captured request body (mock capture file) contains `"strict": true` and the `commit_plan` schema | `acceptance.sh` (assert on capture file) |
| 18 | Unborn branch (fresh `git init`, no HEAD) with multiple added files | Grouping completes; group 1 commits; `read-tree --empty` clear path exercised | `acceptance.sh` (unborn repo) |
| 19 | `gcm --all --dry-run` | Single-commit message printed; exit 0; no grouping call, no staging, no commit | `acceptance.sh` AC-9 variant |
| 20 | Untracked directory `dir/` with nested files; status uses `-uall` | File list and per-file diff both reference `dir/a`, `dir/b` (not `dir/`); no spurious fallback | `cargo test git::` + `acceptance.sh` |
| 21 | Conflicted merge (`UU`) then run gcm | Exit 1, actionable message; index/working tree untouched; no commit; no markers staged | `acceptance.sh` AC-12 |
| 22 | File literally named `a*.txt` alongside `ab.txt`; `a*.txt` in group 1 | Only `a*.txt` staged (literal pathspec), not `ab.txt` | `cargo test git::` (temp repo) |
| 23 | Group 1 with a renamed file (`old -> new`) | Commit completes the rename: `new` added AND `old` deleted in the same commit | `cargo test git::` + `acceptance.sh` AC-4 |

**Edge cases to verify**:
- Rename where the new path sorts differently than the old (ensure orig path is not double-counted as a separate changed file).
- A delete-only group 1 (`git add -A -- <deleted>` stages the deletion; commit succeeds).
- Model returns a single group (whole change set in one commit) - behaves like the tracer but via the grouping path; group 1 = everything.
- Model returns `commit_message: null` for group 1 -> basic validation fails -> fallback (the exact bash null-message bug, `docs/tmp/git-commit-ai.sh:424`, must be caught not silently single-committed).
- Unborn branch (fresh repo, no HEAD): grouping over the initial change set still works (reuse the unborn-branch diff handling in `src/git.rs:98-119`).
- `strict: true` json_schema unsupported at runtime -> escalate (do not silently downgrade).
- Whole-prompt byte cap still engages as a coarse final safeguard after per-file caps (carry `MAX_TOTAL_BYTES`).
- HTTP timeout during the plan call: covered by AC-7's fallback path (carry the existing 30s `TIMEOUT_SECS`); a timeout-specific automated test is a low-priority follow-up (hard to script in `acceptance.sh`) and is not a blocker for this slice.
- Untracked directory expansion (`-uall`): a fully-untracked dir must surface as individual files in the change set so plan paths match diff paths (review-2 #1).
- Unmerged index / conflicted merge: abort, never clear-and-commit (review-2 #2; baking `<<<<<<<` markers is the failure mode). Clean `MERGE_HEAD` -> single-commit completes the merge.
- Pathspec wildcard in a filename (`*`/`?`/`[`): staging must treat it literally (`GIT_LITERAL_PATHSPECS=1`), never glob siblings (review-2 #3).
- Very large group 1 (thousands of paths): staging must not overflow `ARG_MAX` - use `--pathspec-from-file=- --pathspec-file-nul` via stdin (review-2 #4).
- Manual hunk-level staging (`git add -p`) is overridden by file-level grouping; disclosed in `--help`/README, not preserved (review-2 #5).
