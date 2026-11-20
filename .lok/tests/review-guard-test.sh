#!/bin/sh
# Tests for .lok/lib/review-guard.sh: verdict scan, placeholders and named failures.
# POSIX sh. Run from the repository root: sh .lok/tests/review-guard-test.sh

. .lok/lib/review-guard.sh

D=$(mktemp -d) || exit 1
trap 'rm -rf "$D"' EXIT INT TERM
FAILS=0
PAD='Padding text so that the report is longer than the minimum length that review_guard accepts.'

L1='- PASS - no Must Fix items'
L2='- PASS_WITH_NOTES - Must Fix items exist but can be addressed in ONE bounded fix iteration'
L3='- FAIL - any Pivot/Fundamental Scope Issue is present, OR the change diverges materially from the design, OR the change cannot be salvaged in one fix iteration'

# fx <name> <line>...: write one fixture, one argument per line
fx() {
  _n=$1
  shift
  printf '%s\n' "$@" > "$D/$_n.md"
}

# expect <phase> <fixture> <exit>
expect() {
  _rc=0
  review_usable "$1" "$D/$2.md" || _rc=$?
  if [ "$_rc" -eq "$3" ]; then
    echo "ok   $1 $2 exit $_rc"
  else
    echo "FAIL $1 $2 exit $_rc, expected $3"
    FAILS=$((FAILS + 1))
  fi
}

# expect_file <phase> <path> <exit>
expect_file() {
  _rc=0
  review_usable "$1" "$2" || _rc=$?
  if [ "$_rc" -eq "$3" ]; then
    echo "ok   $1 $2 exit $_rc"
  else
    echo "FAIL $1 $2 exit $_rc, expected $3"
    FAILS=$((FAILS + 1))
  fi
}

# expect_text <label> <actual> <expected>
expect_text() {
  if [ "$2" = "$3" ]; then
    echo "ok   $1"
  else
    echo "FAIL $1: got '$2', expected '$3'"
    FAILS=$((FAILS + 1))
  fi
}

# Conflicts
fx conflict '## Verdict: PASS' '' '## Findings' '- none' '' '## Final Verdict: FAIL'
expect prepr conflict 3
fx conflict-syn '## Verdict: PASS' '' '## Must Fix Before PR' '- none' '' '## Final Verdict: FAIL'
expect synthesis-prepr conflict-syn 3
fx conflict-dash '## Verdict: PASS' '' '## Findings' '- none' '' '## Final Verdict: FAIL - PASS criteria were not met'
expect prepr conflict-dash 3
fx conflict-notes '## Verdict: PASS_WITH_NOTES' '' '## Findings' '- none' '' '## Final Verdict: PASS'
expect prepr conflict-notes 3
fx conflict-tpl '## Verdict: PASS | PASS_WITH_NOTES | FAIL' '' '## Verdict: PASS' '' '## Findings' '- none' '' '## Final Verdict: FAIL'
expect prepr conflict-tpl 3
fx conflict-review '## Verdict' 'APPROVE' '' '## Findings' '- none' '' '## Final Verdict' 'APPROVE_WITH_SUGGESTIONS' '' '## Actionable Feedback' '- none'
expect review conflict-review 3
fx conflict-synrev '## Verdict' 'APPROVE' '' '## Priority Actions' '- none' '' '## Final Verdict' 'NEEDS_REVISION'
expect synthesis-review conflict-synrev 3
fx conflict-synbody '## Verdict' '**PASS**' '' '## Must Fix Before PR' '- none' '' '## Final Verdict' 'FAIL'
expect synthesis-prepr conflict-synbody 3
fx conflict-p2 '## Verdict: PASS' '' '## Findings' '- none' '' '## Verdict: [PASS | PASS_WITH_NOTES | FAIL]' 'FAIL' '' '## Missing Items' '- none'
expect prepr conflict-p2 3
fx conflict-3dash '## Verdict: PASS' '' '## Findings' '- none' '' '## Final Verdict: FAIL --- PASS'
expect prepr conflict-3dash 3
fx conflict-glued '## Verdict: PASS' '' '## Findings' '- none' '' '## Final Verdict: FAIL - PASSor FAIL'
expect prepr conflict-glued 3
fx conflict-lower-slash '## Verdict: PASS' '' '## Findings' '- none' '' '## Final Verdict: FAIL / pass'
expect prepr conflict-lower-slash 3
fx conflict-lower-dash '## Verdict: PASS' '' '## Findings' '- none' '' '## Final Verdict: FAIL - pass'
expect prepr conflict-lower-dash 3
fx amb-lower-slash '## Verdict: PASS / fail' '' '## Findings' '- none'
expect prepr amb-lower-slash 3
fx amb-lower-dash '## Verdict: PASS - fail' '' '## Findings' '- none'
expect prepr amb-lower-dash 3
fx amb-3dash '## Verdict: PASS --- FAIL' '' '## Findings' '- none'
expect prepr amb-3dash 3
fx amb-3dash-syn '## Verdict' 'PASS --- FAIL' '' '## Must Fix Before PR' '- none'
expect synthesis-prepr amb-3dash-syn 3
fx conflict-underscore '## Verdict: PASS' '' '## Findings' '- none' '' '## Final Verdict: FAIL - PASS_'
expect prepr conflict-underscore 3
fx conflict-dash-3dash '## Verdict: PASS' '' '## Findings' '- none' '' '## Final Verdict: FAIL - PASS --- details'
expect prepr conflict-dash-3dash 3
fx conflict-dash-syn '## Verdict' 'PASS' '' '## Must Fix Before PR' '- none' '' '## Final Verdict' 'FAIL - PASS criteria were not met'
expect synthesis-prepr conflict-dash-syn 3
fx conflict-fail-unless '## Verdict: PASS' '' '## Findings' '- none' '' '## Final Verdict: FAIL **unless** X'
expect prepr conflict-fail-unless 3
fx fail-unless-fail '## Verdict: FAIL unless X' '' '## Verdict: FAIL' '' '## Findings' '- none'
expect prepr fail-unless-fail 0
fx amb-italic '## Verdict: PASS - _FAIL_' '' '## Findings' '- none'
expect prepr amb-italic 3
fx amb-em-bold '## Verdict: PASS — __FAIL__' '' '## Findings' '- none'
expect prepr amb-em-bold 3
fx fail-lower '## Verdict: FAIL - fail' '' '## Findings' '- none'
expect prepr fail-lower 0
fx ph-if-bold '## Verdict: PASS **if** tests pass' '' '## Findings' '- none'
expect prepr ph-if-bold 1

# Placeholders only
fx ph-if '## Verdict: PASS if no finding is HIGH or above' '' '## Findings' '- none'
expect prepr ph-if 1
fx ph-when '## Verdict: PASS when all checks pass' '' '## Findings' '- none'
expect prepr ph-when 1
fx ph-unless '## Verdict: PASS unless a finding is HIGH' '' '## Findings' '- none'
expect prepr ph-unless 1
fx ph-otherwise '## Verdict' '**FAIL** otherwise' '' '## Findings' '- none'
expect prepr ph-otherwise 1
fx ph-pipe '## Verdict: PASS | PASS_WITH_NOTES | FAIL' '' '## Findings' '- none'
expect prepr ph-pipe 1
fx ph-or '## Verdict: PASS or FAIL depending on the findings' '' '## Findings' '- none'
expect prepr ph-or 1
fx ph-dash '## Verdict: PASS - FAIL' '' '## Findings' '- none'
expect prepr ph-dash 1
fx ph-bracket '## Verdict: [PASS | PASS_WITH_NOTES | FAIL]' '' '## Findings' '- none'
expect prepr ph-bracket 1
fx legend-pass '## Verdict' "$L1" '' '## Must Fix Before PR' '- none'
expect synthesis-prepr legend-pass 1
fx legend-notes '## Verdict' "$L2" '' '## Must Fix Before PR' '- none'
expect synthesis-prepr legend-notes 1
fx legend-fail '## Verdict' "$L3" '' '## Must Fix Before PR' '- none'
expect synthesis-prepr legend-fail 1
fx h3cond '### Verdict: PASS if checks pass' '' '## Final Verdict: PASS'
expect prepr h3cond 1

# Repeats and real verdicts
fx repeat-pass '## Verdict: PASS' '' '## Findings' '- none' '' '## Final Verdict: PASS'
expect prepr repeat-pass 0
fx repeat-fail '## Verdict: FAIL' '' '## Findings' '- none' '' '## Final Verdict: FAIL'
expect prepr repeat-fail 0
fx tpl-real '## Verdict: PASS | PASS_WITH_NOTES | FAIL' '' '## Findings' '- none' '' '## Verdict: FAIL'
expect prepr tpl-real 0
fx prose '## Verdict' '**PASS_WITH_NOTES** - two LOW findings, no PASS blocker' '' '## Findings' '- none'
expect prepr prose 0
fx iffy '## Verdict: PASS iffy wording' '' '## Findings' '- none'
expect prepr iffy 0
fx p2-body '## Verdict: [PASS | PASS_WITH_NOTES | FAIL]' 'FAIL' '' '## Findings' '- none'
expect prepr p2-body 0
fx h3-same '## Verdict: PASS' 'PASS' '' '### Final Verdict: PASS'
expect prepr h3-same 0

# Prompt legend
fx legend-whole-real '## Verdict' 'Put exactly one of PASS, PASS_WITH_NOTES or FAIL on the line after this heading.' "$L1" "$L2" "$L3" '' '## Verdict' 'FAIL' '' '## Must Fix Before PR' '- none'
expect synthesis-prepr legend-whole-real 0
fx legend-whole '## Verdict' 'Put exactly one of PASS, PASS_WITH_NOTES or FAIL on the line after this heading.' "$L1" "$L2" "$L3" '' '## Must Fix Before PR' '- none'
expect synthesis-prepr legend-whole 1
fx legend-prose '## Verdict' 'PASS - no Must Fix items in this diff' '' '## Must Fix Before PR' '- none'
expect synthesis-prepr legend-prose 0
fx legend-bold '## Verdict' '**PASS** - no Must Fix items' '' '## Must Fix Before PR' '- none'
expect synthesis-prepr legend-bold 0

# Edge cases
fx fenced '## Verdict: PASS' '' '## Findings' '```' '## Final Verdict: FAIL' '```'
expect prepr fenced 0
fx lower-pass '## Verdict: pass' '' '## Findings' '- none'
expect prepr lower-pass 1
fx lower-approve '## Verdict: approve' '' '## Findings' '- none'
expect prepr lower-approve 0
fx two-lines '## Verdict' '**PASS**' 'PASS_WITH_NOTES' '' '## Findings' '- none'
expect prepr two-lines 1
fx lookahead-3 '## Verdict: PASS' 'first line' 'second line' 'FAIL' '' '## Findings' '- none'
expect prepr lookahead-3 1
fx lookahead-4 '## Verdict: PASS' 'first line' 'second line' 'third line' 'FAIL' '' '## Findings' '- none'
expect prepr lookahead-4 0
fx lower-approve-alt '## Verdict: approve / rework' '' '## Findings' '- none'
expect prepr lower-approve-alt 1
fx unknown '## Verdict: PASS' '' '## Findings'
expect nosuchphase unknown 2

# Named failures
fx guard-conflict '## Verdict: PASS' '' "$PAD" '' '## Findings' '- none' '' '## Final Verdict: FAIL'
fx guard-none '## Summary' '' "$PAD" '' '## Findings' '- none'
: > "$D/empty.err"
expect_text "review_guard conflict" "$(review_guard Codex m 0 "$D/guard-conflict.md" "$D/empty.err" prepr)" "REVIEW_FAILED: Codex m: conflicting verdicts"
expect_text "review_guard no verdict" "$(review_guard Codex m 0 "$D/guard-none.md" "$D/empty.err" prepr)" "REVIEW_FAILED: Codex m: not a usable report"
expect_text "review_classify conflict" "$(review_classify true "$D/guard-conflict.md" prepr)" "conflicting verdicts"
expect_text "review_classify no verdict" "$(review_classify true "$D/guard-none.md" prepr)" "not a usable report"
expect_text "review_synthesis_reason conflict" "$(review_synthesis_reason true synthesis-prepr "$D/conflict-synbody.md")" "synthesis output has conflicting verdicts"
expect_text "review_synthesis_reason no verdict" "$(review_synthesis_reason true synthesis-prepr "$D/guard-none.md")" "synthesis output failed the synthesis check"
expect_text "review_synthesis_reason skipped" "$(review_synthesis_reason skipped synthesis-prepr "$D/guard-none.md")" "synthesis step did not run"
expect_text "review_synthesis_reason false" "$(review_synthesis_reason false synthesis-prepr "$D/guard-none.md")" "synthesis step failed"

# Stored reports
expect_file review docs/reviews/clo-945-spec-review-gemini.md 0
expect_file review docs/reviews/clo-945-spec-review-codex.md 0
expect_file prepr docs/reviews/clo-942-codex-validation.md 0
expect_file prepr docs/reviews/clo-537-codex-validation.md 0

if [ "$FAILS" -ne 0 ]; then
  echo "$FAILS test(s) failed"
  exit 1
fi
echo "all tests passed"
