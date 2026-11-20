# Pre-PR Validation: clo-945

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Validated**: 2026-11-20
**Pipeline**: implement.md validation gate

---

## Verdict: PASS_WITH_NOTES

The branch changes implement all 26 acceptance criteria and address the 23 defects identified in `/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/specs/2026-11-19-clo-945-review-gates.md`. All deprecated providers, flags, and model pins (`google/`, `--dangerously-skip-permissions`, `opencode ... --dir`, `glm-5`, `gpt-5`, `reasoning.effort`, `ollama launch`, `GEMINI_FALLBACK_MODEL`) are removed from the review paths, model outputs and workflow arguments are passed safely via `shell_escape` and `ENVIRON` without heredoc or `sed` interpolation, and both external and Claude reviewers/synthesizers run with restricted permissions/tools.

## Findings

### MEDIUM
- **Slug validation is omitted in intermediate workflow steps** (`/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/spec-review.toml:58-264`, `/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/design-review.toml:70-305`, `/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml:94-336`):
  `health_check` and `write_reviews`/`write_reports` call `review_valid_slug "$SLUG"`, but the intermediate steps (`gemini_review`, `codex_review`, `review_status`, `claude_fallback`, `fallback_status`, `synthesis`) construct `RUN=".lok/local/review-runs/$SLUG"` without checking `review_valid_slug`. If a caller passes `.` or `..` (which resolve to existing directories `.lok/local/review-runs` or `.lok/local`), `review_status` and subsequent steps will read/write `*.final` and `*.prompt` files outside the intended `<slug>` directory before `write_reviews`/`write_reports` rejects the slug at the end.

### LOW
- **Literal JSON config vs helper variable in `implement.md`** (`/Users/mk/Code/gcm--fix-clo-945-review-gates/.claude/commands/task/phases/implement.md:107`, `/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/lib/review-guard.sh:30`):
  AC20 and Evaluation #3 note that the `OPENCODE_CONFIG_CONTENT` JSON literal appears in both the helper and `implement.md`. `implement.md` instead sources `/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/lib/review-guard.sh` and sets `OPENCODE_CONFIG_CONTENT="$REVIEW_OPENCODE_PERMISSION"`. Runtime behavior is identical and avoids duplicating the policy string, though a literal grep for the JSON inside `implement.md` alone will not match.
- **Unchecked `review_fill_template` in `spec-review.toml` `health_check`** (`/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/spec-review.toml:36` vs `/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/design-review.toml:30-48`):
  `design-review.toml` cleans up `$RUN/review.prompt` if `review_fill_template` fails, whereas `spec-review.toml` ignores the exit status of `review_fill_template`. A partial write on failure could leave a non-empty `$RUN/review.prompt`.
- **Embedded carriage returns in stderr excerpts** (`/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/lib/review-guard.sh:146`):
  `review_stderr_excerpt` strips trailing `\r` (`sub(/\r$/, "")`) rather than all `\r` (`gsub(/\r/, "")`). Progress updates containing mid-line `\r` in the first 5 stderr lines would preserve `\r` in the single-line `REVIEW_FAILED:` diagnostic.
- **Single-line triple-backtick edge case in `review_usable`** (`/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/lib/review-guard.sh:93`):
  `if (line ~ /^[ \t]*(```|~~~)/) { fence = !fence; next }` assumes code fences open and close on separate lines. A line starting with `` ``` `` and closing on the same line would toggle `fence` to `1` for the rest of the report.
- **Gate scratch directories accumulate across runs** (`/Users/mk/Code/gcm--fix-clo-945-review-gates/.claude/commands/task/phases/implement.md:59`):
  `mktemp -d ".lok/local/review-runs/gate-$SLUG.XXXXXX"` leaves a new directory per invocation under `.lok/local/review-runs/` (gitignored). This preserves per-run stderr logs as intended by AC16, though older `gate-*` directories are never pruned.

## Missing Items

- None observed in the changed files. Note that this review is static (read-only Plan mode without shell execution), so runtime checks (Evaluation #4-#23, live AC17, AC19, and AC22 Pi handoff) were not re-executed during this review; `/Users/mk/Code/gcm--fix-clo-945-review-gates/docs/status/clo-945-workflow.yaml:247-251` records their execution and passing results prior to this gate.

## Recommendations

1. Guard `review_valid_slug "$SLUG"` in `review_status` (emitting `usable=none`, `gemini=failed`, `codex=failed` and exiting 0) and at the top of `gemini_review`, `codex_review`, `claude_fallback`, `fallback_status`, and `synthesis` across `/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/spec-review.toml`, `/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/design-review.toml`, and `/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml` so invalid slugs like `..` cannot touch parent directories under `.lok/local/`.
2. Update `/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/spec-review.toml:36` to remove `$RUN/review.prompt` if `review_fill_template` fails, matching `design-review.toml:30-48`.
3. Change `sub(/\r$/, "")` to `gsub(/\r/, "")` in `/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/lib/review-guard.sh:146`.
