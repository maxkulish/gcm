# Shared review guard for the lok review workflows and the implement gate.
#
# POSIX sh. Source it with `. .lok/lib/review-guard.sh` from the repository root.
#
# Functions:
#   review_usable <phase> <file>
#       Exit 0 when <file> is a usable report for <phase>:
#       review | prepr | synthesis-review | synthesis-prepr
#       Exit 1 when it is not usable, 2 for an unknown phase, and 3 when its
#       verdict headings give two or more different verdicts.
#   review_stderr_excerpt <stderr-file>
#       First 5 stderr lines on one line, joined with " | ", at most 500 characters.
#   review_guard <reviewer> <model> <exit-code> <output-file> <stderr-file> <phase>
#       Print the review, or exactly one line
#       "REVIEW_FAILED: <reviewer> <model>: <reason>". Always returns 0.
#   review_fail_line <reviewer> <model> <reason> <stderr-file>
#       Print one REVIEW_FAILED line with the stderr excerpt.
#   review_classify <success> <output-file> <phase>
#       Print "usable", "guard" (the output starts with REVIEW_FAILED:), or the
#       failure reason: "check rejected output", "step failed", "conflicting verdicts",
#       "not a usable report".
#   review_synthesis_reason <syn-success> <phase> <file>
#       Print the reason for a synthesis that is not usable: "synthesis step did not run",
#       "synthesis output has conflicting verdicts", "synthesis output failed the
#       synthesis check", or "synthesis step failed".
#   review_fill_template <template> <out-file> <KEY>...
#       Copy <template> to <out-file>, replacing each __KEY__ with the value of the
#       environment variable RV_KEY, literally (no escapes, no regex).
#   review_valid_slug <slug>
#       Exit 0 when <slug> is safe as one path component under .lok/local/review-runs/:
#       not empty, no leading dot, only letters, digits, ".", "_" and "-".
#   review_publish <src> <target> [<record>]
#       Move <src> to <target> when <target> is missing or a regular file; silent, returns 0.
#       Refuse a directory (or a link to one) at <target> and leave both paths unchanged.
#       On failure return 1 with one line "REVIEW_PUBLISH_FAILED: <target>: <reason>" on
#       stderr. With <record>, append that line to <record> instead and write only
#       "REVIEW_PUBLISH_FAILED: see the run publish-failed file" to stderr.
#
# Variables:
#   REVIEW_OPENCODE_PERMISSION  read-only permission config for opencode (OPENCODE_CONFIG_CONTENT)
#   REVIEW_GUARD_SKIP_USABLE=1  test only: skip the usable-report check inside review_guard

REVIEW_OPENCODE_PERMISSION='{"permission":{"*":"deny","read":"allow","grep":"allow","glob":"allow","list":"allow"}}'

review_valid_slug() {
  case "$1" in
    ''|.*|*[!A-Za-z0-9._-]*) return 1 ;;
  esac
  return 0
}

review_usable() {
  _ru_phase=$1
  _ru_file=$2
  [ -f "$_ru_file" ] || return 1
  awk -v phase="$_ru_phase" '
    function ltrim(s) { sub(/^[ \t\r]+/, "", s); return s }
    function rtrim(s) { sub(/[ \t\r]+$/, "", s); return s }
    function bounded(s, t,   n, c) {
      n = length(t)
      if (substr(s, 1, n) != t) return 0
      c = substr(s, n + 1, 1)
      return (c == "" || c !~ /[A-Za-z0-9_]/)
    }
    function verdict_token(s,   u, i) {
      s = ltrim(s)
      gsub(/^[*_>` \t-]+/, "", s)
      if (tolower(substr(s, 1, 8)) == "overall:") { s = substr(s, 9) }
      s = ltrim(s)
      gsub(/^[*_` \t]+/, "", s)
      if (phase == "review" || phase == "synthesis-review") {
        if (bounded(s, "APPROVE_WITH_SUGGESTIONS")) return 1
        if (bounded(s, "NEEDS_REVISION")) return 1
        if (bounded(s, "APPROVE")) return 1
        return 0
      }
      if (bounded(s, "PASS_WITH_NOTES")) return 1
      if (bounded(s, "PASS")) return 1
      if (bounded(s, "FAIL")) return 1
      if (phase == "prepr") {
        u = toupper(s)
        if (bounded(u, "APPROVE_WITH_CHANGES")) return 1
        if (bounded(u, "APPROVE")) return 1
        if (bounded(u, "REWORK")) return 1
      }
      return 0
    }
    function verdict_value(s,   r, v, t, sep, dash) {
      if (!verdict_token(s)) return ""
      if (phase == "synthesis-prepr") {
        t = rtrim(ltrim(s))
        if (t == "- PASS - no Must Fix items") return "?"
        if (t == "- PASS_WITH_NOTES - Must Fix items exist but can be addressed in ONE bounded fix iteration") return "?"
        if (t == "- FAIL - any Pivot/Fundamental Scope Issue is present, OR the change diverges materially from the design, OR the change cannot be salvaged in one fix iteration") return "?"
      }
      if (phase == "review" || phase == "synthesis-review") r = "(APPROVE_WITH_SUGGESTIONS|NEEDS_REVISION|APPROVE)"
      else if (phase == "prepr") r = "(PASS_WITH_NOTES|PASS|FAIL|APPROVE_WITH_CHANGES|APPROVE|REWORK)"
      else r = "(PASS_WITH_NOTES|PASS|FAIL)"
      s = ltrim(s)
      gsub(/^[*_>` \t-]+/, "", s)
      if (tolower(substr(s, 1, 8)) == "overall:") s = substr(s, 9)
      s = ltrim(s)
      gsub(/^[*_` \t]+/, "", s)
      if (phase == "prepr") s = toupper(s)
      if (!match(s, "^" r)) return ""
      v = substr(s, 1, RLENGTH)
      t = substr(s, RLENGTH + 1)
      sep = "([/|,&]|([Oo][Rr]|[Aa][Nn][Dd])[ \t])"
      dash = "(-+|\342\200\224|\342\200\223)"
      if (match(t, "^[*_` \t]*" sep "[*_` \t]*" r "([^A-Za-z0-9_]|$)")) return "?"
      if (match(t, "^[*_` \t]*" dash "[*_` \t]*" r "([*_` \t.)]*$|[*_` \t]*(" sep "|" dash "))")) return "?"
      if (match(tolower(t), "^[*_` \t]*[ \t](if|when|unless|otherwise)([^a-z0-9_]|$)")) return "?"
      return v
    }
    BEGIN { n = 0; fence = 0; noreviews = 0 }
    {
      line = $0
      sub(/\r$/, "", line)
      if (line ~ /^[ \t]*(```|~~~)/) { fence = !fence; next }
      if (fence) next
      if (index(line, "NO_REVIEWS_AVAILABLE") > 0) noreviews = 1
      n++
      L[n] = line
    }
    END {
      if (phase != "review" && phase != "prepr" && phase != "synthesis-review" && phase != "synthesis-prepr") exit 2
      if (phase ~ /^synthesis/ && noreviews) exit 1
      nv = 0; vline = 0
      for (i = 1; i <= n; i++) {
        if (L[i] !~ /^#+[ \t]/) continue
        lo = tolower(L[i])
        p = index(lo, "verdict")
        if (p == 0) continue
        rest = substr(L[i], p + 7)
        gsub(/^[*_ \t]*:?[ \t]*/, "", rest)
        found = 0; bad = 0; sv = ""
        if (rtrim(rest) != "") { v = verdict_value(rest); if (v == "?") bad = 1; else if (v != "") { found = 1; sv = v } }
        seen = 0
        for (j = i + 1; j <= n && seen < 3; j++) {
          t = rtrim(ltrim(L[j]))
          if (t == "") continue
          if (t ~ /^#+[ \t]/) break
          seen++
          v = verdict_value(t)
          if (v == "") continue
          if (v == "?" || found) bad = 1
          found = 1; sv = v
        }
        if (found && !bad) {
          if (!vline) vline = i
          if (!(sv in V)) { V[sv] = 1; nv++ }
        }
      }
      if (nv > 1) exit 3
      verdict = (nv == 1)
      if (!verdict) exit 1
      af = 0; pa = 0; mf = 0; other = 0; nonaf = 0
      for (i = 1; i <= n; i++) {
        if (L[i] !~ /^##[ \t]/) continue
        lo = tolower(L[i])
        if (index(lo, "actionable feedback")) { af = 1; continue }
        nonaf++
        if (index(lo, "priority actions")) pa = 1
        if (index(lo, "must fix before pr")) mf = 1
        if (i != vline) other++
      }
      if (phase == "review") exit !(af && nonaf >= 2)
      if (phase == "prepr") exit !(other >= 1)
      if (phase == "synthesis-review") exit !pa
      if (phase == "synthesis-prepr") exit !mf
    }
  ' "$_ru_file"
}

review_stderr_excerpt() {
  [ -s "$1" ] || return 0
  awk 'NF { sub(/\r$/, ""); if (c < 5) { out = (c ? out " | " : "") $0; c++ } } END { printf "%s", substr(out, 1, 500) }' "$1"
}

review_fail_line() {
  _rf_excerpt=$(review_stderr_excerpt "$4")
  if [ -n "$_rf_excerpt" ]; then
    printf 'REVIEW_FAILED: %s %s: %s; stderr: %s\n' "$1" "$2" "$3" "$_rf_excerpt"
  else
    printf 'REVIEW_FAILED: %s %s: %s\n' "$1" "$2" "$3"
  fi
}

review_guard() {
  _rg_name=$1
  _rg_model=$2
  _rg_rc=$3
  _rg_out=$4
  _rg_err=$5
  _rg_phase=$6
  if [ "$_rg_rc" -ne 0 ]; then
    if [ "$_rg_rc" -eq 124 ] || [ "$_rg_rc" -eq 137 ]; then
      review_fail_line "$_rg_name" "$_rg_model" "exit $_rg_rc (timeout)" "$_rg_err"
    else
      review_fail_line "$_rg_name" "$_rg_model" "exit $_rg_rc" "$_rg_err"
    fi
    return 0
  fi
  _rg_len=$(awk '{ s = s $0 "\n" } END { gsub(/^[ \t\r\n]+|[ \t\r\n]+$/, "", s); print length(s) }' "$_rg_out" 2>/dev/null)
  if [ "${_rg_len:-0}" -lt 100 ]; then
    review_fail_line "$_rg_name" "$_rg_model" "output too short (${_rg_len:-0} characters)" "$_rg_err"
    return 0
  fi
  _rg_help=$(awk 'NF { c++; if (c > 5) exit
      t = $0; sub(/\r$/, "", t); s = t; sub(/^[ \t]+/, "", s); sub(/[ \t]+$/, "", s)
      if (s == "USAGE") { print "USAGE line"; exit }
      if (t ~ /^Usage:/) { print "Usage: line"; exit }
      if (t ~ /^DESCRIPTION/) { print "DESCRIPTION line"; exit }
      if (index(t, "Unrecognized flag")) { print "Unrecognized flag"; exit } }' "$_rg_out")
  if [ -n "$_rg_help" ]; then
    review_fail_line "$_rg_name" "$_rg_model" "help or usage text ($_rg_help)" "$_rg_err"
    return 0
  fi
  if [ "${REVIEW_GUARD_SKIP_USABLE:-0}" != 1 ]; then
    _rg_urc=0
    review_usable "$_rg_phase" "$_rg_out" || _rg_urc=$?
    if [ "$_rg_urc" -eq 3 ]; then
      review_fail_line "$_rg_name" "$_rg_model" "conflicting verdicts" "$_rg_err"
      return 0
    elif [ "$_rg_urc" -ne 0 ]; then
      review_fail_line "$_rg_name" "$_rg_model" "not a usable report" "$_rg_err"
      return 0
    fi
  fi
  cat "$_rg_out"
  return 0
}

review_classify() {
  _rc_first=$(sed -n '1p' "$2" 2>/dev/null)
  case "$_rc_first" in
    REVIEW_FAILED:*) echo guard; return 0 ;;
  esac
  if [ "$1" != true ]; then
    if grep -q '[^[:space:]]' "$2" 2>/dev/null; then echo "check rejected output"; else echo "step failed"; fi
    return 0
  fi
  _rc_rc=0
  review_usable "$3" "$2" || _rc_rc=$?
  case "$_rc_rc" in
    0) echo usable ;;
    3) echo "conflicting verdicts" ;;
    *) echo "not a usable report" ;;
  esac
}

review_synthesis_reason() {
  case "$1" in
    skipped) echo "synthesis step did not run" ;;
    true)
      _sr_rc=0
      review_usable "$2" "$3" || _sr_rc=$?
      if [ "$_sr_rc" -eq 3 ]; then
        echo "synthesis output has conflicting verdicts"
      else
        echo "synthesis output failed the synthesis check"
      fi ;;
    *) echo "synthesis step failed" ;;
  esac
}

review_fill_template() {
  _ft_tpl=$1
  _ft_out=$2
  shift 2
  awk -v keys="$*" '
    BEGIN { nk = split(keys, K, " ") }
    {
      line = $0; out = ""
      while (1) {
        best = 0; bk = ""
        for (i = 1; i <= nk; i++) {
          p = index(line, "__" K[i] "__")
          if (p > 0 && (best == 0 || p < best)) { best = p; bk = K[i] }
        }
        if (best == 0) break
        out = out substr(line, 1, best - 1) ENVIRON["RV_" bk]
        line = substr(line, best + length(bk) + 4)
      }
      print out line
    }
  ' "$_ft_tpl" > "$_ft_out"
}

review_publish() {
  _rp_src=$1
  _rp_tgt=$2
  _rp_rec=${3-}
  if [ -d "$_rp_tgt" ]; then
    _rp_why="target is a directory"
  else
    _rp_err=$(mv -f -- "$_rp_src" "$_rp_tgt" 2>&1)
    _rp_rc=$?
    if [ "$_rp_rc" -ne 0 ]; then
      _rp_first=$(printf '%s\n' "$_rp_err" | awk 'NF { sub(/\r$/, ""); print; exit }')
      _rp_why="mv failed: ${_rp_first:-exit $_rp_rc}"
    elif [ -f "$_rp_tgt" ]; then
      return 0
    else
      _rp_why="not a regular file after the move"
    fi
  fi
  _rp_line="REVIEW_PUBLISH_FAILED: $_rp_tgt: $_rp_why"
  if [ -n "$_rp_rec" ] && printf '%s\n' "$_rp_line" 2>/dev/null >> "$_rp_rec"; then
    printf '%s\n' 'REVIEW_PUBLISH_FAILED: see the run publish-failed file' >&2
  else
    printf '%s\n' "$_rp_line" >&2
  fi
  return 1
}
