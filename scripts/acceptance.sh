#!/usr/bin/env bash
# End-to-end acceptance checks for the gcm single-commit tracer (CLO-486).
#
# Most cases run offline against a mock Groq server (a tiny python responder that
# captures the request body), so they need no real GROQ_API_KEY and no network.
# Cases that create a real signed commit are gated on whether commit signing works
# in this environment. A real-network smoke test runs only when GCM_LIVE=1.
#
# Usage:  ./scripts/acceptance.sh
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${GCM_BIN:-$ROOT/target/release/gcm}"
PASS=0
FAIL=0
SKIP=0

note()  { printf '\n\033[1m== %s\033[0m\n' "$*"; }
ok()    { PASS=$((PASS+1)); printf '  \033[32mPASS\033[0m %s\n' "$*"; }
bad()   { FAIL=$((FAIL+1)); printf '  \033[31mFAIL\033[0m %s\n' "$*"; }
skip()  { SKIP=$((SKIP+1)); printf '  \033[33mSKIP\033[0m %s\n' "$*"; }

[ -x "$BIN" ] || { echo "building release binary..."; (cd "$ROOT" && cargo build --release) || exit 1; }

# --- mock Groq server -------------------------------------------------------
PORT=8731
CAPTURE="$(mktemp)"
PLAN_FILE="$(mktemp)"   # grouping tests stage a JSON plan here; empty -> fallback
MOCK_PY="$(mktemp).py"
cat > "$MOCK_PY" <<'PY'
import http.server, json, os, sys
CAP = os.environ["CAPTURE_FILE"]
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(n)
        with open(CAP, "ab") as f:
            f.write(body + b"\n")
        # Route by path prefix so error paths are testable (AC-12).
        if "/fail500/" in self.path:
            self.send_response(500); self.end_headers(); self.wfile.write(b"server error"); return
        is_plan = b'"response_format"' in body
        if "/empty/" in self.path:
            content = "   \n  "   # whitespace-only -> EmptyResponse
        elif is_plan:
            # Grouping (structured-output) request: return the JSON plan the
            # current test staged in PLAN_FILE. Absent/empty -> a non-JSON string
            # that forces the parse-failure fallback to single-commit.
            content = "not a json plan"
            try:
                with open(os.environ.get("PLAN_FILE", "")) as pf:
                    txt = pf.read().strip()
                    if txt:
                        content = txt
            except Exception:
                pass
        else:
            content = "chore(test): mock commit message"
        resp = json.dumps({"choices":[{"message":{"content":content}}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type","application/json")
        self.send_header("Content-Length", str(len(resp)))
        self.end_headers()
        self.wfile.write(resp)
    def log_message(self, *a): pass
http.server.HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY

MOCK_PID=""
start_mock() {
  : > "$CAPTURE"
  CAPTURE_FILE="$CAPTURE" PLAN_FILE="$PLAN_FILE" python3 "$MOCK_PY" "$PORT" >/dev/null 2>&1 &
  MOCK_PID=$!
  for _ in $(seq 1 20); do
    if curl -s -o /dev/null "http://127.0.0.1:$PORT" 2>/dev/null; then break; fi
    sleep 0.1
  done
}
stop_mock() { [ -n "$MOCK_PID" ] && kill "$MOCK_PID" 2>/dev/null; MOCK_PID=""; }
cleanup() { stop_mock; rm -f "$CAPTURE" "$MOCK_PY" "$PLAN_FILE"; }
trap cleanup EXIT

MOCK_URL="http://127.0.0.1:$PORT/openai/v1"

# --- scratch repo helper ----------------------------------------------------
new_repo() {
  d="$(mktemp -d)"
  git -C "$d" init -q
  git -C "$d" config user.email test@example.com
  git -C "$d" config user.name "Test"
  echo "$d"
}

# Does signing work here? (global config may require an SSH/GPG key + agent.)
SIGNING_OK=0
probe_signing() {
  d="$(new_repo)"
  echo x > "$d/x"
  git -C "$d" add x
  if git -C "$d" commit -S -m "probe" -q >/dev/null 2>&1; then SIGNING_OK=1; fi
  rm -rf "$d"
}
probe_signing

# ---------------------------------------------------------------------------
note "AC-5: no changes -> exit 0; non-repo -> exit 1"
d="$(new_repo)"; ( cd "$d" && "$BIN" >/tmp/gcm-out 2>&1 ); rc=$?
grep -q "No changes to commit" /tmp/gcm-out && [ $rc -eq 0 ] && ok "clean repo: exit 0 + message" || bad "clean repo (rc=$rc)"
rm -rf "$d"
nd="$(mktemp -d)"; ( cd "$nd" && "$BIN" >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && grep -qi "not a git repository" /tmp/gcm-out && ok "non-repo: exit 1 + message" || bad "non-repo (rc=$rc)"
rm -rf "$nd"

note "AC-9: usage error -> 2; --version build-stamped"
"$BIN" --bogus >/dev/null 2>&1; [ $? -eq 2 ] && ok "bad flag -> exit 2" || bad "bad flag exit code"
"$BIN" --version | grep -Eq '^gcm [0-9]+\.[0-9]+\.[0-9]+\+[0-9a-f]+' && ok "--version has version+sha" || bad "--version format"

note "AC-8/AC-10: egress disclosure + no LLM CLI subprocess"
"$BIN" --help 2>&1 | grep -qi "sent" && ok "--help discloses egress" || bad "--help egress"
grep -qiE "egress|sends your working-tree" "$ROOT/README.md" && ok "README discloses egress" || bad "README egress"
if grep -REn 'Command::new\("(mods|crush|claude)"' "$ROOT/src" >/dev/null 2>&1; then bad "found LLM CLI subprocess"; else ok "no mods/crush/claude subprocess in src"; fi

note "AC-6: missing GROQ_API_KEY -> exit 1, index untouched"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && env -u GROQ_API_KEY GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && grep -q "GROQ_API_KEY" /tmp/gcm-out && ok "missing key -> exit 1 + names var" || bad "missing key (rc=$rc)"
[ -z "$(git -C "$d" diff --cached --name-only)" ] && ok "index untouched after missing-key" || bad "index mutated"
rm -rf "$d"

note "AC-11: non-TTY without --yes -> exit non-zero (no hang)"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" </dev/null >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -ne 0 ] && grep -qi "terminal\|--yes" /tmp/gcm-out && ok "non-TTY no --yes -> exit $rc + guidance" || bad "non-TTY guard (rc=$rc)"
rm -rf "$d"

note "AC-12: unreachable provider -> exit 1, index untouched"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:9/openai/v1" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && ok "unreachable host -> exit 1" || bad "unreachable host (rc=$rc)"
[ -z "$(git -C "$d" diff --cached --name-only)" ] && ok "index untouched after transport error" || bad "index mutated"
rm -rf "$d"

# Cases below talk to the mock server.
start_mock

note "AC-3: gitignored .env never sent to the provider"
d="$(new_repo)"
printf 'SECRET=topsecretvalue123\n' > "$d/.env"
printf '.env\n' > "$d/.gitignore"
printf 'real change\n' > "$d/code.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --dry-run >/tmp/gcm-out 2>&1 )
if grep -q "topsecretvalue123" "$CAPTURE" || grep -q '"\.env"' "$CAPTURE" || grep -q '+++ b/.env' "$CAPTURE"; then
  bad ".env content reached the request body"
else
  ok ".env excluded from request body"
fi
rm -rf "$d"

note "AC-safe-files: untracked symlink/FIFO are name-only (no follow, no freeze)"
outside="$(mktemp -d)"; printf 'SENSITIVE_OUTSIDE_CONTENT_xyz\n' > "$outside/secret"
d="$(new_repo)"; printf 'real\n' > "$d/real.txt"
ln -s "$outside/secret" "$d/link"
mkfifo "$d/pipe" 2>/dev/null
: > "$CAPTURE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" timeout 10 "$BIN" --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
[ "$rc" -ne 124 ] && ok "did not hang on FIFO (rc=$rc)" || bad "hung on FIFO (timeout)"
grep -q "SENSITIVE_OUTSIDE_CONTENT_xyz" "$CAPTURE" && bad "symlink target content leaked" || ok "symlink target not followed"
grep -q "not a regular file" "$CAPTURE" && ok "special files listed name-only" || bad "no name-only marker for special files"
rm -rf "$d" "$outside"

note "AC-4: thousands of untracked files -> cap engages, no freeze"
d="$(new_repo)"; mkdir -p "$d/junk"
# 2000 files: enough to prove no-freeze and the 50-file cap, while the name-only
# listing stays under MAX_TOTAL_BYTES so the count is exact (no mid-entry cut).
# --all takes the single-commit path (one diff gather -> one request), so the
# capture counts are exact (the grouping path would gather twice: plan + fallback).
for i in $(seq 1 2000); do printf 'x' > "$d/junk/f$i.txt"; done
: > "$CAPTURE"
start=$(date +%s)
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --dry-run >/tmp/gcm-out 2>&1 )
elapsed=$(( $(date +%s) - start ))
# The captured request body is JSON (newlines escaped), so count substring
# occurrences, not lines. Every junk file appears as a "+++ b/junk/" header;
# beyond-cap files carry a "untracked cap reached" marker (name-only, no read).
total=$(grep -o '+++ b/junk/' "$CAPTURE" 2>/dev/null | wc -l | tr -d ' '); total=${total:-0}
nameonly=$(grep -o 'untracked cap reached' "$CAPTURE" 2>/dev/null | wc -l | tr -d ' '); nameonly=${nameonly:-0}
content_reads=$(( total - nameonly ))
[ "$elapsed" -le 5 ] && ok "completed in ${elapsed}s (<=5s)" || bad "too slow (${elapsed}s)"
[ "$total" -gt 100 ] && [ "$content_reads" -le 50 ] && ok "content read for <=50 of $total files ($content_reads)" || bad "cap not enforced ($content_reads reads of $total)"
[ "$nameonly" -gt 0 ] && ok "remaining files listed name-only ($nameonly omitted)" || bad "no name-only fallback"
rm -rf "$d"

note "AC-13: failing pre-commit hook -> index restored, exit 1"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; echo hi > "$d/a.txt"
  mkdir -p "$d/.git/hooks"
  printf '#!/bin/sh\nexit 1\n' > "$d/.git/hooks/pre-commit"; chmod +x "$d/.git/hooks/pre-commit"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 1 ] && ok "pre-commit reject -> exit 1" || bad "pre-commit reject (rc=$rc)"
  git -C "$d" diff --cached --quiet && ok "index restored after failed commit" || bad "index left staged"
  [ -z "$(git -C "$d" log --oneline 2>/dev/null)" ] && ok "no commit created" || bad "a commit slipped through"
  rm -rf "$d"
else
  skip "AC-13 needs working commit signing (not available here)"
fi

note "AC-1: dirty repo (binary + unicode name) -> one signed commit (mock message)"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"
  echo "code change" > "$d/main.txt"
  printf '\x00\x01\x02\x03\xff\xfe' > "$d/blob.bin"
  printf 'unicode body\n' > "$d/файл.txt"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "exit 0" || bad "commit run (rc=$rc; $(tail -1 /tmp/gcm-out))"
  n=$(git -C "$d" log --oneline 2>/dev/null | wc -l | tr -d ' ')
  [ "$n" = "1" ] && ok "exactly one commit" || bad "commit count = $n"
  git -C "$d" log -1 --pretty=%s | grep -Eq '^(feat|fix|docs|style|refactor|test|chore)(\(.+\))?!?: .+' && ok "message matches CC header" || bad "message not CC-shaped"
  # The commit carries a signature (gpgsig header) regardless of whether this env
  # can verify it (SSH verification needs an allowedSignersFile).
  git -C "$d" cat-file commit HEAD | grep -q '^gpgsig' && ok "commit is signed (gpgsig header present)" || bad "commit not signed"
  git -C "$d" -c core.quotePath=false ls-files | grep -q 'файл.txt' && ok "unicode-named file committed" || bad "unicode file missing"
  rm -rf "$d"
else
  skip "AC-1 needs working commit signing (not available here)"
fi

note "AC-14: unborn branch -> first signed commit"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; echo "first file" > "$d/first.txt"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "first commit on unborn branch" || bad "unborn first commit (rc=$rc)"
  rm -rf "$d"
else
  skip "AC-14 needs working commit signing (not available here)"
fi

note "AC-12b: provider HTTP 500 -> exit 1, index untouched"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/fail500/v1" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && ok "HTTP 500 -> exit 1" || bad "HTTP 500 (rc=$rc)"
[ -z "$(git -C "$d" diff --cached --name-only)" ] && ok "index untouched after 500" || bad "index mutated after 500"
rm -rf "$d"

note "AC-12c: empty/whitespace provider response -> exit 1"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/empty/v1" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && grep -qi "empty" /tmp/gcm-out && ok "empty response -> exit 1" || bad "empty response (rc=$rc)"
rm -rf "$d"

note "AC-14b: unborn branch, staged-then-modified file -> unstaged delta captured"
d="$(new_repo)"; printf 'one\n' > "$d/s.txt"; git -C "$d" add s.txt; printf 'two\n' >> "$d/s.txt"
: > "$CAPTURE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --dry-run >/tmp/gcm-out 2>&1 )
grep -q '+two' "$CAPTURE" && ok "unstaged change to staged file is in the diff" || bad "unstaged delta missing on unborn"
rm -rf "$d"

note "AC-2: abort path leaves the index unchanged (PTY)"
if command -v expect >/dev/null 2>&1 && [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; echo hi > "$d/a.txt"; git -C "$d" add a.txt; echo more >> "$d/a.txt"
  before="$(git -C "$d" write-tree)"
  GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" GCM_BIN="$BIN" GCM_DIR="$d" expect -c '
    set timeout 20
    spawn -noecho sh -c "cd $env(GCM_DIR) && $env(GCM_BIN)"
    expect {
      -re {\[Y/n/e} { send "n\r" }
      timeout { exit 3 }
    }
    expect eof
    catch wait result
    exit [lindex $result 3]
  ' >/tmp/gcm-out 2>&1; rc=$?
  after="$(git -C "$d" write-tree)"
  [ $rc -eq 0 ] && ok "abort -> exit 0" || bad "abort exit (rc=$rc)"
  [ "$before" = "$after" ] && ok "index tree unchanged after abort" || bad "index changed after abort"
  [ -z "$(git -C "$d" log --oneline 2>/dev/null)" ] && ok "no commit created on abort" || bad "commit created on abort"
  rm -rf "$d"
else
  skip "AC-2 PTY abort needs 'expect' + signing (covered structurally: staging only happens post-confirm; restore path covered by AC-13)"
fi

note "AC-7: edit path"
skip "AC-7 (\$EDITOR edit) requires interactive TTY; verify manually"

# --- CLO-487 semantic grouping ---------------------------------------------
# These stage a JSON plan in $PLAN_FILE; the mock returns it for the grouping
# (structured-output) request. Setup commits disable signing so they run even
# where signing is unavailable; the gcm commit itself still uses `-S`.

note "AC-G1: mixed change set splits; group 1 commits, the rest stays dirty"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"
  printf 'v1\n' > "$d/src.txt"; printf 'v1\n' > "$d/docs.md"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/src.txt"; printf 'v2\n' > "$d/docs.md"
  printf '%s' '{"groups":[{"files":["src.txt"],"summary":"source","commit_message":"feat: update src"},{"files":["docs.md"],"summary":"docs","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "exit 0" || bad "group commit (rc=$rc; $(tail -1 /tmp/gcm-out))"
  [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "2" ] && ok "one new commit (group 1)" || bad "wrong commit count"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'src.txt' && ok "group 1 file committed" || bad "src.txt not committed"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'docs.md' && bad "docs.md leaked into group 1" || ok "group 2 file excluded from commit"
  git -C "$d" status --porcelain | grep -q 'docs.md' && ok "group 2 file left dirty for next run" || bad "docs.md not left dirty"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-G1 needs signing"
fi

note "AC-G2: re-run commits the next group (progression without a cache)"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"
  printf 'v1\n' > "$d/src.txt"; printf 'v1\n' > "$d/docs.md"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/src.txt"; printf 'v2\n' > "$d/docs.md"
  printf '%s' '{"groups":[{"files":["src.txt"],"summary":"source","commit_message":"feat: src"},{"files":["docs.md"],"summary":"docs","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  printf '%s' '{"groups":[{"files":["docs.md"],"summary":"docs","commit_message":"docs: update docs"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "3" ] && ok "two grouped commits total" || bad "progression commit count"
  [ -z "$(git -C "$d" status --porcelain)" ] && ok "tree clean after both groups" || bad "tree still dirty"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-G2 needs signing"
fi

note "AC-G4: rename + delete + ->-in-name + unicode group without fallback"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"
  printf 'old\n' > "$d/orig.txt"; printf 'gone\n' > "$d/del.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  git -C "$d" mv orig.txt renamed.txt
  git -C "$d" rm -q del.txt
  printf 'arrow\n' > "$d/a -> b.txt"
  printf 'uni\n' > "$d/файл.txt"
  printf '%s' '{"groups":[{"files":["renamed.txt","del.txt","a -> b.txt","файл.txt"],"summary":"mixed","commit_message":"chore: reshuffle files"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "exit 0" || bad "tricky-name group (rc=$rc; $(tail -1 /tmp/gcm-out))"
  grep -qi "Falling back" /tmp/gcm-out && bad "tripped into single-commit fallback" || ok "no fallback (grouping path held)"
  git -C "$d" -c core.quotePath=false ls-files | grep -q 'renamed.txt' && ok "rename new path committed" || bad "rename new path missing"
  git -C "$d" -c core.quotePath=false ls-files | grep -q 'orig.txt' && bad "rename old path still tracked" || ok "rename old path deleted (rename completed)"
  git -C "$d" -c core.quotePath=false ls-files | grep -qF 'a -> b.txt' && ok "arrow-in-name file committed" || bad "arrow-name file missing"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-G4 needs signing"
fi

note "AC-G13: a filename containing * stages only the literal file (no glob leak)"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"
  printf '1\n' > "$d/ab.txt"; star="$d/a*.txt"; printf '1\n' > "$star"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf '2\n' > "$d/ab.txt"; printf '2\n' > "$star"
  printf '%s' '{"groups":[{"files":["a*.txt"],"summary":"star","commit_message":"feat: star file"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "exit 0" || bad "glob-name group (rc=$rc)"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'ab.txt' && bad "glob leaked ab.txt into the commit" || ok "only the literal a*.txt staged"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-G13 needs signing"
fi

note "AC-G6: plan referencing an unknown file -> announced fallback"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; printf 'v1\n' > "$d/real.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/real.txt"
  printf '%s' '{"groups":[{"files":["ghost.txt"],"summary":"phantom","commit_message":"feat: ghost"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  grep -qi "Falling back" /tmp/gcm-out && ok "unknown file -> fallback announced" || bad "no fallback on unknown file"
  grep -qi "unknown file" /tmp/gcm-out && ok "reason names the unknown file" || bad "reason missing"
  [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "2" ] && ok "fallback made a single commit" || bad "fallback commit count"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-G6 needs signing"
fi

note "AC-G7: unparseable plan JSON -> fallback to single-commit"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; printf 'v2\n' > "$d/real.txt"
  printf '%s' '{ this is not valid json' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  grep -qi "Falling back" /tmp/gcm-out && ok "malformed plan -> fallback" || bad "no fallback on malformed plan"
  [ $rc -eq 0 ] && [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "fallback single commit created" || bad "fallback commit (rc=$rc)"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-G7 needs signing"
fi

note "AC-G8: --dry-run previews the plan and commits nothing"
d="$(new_repo)"
printf 'v1\n' > "$d/x.txt"; printf 'v1\n' > "$d/y.txt"
git -C "$d" -c commit.gpgsign=false add -A >/dev/null
git -C "$d" -c commit.gpgsign=false commit -qm init
printf 'v2\n' > "$d/x.txt"; printf 'v2\n' > "$d/y.txt"
before="$(git -C "$d" status --porcelain | sort)"
printf '%s' '{"groups":[{"files":["x.txt"],"summary":"x change","commit_message":"feat: x"},{"files":["y.txt"],"summary":"y change","commit_message":null}]}' > "$PLAN_FILE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 0 ] && ok "dry-run exit 0" || bad "dry-run (rc=$rc)"
grep -q "Found 2 group" /tmp/gcm-out && ok "plan groups displayed" || bad "groups not displayed"
grep -q "committing now" /tmp/gcm-out && ok "group 1 marked committing now" || bad "group 1 marker missing"
[ "$before" = "$(git -C "$d" status --porcelain | sort)" ] && ok "working tree unchanged" || bad "dry-run mutated the tree"
[ -z "$(git -C "$d" diff --cached --name-only)" ] && ok "nothing staged" || bad "dry-run staged something"
[ -z "$(git -C "$d" log --oneline 2>/dev/null | sed -n 2p)" ] && ok "no new commit" || bad "dry-run committed"
: > "$PLAN_FILE"; rm -rf "$d"

note "AC-G9: --all bypasses grouping (single commit, no plan request)"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; printf 'a\n' > "$d/a.txt"; printf 'b\n' > "$d/b.txt"
  : > "$CAPTURE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "--all -> one commit" || bad "--all commit (rc=$rc)"
  grep -q 'response_format' "$CAPTURE" && bad "--all still issued a grouping request" || ok "--all skipped the grouping request"
  rm -rf "$d"
else
  skip "AC-G9 needs signing"
fi

note "AC-G12: unresolved merge conflict -> abort, merge state intact"
d="$(new_repo)"
printf 'base\n' > "$d/f.txt"
git -C "$d" -c commit.gpgsign=false add -A >/dev/null
git -C "$d" -c commit.gpgsign=false commit -qm base
main_b="$(git -C "$d" branch --show-current)"
git -C "$d" switch -q -c feature
printf 'feature\n' > "$d/f.txt"; git -C "$d" -c commit.gpgsign=false commit -qam feat
git -C "$d" switch -q "$main_b"
printf 'mainline\n' > "$d/f.txt"; git -C "$d" -c commit.gpgsign=false commit -qam mainline
git -C "$d" merge feature >/dev/null 2>&1 || true
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && ok "conflict -> exit 1" || bad "conflict exit (rc=$rc)"
grep -qi "conflict" /tmp/gcm-out && ok "message names the merge conflict" || bad "no conflict message"
git -C "$d" rev-parse --verify --quiet MERGE_HEAD >/dev/null && ok "merge still in progress (gcm did not commit)" || bad "merge state lost"
# --all must NOT bypass the conflict guard (the guard runs before --all).
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && grep -qi "conflict" /tmp/gcm-out && ok "--all also aborts on a conflict (no marker baking)" || bad "--all bypassed the conflict guard (rc=$rc)"
git -C "$d" rev-parse --verify --quiet MERGE_HEAD >/dev/null && ok "--all left the merge in progress" || bad "--all committed during a conflict"
rm -rf "$d"

note "AC-G12c: clean merge-in-progress (MERGE_HEAD, no conflict) -> single merge commit"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"
  printf 'a\n' > "$d/a.txt"; printf 'b\n' > "$d/b.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm base
  main_b="$(git -C "$d" branch --show-current)"
  git -C "$d" switch -q -c feature
  printf 'a2\n' > "$d/a.txt"; git -C "$d" -c commit.gpgsign=false commit -qam feat
  git -C "$d" switch -q "$main_b"
  printf 'b2\n' > "$d/b.txt"; git -C "$d" -c commit.gpgsign=false commit -qam mainline
  git -C "$d" merge --no-commit --no-ff feature >/dev/null 2>&1 || true   # clean, staged, MERGE_HEAD set
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "clean merge -> exit 0" || bad "clean merge (rc=$rc; $(tail -1 /tmp/gcm-out))"
  git -C "$d" rev-parse --verify --quiet MERGE_HEAD >/dev/null && bad "merge not finalized" || ok "merge finalized (MERGE_HEAD cleared)"
  parents=$(git -C "$d" show -s --format=%P HEAD | wc -w | tr -d ' ')
  [ "$parents" = "2" ] && ok "HEAD is a two-parent merge commit" || bad "merge commit has $parents parents"
  rm -rf "$d"
else
  skip "AC-G12c needs signing"
fi

note "AC-uall: untracked directory expands to individual files (path agreement)"
d="$(new_repo)"
printf 'init\n' > "$d/seed.txt"
git -C "$d" -c commit.gpgsign=false add -A >/dev/null
git -C "$d" -c commit.gpgsign=false commit -qm init
mkdir "$d/pkg"; printf '1\n' > "$d/pkg/a.txt"; printf '2\n' > "$d/pkg/b.txt"
printf '%s' '{"groups":[{"files":["pkg/a.txt","pkg/b.txt"],"summary":"pkg","commit_message":"feat: pkg"}]}' > "$PLAN_FILE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 0 ] && ok "dry-run exit 0" || bad "uall dry-run (rc=$rc)"
grep -qi "Falling back" /tmp/gcm-out && bad "fallback: status collapsed pkg/ (no -uall expansion)" || ok "individual files matched plan (-uall agreement)"
grep -q "Found 1 group" /tmp/gcm-out && ok "grouping ran on the expanded files" || bad "grouping did not run"
: > "$PLAN_FILE"; rm -rf "$d"

stop_mock

# --- optional real-network smoke test --------------------------------------
if [ "${GCM_LIVE:-0}" = "1" ] && [ -n "${GROQ_API_KEY:-}" ]; then
  note "LIVE: real Groq call (GCM_LIVE=1)"
  if [ "$SIGNING_OK" -eq 1 ]; then
    d="$(new_repo)"; echo "live test change" > "$d/live.txt"
    ( cd "$d" && "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
    [ $rc -eq 0 ] && [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "live Groq -> one signed commit" || bad "live run (rc=$rc; $(tail -2 /tmp/gcm-out))"
    rm -rf "$d"
  else
    skip "live test needs working signing"
  fi
fi

printf '\n\033[1m== Summary ==\033[0m  PASS=%d FAIL=%d SKIP=%d\n' "$PASS" "$FAIL" "$SKIP"
[ "$FAIL" -eq 0 ]
