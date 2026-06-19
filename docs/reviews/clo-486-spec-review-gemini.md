# Spec Review: clo-486

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-06-19
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is clear, complete, and highly accurate. It perfectly contextualizes `gcm` as a greenfield Rust rewrite of the existing bash script (`git-commit-ai.sh` v2.7). It correctly positions CLO-486 as the "tracer bullet" (thinnest end-to-end path) to validate the pipeline without the complexity of semantic grouping, plan caching, or the provider trait. The two deliberate departures from the legacy bash script (direct Groq REST calls and read-only diff gathering before user confirmation) are well-justified and align precisely with the requirements of the Linear task description.

## 2. Acceptance Criteria Review
**Strong**: 
- The acceptance criteria are highly measurable, specific, and testable.
- Criteria such as **AC-2** (transactional abort) and **AC-3** (gitignore safety) establish strong security and correctness guarantees.
- The mapping of exit codes in **AC-9** and the exclusion of LLM CLI subprocesses in **AC-10** are concrete constraints that can be validated via automation scripts or source code audits.

**Gaps**: 
- **Non-interactive/non-TTY execution**: There is a minor contradiction between the specification and ADR-001. ADR-001 Decision 10 states that running in a non-TTY context without automation flags must error and exit. However, the spec's UI section suggests that non-TTY handling is out of scope and the tool may simply proceed as if interactive, which will cause it to hang or crash when trying to read from a closed stdin. 
- **Unborn branch handling**: While mentioned in the edge cases, the behavior of running the tool on a completely fresh repository with no previous commits (unborn branch) should be elevated to a formal acceptance criterion under **AC-5** (no-change + non-repo) or as an extension of **AC-1** to ensure correct empty-tree diffing.

## 3. Constraints Check
**Aligned**: 
- The constraints are exceptionally well-aligned with the project's foundational decisions (ADR-001). 
- Shelling out to the user's `git` binary ensures native signature GPG/SSH validation and `includeIf` config resolution without bloating the codebase.
- The use of a blocking HTTP client (`ureq`) instead of an async runtime (`tokio`) maintains a low cold-start latency and simplified synchronous control flow.

**Concerns**: 
- No contradictions were identified. The styling constraint preventing the use of em dashes (using regular dashes instead) and omitting historical-change or "generated-by" comments is noted and respected.

## 4. Decomposition Quality
**Well-scoped**: 
- The decomposition splits the implementation into 6 logical sub-tasks. 
- Sub-task 1 establishes the Cargo scaffold and GitHub Actions CI, which are crucial for a greenfield project.
- Sub-tasks 2 (Git), 3 (Diff), 4 (Groq), and 5 (UI) are independent modules that can be implemented and unit-tested in parallel once the skeleton is in place.

**Issues**: 
- **CI version validation**: Sub-task 1 marks the `build.rs` for appending the git short SHA as "optional". However, **AC-1** requires `gcm --version` to print a "build-stamped version string." If a build-time stamp is required, `build.rs` is mandatory to inject the Git SHA or build timestamp into the compiled binary.
- **Incremental verification**: Sub-task 6 integrates all layers. It should explicitly define intermediate milestones (such as verifying raw diff gathering before sending it to the network) to simplify debugging.

## 5. Evaluation Coverage
**Covered**: 
- The evaluation matrix maps a realistic test scenario to every single acceptance criterion.
- Testing boundaries are well-defined, categorizing tests into automated unit tests, semi-automated integration tests (`scripts/acceptance.sh`), and manual interactive checks.

**Gaps**: 
- **Automated interactive prompts**: The integration script (`scripts/acceptance.sh`) needs to simulate user input (`Y/n/e`) at the interactive prompt. The spec does not explain how the prompt can be driven non-interactively in tests without a `--yes` flag or allowing piped input on stdin. 
- **Network timeout and failure cases**: The evaluation table lacks test cases for network timeouts or DNS resolution failures, which should map cleanly to runtime exit code 1.

## 6. Codebase Alignment
**Violations**: 
- None. Since this is a greenfield codebase, no existing Rust code exists to conflict with.

**Alignment**: 
- The specified flat module layout (`git.rs`, `diff.rs`, `groq.rs`, `ui.rs`, `cli.rs`, `error.rs`, `main.rs`) is modular and provides a natural refactoring path for CLO-489 when the generic `Provider` trait is introduced.
- The reliance on standard Rust crates like `ureq` (with `rustls`), `clap`, and `serde`/`serde_json` matches standard CLI patterns.

## 7. Blind Spots
- **Subprocess standard I/O inheritance**: When spawning `$EDITOR` for message editing or `git commit -S` for signed commits, the subprocess must inherit the parent terminal's stdin, stdout, and stderr. Failure to configure `Stdio::inherit()` will break interactive editors (like `vim`) and terminal-based GPG passphrase prompts (like `pinentry-curses`), causing the CLI to hang.
- **Index restoration on failure**: Staging occurs right before committing. If the commit fails (e.g., a pre-commit hook rejects it or GPG signing fails), the index remains mutated. A clean way to preserve the transactional promise is using `git write-tree` to capture the pre-run index state as a tree SHA, and running `git read-tree <SHA>` to restore it on failure or abort.
- **Adaptive Groq payload**: If the user overrides the model to a reasoning model like `qwen/qwen3.6-27b` via `GCM_GROQ_MODEL`, hardcoding `include_reasoning: false` may fail or be ignored. The REST payload should adapt dynamically based on the model ID, sending `reasoning_effort: "none"` for Qwen to suppress reasoning at the source.
- **Temporary file cleanup**: Writing the generated message to a temp file for `$EDITOR` requires robust cleanup. If the user cancels or the program is interrupted (Ctrl+C), the temp file should be deleted. This can be handled by using the `tempfile` crate or wrapping file paths in a struct that implements `Drop`.
- **Magic SHA for unborn branch**: Diffing an unborn branch (no commits) against the empty tree requires using the standard empty tree SHA `4b825dc642cb6eb9a0fa8e4ced7b1d4154961131`. This should be explicitly documented.

## 8. Verdict
**APPROVE_WITH_SUGGESTIONS**

## 9. Actionable Feedback
1. **Enforce Non-TTY Guard (High Priority)**: Implement a check in the UI layer using `std::io::stdin().is_terminal()`. If stdin is not a terminal and no auto-confirm flag is provided, exit 1 with an actionable error. This prevents hangs in CI and automation.
2. **Inherit Subprocess I/O (High Priority)**: Ensure that both the `$EDITOR` and `git commit` processes are launched with `stdin(Stdio::inherit())`, `stdout(Stdio::inherit())`, and `stderr(Stdio::inherit())` to support terminal-based editing and interactive GPG passphrase entry.
3. **Use Robust Git Index Saving (High Priority)**: Implement index transactions by running `git write-tree` before staging files, and restoring the index with `git read-tree <tree-SHA>` if the user aborts or if the commit fails.
4. **Adaptive Groq Parameters (Medium Priority)**: Update `groq.rs` to inspect the model ID. Send `include_reasoning: false` for `gpt-oss` models, and send `reasoning_effort: "none"` (or `reasoning_format: "hidden"`) for `qwen` models to avoid generating unnecessary tokens.
5. **Secure Temp File Lifecycle (Medium Priority)**: Implement temp file creation for the `$EDITOR` path using a scoped wrapper or the `tempfile` crate to guarantee file deletion on early exit or drop.
6. **Mandate `build.rs` for Versioning (Low Priority)**: Change the status of `build.rs` from "optional" to "mandatory" in Sub-task 1 to satisfy the "build-stamped" requirement of **AC-1**.
7. **Document the Empty Tree Magic SHA (Low Priority)**: Add the magic empty tree SHA (`4b825dc642cb6eb9a0fa8e4ced7b1d4154961131`) to the spec's git-layer guidelines.
