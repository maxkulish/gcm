# Spec: Single-commit tracer - AI message via Groq with safe diff read (CLO-486)

**Created**: 2026-06-19
**Linear**: [CLO-486](https://linear.app/cloud-ai/issue/CLO-486/add-single-commit-tracer-ai-message-via-groq-with-safe-diff-read) (slice S1, label `AFK`)
**Estimated scope**: M (~10 source/config files, 6 sub-tasks)
**Architecture baseline**: [ADR-001](../adrs/001-foundational-architecture-decisions.md) (all 13 decisions Accepted)
**Covers FR**: 4, 5, 6, 9, 10, 18(Groq), 31(basic), 32, 34, 35(basic), 36, 39, 41, 47, 48, 49, 51(basic), 57
**AI review**: APPROVE_WITH_SUGGESTIONS (Gemini + Ollama, 2026-06-19); all suggestions applied - see "Review feedback applied" at the end.

## 1. Problem Statement

`gcm` is a greenfield Rust rewrite of the bash tool `docs/tmp/git-commit-ai.sh` (v2.7). No Rust code exists yet - the repo holds only docs, planning files, and a Go-flavored `.gitignore` left by an earlier scaffold. ADR-001 has locked every foundational decision (git access, runtime, config, providers), so implementation can begin.

This task delivers the **tracer bullet**: the thinnest end-to-end path that proves the whole pipeline works, with **no grouping, no plan cache, no structured-output plan, no retries, and no provider trait** (those arrive in CLO-487 / CLO-491 / CLO-488 / CLO-489). Concretely, a user runs `gcm` in a dirty repo and gets one AI-written, GPG-signed conventional commit.

The path mirrors the bash single-commit routine `fallback_single_commit` (`docs/tmp/git-commit-ai.sh:122-194`), with two deliberate departures:

1. **Direct Groq REST instead of the `mods` CLI** (FR-10). The bash routine calls `llm_call`, which shells out to `mods`. The tracer calls `https://api.groq.com/openai/v1/chat/completions` directly with a blocking HTTP client, per ADR-001 Decision 2.
2. **Read-only diff gather - the index is never mutated before the user confirms** (FR-47). The bash routine runs `git add -A` *before* showing the message, so declining (`n`) leaves everything staged ("Changes remain staged"). That violates FR-47. The tracer gathers the diff from the working tree without staging anything; staging happens only between "Y" and `git commit`, and the pre-run index is captured as a tree object so it can be restored on *any* non-success exit (decline, generation failure, signing failure, pre-commit-hook rejection).

The diff gather must also be **safe** in three senses the tracer is explicitly responsible for:
- **Binary elision** (FR-32): a binary file (or a text-misclassified file full of NUL bytes) must not dump raw bytes into the prompt.
- **Gitignore respect** (FR-48): untracked content is gathered with `git ... --exclude-standard`, so a gitignored `.env` never reaches Groq. This is the primary secret-leak guard.
- **Bounded untracked expansion** (FR-57): an un-ignored directory of thousands of files (e.g. a stray `target/` or `node_modules/`) must not freeze the CLI by stat-ing and reading every file. A file-count/byte cap engages and the tool includes names only (or aborts with a warning) instead of hanging.

Affected: the primary user (Max) on first cutover, and every future adopter and **agent/CI caller** (this task is labeled `AFK` - autonomous use is a first-class case, which is why the non-interactive guard below matters). Until this lands, four downstream slices (CLO-487 grouping, CLO-488 retries, CLO-489 provider trait, CLO-490 secret scan) are blocked.

## 2. Acceptance Criteria

- [ ] **AC-1 (signed commit, FR-4/5/6/36/41):** In a dirty repo that includes a binary file and a file with a unicode (non-ASCII) name, running `gcm` produces exactly one real `git` commit that is signed (verified by `git log --show-signature -1`) and carries a message whose header matches the Conventional-Commits shape `^(feat|fix|docs|style|refactor|test|chore)(\(.+\))?!?: .+`. `gcm --version` prints a version string that includes the crate version and a build-stamped git short SHA.
- [ ] **AC-2 (transactional abort, FR-9/47):** Declining at the `[Y/n/e]` prompt exits 0 and leaves the index and working tree exactly as they were before the run (verified by comparing `git status --porcelain=v1 -z` and the index tree SHA before and after). No commit is created.
- [ ] **AC-3 (gitignore safety, FR-48):** A gitignored `.env` (containing a secret) present in the working tree is never included in the request body sent to Groq.
- [ ] **AC-4 (bounded I/O, FR-57):** With an un-ignored directory of >=5000 untracked files, `gcm --dry-run` completes within a fixed bound (<= 5 s on the dev machine) and the assembled request contains per-file content for at most `cap` files (default 50); all files beyond the cap appear by name only. The contract is the cap, not wall-clock - the run must not read content for every file.
- [ ] **AC-5 (no-change + non-repo, FR-9/39):** Run in a clean repo exits 0 with "No changes to commit" and creates no commit. Run outside any git repo exits 1 with a clear "not a git repository" error.
- [ ] **AC-6 (missing key, FR-10/18):** With `GROQ_API_KEY` unset, `gcm` in a dirty repo exits 1 with an actionable error naming the missing env var, and never mutates the index.
- [ ] **AC-7 (edit path, FR-5):** Choosing `e` at the prompt opens `$EDITOR` (default `vim`) on the generated message with inherited terminal I/O; the saved edited text is what gets committed; the temp file is removed afterward on every exit path.
- [ ] **AC-8 (egress disclosure, FR-49):** `gcm --help` and the README both state plainly that the working-tree diff and untracked file content are sent to the configured LLM provider.
- [ ] **AC-9 (exit codes, FR-39):** Invalid CLI usage exits 2; runtime errors exit 1; success and user-abort exit 0.
- [ ] **AC-10 (no LLM CLI, FR-10):** The runtime makes its LLM call over HTTP to Groq; it invokes no `mods`/`crush`/`claude` subprocess (verifiable by grep of the source and by the binary's dependency tree).
- [ ] **AC-11 (non-interactive guard, FR-51/ADR-001 #10):** In a non-TTY context (stdin is not a terminal) **without** `--yes`/`--no-input`, `gcm` exits non-zero with an actionable message (it does not hang on a prompt it cannot answer). With `--yes`/`--no-input`, it auto-confirms and commits without prompting.
- [ ] **AC-12 (HTTP failure handling, FR-10):** A Groq call that times out, returns 4xx/5xx, fails DNS/transport, or returns an empty/whitespace-only message causes a clean exit 1 with a distinguishable error; the index is left exactly as before the run. A default request timeout (30 s) bounds the call. (Retries/backoff are explicitly deferred to CLO-488.)
- [ ] **AC-13 (commit/signing failure restore, FR-47):** If `git commit -S` fails (signing key unavailable, or a pre-commit hook rejects it), `gcm` restores the index to its captured pre-run state, surfaces the underlying git error, and exits 1. (FR-58's "leave staged for retry" refinement is out of scope; the tracer chooses full FR-47 restore.)
- [ ] **AC-14 (unborn branch, FR-31):** In a freshly `git init`-ed repo with no commits (unborn branch, no `HEAD`), `gcm` diffs against the empty tree and can produce the repository's first signed commit.

**Verification method**: `cargo test` for unit-testable pieces (diff assembly, binary detection incl. NUL-misclassification, cap logic, `<think>` stripping, message parsing, CC-header regex, exit-code mapping, version-string format). `scripts/acceptance.sh` builds the binary and drives the non-interactive cases against a throwaway scratch repo: AC-1 via `--yes`, AC-3/AC-4 via `--dry-run`, AC-5/AC-6/AC-9/AC-11/AC-14 directly, AC-10 via source grep. AC-2 (abort) and AC-7 (edit) require a TTY and are driven via a PTY helper (`expect`/`script`) or documented as manual checks; the underlying restore logic (AC-2/AC-13) is also unit-tested via the write-tree/read-tree path. Cases that would call the network (AC-1) are gated behind a `GROQ_API_KEY` presence check and skipped-with-notice when absent (never silently passed).

## 3. Constraints

**Must**:
- Shell out to the `git` binary for all git operations (ADR-001 #1); do **not** add `git2`/libgit2 or `gix`. Commit with `git commit -S` so the user's real signing config (SSH/GPG, `includeIf` identity) applies (FR-4).
- Use a **blocking** HTTP client; do **not** add `tokio` or any async runtime (ADR-001 #2). The Groq call is one synchronous request with a default 30 s timeout.
- Resolve the Groq API key from `GROQ_API_KEY` only; a missing key is a clear, actionable, non-zero-exit error (FR-18). Never log the key or place it in the prompt.
- Gather untracked files with `--exclude-standard` so gitignored paths are excluded (FR-48). This is a hard security requirement, not cosmetic.
- Gather the diff **without staging**. Capture the pre-run index as a tree object via `git write-tree` *before* any staging, and restore it with `git read-tree <tree-sha>` (or `git read-tree HEAD` semantics for the index) on user abort, generation failure, HTTP failure, signing failure, or pre-commit rejection (FR-47). Staging (`git add -A`) occurs only immediately before `git commit`.
- Parse git path output from NUL-delimited fields (`-z`) with `-c core.quotePath=false`, so unicode/space/quote/newline filenames and the acceptance unicode-named file are handled correctly (FR-31 basic). On an unborn branch (no `HEAD`), diff against the empty tree (magic SHA `4b825dc642cb6eb9a0fa8e4ced7b1d4154961131`) rather than `HEAD`.
- Detect and elide binary content in both tracked diffs and untracked files; non-UTF-8 / NUL bytes must never corrupt the prompt or the tool, including files git's 8000-byte heuristic misclassifies as text (FR-32).
- Cap untracked-directory expansion and content reading by file count (default 50) **and** cumulative bytes (default ~256 KB), evaluated over paths in sorted order: a file whose content would exceed the remaining byte budget is included by name only (its content is not read), and once either cap is reached every remaining untracked file is name-only (FR-57). A coarse overall request-size safeguard caps the total assembled diff as a final defense.
- The assembled prompt diff is built in a stable, deterministic order (paths sorted) so behavior is reproducible.
- In a non-TTY context without `--yes`/`--no-input`, error with a non-zero exit and an actionable message rather than blocking on a prompt (FR-51, ADR-001 #10). Guard via `std::io::IsTerminal` on stdin.
- Spawn `$EDITOR` and `git commit -S` with inherited parent stdio (`Stdio::inherit()` for stdin/stdout/stderr) so interactive editors and GPG/SSH passphrase (pinentry) prompts work. `$EDITOR` defaults to `vim` when unset.
- The `$EDITOR` temp file is created and removed safely on every exit path (success, abort, error, signal) - use the `tempfile` crate (or a `Drop`-guarded wrapper).
- Distinct exit codes: 0 = success or user abort (FR-9), 1 = runtime error, 2 = CLI usage error (FR-39).
- Ship as a single self-contained binary with no runtime dependency on an LLM CLI (FR-10, FR-41).
- `--help` and the README disclose third-party diff egress (FR-49).
- No em dashes in any generated source, docs, or commit text (use regular dashes) - repo style rule. No "generated by" / historical-change comments.

**Must-not**:
- Do **not** implement semantic grouping, the plan cache, structured-output/json_schema plans, typed-error retries/backoff, or the provider trait - those are CLO-487/491/488/489 and would bloat the tracer. The Groq message request returns **plain text**, not a JSON plan.
- Do **not** invoke `mods`, `crush`, or `claude` as a subprocess (FR-10).
- Do **not** hardcode the user's paths or a personal config location (FR-40 spirit); the model id has a sensible default but stays overridable.
- Do **not** preserve hunk-level staging or otherwise honor a curated partial index beyond committing it as part of the single commit (ADR-001 #9; full FR-46 handling is later).

**Prefer**:
- `ureq` (blocking, rustls-based, minimal dependency tree, good cross-platform story per FR-42) over `reqwest::blocking`. Either satisfies ADR-001 #2; `ureq` keeps the tracer binary small.
- `clap` (derive API) for the CLI surface - gives `--version`, `--help`, and exit-code-2-on-usage-error for free (FR-35/36/39).
- `serde`/`serde_json` for building the request body and reading `choices[0].message.content`.
- A flat module layout (`git.rs`, `diff.rs`, `groq.rs`, `ui.rs`, `cli.rs`, `error.rs`, `main.rs`) - separate units per the PRD maintainability NFR, without prematurely building the provider trait (CLO-489 introduces it). Keep `groq::generate_commit_message` sync and `Result`-returning so the CLO-489 trait refactor is mechanical.
- Default Groq model `openai/gpt-oss-120b` (ADR-001 #5), overridable via `GCM_GROQ_MODEL`. Select reasoning-suppression params by model family: `include_reasoning: false` for `gpt-oss` models; `reasoning_effort: "none"` for `qwen` models. Keep a residual `<think>...</think>` strip as the universal last-resort defense (capability matrix: gpt-oss reasoning is hide-only).

**Escalate when**:
- A Groq smoke test shows the chosen reasoning-suppression params do **not** keep chain-of-thought out of the message for `openai/gpt-oss-120b`, or that model is unavailable on the account's tier (ADR-001 carries this as a MEDIUM/LOW-confidence caveat) - surface findings before finalizing the Groq defaults.
- Signing fails in a way that suggests the environment lacks the locked git-signing config - report rather than silently committing unsigned.
- Any requirement here appears to conflict with ADR-001 - stop and flag; do not re-litigate a locked decision unilaterally.

## 4. Decomposition

1. **Cargo scaffold + CLI skeleton + CI** - `Cargo.toml` (bin `gcm`; deps: `clap` (derive), `ureq` (rustls), `serde`, `serde_json`, `tempfile`), **mandatory** `build.rs` that embeds the git short SHA so `--version` is build-stamped (AC-1), Rust `.gitignore` (replace Go ignores with `/target`, `**/*.rs.bk`, etc.; **preserve** the existing "AI dev-tooling migrated from lok" block: `node_modules/`, `.lok/local/`, and the `.env` line), `src/main.rs` + `src/cli.rs` (clap with build-stamped `--version`, `--help` carrying the FR-49 egress disclosure, flags: `--dry-run`, `--all`, `--yes`/`--no-input`), `src/error.rs` (error enum + `-> exit code` mapping: usage 2 / runtime 1 / ok+abort 0), `.github/workflows/ci.yml` (fmt + clippy + test + build on macOS + Linux). Files: `Cargo.toml`, `build.rs`, `.gitignore`, `src/main.rs`, `src/cli.rs`, `src/error.rs`, `.github/workflows/ci.yml`.
2. **Git layer** - `src/git.rs`: `is_inside_work_tree()`, `repo_root()`, `has_changes()` (tracked diff OR staged OR untracked-via-exclude-standard; false -> caller exits 0), unborn-branch detection (empty-tree SHA vs `HEAD`), `snapshot_index() -> tree_sha` (`git write-tree`) and `restore_index(tree_sha)` (`git read-tree`) for the FR-47 transaction, `stage_all()` and `commit_signed(msg)` (`git commit -S -F -`), all run with `-c core.quotePath=false` and `-z` where paths are read. Unit-test the snapshot/restore round-trip. Files: `src/git.rs`.
3. **Safe diff gather** - `src/diff.rs`: assemble the prompt diff = tracked diff (`git diff HEAD` / empty-tree on unborn) through binary-elision (port the perl heuristic from `git-commit-ai.sh:87-119`: per-file, elide bodies >10% non-text over a >200-byte sample, keep the header), plus untracked text-file content via `git ls-files --others --exclude-standard -z` (binary untracked -> `[binary file]` placeholder), enforcing the FR-57 file-count + cumulative-byte cap in sorted order with name-only fallback, and the coarse total-size safeguard. Unit-test: binary elision, NUL-misclassified-as-text file, cap engagement, exclude-standard respect. Files: `src/diff.rs`.
4. **Groq provider call** - `src/groq.rs`: `GroqError` enum (`MissingKey`, `Http(u16)`, `Timeout`, `Transport`, `EmptyResponse`, `Deserialize`); read `GROQ_API_KEY` (->`MissingKey`); build the chat-completions request (system prompt = the conventional-commit instruction from `git-commit-ai.sh:143-154`; user content = diff stat + assembled diff; `model` from `GCM_GROQ_MODEL` or default; family-selected reasoning params); POST via ureq with a 30 s timeout; parse `choices[0].message.content`, trim, strip residual `<think>...</think>`; empty/whitespace after stripping -> `EmptyResponse`. Returns the plain-text message or a typed error mapped to exit 1. Files: `src/groq.rs`.
5. **Confirm + edit UI** - `src/ui.rs`: if stdin is not a terminal and no `--yes`/`--no-input`, return the non-TTY-guard error (AC-11); with `--yes`/`--no-input`, auto-confirm; otherwise print the message in a framed block and prompt `[Y/n/e]`. `Y`/empty -> commit; `n` -> abort (exit 0); `e` -> write the message to a `tempfile`, spawn `$EDITOR` (default `vim`) with inherited stdio, read back the edited text. Files: `src/ui.rs`.
6. **End-to-end wiring + docs + acceptance script** - `src/main.rs` orchestration in FR-47 order: in-repo check -> has-changes-or-exit-0 -> **snapshot index (write-tree)** -> gather diff (read-only) -> Groq generate -> confirm/edit (or non-TTY guard / `--yes`) -> stage-all -> `git commit -S`; on any failure or abort after the snapshot, restore the index; map all errors to exit codes; `--dry-run` stops after printing the message (no snapshot mutation needed). Intermediate milestone: verify the assembled diff (e.g. behind `--dry-run` or a debug print) before wiring the network send. Add `README.md` (install, usage, FR-49 egress disclosure) and `scripts/acceptance.sh` driving AC-1..AC-14 as described in the verification method. Files: `src/main.rs`, `README.md`, `scripts/acceptance.sh`.

**Dependency order**: Sub-task 1 first (everything compiles against the scaffold). Sub-tasks 2, 3, 4, 5 are largely independent after 1 (note: 3 calls 2's git helpers, so land 2 before completing 3). Sub-task 6 integrates 2-5 and must be last.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Dirty scratch repo with a text file, a small binary (PNG), and a unicode-named file (`файл.txt`); run `gcm --yes` | One signed commit; `git log --show-signature -1` shows a good signature; message header matches the CC regex; all three files committed | `scripts/acceptance.sh` case 1 (needs `GROQ_API_KEY`) |
| 2 | Stage one file, run `gcm`, answer `n` (PTY) | Exit 0; index tree SHA and `git status --porcelain=v1 -z` identical before/after; no new commit | acceptance.sh case 2 (PTY) + git.rs unit test of write-tree/read-tree |
| 3 | Add gitignored `.env` with `SECRET=xyz`; run `gcm --dry-run` with request capture | Captured request body contains no `.env` path and no `SECRET=xyz` | acceptance.sh case 3 |
| 4 | Create `junk/` with 5000 untracked files; run `gcm --dry-run` | Completes <= 5 s; request has content for <= 50 files, rest name-only; CLI never hangs | acceptance.sh case 4 |
| 5 | Clean repo -> `gcm`; then `gcm` from a non-repo dir | Clean: exit 0 "No changes to commit". Non-repo: exit 1 "not a git repository" | acceptance.sh case 5 |
| 6 | Unset `GROQ_API_KEY`, dirty repo, `gcm --yes` | Exit 1, error names `GROQ_API_KEY`; index unchanged | acceptance.sh case 6 |
| 7 | Dirty repo, `gcm`, answer `e`, edit, save (PTY) | Commit message equals edited text; temp file gone afterward | manual / PTY |
| 8 | `gcm --help`; inspect `README.md` | Both disclose diff + untracked content egress | `gcm --help`; `rg -i 'sent to' README.md` |
| 9 | `gcm --bogus-flag`; `gcm --version` | Bogus flag exits 2; `--version` prints `gcm <ver>+<gitsha>`, exit 0 | acceptance.sh case 9 |
| 10 | Source + binary audit | No `mods`/`crush`/`claude` subprocess; no `tokio`/`git2` dep | `rg -n 'mods\|crush\|claude\|tokio\|git2' src Cargo.toml` |
| 11 | Pipe empty stdin (`gcm < /dev/null`), then `gcm --yes < /dev/null` | No flag: exit non-zero, actionable message. `--yes`: proceeds (commits/needs key) | acceptance.sh case 11 |
| 12 | Point Groq base URL at a black-hole/unreachable host (or unset network); `gcm --yes` | Exit 1 within ~30 s timeout, distinguishable transport/timeout error; index unchanged | acceptance.sh case 12 (stub URL) |
| 13 | Dirty repo with a failing pre-commit hook; `gcm --yes` | `git commit -S` fails -> index restored to pre-run tree, git error surfaced, exit 1 | acceptance.sh case 13 |
| 14 | Fresh `git init` (no commits), add a file, `gcm --yes` | Diffs against empty tree; first signed commit created | acceptance.sh case 14 (needs `GROQ_API_KEY`) |

**Edge cases to verify**:
- Unborn branch (covered by AC-14 / test 14).
- File misclassified as text but full of NUL bytes -> body elided, header kept (test in diff.rs).
- Untracked binary file -> `[binary file]` placeholder, never raw bytes.
- Groq returns reasoning despite suppression params -> residual `<think>` strip keeps it out of the message.
- Empty/whitespace Groq message -> `EmptyResponse` -> exit 1, index untouched (test 12 variant).
- `$EDITOR` unset -> falls back to `vim`; `$EDITOR` points to a non-existent binary -> clean error, temp file cleaned, index restored.

---

## Implementation notes (non-normative)

- **Binary-elision heuristic**: port `git-commit-ai.sh:87-119` - per file, sample the hunk bodies; if >10% of bytes are outside `\t\n\x20-\x7E` over a >200-byte sample, replace the body with `Binary files differ (body elided: N bytes)` and keep the `diff --git` header.
- **Conventional-commit prompt**: reuse the wording at `git-commit-ai.sh:143-154` (type list, <72-char first line, blank line + bullets for multiple changes, "output ONLY the commit message").
- **Groq request shape** (OpenAI-compatible): `POST https://api.groq.com/openai/v1/chat/completions`, header `Authorization: Bearer $GROQ_API_KEY`, body `{ "model": <id>, "messages": [{role:system,...},{role:user,...}], <reasoning params by family>, "temperature": <low> }`. Read `.choices[0].message.content`. Base URL overridable (env) to enable test 12.
- **Index transaction**: `git write-tree` returns the current index tree SHA before staging; on any abort/failure path after that point, `git read-tree <sha>` restores the index. This is the concrete mechanism behind AC-2/AC-13.
- **Provider trait is intentionally absent**: `groq.rs` exposes `fn generate_commit_message(diff: &Diff) -> Result<String, GroqError>`. CLO-489 generalizes this into the `Provider` trait; the sync `Result` signature keeps that refactor mechanical.

## Review feedback applied (AI review, 2026-06-19)

Both reviewers returned APPROVE_WITH_SUGGESTIONS; all items were auto-applied (additive / refinement / ADR-alignment; none contradicted ADR-001).

- **Non-TTY guard** (Agreement #1) - aligned UI with ADR-001 #10: added `--yes`/`--no-input`, the `IsTerminal` guard, and AC-11. Pulls FR-51(basic) into scope.
- **Groq HTTP failure + timeout** (Agreement #2, Novel #8/#11) - 30 s timeout constraint, `GroqError` taxonomy, AC-12, eval rows 12; empty-response handling.
- **Index restore on commit/signing failure** (Agreement #3/#5, Novel #2) - `git write-tree`/`read-tree` transaction, AC-13, eval row 13. FR-58 "leave staged" explicitly deferred in favor of FR-47 restore.
- **Subprocess stdio inheritance** (Agreement #4) - `Stdio::inherit()` constraint for `$EDITOR` + `git commit -S`; `$EDITOR`->`vim` fallback as a constraint.
- **Unborn branch** (Agreement #6, Novel #3) - AC-14, empty-tree magic SHA documented.
- **Temp-file cleanup** (Agreement #7) - `tempfile` crate constraint, all exit paths.
- **acceptance.sh scope + prompt simulation** (Agreement #8) - explicit in sub-task 6; `--yes`/`--dry-run`/PTY drive mechanisms defined.
- **build.rs mandatory + version test** (Agreement #9) - promoted to mandatory; AC-1 build-stamp + eval row 9.
- **Diff-cap semantics** (Novel #5/#10) - file-count + cumulative-byte cap in sorted order, name-only fallback; AC-4 made cap-based (not wall-clock-only).
- **`--all` vs FR-6** (Novel #6) - reworded: the tracer's only mode is FR-6 single-commit-all; `--all` selects it explicitly and becomes distinct from the default once grouping lands (CLO-487).
- **CC format check** (Novel #7) - AC-1 verifies the generated header against the CC regex (test-only; FR-59 regenerate-on-malformed remains deferred).
- **NUL-misclassification test** (Novel #9), **intermediate verification milestone** (Novel #4), **stable diff ordering** (Novel #12) - added to constraints / sub-tasks 3 and 6.
- **Adaptive Groq reasoning params** (Novel #1) - family-selected params (`include_reasoning:false` for gpt-oss, `reasoning_effort:"none"` for qwen) + `<think>` backstop.
