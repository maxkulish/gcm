# Lessons: CLO-945 review gates

Durable rules from the review gates that saved failures as reviews.

---

## L1 - A reviewer output counts only when it passes a usable-report check, not when it is non-empty

**Source incident:** CLO-945 (`docs/specs/2026-11-19-clo-945-review-gates.md`, defects 1, 8 and 10). In CLO-941 and CLO-942 the lok workflows called `opencode run --dir`, a flag that opencode v2.x does not have. opencode printed its help text and exited 1. The output was not empty, so the help text was saved as the Gemini review. The `min_length(200)` check and the Haiku validator with `on_error = "pass"` let placeholder text through in the same way.

**Rule:** Every reviewer call goes through `.lok/lib/review-guard.sh`. The guard checks the exit status, rejects help or usage text, and requires exactly one verdict. A failure becomes a report whose first line starts with `REVIEW_FAILED:`, and readers detect failure by that prefix, not by a substring anywhere in the text.

**How to apply:** When you add or change a reviewer step, call `review_guard` on its output and test it with a stub CLI that prints help text, prints nothing, and exits non-zero. When you pin a new CLI flag, check it against `<cli> --help` of the installed version first.

## L2 - opencode `--agent plan` is not read-only

**Source incident:** CLO-945 defect 11. `opencode debug agents` on v2.3.2 showed that the `plan` agent allows `*` and denies only `edit` outside its plan directory. With `--agent plan --auto`, Gemini ran a shell command on request. `OPENCODE_CONFIG_CONTENT` is ignored by the default background service.

**Rule:** A review run through opencode uses `--standalone` together with `OPENCODE_CONFIG_CONTENT="$REVIEW_OPENCODE_PERMISSION"` from the guard. Without `--standalone`, the permission config has no effect.

**How to apply:** After an opencode upgrade, rerun the read-only probe from the CLO-945 evaluation (#15) before you trust a review run.

## L3 - A failing last lok step does not fail the lok run

**Source incident:** CLO-945 defects 13 and 14, reproduced on lok 20261107.0.2. A writer that is the last step and exits 1 leaves `lok run` at exit 0. lok exits 1 only when a later step depends on the failed step. A validator with `replace_output = true` can also replace a guarded output after the guard ran.

**Rule:** A workflow that must fail on a bad result ends with a gate step that depends on the writer. A check that must hold on the final text runs after the validator, on the step output that later steps read.

**How to apply:** For each new lok workflow, test with a stub that makes the writer fail, and assert the exit status of `lok run`, not only the files written.
