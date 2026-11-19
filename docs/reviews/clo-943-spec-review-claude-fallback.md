# Spec Review: clo-943

**Reviewer**: Claude (fallback)
**Reviewed**: 2026-11-19
**Pipeline**: lok spec-review
**Note**: Both external reviewers failed; this is the fallback review

---

# Spec review: CLO-943, documenting the `message` field

I checked every code and line reference in the spec against the worktree and against `scripts/acceptance.sh`, and all of them hold. The spec is accurate and small. Two places need a wording fix before implementation.

## 1. Problem Statement

The problem statement is correct and has evidence for each claim:

- **README:** The section is at `README.md:272-284`. It names `v`, `status` and `mode`, and it has one `jq` example at line 283. It does not mention `message`.
- **Envelope field:** `Envelope.message` is `Option<String>` with `skip_serializing_if` (`src/output.rs:52-55`). Only `single_preview` sets it. The function starts at `src/output.rs:138`, and the field is assigned at line 152.
- **Call site:** `single_preview` has one call site, `src/main.rs:983`. It runs only after `generate_message` succeeds and when `--dry-run` or `--plan-only` is set.
- **Ticket wording:** The spec reads the ticket's "zero requests" case correctly as merge `--plan-only` (CLO-940-4 asserts `reqs = 0`). The spec also separates it from fallback `--plan-only`, which makes one request (CLO-940-5).
- **Name collision:** The spec correctly flags the risk of confusing the top-level `message` with `commit.message` (`src/output.rs:69-74`).

## 2. Acceptance Criteria

- **AC-1, AC-3, AC-4 and AC-6:** These criteria are testable and match the code.
- **AC-2 is too narrow:** It says "when the grouping plan does not parse". The code sends a run to `run_fallback`, and then to `single_preview` under `--dry-run`, for four reasons:
  - a provider error that `build_plan` classes as fallback (`src/main.rs:689-695`)
  - a plan that fails validation (`src/main.rs:699`)
  - a prompt-budget refusal (`src/main.rs:621`)
  - an invalid cached plan (`src/main.rs:494-511`)

  CLO-940-5 tests only the empty-plan case. If the README follows AC-2 literally, it will under-document when `message` appears. The absent case in AC-3 has the same narrow wording. The README already has a broader phrase, "grouping-to-single-commit announcement". AC-2 and AC-3 can say "when grouping falls back to a single commit" instead.
- **AC-5:** This criterion is testable, but the "Prefer" clause that it depends on has a factual error (see section 3).

## 3. Constraints & Assumptions

- **The `jq -er '.message'` claim is false.** The spec says this filter means "a script sees an error instead of `null`". I ran it on an envelope without `message`. It printed `null` on stdout and exited 1. Without `set -e` or `pipefail`, `msg=$(gcm ... | jq -er .message)` stores the string `"null"`. Two alternatives work:
  - `jq -er '.message // empty'`: I verified that it prints nothing and exits 4.
  - `jq -er '.message // error("no message")'`
- **The other constraints hold:**
  - The README has no contractions; the matches for `'s` are possessives.
  - `v` stays 1.
  - `single_preview` has only one call site, so the escalation trigger has not fired.
  - Keeping the change inside one section fits the S estimate.

## 4. Decomposition / Phases

One sub-task in one file is the right size. There is one formatting point. The current README says "Example:" in the singular, above one block. A second block under that label will read awkwardly. The new paragraph about `message` can introduce its own example, and the existing lines then stay as they are, which AC-6 requires.

## 5. Risks & Open Questions

- **Evaluation step #5 cannot be run as written.** The mock provider is a Python heredoc inside `scripts/acceptance.sh`, and it has no standalone launcher. Later cases also overwrite `/tmp/gcm-json.out`, so the AC-493-10 envelope does not survive a full run. A saved envelope built from the AC-493-10 assertions is simpler:

  ```json
  {"v":1,"status":"plan","mode":"single","plan":{"groups":[]},"changed_files":["a.txt"],"cached":false,"message":"chore(test): mock commit message"}
  ```
- **A table row cites the wrong case.** The row "`committed`, `fallback`, `noop`, `error` envelopes" cites "CLO-940 AC-4, AC-5". The error envelope without `message` is asserted in acceptance case CLO-940-7.
- **The edge case does not check stdout.** The edge case for a missing `message` checks only for a non-zero exit. It also needs to check that stdout is empty, because of the `jq` behavior in section 3.
- **The AC-488-1 exclusion in step #7 is justified.** CLO-944 tracks that failure in `docs/ROADMAP.md:108` and `docs/PROJECT.md:111`.

## Verdict

**APPROVE_WITH_SUGGESTIONS**

## Priority Actions

1. **Fix the `jq` guidance.** In the "Prefer" clause, replace `jq -er '.message'` with `jq -er '.message // empty'` or `.message // error("no message")`. Then update the matching edge case so that it also requires empty stdout.
2. **Widen the fallback wording.** In AC-2, AC-3 and the presence table, replace "when the grouping plan does not parse" with "when grouping falls back to a single commit", and cite `src/main.rs:494-511, 621, 689-699` as the triggers.
3. **Make step #5 runnable.** Change evaluation step #5 to use a saved envelope, such as the JSON line in section 5.
4. **Correct the citation.** In the table row for non-`plan` envelopes, cite CLO-940-7 for the error envelope.
5. **Give the new example its own lead-in.** It must not go under the existing "Example:" label.
