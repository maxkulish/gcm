# Phase: Implement

**Purpose**: Execute the implementation plan phase by phase, tracking commits and pushing to remote. Run external model validation before transitioning to PR.

**Entry conditions**: `current_phase: implement`

---

## Status: pending or in_progress

1. Update state: `phases.implement.status: in_progress`
2. **Invoke**: `/plan:implement CLO-XX`

3. After each phase completion within `/plan:implement`:
   - Update workflow state:
     - `phases.implement.last_phase_completed: [phase name]`
     - Add commit SHA to `phases.implement.commits[]`
   - **Push to remote**:
     ```bash
     git push origin feat/clo-XX-short-desc
     ```
   - Add history entry: `phase_completed` with details of phase name
   - Add history entry: `pushed_to_remote`

4. When `/plan:implement` reaches 100%:
   - Add history entry: `implementation_complete`
   - **Continue to Validation Gate** (Step 5)

---

## Step 5: Codex + Gemini Validation Gate

**After implementation is complete, before creating a PR**, run external model validation to catch issues Claude may have blind spots for.

### Choose the Review Inputs

Set three values from the workflow YAML:

| Task type | `DOC1` | `DOC2` |
|-----------|--------|--------|
| specification | `phases.spec.spec_file` | `phases.spec.spec_file` |
| development | `phases.design.design_doc` | `phases.plan.plan_file` |

`SLUG` is the lower-case task ID (`clo-XX`).

### Run Validation in Parallel

Run this block from the repository root with the three values filled in. The block saves the script to a temporary file and runs it from there: run from stdin (`sh -s`), the background reviewers move the shell's read position in the script. It writes the branch diff to `.lok/local/review-input/<slug>-branch.diff`, runs Codex and Gemini in parallel with read-only permissions, waits for each process by PID, checks both outputs with the shared guard (`.lok/lib/review-guard.sh`), and only then writes the two reports through a temporary file renamed over the target. Reviewer output never goes straight to `docs/reviews/`.

Environment overrides for one run: `GEMINI_MODEL` (model without provider, default `gemini-4-hydrogen#high`, always run as `google-vertex/<model>`) and `CODEX_MODEL` (default `gpt-6.2-sol`).

```bash
GATE_SH=$(mktemp) && cat > "$GATE_SH" <<'GATE'
. .lok/lib/review-guard.sh
review_valid_slug "$SLUG" || { echo "REVIEW_INPUT_FAILED: invalid slug '$SLUG'"; exit 3; }
GM="google-vertex/${GEMINI_MODEL:-gemini-4-hydrogen#high}"
CM="${CODEX_MODEL:-gpt-6.2-sol}"
mkdir -p .lok/local/review-runs .lok/local/review-input docs/reviews || exit 1
T=$(mktemp -d ".lok/local/review-runs/gate-$SLUG.XXXXXX") || exit 1
DIFF=".lok/local/review-input/$SLUG-branch.diff"
WHY=""
if git rev-parse --verify -q 'main^{commit}' >/dev/null; then BASE=main
elif git rev-parse --verify -q 'origin/main^{commit}' >/dev/null; then BASE=origin/main
else WHY="no main or origin/main branch to diff against"
fi
if [ -z "$WHY" ]; then
  if ! { git diff --stat "$BASE...HEAD" && git diff "$BASE...HEAD"; } > "$DIFF.tmp" 2>"$T/diff.stderr"; then
    WHY="git diff $BASE...HEAD failed: $(review_stderr_excerpt "$T/diff.stderr")"
  elif ! PUB=$(review_publish "$DIFF.tmp" "$DIFF" 2>&1); then
    WHY="cannot write $DIFF: ${PUB#"REVIEW_PUBLISH_FAILED: $DIFF: "}"
  fi
fi
if [ -n "$WHY" ]; then rm -f "$DIFF.tmp"; echo "REVIEW_INPUT_FAILED: $WHY"; exit 3; fi

cat > "$T/prompt.tpl" <<'ENDTPL'
You are a senior code reviewer. Review all changes on this branch against the task documents.

FILES TO READ:
1. The design document or specification: __DOC1__
2. The implementation plan or specification: __DOC2__
3. The branch diff: __DIFF__ (git diff --stat and git diff against the main branch)
4. Any new or significantly modified source file named in the diff

CHECK FOR:
1. CORRECTNESS: Do the changes implement what the task documents specify?
2. COMPLETENESS: Are all acceptance criteria covered?
3. REGRESSIONS: Could any changes break existing functionality?
4. CODE QUALITY: Clean interfaces, proper error handling, no dead code
5. SECURITY: No hardcoded secrets, proper input validation, safe process spawning

OUTPUT FORMAT:
## Verdict: [PASS | PASS_WITH_NOTES | FAIL]

## Findings
[List each finding with severity: CRITICAL / HIGH / MEDIUM / LOW]

## Missing Items
[Any acceptance criteria not yet implemented]

## Recommendations
[Specific actionable improvements]
ENDTPL
export RV_DOC1="$DOC1" RV_DOC2="$DOC2" RV_DIFF="$DIFF"
review_fill_template "$T/prompt.tpl" "$T/prompt" DOC1 DOC2 DIFF || exit 1
PROMPT=$(cat "$T/prompt")

OPENCODE_CONFIG_CONTENT="$REVIEW_OPENCODE_PERMISSION" \
  timeout -k 30 1140 opencode run "$PROMPT" --model "$GM" --agent plan --auto --standalone </dev/null >"$T/gemini.out" 2>"$T/gemini.stderr" &
GP=$!
timeout -k 30 1140 codex exec -m "$CM" -c model_reasoning_effort='"high"' -s read-only --ephemeral -o "$T/codex.out" "$PROMPT" </dev/null >"$T/codex.stdout" 2>"$T/codex.stderr" &
CP=$!
GRC=0; wait "$GP" || GRC=$?
CRC=0; wait "$CP" || CRC=$?

DATE=$(date +%Y-%m-%d)
# stage <name> <model> <label> <rc> <out> <stderr>: write <out>.md, set ST
stage() {
  review_guard "$1" "$2" "$4" "$5" "$6" prepr > "$5.final" || return 1
  case "$(sed -n '1p' "$5.final")" in
    REVIEW_FAILED:*)
      { sed -n '1p' "$5.final"; printf '%s\n' '' "**Reviewer**: $3" "**Validated**: $DATE" '**Pipeline**: implement.md validation gate'
        EX=$(review_stderr_excerpt "$6"); if [ -n "$EX" ]; then printf '%s\n' '' "**Stderr**: $EX"; fi; } > "$5.md" || return 1
      ST=failed ;;
    *)
      { printf '%s\n' "# Pre-PR Validation: $SLUG" '' "**Reviewer**: $3" "**Validated**: $DATE" '**Pipeline**: implement.md validation gate' '' '---' ''; cat "$5.final"; } > "$5.md" || return 1
      ST=usable ;;
  esac
}
CL="Codex ($CM, effort high)"; GL="Gemini ($GM)"
CR="docs/reviews/$SLUG-codex-validation.md"; GR="docs/reviews/$SLUG-gemini-validation.md"
stage Codex "$CM" "$CL" "$CRC" "$T/codex.out" "$T/codex.stderr" || exit 1; CST=$ST
stage Gemini "$GM" "$GL" "$GRC" "$T/gemini.out" "$T/gemini.stderr" || exit 1; GST=$ST
review_publish "$T/codex.out.md" "$CR" && review_publish "$T/gemini.out.md" "$GR" || exit 1
N=0
[ "$CST" = usable ] && N=$((N + 1))
[ "$GST" = usable ] && N=$((N + 1))
echo "codex=$CST report=$CR label=$CL"
echo "gemini=$GST report=$GR label=$GL"
echo "usable=$N"
GATE
SLUG='clo-XX' DOC1='<DOC1>' DOC2='<DOC2>' sh "$GATE_SH"; GATE_RC=$?; rm -f "$GATE_SH"; echo "gate exit $GATE_RC"
```

The block prints one line per reviewer (`codex=usable|failed`, `gemini=usable|failed`, with the report path and the model label) and `usable=<N>`. A usable report has passed the shared usable-report check; a failed report starts with `REVIEW_FAILED:` and names the reason.

If the block prints `REVIEW_INPUT_FAILED: <reason>` (exit 3), no reviewer ran. Show the reason and offer only `[pause]`.

If the block ends with any other non-zero exit (`gate exit 1`), a setup or write step failed. A line `REVIEW_PUBLISH_FAILED: <target>: <reason>` names a report that could not be published, for example because `<target>` is a directory. The block moves the reports into `docs/reviews/` only after both are staged, but a failed second move can leave one new report next to an older one. Read no report, show the last lines of the block output, and offer only `[pause]`.

### Display Results

Use the model labels the block printed, not fixed model names.

```
VALIDATION GATE RESULTS (CLO-XX)
=================================

Codex (<CODEX_MODEL>, effort high): usable | REVIEW_FAILED: <reason>
  Verdict: [PASS | PASS_WITH_NOTES | FAIL]
  Report: docs/reviews/clo-XX-codex-validation.md
  Key Findings: [top 3 findings]

Gemini (google-vertex/<GEMINI_MODEL>): usable | REVIEW_FAILED: <reason>
  Verdict: [PASS | PASS_WITH_NOTES | FAIL]
  Report: docs/reviews/clo-XX-gemini-validation.md
  Key Findings: [top 3 findings]

Options:
1. [proceed]  - Continue to PR creation
2. [fix]      - Address findings before PR (recommended if FAIL)
3. [override] - Skip validation and proceed (not recommended)
4. [pause]    - Pause workflow

Your choice:
```

The options depend on the usable-report count that the block printed:

- `usable=1` or `usable=2`: all four options. Name the reviewer that failed, if any.
- `usable=0`: offer only `[override]` and `[pause]`.

### Decision Handling

- **If proceed**: Update state and continue to PR phase
- **If fix**: Address findings, re-commit, re-run validation
- **If override**: Log override in history, proceed with warning. With `usable=0`, the history action is `validation_override_no_reviews`
- **If pause**: Save state, exit

### Update State

- `phases.implement.codex_validated: true` only when the block printed `codex=usable`; otherwise `false`
- `phases.implement.codex_verdict: [verdict]` (from the Codex report, when usable)
- `phases.implement.codex_report: docs/reviews/clo-XX-codex-validation.md`
- `phases.implement.gemini_validation_report: docs/reviews/clo-XX-gemini-validation.md`
- Add history entry: `codex_validation_complete`

### Transition to PR

- `phases.implement.status: complete`
- `workflow.current_phase: pr`
- `workflow.status: in_progress`
- **Continue to PR phase**

---

## YAML Checkpoint (Required before transition)

Before signaling completion to the dispatcher, verify:
- `phases.implement.status: complete`
- `phases.implement.commits` is non-empty
- History contains `implementation_complete`
- `phases.implement.codex_validated` is set (true if ran, false if skipped/unavailable)
