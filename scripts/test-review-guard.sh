#!/bin/sh
# Tests for review_publish in .lok/lib/review-guard.sh and for its wiring in the review gates.
# POSIX sh. Prints one line per case and exits non-zero when any case fails.

ROOT=$(git rev-parse --show-toplevel) || exit 1
cd "$ROOT" || exit 1
. .lok/lib/review-guard.sh

W=$(mktemp -d) || exit 1
trap 'chmod -R u+w "$W" 2>/dev/null; rm -rf "$W"' EXIT INT TERM

FAILS=0
ok() { echo "ok - $1"; }
fail() { echo "FAIL - $1: $2"; FAILS=$((FAILS + 1)); }

# run_publish <case-dir> <args...>: run review_publish, keep rc, stdout and stderr
run_publish() {
  _d=$1; shift
  review_publish "$@" >"$_d.out" 2>"$_d.err"
  RC=$?
  OUT=$(cat "$_d.out")
  ERR=$(cat "$_d.err")
}

# refused <name> <case-dir> <target> <expected stderr> [<record>]
refused() {
  _c=$2; _t=$3
  printf 'report\n' > "$_c/src"
  run_publish "$_c/run" "$_c/src" "$_t" ${5:+"$5"}
  if [ "$RC" -eq 0 ]; then fail "$1" "returned 0"
  elif [ -n "$OUT" ]; then fail "$1" "stdout not empty: $OUT"
  elif [ "$ERR" != "$4" ]; then fail "$1" "stderr: $ERR"
  elif [ ! -f "$_c/src" ]; then fail "$1" "source was moved"
  elif [ -n "$(ls -A "$_t")" ]; then fail "$1" "target directory gained an entry"
  else ok "$1"
  fi
}

# Directory at the target
C="$W/dir"; mkdir -p "$C/target"
refused "target is a directory" "$C" "$C/target" "REVIEW_PUBLISH_FAILED: $C/target: target is a directory"

# Symbolic link to a directory at the target
C="$W/link"; mkdir -p "$C/real"; ln -s "$C/real" "$C/target"
refused "target is a link to a directory" "$C" "$C/target" "REVIEW_PUBLISH_FAILED: $C/target: target is a directory"
[ -L "$C/target" ] || fail "target is a link to a directory" "link was replaced"

# published <name> <case-dir> <target> [<record>]
published() {
  _c=$2; _t=$3
  printf 'new report\n' > "$_c/src"
  run_publish "$_c/run" "$_c/src" "$_t" ${4:+"$4"}
  if [ "$RC" -ne 0 ]; then fail "$1" "returned $RC, stderr: $ERR"
  elif [ -n "$OUT$ERR" ]; then fail "$1" "output not empty: $OUT$ERR"
  elif [ ! -f "$_t" ] || [ -L "$_t" ]; then fail "$1" "target is not a regular file"
  elif [ "$(cat "$_t")" != "new report" ]; then fail "$1" "target content: $(cat "$_t")"
  elif [ -e "$_c/src" ]; then fail "$1" "source still exists"
  elif [ -n "${4:-}" ] && [ -e "$4" ]; then fail "$1" "record file was created"
  else ok "$1"
  fi
}

C="$W/missing"; mkdir -p "$C"
published "target missing" "$C" "$C/target"

C="$W/regular"; mkdir -p "$C"; printf 'old report\n' > "$C/target"
published "target is a regular file" "$C" "$C/target"

# mv fails: the target's parent directory is read-only
if [ "$(id -u)" = 0 ]; then
  echo "skip - mv fails on a read-only directory: running as root"
else
  C="$W/readonly"; mkdir -p "$C/ro"; printf 'report\n' > "$C/src"; chmod a-w "$C/ro"
  run_publish "$C/run" "$C/src" "$C/ro/target"
  chmod u+w "$C/ro"
  case "$ERR" in
    "REVIEW_PUBLISH_FAILED: $C/ro/target: mv failed: "?*)
      if [ "$RC" -eq 0 ]; then fail "mv fails on a read-only directory" "returned 0"
      elif [ "$(printf '%s\n' "$ERR" | wc -l | tr -d ' ')" != 1 ]; then fail "mv fails on a read-only directory" "stderr has more than one line"
      elif [ -n "$OUT" ]; then fail "mv fails on a read-only directory" "stdout not empty"
      elif [ ! -f "$C/src" ]; then fail "mv fails on a read-only directory" "source was moved"
      else ok "mv fails on a read-only directory"
      fi ;;
    *) fail "mv fails on a read-only directory" "stderr: $ERR" ;;
  esac
fi

# stub_mv <case-dir> <stub body>: run review_publish with a stub mv first on PATH
stub_mv() {
  mkdir -p "$1/bin"
  printf '#!/bin/sh\n%s\n' "$2" > "$1/bin/mv"; chmod +x "$1/bin/mv"
  printf 'report\n' > "$1/src"
  _p=$PATH; PATH="$1/bin:$PATH"
  run_publish "$1/run" "$1/src" "$1/target"
  PATH=$_p
}

C="$W/mv-silent"; stub_mv "$C" 'exit 4'
if [ "$RC" -ne 0 ] && [ -z "$OUT" ] && [ "$ERR" = "REVIEW_PUBLISH_FAILED: $C/target: mv failed: exit 4" ]; then
  ok "mv fails with empty stderr"
else
  fail "mv fails with empty stderr" "rc=$RC stdout=$OUT stderr=$ERR"
fi

C="$W/mv-multi"; stub_mv "$C" 'printf "\n  \nfirst reason\nsecond line\n" >&2; exit 1'
if [ "$RC" -ne 0 ] && [ -z "$OUT" ] && [ "$ERR" = "REVIEW_PUBLISH_FAILED: $C/target: mv failed: first reason" ]; then
  ok "mv fails with several stderr lines"
else
  fail "mv fails with several stderr lines" "rc=$RC stdout=$OUT stderr=$ERR"
fi

# Record cannot be written: stderr holds only the full line
C="$W/record-dir"; mkdir -p "$C/target" "$C/publish-failed"; printf 'report\n' > "$C/src"
run_publish "$C/run" "$C/src" "$C/target" "$C/publish-failed"
if [ "$RC" -ne 0 ] && [ -z "$OUT" ] && [ "$ERR" = "REVIEW_PUBLISH_FAILED: $C/target: target is a directory" ]; then
  ok "record cannot be written"
else
  fail "record cannot be written" "rc=$RC stdout=$OUT stderr=$ERR"
fi

# Source is a directory: mv succeeds, the target is not a regular file afterwards
C="$W/srcdir"; mkdir -p "$C/src"
run_publish "$C/run" "$C/src" "$C/target"
if [ "$RC" -ne 0 ] && [ -z "$OUT" ] && [ "$ERR" = "REVIEW_PUBLISH_FAILED: $C/target: not a regular file after the move" ]; then
  ok "source is a directory"
else
  fail "source is a directory" "rc=$RC stdout=$OUT stderr=$ERR"
fi

# With a record: the full line goes to the record, stderr gets the fixed line
C="$W/record"; mkdir -p "$C/target"
refused "record, target is a directory" "$C" "$C/target" "REVIEW_PUBLISH_FAILED: see the run publish-failed file" "$C/publish-failed"
if [ "$(cat "$C/publish-failed" 2>/dev/null)" = "REVIEW_PUBLISH_FAILED: $C/target: target is a directory" ]; then
  ok "record holds the full line"
else
  fail "record holds the full line" "record: $(cat "$C/publish-failed" 2>/dev/null)"
fi

C="$W/record-ok"; mkdir -p "$C"
published "record, target missing" "$C" "$C/target" "$C/publish-failed"

# Wiring: the gate files publish only through review_publish
TOMLS=".lok/workflows/spec-review.toml .lok/workflows/design-review.toml .lok/workflows/pre-pr-validation.toml"
IMPL=".claude/commands/task/phases/implement.md"
awk '/<<.GATE.$/ { on = 1; next } /^GATE$/ { on = 0 } on' "$IMPL" > "$W/gate-block.sh"
[ -s "$W/gate-block.sh" ] || fail "wiring" "gate block not found in $IMPL"

MV=$(grep -nE '(^|[;&|({[:space:]])mv +[-"$]' $TOMLS "$W/gate-block.sh")
if [ -z "$MV" ]; then ok "wiring: no mv command"; else fail "wiring: no mv command" "$MV"; fi

# count <file>: number of review_publish calls (occurrences, not lines)
count() { grep -o 'review_publish "' "$1" | wc -l | tr -d ' '; }
for pair in spec-review:4 design-review:4 pre-pr-validation:6; do
  f=".lok/workflows/${pair%%:*}.toml"; n=$(count "$f")
  if [ "$n" = "${pair#*:}" ]; then ok "wiring: $n calls in $f"; else fail "wiring: calls in $f" "found $n, expected ${pair#*:}"; fi
done
n=$(count "$W/gate-block.sh")
if [ "$n" = 3 ]; then ok "wiring: 3 calls in the $IMPL gate block"; else fail "wiring: calls in the $IMPL gate block" "found $n, expected 3"; fi

BAD=$(grep -n 'review_publish "' $TOMLS "$W/gate-block.sh" | grep -v -e '|| exit 1' -e 'elif !')
if [ -z "$BAD" ]; then ok "wiring: every call has failure handling"; else fail "wiring: every call has failure handling" "$BAD"; fi

for pair in spec-review:4 design-review:4 pre-pr-validation:5; do
  f=".lok/workflows/${pair%%:*}.toml"
  n=$(grep -o 'review_publish "[^"]*" "[^"]*" "\$RUN/publish-failed"' "$f" | wc -l | tr -d ' ')
  if [ "$n" = "${pair#*:}" ]; then ok "wiring: $n record calls in $f"; else fail "wiring: record calls in $f" "found $n, expected ${pair#*:}"; fi
done

IF=$(awk '/^if \[ -f "\$RUN\/input-failed" \]; then$/ { on = 1; next } /^fi$/ { on = 0 } on && /review_publish "\$STAGE\/synthesis.md" "\$SYN_TARGET" "\$RUN\/publish-failed" \|\| exit 1/ { print }' .lok/workflows/pre-pr-validation.toml)
if [ -n "$IF" ]; then ok "wiring: input-failed synthesis passes the record"; else fail "wiring: input-failed synthesis passes the record" "call not found"; fi

if [ "$FAILS" -ne 0 ]; then echo "$FAILS case(s) failed"; exit 1; fi
echo "all cases passed"
