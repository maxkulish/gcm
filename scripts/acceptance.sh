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
        resp = json.dumps({"choices":[{"message":{"content":"chore(test): mock commit message"}}]}).encode()
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
  CAPTURE_FILE="$CAPTURE" python3 "$MOCK_PY" "$PORT" >/dev/null 2>&1 &
  MOCK_PID=$!
  for _ in $(seq 1 20); do
    if curl -s -o /dev/null "http://127.0.0.1:$PORT" 2>/dev/null; then break; fi
    sleep 0.1
  done
}
stop_mock() { [ -n "$MOCK_PID" ] && kill "$MOCK_PID" 2>/dev/null; MOCK_PID=""; }
cleanup() { stop_mock; rm -f "$CAPTURE" "$MOCK_PY"; }
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

note "AC-4: thousands of untracked files -> cap engages, no freeze"
d="$(new_repo)"; mkdir -p "$d/junk"
for i in $(seq 1 5000); do printf 'x' > "$d/junk/f$i.txt"; done
: > "$CAPTURE"
start=$(date +%s)
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --dry-run >/tmp/gcm-out 2>&1 )
elapsed=$(( $(date +%s) - start ))
filecount=$(grep -c '+++ b/junk/' "$CAPTURE" 2>/dev/null || true); filecount=${filecount:-0}
[ "$elapsed" -le 5 ] && ok "completed in ${elapsed}s (<=5s)" || bad "too slow (${elapsed}s)"
[ "$filecount" -le 50 ] && grep -q "cap reached" "$CAPTURE" && ok "content for <=50 files ($filecount) + cap notice" || bad "cap not enforced ($filecount files)"
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

stop_mock

note "AC-2 / AC-7: abort and edit paths"
skip "AC-2 (abort) and AC-7 (edit) require a TTY; verify manually (restore path is covered by AC-13)"

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
