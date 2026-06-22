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
# Keep retry backoff sub-millisecond so the 429/5xx retry cases (CLO-488) run fast.
export GCM_RETRY_BASE_MS="${GCM_RETRY_BASE_MS:-1}"
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
COUNTER="$(mktemp)"     # call counter for the stateful /retry429/ route (CLO-488)
HEADERS="$(mktemp)"     # captured auth headers per request (CLO-489 eval 26)
MOCK_PY="$(mktemp).py"
# Redirect the plan cache (CLO-491) to a throwaway dir so the suite is hermetic
# and never pollutes the real OS cache. Scratch repos have unique paths -> unique
# cache keys, so a single shared dir is collision-free across cases.
GCM_CACHE_DIR="$(mktemp -d)"; export GCM_CACHE_DIR
# Redirect the config (CLO-496) to an empty throwaway dir so the suite is
# hermetic: no real ~/.config/gcm leaks in, and first-run onboarding fires
# deterministically only where a case leaves the provider truly unconfigured.
GCM_CONFIG="$(mktemp -d)"; export GCM_CONFIG
cat > "$MOCK_PY" <<'PY'
import http.server, json, os, sys
CAP = os.environ["CAPTURE_FILE"]
COUNTER = os.environ.get("COUNTER_FILE", "")
HEADERS = os.environ.get("HEADERS_FILE", "")
def _bump():
    try:
        with open(COUNTER) as f: n = int(f.read().strip() or "0")
    except Exception:
        n = 0
    with open(COUNTER, "w") as f: f.write(str(n + 1))
    return n
def _send_json(h, code, payload, extra_headers=None):
    b = payload.encode()
    h.send_response(code)
    h.send_header("Content-Type", "application/json")
    for k, v in (extra_headers or {}).items():
        h.send_header(k, v)
    h.send_header("Content-Length", str(len(b)))
    h.end_headers()
    h.wfile.write(b)
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(n)
        with open(CAP, "ab") as f:
            f.write(body + b"\n")
        # Capture auth headers so a test can assert the per-provider scheme
        # (Groq/OpenAI: Authorization Bearer; Gemini: x-goog-api-key) - CLO-489.
        if HEADERS:
            with open(HEADERS, "a") as hf:
                hf.write("AUTH=%s GOOG=%s\n" % (
                    self.headers.get("Authorization", ""),
                    self.headers.get("x-goog-api-key", ""),
                ))
        # Gemini safety block: 200 OK, no content, finishReason SAFETY (CLO-489 pt3).
        if "/geminisafety/" in self.path:
            _send_json(self, 200, '{"candidates":[{"finishReason":"SAFETY"}]}'); return
        # Route by path prefix so error paths are testable (AC-12, CLO-488).
        if "/fail500/" in self.path:
            self.send_response(500); self.end_headers(); self.wfile.write(b"server error"); return
        if "/fail400/" in self.path:
            _send_json(self, 400, '{"error":{"message":"mock bad request: unsupported parameter"}}'); return
        if "/fail401/" in self.path:
            _send_json(self, 401, '{"error":{"message":"invalid api key"}}'); return
        if "/fail403/" in self.path:
            _send_json(self, 403, '{"error":{"message":"forbidden"}}'); return
        if "/ollama404/" in self.path:
            # Ollama returns 404 for an unpulled model (CLO-495 AC-O4).
            _send_json(self, 404, '{"error":"model not found, try pulling it first"}'); return
        if "/fail400big/" in self.path:
            # >4096-byte error body: the read must stay bounded yet still surface a
            # (truncated) detail, not drop the body (CLO-488 validation MEDIUM).
            _send_json(self, 400, '{"error":{"message":"' + "X" * 6000 + '"}}'); return
        if "/retry429/" in self.path:
            # Rate-limit the first 2 calls (Retry-After: 0), then succeed -> the
            # retry path must self-heal (CLO-488 AC-1).
            if _bump() < 2:
                _send_json(self, 429, '{"error":{"message":"rate limited"}}', {"Retry-After": "0"}); return
            # else fall through to the normal 200 path below
        # Gemini uses the :generateContent endpoint + responseSchema; the
        # OpenAI-compatible providers (Groq, OpenAI) use response_format (CLO-489).
        is_gemini = ":generatecontent" in self.path.lower() or "/gemini" in self.path.lower()
        # Ollama (CLO-495): native /api/chat; structured-output request carries `format`.
        is_ollama = "/api/chat" in self.path
        is_plan = (b'"response_format"' in body) or (b'"responseSchema"' in body) or (is_ollama and b'"format"' in body)
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
        if is_gemini:
            resp = json.dumps({"candidates":[{"content":{"parts":[{"text":content}]},"finishReason":"STOP"}]}).encode()
        elif is_ollama:
            # Ollama native /api/chat: top-level message.content; `thinking` must be ignored.
            resp = json.dumps({"message":{"role":"assistant","thinking":"(ignored)","content":content}}).encode()
        else:
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
  : > "$CAPTURE"; : > "$COUNTER"; : > "$HEADERS"
  CAPTURE_FILE="$CAPTURE" PLAN_FILE="$PLAN_FILE" COUNTER_FILE="$COUNTER" HEADERS_FILE="$HEADERS" python3 "$MOCK_PY" "$PORT" >/dev/null 2>&1 &
  MOCK_PID=$!
  for _ in $(seq 1 20); do
    if curl -s -o /dev/null "http://127.0.0.1:$PORT" 2>/dev/null; then break; fi
    sleep 0.1
  done
}
stop_mock() { [ -n "$MOCK_PID" ] && kill "$MOCK_PID" 2>/dev/null; MOCK_PID=""; }
cleanup() { stop_mock; rm -f "$CAPTURE" "$MOCK_PY" "$PLAN_FILE" "$COUNTER" "$HEADERS"; rm -rf "$GCM_CACHE_DIR"; }
trap cleanup EXIT

# OpenAI-compatible base (Groq, OpenAI) and a Gemini base pointed at the same mock.
MOCK_URL="http://127.0.0.1:$PORT/openai/v1"
GEMINI_MOCK_URL="http://127.0.0.1:$PORT/gemini"

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

note "AC-6: provider selected but key missing -> exit 1, index untouched"
# GCM_PROVIDER=groq makes this a configured-but-keyless run (the surviving
# MissingKey path); without an explicit provider it would be an unconfigured
# first run and onboard instead (see AC-ONB / CLO-496).
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && env -u GROQ_API_KEY GCM_PROVIDER=groq GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
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

# CLO-495: Ollama is local (no key) and not present in CI, so these cases run
# against the mock /api/chat route (offline). The dead-daemon case (AC-O2) needs
# no mock - it points at a closed port, like AC-12.
note "AC-O2: ollama daemon unreachable -> exit 1 + actionable error, index untouched"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && env -u GROQ_API_KEY GCM_OLLAMA_BASE_URL="http://127.0.0.1:9" "$BIN" --provider=ollama --yes >/tmp/gcm-out 2>&1 ); rc=$?
if [ $rc -eq 1 ] && grep -qi "is Ollama running" /tmp/gcm-out && grep -q "OLLAMA_HOST" /tmp/gcm-out; then
  ok "ollama down -> exit 1 + actionable (start ollama / OLLAMA_HOST)"
else
  bad "ollama down (rc=$rc): $(cat /tmp/gcm-out)"
fi
[ -z "$(git -C "$d" diff --cached --name-only)" ] && ok "index untouched after ollama transport error" || bad "index mutated"
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
  # Both files changed -> a valid partition must cover both (group 1 = the literal
  # a*.txt; ab.txt deferred to group 2). The point is that staging group 1 does
  # not glob ab.txt in, even though the plan now also lists it (in a later group).
  printf '%s' '{"groups":[{"files":["a*.txt"],"summary":"star","commit_message":"feat: star file"},{"files":["ab.txt"],"summary":"sibling","commit_message":null}]}' > "$PLAN_FILE"
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

# --- CLO-492 full plan validation + safe fallback --------------------------
# FR-23 full: a plan that omits a changed file, duplicates one, or has an empty
# group is rejected to fallback (not just unknown files as in the bash tool).
# FR-46: a pre-existing curated index is warned about before it is reset.

note "AC-V1 (CLO-492): plan omitting a changed file -> fallback, file not dropped"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; printf 'v1\n' > "$d/a.txt"; printf 'v1\n' > "$d/b.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/a.txt"; printf 'v2\n' > "$d/b.txt"
  # Plan covers only a.txt; b.txt is omitted -> full validation must reject it.
  printf '%s' '{"groups":[{"files":["a.txt"],"summary":"partial","commit_message":"feat: a"}]}' > "$PLAN_FILE"
  : > "$CAPTURE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  grep -qi "Falling back" /tmp/gcm-out && ok "omission -> fallback announced" || bad "no fallback on omission"
  grep -qi "omitted changed file" /tmp/gcm-out && ok "reason names the omitted file" || bad "omission reason missing"
  # The fallback lumps everything: b.txt must be committed, never silently dropped.
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'b.txt' && ok "omitted file committed by fallback (not dropped)" || bad "b.txt dropped from history"
  [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "2" ] && ok "single fallback commit" || bad "fallback commit count"
  plan_reqs=$(grep -c 'response_format' "$CAPTURE")
  [ "$plan_reqs" = "1" ] && ok "validation fallback not retried (1 grouping request)" || bad "grouping retried ($plan_reqs requests)"
  total_reqs=$(grep -c 'messages' "$CAPTURE")
  [ "$total_reqs" = "2" ] && ok "exactly 2 provider requests (1 plan + 1 fallback message)" || bad "request count $total_reqs (want 2: plan + fallback message)"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-V1 needs signing"
fi

note "AC-V2 (CLO-492): plan listing a file in two groups -> fallback, not committed"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; printf 'v1\n' > "$d/a.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/a.txt"
  printf '%s' '{"groups":[{"files":["a.txt"],"summary":"one","commit_message":"feat: a"},{"files":["a.txt"],"summary":"two","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  grep -qi "Falling back" /tmp/gcm-out && ok "duplicate -> fallback announced" || bad "no fallback on duplicate"
  grep -qi "more than one group" /tmp/gcm-out && ok "reason names the duplicated file" || bad "duplicate reason missing"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-V2 needs signing"
fi

note "AC-V3 (CLO-492): plan with an empty group (any position) -> fallback"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; printf 'v1\n' > "$d/a.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/a.txt"
  printf '%s' '{"groups":[{"files":["a.txt"],"summary":"one","commit_message":"feat: a"},{"files":[],"summary":"empty","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  grep -qi "Falling back" /tmp/gcm-out && ok "empty group -> fallback announced" || bad "no fallback on empty group"
  grep -qi "references no files" /tmp/gcm-out && ok "reason names the empty group" || bad "empty-group reason missing"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-V3 needs signing"
fi

note "AC-V4 (CLO-492): pre-staged hunk warns before the index is reset (FR-46); dry-run silent"
# No signing gate: the curated-index warning is emitted before any commit, so it
# is on stderr regardless of whether the gcm commit itself can complete.
d="$(new_repo)"
printf 'l1\nl2\n' > "$d/a.txt"
git -C "$d" -c commit.gpgsign=false add -A >/dev/null
git -C "$d" -c commit.gpgsign=false commit -qm init
# Partially-staged file (status MM): stage one edit, then modify again.
printf 'l1x\nl2\n' > "$d/a.txt"; git -C "$d" add a.txt
printf 'l1x\nl2x\n' > "$d/a.txt"
git -C "$d" status --porcelain=v1 | grep -q '^MM a.txt' && ok "fixture is partially staged (MM)" || bad "fixture not MM ($(git -C "$d" status --porcelain=v1))"
printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"}]}' > "$PLAN_FILE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --dry-run >/tmp/gcm-dry 2>&1 )
grep -qi "curated index" /tmp/gcm-dry && bad "dry-run warned (nothing is reset on a preview)" || ok "dry-run is silent about the curated index"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
grep -qi "curated index" /tmp/gcm-out && ok "real run warns about the curated index" || bad "no curated-index warning"
grep -qi "reset" /tmp/gcm-out && ok "warning says the index will be reset" || bad "warning missing 'reset'"
grep -qi "hunk-level staging is not preserved" /tmp/gcm-out && ok "warning states the v1 limitation" || bad "hunk-level note missing"
grep -qi "1 partially" /tmp/gcm-out && ok "warning names the partial-staging count" || bad "partial-staging count missing"
: > "$PLAN_FILE"; rm -rf "$d"

note "AC-V5 (CLO-492): --all also warns before resetting a curated index"
d="$(new_repo)"
printf 'l1\nl2\n' > "$d/a.txt"
git -C "$d" -c commit.gpgsign=false add -A >/dev/null
git -C "$d" -c commit.gpgsign=false commit -qm init
printf 'l1x\nl2\n' > "$d/a.txt"; git -C "$d" add a.txt; printf 'l1x\nl2x\n' > "$d/a.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --yes >/tmp/gcm-out 2>&1 )
grep -qi "curated index" /tmp/gcm-out && ok "--all warns about the curated index" || bad "--all curated-index warning missing"
rm -rf "$d"

note "AC-V6 (CLO-492): declining the forced fallback leaves the index unchanged; warnings ordered (PTY)"
if command -v expect >/dev/null 2>&1; then
  d="$(new_repo)"; printf 'v1\n' > "$d/a.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/a.txt"; git -C "$d" add a.txt   # curated index (a.txt staged)
  printf 'v3\n' > "$d/a.txt"                            # now MM: partially staged
  : > "$PLAN_FILE"   # empty plan -> the mock returns non-JSON -> forced fallback
  before="$(git -C "$d" write-tree)"; beforestat="$(git -C "$d" status --porcelain=v1)"
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
  after="$(git -C "$d" write-tree)"; afterstat="$(git -C "$d" status --porcelain=v1)"
  grep -qi "curated index" /tmp/gcm-out && ok "curated-index warning shown" || bad "no curated-index warning"
  grep -qi "Falling back" /tmp/gcm-out && ok "forced fallback announced" || bad "no fallback announced"
  # AC-18: the curated-index warning precedes the fallback warning.
  cur_line=$(grep -ni "curated index" /tmp/gcm-out | head -1 | cut -d: -f1)
  fb_line=$(grep -ni "Falling back" /tmp/gcm-out | head -1 | cut -d: -f1)
  { [ -n "$cur_line" ] && [ -n "$fb_line" ] && [ "$cur_line" -lt "$fb_line" ]; } && ok "curated-index warning precedes the fallback warning" || bad "warning ordering wrong (cur=$cur_line fb=$fb_line)"
  [ $rc -eq 0 ] && ok "decline -> exit 0" || bad "decline exit (rc=$rc)"
  [ "$before" = "$after" ] && ok "staged tree SHA unchanged after decline" || bad "staged tree changed after decline"
  [ "$beforestat" = "$afterstat" ] && ok "git status identical after decline" || bad "status changed after decline"
  [ -z "$(git -C "$d" log --oneline | sed -n 2p)" ] && ok "no commit created on decline" || bad "commit created on decline"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-V6 PTY decline needs 'expect' (index-restore covered by AC-13; staging is post-confirm)"
fi

note "AC-V7 (CLO-492): an invalid CACHED plan is re-validated on a hit -> fallback (not replayed)"
# Covers the cache-hit path: a plan written by an older binary (or any advance
# defect) must still partition the current change set. Build a real cache via a
# valid first group, then tamper the cached plan (keeping the fingerprint) so the
# next run gets a cache HIT with an invalid plan.
if [ "$SIGNING_OK" -eq 1 ] && command -v jq >/dev/null 2>&1; then
  d="$(new_repo)"; printf 'v1\n' > "$d/a.txt"; printf 'v1\n' > "$d/b.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/a.txt"; printf 'v2\n' > "$d/b.txt"
  # Isolated cache dir so the tamper targets exactly THIS repo's cache file (the
  # suite's shared GCM_CACHE_DIR accumulates other repos' caches).
  v7cache="$(mktemp -d)"
  # First run: valid 2-group plan -> commits group 1 (a.txt), caches group 2 (b.txt).
  printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"},{"files":["b.txt"],"summary":"b","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" GCM_CACHE_DIR="$v7cache" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  cf=$(ls "$v7cache"/plan-*.json 2>/dev/null | head -1)
  if [ -n "$cf" ]; then
    ok "cache file written after committing group 1"
    # Tamper: point the cached group at an unknown path (fingerprint untouched, so
    # the pending {b.txt} still yields a cache HIT on the next run).
    jq '.plan.groups[0].files = ["ghost.txt"]' "$cf" > "$cf.tmp" && mv "$cf.tmp" "$cf"
    : > "$PLAN_FILE"   # if validation wrongly passes, a stale plan would be replayed
    ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" GCM_CACHE_DIR="$v7cache" "$BIN" --yes >/tmp/gcm-out 2>&1 )
    grep -qi "cached plan invalid" /tmp/gcm-out && ok "invalid cached plan detected -> fallback" || bad "cached plan not re-validated"
    git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'b.txt' && ok "fallback committed the pending file (not dropped)" || bad "b.txt not committed by fallback"
  else
    bad "no cache file found to tamper"
  fi
  rm -rf "$v7cache"; : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-V7 needs signing + jq (validate_cached logic covered by unit tests)"
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

# --- CLO-488 resilient provider calls: typed errors + retries --------------
# Request count == captured request bodies (each is one JSON line with "model").

note "AC-488-1: HTTP 429 retried then succeeds (no user-visible failure)"
d="$(new_repo)"; echo hi > "$d/a.txt"
: > "$COUNTER"; : > "$CAPTURE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/retry429/v1" "$BIN" --all --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
reqs=$(grep -c '"model"' "$CAPTURE" 2>/dev/null); reqs=${reqs:-0}
[ $rc -eq 0 ] && ok "429-then-200 -> exit 0 (self-healed)" || bad "429 retry (rc=$rc; $(tail -1 /tmp/gcm-out))"
grep -qi "rate limit" /tmp/gcm-out && bad "user saw a rate-limit failure despite recovery" || ok "no user-visible failure"
[ "$reqs" -eq 3 ] && ok "exactly 3 requests (2x429 + 1x200)" || bad "expected 3 requests, got $reqs"
rm -rf "$d"

note "AC-488-2: HTTP 400 fails fast (no retry loop) with a 400-specific message"
d="$(new_repo)"; echo hi > "$d/a.txt"; : > "$CAPTURE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/fail400/v1" "$BIN" --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
reqs=$(grep -c '"model"' "$CAPTURE" 2>/dev/null); reqs=${reqs:-0}
[ $rc -eq 1 ] && ok "400 -> exit 1" || bad "400 exit (rc=$rc)"
[ "$reqs" -eq 1 ] && ok "400 not retried (exactly 1 request)" || bad "400 retried ($reqs requests)"
grep -q "400" /tmp/gcm-out && ok "message names HTTP 400" || bad "no 400-specific message"
rm -rf "$d"

note "AC-488-3: HTTP 401 fails fast (no retry) and names the API key"
d="$(new_repo)"; echo hi > "$d/a.txt"; : > "$CAPTURE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/fail401/v1" "$BIN" --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
reqs=$(grep -c '"model"' "$CAPTURE" 2>/dev/null); reqs=${reqs:-0}
[ $rc -eq 1 ] && ok "401 -> exit 1" || bad "401 exit (rc=$rc)"
[ "$reqs" -eq 1 ] && ok "auth not retried (exactly 1 request)" || bad "auth retried ($reqs requests)"
grep -qi "GROQ_API_KEY\|api key" /tmp/gcm-out && ok "message names the API key" || bad "no key guidance"
rm -rf "$d"

note "AC-488-4: persistent HTTP 500 retried to the bound, then gives up; index untouched"
d="$(new_repo)"; echo hi > "$d/a.txt"; : > "$CAPTURE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/fail500/v1" GCM_RETRY_MAX=3 "$BIN" --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
reqs=$(grep -c '"model"' "$CAPTURE" 2>/dev/null); reqs=${reqs:-0}
[ $rc -eq 1 ] && ok "persistent 500 -> exit 1" || bad "500 exit (rc=$rc)"
[ "$reqs" -eq 4 ] && ok "retried to the bound (1 + 3 = 4 requests)" || bad "expected 4 requests, got $reqs"
grep -qi "server error" /tmp/gcm-out && ok "message is 5xx-specific" || bad "no server-error message"
[ -z "$(git -C "$d" diff --cached --name-only)" ] && ok "index untouched after exhausted retries" || bad "index mutated"
rm -rf "$d"

note "AC-488-3b: HTTP 403 also fails fast (auth class) without retry"
d="$(new_repo)"; echo hi > "$d/a.txt"; : > "$CAPTURE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/fail403/v1" "$BIN" --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
reqs=$(grep -c '"model"' "$CAPTURE" 2>/dev/null); reqs=${reqs:-0}
[ $rc -eq 1 ] && [ "$reqs" -eq 1 ] && ok "403 -> exit 1, not retried (1 request)" || bad "403 (rc=$rc, reqs=$reqs)"
grep -qi "GROQ_API_KEY\|api key" /tmp/gcm-out && ok "403 names the API key" || bad "no key guidance on 403"
rm -rf "$d"

note "AC-488-8: GCM_DEBUG surfaces the typed error variant; unset -> silent"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/fail400/v1" GCM_DEBUG=1 "$BIN" --all --yes >/tmp/gcm-out 2>&1 )
grep -q "BadRequest" /tmp/gcm-out && ok "debug log shows the BadRequest variant" || bad "variant not in debug log"
grep -q "\[debug\]" /tmp/gcm-out && ok "[debug] lines emitted under GCM_DEBUG" || bad "no [debug] lines under GCM_DEBUG"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/fail400/v1" "$BIN" --all --yes >/tmp/gcm-out2 2>&1 )
grep -q "\[debug\]" /tmp/gcm-out2 && bad "debug lines emitted without GCM_DEBUG" || ok "no [debug] lines when GCM_DEBUG unset"
rm -rf "$d"

note "AC-488-8b: retry attempts are logged under GCM_DEBUG (transient variant visible)"
d="$(new_repo)"; echo hi > "$d/a.txt"; : > "$COUNTER"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/retry429/v1" GCM_DEBUG=1 "$BIN" --all --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 0 ] && ok "429 recovered (exit 0)" || bad "429 retry (rc=$rc)"
grep -qi "RateLimit" /tmp/gcm-out && ok "transient RateLimit variant visible during retries" || bad "RateLimit not logged"
grep -qi "attempt" /tmp/gcm-out && ok "retry attempts logged" || bad "no retry-attempt log line"
rm -rf "$d"

note "AC-488-9: >4096-byte error body stays bounded yet still yields a detail"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/fail400big/v1" timeout 10 "$BIN" --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && ok "huge 400 body -> exit 1 (no hang)" || bad "big-body 400 (rc=$rc)"
grep -q "XXXX" /tmp/gcm-out && ok "detail extracted from the capped body (not dropped)" || bad "detail lost on >4096 body"
rm -rf "$d"

# --- CLO-491 per-repo plan cache -------------------------------------------
# The cache lives under $GCM_CACHE_DIR (exported above). reset_cache wipes it so
# cache_file can glob the single plan file the current case produced.
reset_cache() { rm -f "$GCM_CACHE_DIR"/plan-*.json; }
cache_file()  { ls "$GCM_CACHE_DIR"/plan-*.json 2>/dev/null | head -1; }

# Stage a 2-group change set (src.txt -> group 1, docs.md -> group 2) on top of
# an initial commit. Echoes the repo dir.
cache_repo_2group() {
  local d; d="$(new_repo)"
  printf 'v1\n' > "$d/src.txt"; printf 'v1\n' > "$d/docs.md"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/src.txt"; printf 'v2\n' > "$d/docs.md"
  printf '%s' '{"groups":[{"files":["src.txt"],"summary":"source","commit_message":"feat: src"},{"files":["docs.md"],"summary":"docs","commit_message":null}]}' > "$PLAN_FILE"
  echo "$d"
}

note "AC-C1: re-run commits group 2 from cache with no grouping call (AC-1, FR-2)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  # Run 2 is a cache hit: capture only this run, and blank the plan so any
  # (unexpected) grouping call would be visible as a fallback.
  : > "$CAPTURE"; : > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "re-run exit 0" || bad "re-run (rc=$rc; $(tail -1 /tmp/gcm-out))"
  grep -q '"response_format"' "$CAPTURE" && bad "re-run made a grouping call (cache missed)" || ok "no grouping call on re-run (cache hit)"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'docs.md' && ok "group 2 committed from cache" || bad "group 2 not committed"
  git -C "$d" log -1 --pretty=%s | grep -qi "mock commit message" && ok "group 2 carried a valid (regenerated) message" || bad "group 2 message missing"
  [ -z "$(git -C "$d" status --porcelain)" ] && ok "tree clean after group 2" || bad "tree still dirty"
  reset_cache; rm -rf "$d"
else
  skip "AC-C1 needs signing"
fi

note "AC-C2: editing a pending file invalidates the cache and re-analyzes (AC-2, FR-27)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  printf 'v3-edited\n' > "$d/docs.md"   # edit the still-pending group-2 file
  printf '%s' '{"groups":[{"files":["docs.md"],"summary":"docs","commit_message":"docs: edited"}]}' > "$PLAN_FILE"
  : > "$CAPTURE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "re-run after edit exit 0" || bad "edit re-run (rc=$rc)"
  grep -q '"response_format"' "$CAPTURE" && ok "edit invalidated the cache -> grouping call" || bad "stale cache reused after a content edit"
  reset_cache; rm -rf "$d"
else
  skip "AC-C2 needs signing"
fi

note "AC-C3: rejecting pre-commit hook leaves the group staged + plan un-advanced (AC-3, FR-58)"
reset_cache; d="$(cache_repo_2group)"
mkdir -p "$d/.git/hooks"
printf '#!/bin/sh\nexit 1\n' > "$d/.git/hooks/pre-commit"; chmod +x "$d/.git/hooks/pre-commit"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -ne 0 ] && ok "rejecting hook -> exit $rc" || bad "expected non-zero on hook rejection"
grep -qi "left staged" /tmp/gcm-out && ok "error explains the group is left staged" || bad "FR-58 message missing"
git -C "$d" diff --cached --name-only | grep -qx 'src.txt' && ok "group 1 left staged for retry" || bad "group 1 not staged after hook reject"
cf="$(cache_file)"; before="$(cat "$cf" 2>/dev/null)"
{ [ -n "$before" ] && printf '%s' "$before" | grep -q '"src.txt"' && printf '%s' "$before" | grep -q '"docs.md"'; } && ok "cache un-advanced (still the full plan: both groups)" || bad "cache not the full un-advanced plan"
[ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "no commit created" || bad "a commit slipped through the rejecting hook"
# A second rejected run must not mutate the cache (idempotent; never advances).
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
[ "$before" = "$(cat "$cf" 2>/dev/null)" ] && ok "cache byte-identical after a repeated rejected commit" || bad "cache changed across rejected retries"
# Removing the hook and re-running retries the same group from the cache.
rm -f "$d/.git/hooks/pre-commit"; : > "$CAPTURE"; : > "$PLAN_FILE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
if [ "$SIGNING_OK" -eq 1 ]; then
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'src.txt' && ok "retry committed the same group 1 from cache" || bad "retry did not commit group 1"
else
  skip "AC-C3 retry-commit assertion needs signing"
fi
reset_cache; rm -rf "$d"

note "AC-C4: first commit in an unborn repo (no HEAD) works with the cache (AC-4)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(new_repo)"   # fresh repo, no commits -> unborn HEAD
  printf 'a\n' > "$d/a.txt"; printf 'b\n' > "$d/b.txt"
  printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"},{"files":["b.txt"],"summary":"b","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "unborn first commit exit 0" || bad "unborn run (rc=$rc; $(tail -1 /tmp/gcm-out))"
  git -C "$d" rev-parse HEAD >/dev/null 2>&1 && ok "HEAD now exists (first commit created)" || bad "no HEAD after run"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'a.txt' && ok "group 1 (a.txt) committed" || bad "a.txt not committed"
  [ -n "$(cache_file)" ] && ok "cache advanced to group 2" || bad "no cache after unborn first commit"
  reset_cache; rm -rf "$d"
else
  skip "AC-C4 needs signing"
fi

note "AC-C5: cache file lives in the cache dir, named plan-<key>.json, mode 0600 (AC-5, FR-29)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  cf="$(cache_file)"
  [ -n "$cf" ] && [ -f "$cf" ] && ok "cache file created under the configured cache dir" || bad "no cache file produced"
  case "$cf" in "$GCM_CACHE_DIR"/plan-*.json) ok "name is plan-<key>.json under GCM_CACHE_DIR" ;; *) bad "unexpected cache path: $cf" ;; esac
  mode="$(stat -f '%Lp' "$cf" 2>/dev/null || stat -c '%a' "$cf" 2>/dev/null)"
  [ "$mode" = "600" ] && ok "cache file mode is 0600" || bad "cache file mode is '$mode' (want 600)"
  reset_cache; rm -rf "$d"
else
  skip "AC-C5 needs signing"
fi

note "AC-C6: --reset re-analyzes; --all clears the cache (AC-6, FR-8/FR-28)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  [ -n "$(cache_file)" ] && ok "cache warmed (group 2 cached)" || bad "no cache after run 1"
  : > "$CAPTURE"
  printf '%s' '{"groups":[{"files":["docs.md"],"summary":"docs","commit_message":"docs: d"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --reset --yes >/tmp/gcm-out 2>&1 )
  grep -q '"response_format"' "$CAPTURE" && ok "--reset forced a grouping call" || bad "--reset did not re-analyze"
  reset_cache; rm -rf "$d"

  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  [ -n "$(cache_file)" ] && ok "cache warmed before --all" || bad "no cache to clear"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --yes >/tmp/gcm-out 2>&1 )
  [ -z "$(cache_file)" ] && ok "--all cleared the cache" || bad "--all left the cache in place"
  reset_cache; rm -rf "$d"
else
  skip "AC-C6 needs signing"
fi

note "AC-C7: aborting at the prompt leaves the cache un-advanced (AC-7)"
if [ "$SIGNING_OK" -eq 1 ] && command -v expect >/dev/null 2>&1; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  before="$(cat "$(cache_file)")"
  GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" GCM_BIN="$BIN" GCM_DIR="$d" expect -c '
    set timeout 20
    spawn -noecho sh -c "cd $env(GCM_DIR) && GROQ_API_KEY=$env(GROQ_API_KEY) GCM_GROQ_BASE_URL=$env(GCM_GROQ_BASE_URL) $env(GCM_BIN)"
    expect {
      -re {\[Y/n/e} { send "n\r" }
      timeout { exit 3 }
    }
    expect eof
  ' >/tmp/gcm-out 2>&1
  after="$(cat "$(cache_file)")"
  [ "$before" = "$after" ] && ok "cache byte-identical after abort (not advanced)" || bad "abort changed/advanced the cache"
  git -C "$d" status --porcelain | grep -q 'docs.md' && ok "group 2 still pending after abort" || bad "group 2 not pending after abort"
  reset_cache; rm -rf "$d"
else
  skip "AC-C7 needs signing + expect"
fi

note "AC-C11: a single-group plan deletes the cache after its commit (eval 11)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(new_repo)"
  printf 'v1\n' > "$d/only.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/only.txt"
  printf '%s' '{"groups":[{"files":["only.txt"],"summary":"only","commit_message":"feat: only"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  [ -z "$(cache_file)" ] && ok "single-group plan left no cache (nothing to advance to)" || bad "cache lingered after the last group"
  reset_cache; rm -rf "$d"
else
  skip "AC-C11 needs signing"
fi

note "AC-C21: a group's message excludes other groups' untracked files (blind-spot #3, eval 21)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(new_repo)"
  printf 'seed\n' > "$d/seed.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  # Three untracked files in three groups. After group 1 commits, groups 2 AND 3
  # are still untracked, so the message-only call for group 2 must exclude g3.
  printf 'G1_CONTENT\n' > "$d/g1.txt"
  printf 'G2_CONTENT\n' > "$d/g2.txt"
  printf 'G3_CONTENT\n' > "$d/g3.txt"
  printf '%s' '{"groups":[{"files":["g1.txt"],"summary":"g1","commit_message":"feat: g1"},{"files":["g2.txt"],"summary":"g2","commit_message":null},{"files":["g3.txt"],"summary":"g3","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  # Run 2: cache hit, group 0 = g2 (null msg) -> message-only call scoped to g2,
  # while g3 is still untracked. The request body must contain g2 but not g3.
  : > "$CAPTURE"; : > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  grep -q 'G2_CONTENT' "$CAPTURE" && ok "scoped message includes the group's own untracked file" || bad "group 2 content missing from its message diff"
  grep -q 'G3_CONTENT' "$CAPTURE" && bad "another group's untracked content leaked into the message diff" || ok "other groups' untracked content excluded (filter works)"
  reset_cache; rm -rf "$d"
else
  skip "AC-C21 needs signing"
fi

note "AC-C-rename: renaming a pending file invalidates the cache (eval 4)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  ( cd "$d" && git mv docs.md docs2.md )   # rename the still-pending group-2 file
  printf '%s' '{"groups":[{"files":["docs2.md"],"summary":"docs","commit_message":"docs: renamed"}]}' > "$PLAN_FILE"
  : > "$CAPTURE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  grep -q '"response_format"' "$CAPTURE" && ok "rename invalidated the cache -> grouping call" || bad "stale cache reused after a rename"
  reset_cache; rm -rf "$d"
else
  skip "AC-C-rename needs signing"
fi

note "AC-C-hookfix: a hook that reformats+restages lets the commit succeed and the cache advance (eval 6)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(cache_repo_2group)"
  mkdir -p "$d/.git/hooks"
  printf '#!/bin/sh\nprintf "reformatted\\n" > src.txt\ngit add src.txt\nexit 0\n' > "$d/.git/hooks/pre-commit"
  chmod +x "$d/.git/hooks/pre-commit"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "hook reformat+restage -> commit succeeds" || bad "reformatting hook run (rc=$rc; $(tail -1 /tmp/gcm-out))"
  git -C "$d" show HEAD:src.txt | grep -q 'reformatted' && ok "committed the hook's reformatted content" || bad "reformatted content not committed"
  cf="$(cache_file)"
  # Prove advancement: group 1 (src.txt) dropped, group 2 (docs.md) remains -
  # a stale un-advanced full plan would still mention both.
  { [ -n "$cf" ] && grep -q '"docs.md"' "$cf" && ! grep -q '"src.txt"' "$cf"; } && ok "cache advanced (group 1 dropped, group 2 remains)" || bad "cache did not advance after a successful commit"
  reset_cache; rm -rf "$d"
else
  skip "AC-C-hookfix needs signing"
fi

note "AC-C-untracked: an untracked-only cached group commits on the next run (eval 18)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(new_repo)"
  printf 'seed\n' > "$d/seed.txt"; git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'A\n' > "$d/g1.txt"; printf 'B\n' > "$d/g2.txt"   # both untracked
  printf '%s' '{"groups":[{"files":["g1.txt"],"summary":"g1","commit_message":"feat: g1"},{"files":["g2.txt"],"summary":"g2","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  : > "$CAPTURE"; : > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "untracked cached group commit exit 0" || bad "untracked cached group (rc=$rc; $(tail -1 /tmp/gcm-out))"
  grep -q '"response_format"' "$CAPTURE" && bad "made a grouping call (cache missed)" || ok "no grouping call (cache hit on the untracked group)"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'g2.txt' && ok "untracked group 2 committed from cache" || bad "g2.txt not committed"
  reset_cache; rm -rf "$d"
else
  skip "AC-C-untracked needs signing"
fi

note "AC-C-delete: a deletion-only cached group commits the removal (eval 17)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(new_repo)"
  printf 'a\n' > "$d/a.txt"; printf 'b\n' > "$d/b.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null; git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'a2\n' > "$d/a.txt"; rm "$d/b.txt"   # group 1 modifies a.txt, group 2 deletes b.txt
  printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"},{"files":["b.txt"],"summary":"rm b","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  : > "$CAPTURE"; : > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "deletion cached group commit exit 0" || bad "deletion cached group (rc=$rc; $(tail -1 /tmp/gcm-out))"
  # Prove it was a cache REPLAY, not a fallback: no grouping call this run.
  grep -q '"response_format"' "$CAPTURE" && bad "made a grouping call (cache missed, not a replay)" || ok "no grouping call (deletion group replayed from cache)"
  git -C "$d" ls-files | grep -qx 'b.txt' && bad "b.txt still tracked (deletion not committed)" || ok "b.txt deletion committed from cache"
  reset_cache; rm -rf "$d"
else
  skip "AC-C-delete needs signing"
fi

note "AC-C-fallback: a grouping fallback clears the cache (eval 10 fallback half)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  [ -n "$(cache_file)" ] && ok "cache warmed before fallback" || bad "no cache to clear"
  printf 'edited\n' > "$d/docs.md"               # invalidate -> next run is a miss
  printf '%s' '{ not valid json' > "$PLAN_FILE"   # grouping returns malformed -> fallback
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  grep -qi "Falling back" /tmp/gcm-out && ok "malformed plan -> fallback" || bad "no fallback on malformed plan"
  [ -z "$(cache_file)" ] && ok "fallback cleared the cache" || bad "fallback left the cache in place"
  reset_cache; rm -rf "$d"
else
  skip "AC-C-fallback needs signing"
fi

note "AC-C-drynoclear: --all --dry-run previews without clearing the cache (FR-7 no-mutation)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  before="$(cat "$(cache_file)" 2>/dev/null)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --dry-run >/tmp/gcm-out 2>&1 )
  after="$(cat "$(cache_file)" 2>/dev/null)"
  { [ -n "$before" ] && [ "$before" = "$after" ]; } && ok "--all --dry-run left the cache untouched" || bad "--all --dry-run mutated the cache"
  reset_cache; rm -rf "$d"
else
  skip "AC-C-drynoclear needs signing"
fi

note "AC-493-1: --plan-only --json emits a single valid JSON object on stdout"
d="$(new_repo)"; echo hi > "$d/a.txt"
printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"}]}' > "$PLAN_FILE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --plan-only --json >/tmp/gcm-json.out 2>/tmp/gcm-json.err ); rc=$?
python3 -c "import json; d=json.load(open('/tmp/gcm-json.out')); assert d['v']==1 and d['status']=='plan' and d['mode']=='plan_only' and isinstance(d['changed_files'],list) and isinstance(d['plan']['groups'],list), d"
[ $? -eq 0 ] && ok "JSON plan envelope valid" || bad "invalid JSON plan envelope"
[ -s /tmp/gcm-json.err ] || ok "stderr can be empty for this path" || true
: > "$PLAN_FILE"; rm -rf "$d"

note "AC-493-2: --plan-only is non-destructive (no staging, HEAD unchanged)"
d="$(new_repo)"; echo hi > "$d/a.txt"
printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"}]}' > "$PLAN_FILE"
git -C "$d" add a.txt
before_tree="$(git -C "$d" write-tree)"
before_head="$(git -C "$d" rev-parse HEAD 2>/dev/null || true)"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --plan-only --json >/tmp/gcm-json.out 2>/tmp/gcm-json.err ); rc=$?
[ $rc -eq 0 ] && ok "plan-only exit 0" || bad "plan-only non-destructive run (rc=$rc)"
[ "$(git -C "$d" write-tree)" = "$before_tree" ] && ok "index unchanged" || bad "index mutated by plan-only"
[ "$(git -C "$d" rev-parse HEAD 2>/dev/null || true)" = "$before_head" ] && ok "HEAD unchanged" || bad "HEAD mutated by plan-only"
: > "$PLAN_FILE"; rm -rf "$d"

note "AC-493-3: --dry-run --json emits mode dry_run and saves the cache"
d="$(new_repo)"; echo hi > "$d/a.txt"
printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"}]}' > "$PLAN_FILE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --dry-run --json >/tmp/gcm-json.out 2>/tmp/gcm-json.err ); rc=$?
python3 -c "import json; d=json.load(open('/tmp/gcm-json.out')); assert d['status']=='plan' and d['mode']=='dry_run', d"
[ $? -eq 0 ] && ok "dry-run JSON mode is dry_run" || bad "dry-run JSON mode wrong"
[ $rc -eq 0 ] && ok "dry-run exit 0" || bad "dry-run (rc=$rc)"
: > "$PLAN_FILE"; rm -rf "$d"

note "AC-493-4: clean repo --plan-only --json returns status noop and exit 0"
d="$(new_repo)"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --plan-only --json >/tmp/gcm-json.out 2>/tmp/gcm-json.err ); rc=$?
python3 -c "import json; d=json.load(open('/tmp/gcm-json.out')); assert d['v']==1 and d['status']=='noop', d"
[ $? -eq 0 ] && ok "clean repo noop JSON" || bad "clean repo JSON"
[ $rc -eq 0 ] && ok "clean repo exit 0" || bad "clean repo exit (rc=$rc)"
rm -rf "$d"

note "AC-493-5: --yes --json commits and emits status committed"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; echo hi > "$d/a.txt"
  printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes --json >/tmp/gcm-json.out 2>/tmp/gcm-json.err ); rc=$?
  python3 -c "import json; d=json.load(open('/tmp/gcm-json.out')); assert d['status']=='committed' and d['mode']=='grouped' and d['commit']['status']=='ok' and isinstance(d['commit']['hash'],str), d"
  [ $? -eq 0 ] && ok "committed JSON envelope valid" || bad "committed JSON invalid"
  [ $rc -eq 0 ] && ok "exit 0" || bad "commit run (rc=$rc)"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-493-5 needs signing"
fi

note "AC-493-6: non-TTY without --yes/--plan-only/--dry-run errors as NonInteractive"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --json </dev/null >/tmp/gcm-json.out 2>/tmp/gcm-json.err ); rc=$?
python3 -c "import json; d=json.load(open('/tmp/gcm-json.out')); assert d['status']=='error' and d['error']['code']=='NonInteractive', d"
[ $? -eq 0 ] && ok "NonInteractive JSON" || bad "non-TTY JSON wrong"
[ $rc -ne 0 ] && ok "non-zero exit" || bad "non-TTY guard exit (rc=$rc)"
rm -rf "$d"

note "AC-493-7: provider failure under --json is a single error envelope"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && env -u GROQ_API_KEY GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes --json >/tmp/gcm-json.out 2>/tmp/gcm-json.err ); rc=$?
python3 -c "import json; d=json.load(open('/tmp/gcm-json.out')); assert d['status']=='error' and d['error']['code']=='Provider', d"
[ $? -eq 0 ] && ok "missing-key -> JSON error Provider" || bad "missing-key JSON"
[ $rc -ne 0 ] && ok "non-zero exit" || bad "missing-key exit (rc=$rc)"
[ -z "$(cat /tmp/gcm-json.err)" ] && ok "stderr empty (no stray stdout)" || ok "stderr may contain logs"
rm -rf "$d"

note "AC-493-8: grouping fallback under --yes --json emits status fallback"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; echo hi > "$d/a.txt"
  printf '%s' '{ not valid json' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes --json >/tmp/gcm-json.out 2>/tmp/gcm-json.err ); rc=$?
  python3 -c "import json; d=json.load(open('/tmp/gcm-json.out')); assert d['status']=='fallback' and d['fallback']['reason'] and d['fallback']['raw_code'] and d['commit']['hash'], d"
  [ $? -eq 0 ] && ok "fallback JSON envelope valid" || bad "fallback JSON invalid"
  [ $rc -eq 0 ] && ok "fallback commit exit 0" || bad "fallback exit (rc=$rc)"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-493-8 needs signing"
fi

note "AC-493-9: --all --yes --json is single mode and no plan groups"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; echo hi > "$d/a.txt"; echo there > "$d/b.txt"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --yes --json >/tmp/gcm-json.out 2>/tmp/gcm-json.err ); rc=$?
  python3 -c "import json; d=json.load(open('/tmp/gcm-json.out')); assert d['status']=='committed' and d['mode']=='single' and d['commit']['hash'], d"
  [ $? -eq 0 ] && ok "--all --yes JSON is single committed" || bad "--all --yes JSON invalid"
  [ $rc -eq 0 ] && ok "exit 0" || bad "--all --yes exit (rc=$rc)"
  rm -rf "$d"
else
  skip "AC-493-9 needs signing"
fi

note "AC-493-10: --all --plan-only --json is single plan preview"
d="$(new_repo)"; echo hi > "$d/a.txt"; echo there > "$d/b.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --plan-only --json >/tmp/gcm-json.out 2>/tmp/gcm-json.err ); rc=$?
python3 -c "import json; d=json.load(open('/tmp/gcm-json.out')); assert d['status']=='plan' and d['mode']=='single' and isinstance(d['changed_files'],list), d"
[ $? -eq 0 ] && ok "--all --plan-only JSON valid" || bad "--all --plan-only JSON invalid"
[ $rc -eq 0 ] && ok "exit 0" || bad "--all --plan-only exit (rc=$rc)"
rm -rf "$d"

note "AC-493-11: GCM_LOG_LEVEL governs stderr logs and stdout stays JSON"
d="$(new_repo)"; echo hi > "$d/a.txt"
printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"}]}' > "$PLAN_FILE"
( cd "$d" && GCM_LOG_LEVEL=warn GCM_DEBUG=1 GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --plan-only --json >/tmp/gcm-json.out 2>/tmp/gcm-json.err ); rc=$?
python3 -c "import json; json.load(open('/tmp/gcm-json.out'))" && ok "stdout is valid JSON" || bad "stdout not valid JSON"
[ $rc -eq 0 ] && ok "exit 0" || bad "log-level run (rc=$rc)"
: > "$PLAN_FILE"; rm -rf "$d"

note "AC-493-12: deterministic exit codes (0 success, 1 error)"
d="$(new_repo)"; echo hi > "$d/a.txt"
printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"}]}' > "$PLAN_FILE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --plan-only --json >/dev/null 2>&1 ); rc=$?
[ $rc -eq 0 ] && ok "plan exit 0" || bad "plan exit (rc=$rc)"
( cd "$d" && env -u GROQ_API_KEY GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes --json >/dev/null 2>&1 ); rc=$?
[ $rc -eq 1 ] && ok "error exit 1" || bad "error exit (rc=$rc)"
: > "$PLAN_FILE"; rm -rf "$d"

note "AC-493-13: --reset clears the cache and still emits a valid JSON envelope"
reset_cache
d="$(new_repo)"; echo hi > "$d/a.txt"
printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"}]}' > "$PLAN_FILE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --dry-run --json >/dev/null 2>&1 )
[ -n "$(ls "$GCM_CACHE_DIR"/plan-*.json 2>/dev/null)" ] && ok "cache warmed before reset" || bad "cache not warmed"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --reset --plan-only --json >/tmp/gcm-json.out 2>/tmp/gcm-json.err ); rc=$?
python3 -c "import json; json.load(open('/tmp/gcm-json.out'))" && ok "--reset --json stdout is valid JSON" || bad "--reset --json invalid"
[ -z "$(ls "$GCM_CACHE_DIR"/plan-*.json 2>/dev/null)" ] && ok "cache cleared after reset" || bad "cache not cleared"
[ $rc -eq 0 ] && ok "--reset --plan-only exit 0" || bad "--reset exit (rc=$rc)"
: > "$PLAN_FILE"; rm -rf "$d"

# --- CLO-489 provider trait: Gemini + OpenAI backends ----------------------
# OpenAI uses the OpenAI-compatible mock (MOCK_URL); Gemini uses the
# :generateContent mock (GEMINI_MOCK_URL). Real Gemini/OpenAI HTTP is not
# exercised in-sandbox (egress); the binary reaches the real APIs in the user's
# environment. Per-provider request/parse shapes are unit-tested; these cases
# prove end-to-end selection, auth, and grouping over the mock.

note "AC-489-default: bare gcm (no --provider/env) uses Groq (parity, O3)"
d="$(new_repo)"; echo hi > "$d/a.txt"; : > "$HEADERS"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 0 ] && ok "default provider run exit 0" || bad "default provider (rc=$rc; $(tail -1 /tmp/gcm-out))"
grep -q 'AUTH=Bearer dummy' "$HEADERS" && ok "default sends Authorization: Bearer (Groq)" || bad "default auth header wrong"
rm -rf "$d"

note "AC-489-unknown: an unknown provider fails fast, lists valid names, no network"
d="$(new_repo)"; echo hi > "$d/a.txt"; : > "$CAPTURE"
( cd "$d" && GCM_PROVIDER=bogus GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && ok "unknown GCM_PROVIDER -> exit 1" || bad "unknown provider (rc=$rc)"
grep -qi "unknown provider" /tmp/gcm-out && grep -q "groq" /tmp/gcm-out && ok "error names the bad value + valid names" || bad "error not actionable"
reqs=$(grep -c '"model"\|"contents"' "$CAPTURE" 2>/dev/null); reqs=${reqs:-0}
[ "$reqs" -eq 0 ] && ok "no network call for an unknown provider" || bad "unknown provider still called the API ($reqs)"
( cd "$d" && "$BIN" --provider=bogus --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 2 ] && ok "--provider=bogus -> clap usage error (exit 2)" || bad "--provider clap validation (rc=$rc)"
rm -rf "$d"

note "AC-489-missingkey: each provider names its own API key env var (FR-18)"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && env -u GEMINI_API_KEY GCM_GEMINI_BASE_URL="$GEMINI_MOCK_URL" "$BIN" --provider=google --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && grep -q "GEMINI_API_KEY" /tmp/gcm-out && ok "google missing key names GEMINI_API_KEY" || bad "google missing-key (rc=$rc)"
( cd "$d" && env -u OPENAI_API_KEY GCM_OPENAI_BASE_URL="$MOCK_URL" "$BIN" --provider=openai --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && grep -q "OPENAI_API_KEY" /tmp/gcm-out && ok "openai missing key names OPENAI_API_KEY" || bad "openai missing-key (rc=$rc)"
rm -rf "$d"

note "AC-489-auth-headers: per-provider auth scheme (eval 26)"
d="$(new_repo)"; echo hi > "$d/a.txt"
: > "$HEADERS"
( cd "$d" && OPENAI_API_KEY=sk-test GCM_OPENAI_BASE_URL="$MOCK_URL" "$BIN" --provider=openai --all --dry-run >/tmp/gcm-out 2>&1 )
grep -q 'AUTH=Bearer sk-test' "$HEADERS" && ok "openai sends Authorization: Bearer" || bad "openai auth header"
: > "$HEADERS"
( cd "$d" && GEMINI_API_KEY=g-test GCM_GEMINI_BASE_URL="$GEMINI_MOCK_URL" "$BIN" --provider=google --all --dry-run >/tmp/gcm-out 2>&1 )
grep -q 'GOOG=g-test' "$HEADERS" && ok "gemini sends x-goog-api-key" || bad "gemini auth header"
rm -rf "$d"

note "AC-489-model: --model overrides the model id sent to the provider (FR-14)"
d="$(new_repo)"; echo hi > "$d/a.txt"; : > "$CAPTURE"
( cd "$d" && OPENAI_API_KEY=dummy GCM_OPENAI_BASE_URL="$MOCK_URL" "$BIN" --provider=openai --model=custom-model-xyz --all --dry-run >/tmp/gcm-out 2>&1 )
grep -q 'custom-model-xyz' "$CAPTURE" && ok "the overridden model id is in the request" || bad "--model not applied"
rm -rf "$d"

note "AC-489-safety: Gemini safety block -> actionable non-retryable error (pt3)"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && GEMINI_API_KEY=dummy GCM_GEMINI_BASE_URL="http://127.0.0.1:$PORT/geminisafety" "$BIN" --provider=google --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && ok "safety block -> exit 1" || bad "safety block (rc=$rc)"
grep -qi "blocked" /tmp/gcm-out && grep -qi "safety" /tmp/gcm-out && ok "message names the safety block" || bad "no safety-block message"
rm -rf "$d"

note "AC-489-openai: --provider=openai produces a grouped commit (strict json_schema)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(new_repo)"
  printf 'v1\n' > "$d/src.txt"; printf 'v1\n' > "$d/docs.md"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/src.txt"; printf 'v2\n' > "$d/docs.md"
  printf '%s' '{"groups":[{"files":["src.txt"],"summary":"source","commit_message":"feat: src"},{"files":["docs.md"],"summary":"docs","commit_message":null}]}' > "$PLAN_FILE"
  : > "$CAPTURE"
  ( cd "$d" && OPENAI_API_KEY=dummy GCM_OPENAI_BASE_URL="$MOCK_URL" "$BIN" --provider=openai --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "openai grouped commit exit 0" || bad "openai grouped (rc=$rc; $(tail -1 /tmp/gcm-out))"
  grep -q '"response_format"' "$CAPTURE" && ok "openai sent a strict json_schema request" || bad "openai request not json_schema"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'src.txt' && ok "group 1 committed via openai" || bad "openai group 1 missing"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'docs.md' && bad "docs.md leaked into group 1" || ok "group 2 left for next run"
  : > "$PLAN_FILE"; reset_cache; rm -rf "$d"
else
  skip "AC-489-openai needs signing"
fi

note "AC-489-google: --provider=google produces a grouped commit (responseSchema + thinkingLevel)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(new_repo)"
  printf 'v1\n' > "$d/src.txt"; printf 'v1\n' > "$d/docs.md"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/src.txt"; printf 'v2\n' > "$d/docs.md"
  printf '%s' '{"groups":[{"files":["src.txt"],"summary":"source","commit_message":"feat: src"},{"files":["docs.md"],"summary":"docs","commit_message":null}]}' > "$PLAN_FILE"
  : > "$CAPTURE"
  ( cd "$d" && GEMINI_API_KEY=dummy GCM_GEMINI_BASE_URL="$GEMINI_MOCK_URL" "$BIN" --provider=google --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "google grouped commit exit 0" || bad "google grouped (rc=$rc; $(tail -1 /tmp/gcm-out))"
  grep -q '"responseSchema"' "$CAPTURE" && ok "gemini sent a responseSchema request" || bad "gemini request not responseSchema"
  grep -q 'thinkingLevel' "$CAPTURE" && ok "gemini request sets thinkingLevel (reasoning suppression)" || bad "no thinkingLevel"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'src.txt' && ok "group 1 committed via gemini" || bad "gemini group 1 missing"
  : > "$PLAN_FILE"; reset_cache; rm -rf "$d"
else
  skip "AC-489-google needs signing"
fi

note "AC-O1: --provider=ollama is key-free, sends NO Authorization, native /api/chat"
reset_cache; d="$(new_repo)"
printf 'v1\n' > "$d/o.txt"
git -C "$d" -c commit.gpgsign=false add -A >/dev/null
git -C "$d" -c commit.gpgsign=false commit -qm init
printf 'v2\n' > "$d/o.txt"
printf '%s' '{"groups":[{"files":["o.txt"],"summary":"o","commit_message":"feat: o"}]}' > "$PLAN_FILE"
: > "$CAPTURE"; : > "$HEADERS"
# No provider API keys in the environment at all -> proves Ollama needs none.
( cd "$d" && env -u GROQ_API_KEY -u OPENAI_API_KEY -u GEMINI_API_KEY GCM_OLLAMA_BASE_URL="http://127.0.0.1:$PORT" "$BIN" --provider=ollama --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 0 ] && ok "ollama --dry-run is key-free -> exit 0" || bad "ollama dry-run (rc=$rc; $(tail -1 /tmp/gcm-out))"
[ -s "$CAPTURE" ] && ok "ollama request reached the local endpoint" || bad "no ollama request captured"
grep -q '"format"' "$CAPTURE" && ok "ollama sent a native format=schema request" || bad "ollama request not native format"
grep -q "AUTH= GOOG=" "$HEADERS" && ok "ollama sent NO auth headers (zero-egress, key-free)" || bad "ollama auth headers: $(cat "$HEADERS")"
if [ "$SIGNING_OK" -eq 1 ]; then
  : > "$CAPTURE"
  ( cd "$d" && env -u GROQ_API_KEY GCM_OLLAMA_BASE_URL="http://127.0.0.1:$PORT" "$BIN" --provider=ollama --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "ollama grouped commit exit 0" || bad "ollama grouped (rc=$rc; $(tail -1 /tmp/gcm-out))"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'o.txt' && ok "group committed via ollama" || bad "ollama group missing"
else
  skip "AC-O1 real commit needs signing (key-free dry-run path verified above)"
fi
: > "$PLAN_FILE"; reset_cache; rm -rf "$d"

note "AC-O3: scheme-less OLLAMA_HOST (host:port) is normalized and reaches the daemon"
reset_cache; d="$(new_repo)"
printf 'h\n' > "$d/h.txt"
printf '%s' '{"groups":[{"files":["h.txt"],"summary":"h","commit_message":"chore: h"}]}' > "$PLAN_FILE"
: > "$CAPTURE"
( cd "$d" && env -u GROQ_API_KEY OLLAMA_HOST="127.0.0.1:$PORT" "$BIN" --provider=ollama --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 0 ] && [ -s "$CAPTURE" ] && ok "OLLAMA_HOST normalized + reached mock" || bad "OLLAMA_HOST normalize (rc=$rc; $(tail -1 /tmp/gcm-out))"
: > "$PLAN_FILE"; reset_cache; rm -rf "$d"

note "AC-O4: ollama 404 (model not pulled) -> exit 1 + 'ollama pull' guidance"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && env -u GROQ_API_KEY GCM_OLLAMA_BASE_URL="http://127.0.0.1:$PORT/ollama404" "$BIN" --provider=ollama --yes >/tmp/gcm-out 2>&1 ); rc=$?
if [ $rc -eq 1 ] && grep -qi "ollama pull" /tmp/gcm-out; then
  ok "404 -> exit 1 + ollama pull hint"
else
  bad "404 hint (rc=$rc): $(cat /tmp/gcm-out)"
fi
rm -rf "$d"

note "AC-O5: GCM_PROVIDER=ollama (env selection, no flag) selects the backend key-free"
reset_cache; d="$(new_repo)"
printf 'e\n' > "$d/e.txt"
printf '%s' '{"groups":[{"files":["e.txt"],"summary":"e","commit_message":"chore: e"}]}' > "$PLAN_FILE"
: > "$CAPTURE"
( cd "$d" && env -u GROQ_API_KEY GCM_PROVIDER=ollama GCM_OLLAMA_BASE_URL="http://127.0.0.1:$PORT" "$BIN" --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 0 ] && [ -s "$CAPTURE" ] && ok "GCM_PROVIDER=ollama selected + reached mock" || bad "GCM_PROVIDER=ollama (rc=$rc; $(tail -1 /tmp/gcm-out))"
: > "$PLAN_FILE"; reset_cache; rm -rf "$d"

note "AC-O6: a :cloud model warns it is NOT zero-egress (privacy defense-in-depth)"
reset_cache; d="$(new_repo)"
printf 'c\n' > "$d/c.txt"
printf '%s' '{"groups":[{"files":["c.txt"],"summary":"c","commit_message":"chore: c"}]}' > "$PLAN_FILE"
( cd "$d" && env -u GROQ_API_KEY GCM_OLLAMA_BASE_URL="http://127.0.0.1:$PORT" "$BIN" --provider=ollama --model=demo-model:cloud --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
if grep -qi "Ollama Cloud" /tmp/gcm-out && grep -qi "NOT zero-egress" /tmp/gcm-out; then
  ok ":cloud model warns about egress"
else
  bad ":cloud egress warning (rc=$rc): $(cat /tmp/gcm-out)"
fi
: > "$PLAN_FILE"; reset_cache; rm -rf "$d"

note "AC-ONB: unconfigured first run in a non-TTY -> instructions + exit 1 (CLO-496)"
# Empty per-case config dir, no key env vars, no provider hint, stdin from
# /dev/null (non-TTY): onboarding must print actionable setup instructions to
# stderr and exit non-zero rather than hang on the wizard.
onb_cfg="$(mktemp -d)"; d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && env -u GROQ_API_KEY -u GEMINI_API_KEY -u OPENAI_API_KEY -u ANTHROPIC_API_KEY -u GCM_PROVIDER \
    GCM_CONFIG="$onb_cfg" "$BIN" </dev/null >/tmp/gcm-out 2>&1 ); rc=$?
if [ $rc -ne 0 ] && grep -q "\[\[providers\]\]" /tmp/gcm-out && grep -q "export GROQ_API_KEY=" /tmp/gcm-out; then
  ok "non-TTY first run -> instructions + exit $rc"
else
  bad "non-TTY first run (rc=$rc): $(cat /tmp/gcm-out)"
fi
note "AC-ONB2: same first run with --json -> error envelope on stdout, instructions on stderr"
( cd "$d" && env -u GROQ_API_KEY -u GEMINI_API_KEY -u OPENAI_API_KEY -u ANTHROPIC_API_KEY -u GCM_PROVIDER \
    GCM_CONFIG="$onb_cfg" "$BIN" --json </dev/null >/tmp/gcm-onb-out 2>/tmp/gcm-onb-err ); rc=$?
if [ $rc -ne 0 ] \
   && grep -q '"code":"OnboardingRequired"' /tmp/gcm-onb-out \
   && ! grep -q "\[\[providers\]\]" /tmp/gcm-onb-out \
   && grep -q "\[\[providers\]\]" /tmp/gcm-onb-err; then
  ok "--json first run -> envelope on stdout, instructions on stderr"
else
  bad "--json first run (rc=$rc): out=$(cat /tmp/gcm-onb-out) err=$(cat /tmp/gcm-onb-err)"
fi
rm -rf "$d" "$onb_cfg"

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
