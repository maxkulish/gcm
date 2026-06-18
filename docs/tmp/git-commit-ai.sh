#!/usr/bin/env bash
set -euo pipefail

# Git commit with AI-generated message and intelligent grouping
# Usage: git-commit-ai.sh [--dry-run] [--all] [--reset] [--provider haiku|cerebras|groq|google]
#   Groq model override: GCM_GROQ_MODEL=<id> (default openai/gpt-oss-120b; e.g. openai/gpt-oss-20b, qwen/qwen3.6-27b)

# ── Phase 0: Args & checks ──────────────────────────────────────────

DRY_RUN=false
ALL_MODE=false
PROVIDER="${GCM_PROVIDER:-haiku}"
RESET_CACHE=false

for arg in "$@"; do
    case "$arg" in
        --dry-run)          DRY_RUN=true ;;
        --all)              ALL_MODE=true ;;
        --reset)            RESET_CACHE=true ;;
        --provider=*)       PROVIDER="${arg#--provider=}" ;;
        --provider)         echo "Error: --provider requires a value (haiku, cerebras, groq)" >&2; exit 1 ;;
        *)                  echo "Unknown option: $arg" >&2; exit 1 ;;
    esac
done

# ── LLM dispatch ─────────────────────────────────────────────────────

PROVIDER_LABEL=""
case "$PROVIDER" in
    haiku)
        PROVIDER_LABEL="Claude Haiku"
        llm_call() { claude --model haiku -p "$1" 2>/dev/null; }
        ;;
    cerebras)
        PROVIDER_LABEL="Cerebras Qwen3 235B"
        llm_call() { echo "$1" | mods -a cerebras -m qwen-3-235b-a22b-instruct-2507 -r --no-cache -q 2>/dev/null; }
        ;;
    groq)
        # GCM_GROQ_MODEL selects the Groq model (gcmq=default, gcmq20, gcmq27 set it)
        GROQ_MODEL="${GCM_GROQ_MODEL:-openai/gpt-oss-120b}"
        PROVIDER_LABEL="Groq ${GROQ_MODEL}"
        # qwen3.6 is a thinking model and prints <think>…</think> to stdout; strip it so it
        # pollutes neither the JSON plan nor the commit message (no-op for the gpt-oss models)
        llm_call() { echo "$1" | mods -a groq -m "$GROQ_MODEL" -r --no-cache -q 2>/dev/null | perl -0777 -pe 's{<think>.*?</think>\s*}{}gs'; }
        ;;
    google)
        PROVIDER_LABEL="Gemini 3.1 Flash Lite"
        llm_call() { echo "$1" | mods -a google -m gemini-3.1-flash-lite -r --no-cache -q 2>/dev/null; }
        ;;
    *)
        echo "Error: Unknown provider '$PROVIDER' (use haiku, cerebras, groq, or google)" >&2
        exit 1
        ;;
esac

if ! git rev-parse --is-inside-work-tree &>/dev/null; then
    echo "Error: Not a git repository" >&2
    exit 1
fi

# Ensure all paths are consistent — porcelain output, diffs, and filesystem
# operations all use repo-root-relative paths
cd "$(git rev-parse --show-toplevel)"

# Per-repo LLM plan cache (provider-agnostic: gcm/gcmq/gcmc share the same plan)
_CACHE_KEY=$(printf '%s' "$(git rev-parse --show-toplevel)" | shasum -a 256 | cut -d' ' -f1 | head -c 16)
GCM_CACHE_FILE="/tmp/gcm-plan-${_CACHE_KEY}.json"
if $RESET_CACHE && [[ -f "$GCM_CACHE_FILE" ]]; then
    rm -f "$GCM_CACHE_FILE"
    echo "🗑  Cache cleared, re-analyzing..."
fi

if git diff --quiet && git diff --cached --quiet && [[ -z "$(git ls-files --others --exclude-standard)" ]]; then
    echo "No changes to commit"
    exit 0
fi

# ── Helpers ──────────────────────────────────────────────────────────

# Sanitize git diff output for LLM consumption.
# Git's text/binary heuristic only checks the first 8000 bytes for NULs, so a
# file (e.g. a PDF with large XMP metadata up front) can be misclassified as
# text and dump megabytes of binary as a "+" diff. That output contains NULs
# (which bash command-substitution strips, warning each time) and wastes the
# LLM's context. Per-file: if the body is binary-looking, elide it while
# preserving the file header so the LLM still sees that the file changed.
_safe_diff() {
    git diff "$@" 2>/dev/null | LC_ALL=C perl -e '
        use strict; use warnings;
        my $buf = "";
        sub flush {
            my ($b) = @_;
            return unless length $b;
            my @lines = split /\n/, $b, -1;
            my (@header, $body, $in_body); $body = ""; $in_body = 0;
            for my $l (@lines) {
                if (!$in_body && $l =~ /^@@/) { $in_body = 1; }
                if ($in_body) { $body .= $l . "\n"; }
                else { push @header, $l; }
            }
            my $sample = $body;
            $sample =~ s/^[+\- ]//mg;
            my $total = length $sample;
            my $non = () = $sample =~ /[^\x09\x0A\x20-\x7E]/g;
            if ($total > 200 && $non / $total > 0.10) {
                print join("\n", @header), "\n";
                print "Binary files differ (body elided: ${total} bytes, ", scalar(@lines), " diff lines)\n";
            } else {
                $b =~ s/\x00//g;
                print $b;
            }
        }
        while (<STDIN>) {
            if (/^diff --git /) { flush($buf); $buf = $_; }
            else { $buf .= $_; }
        }
        flush($buf);
    '
}

# Fallback: stage everything, generate single commit message (original behavior)
fallback_single_commit() {
    local reason="${1:-}"
    # Clear cache: committing all files in one shot invalidates any saved plan
    [[ -f "${GCM_CACHE_FILE:-}" ]] && rm -f "$GCM_CACHE_FILE"
    [[ -n "$reason" ]] && echo "⚠️  $reason" && echo ""

    echo "📦 Staging all changes..."
    git add -A

    local diff_stat diff_full
    diff_stat=$(git diff --cached --stat)
    diff_full=$(_safe_diff --cached)

    if [[ -z "$diff_stat" ]]; then
        echo "No staged changes to commit"
        exit 0
    fi

    echo "📝 Generating commit message ($PROVIDER_LABEL)..."

    local commit_msg
    commit_msg=$(llm_call "Analyze this git diff and generate a concise, conventional commit message.
Use format: <type>(<scope>): <description>
Types: feat, fix, docs, style, refactor, test, chore
Keep the first line under 72 characters.
Add a blank line and bullet points for details if there are multiple significant changes.
Do NOT include any explanation - output ONLY the commit message.

Diff stats:
$diff_stat

Full diff:
$diff_full")

    if [[ -z "$commit_msg" ]]; then
        echo "Error: Failed to generate commit message" >&2
        exit 1
    fi

    echo ""
    echo "💬 Commit message:"
    echo "─────────────────────────────"
    echo "$commit_msg"
    echo "─────────────────────────────"
    echo ""

    if $DRY_RUN; then
        echo "[Dry run] Would commit with the above message"
        git reset HEAD &>/dev/null
        exit 0
    fi

    read -r -p "Commit with this message? [Y/n/e(dit)] " response
    case "$response" in
        [nN])
            echo "Aborted. Changes remain staged."
            exit 0
            ;;
        [eE])
            local tmpfile
            tmpfile=$(mktemp)
            echo "$commit_msg" > "$tmpfile"
            ${EDITOR:-vim} "$tmpfile"
            commit_msg=$(cat "$tmpfile")
            rm "$tmpfile"
            ;;
    esac

    git commit -S -m "$commit_msg"
    echo ""
    echo "✅ Committed successfully!"
    exit 0
}

# ── Phase 1: Gather context (NO staging) ────────────────────────────

echo "📋 Current changes:"
git status --short
echo ""

# --all bypasses grouping entirely
if $ALL_MODE; then
    fallback_single_commit
fi

# Get porcelain status for file list
STATUS=$(git status --porcelain)

# Collect all changed file paths (using NEW name for renames)
mapfile -t ALL_FILES < <(echo "$STATUS" | awk '{
    code = substr($0, 1, 2)
    if (code ~ /^R/) {
        # Renamed: extract new path (after " -> ")
        sub(/.*-> /, "")
        print
    } else {
        # All other statuses: strip XY + space (first 3 chars)
        print substr($0, 4)
    }
}' | sort -u)

# Expand untracked directory entries (e.g. "beads/") to individual files.
# git status --porcelain collapses entire untracked dirs to a single "?? dir/" line,
# but the LLM sees individual file paths from the diff — so VALID_FILES must match.
_EXPANDED=()
for _f in "${ALL_FILES[@]}"; do
    if [[ "$_f" == */ ]]; then
        while IFS= read -r _uf; do
            _EXPANDED+=("$_uf")
        done < <(git ls-files --others --exclude-standard "$_f")
    else
        _EXPANDED+=("$_f")
    fi
done
ALL_FILES=("${_EXPANDED[@]}")

if [[ ${#ALL_FILES[@]} -eq 0 ]]; then
    echo "No changes to commit"
    exit 0
fi

# Gather diffs — combine staged + unstaged for full picture
DIFF_STAT=$(git diff --stat HEAD 2>/dev/null || git diff --stat --cached 2>/dev/null || echo "(new repo — no HEAD)")
DIFF_FULL=$(_safe_diff HEAD || _safe_diff --cached || echo "")

# For untracked files, include their content in the diff
UNTRACKED=$(git ls-files --others --exclude-standard)
if [[ -n "$UNTRACKED" ]]; then
    UNTRACKED_DIFF=""
    while IFS= read -r f; do
        if [[ -f "$f" ]] && file --brief "$f" | grep -qi text; then
            UNTRACKED_DIFF+="
--- /dev/null
+++ b/$f
$(head -c 8192 "$f" | LC_ALL=C sed 's/^/+/')
"
        elif [[ -f "$f" ]]; then
            UNTRACKED_DIFF+="
--- /dev/null
+++ b/$f
+[binary file]
"
        fi
    done <<< "$UNTRACKED"
    DIFF_FULL="${DIFF_FULL}${UNTRACKED_DIFF}"
fi

# Truncate diff to stay within context limits (80K for Haiku, higher for others)
case "$PROVIDER" in
    haiku)    MAX_DIFF=80000 ;;
    cerebras) MAX_DIFF=400000 ;;
    groq)     MAX_DIFF=350000 ;;
    google)   MAX_DIFF=500000 ;;
esac
if [[ ${#DIFF_FULL} -gt $MAX_DIFF ]]; then
    DIFF_FULL="${DIFF_FULL:0:$MAX_DIFF}
... (diff truncated at ${MAX_DIFF} chars)"
fi

# ── Cache check ──────────────────────────────────────────────────────

_USE_CACHED_PLAN=false
if [[ -f "$GCM_CACHE_FILE" ]]; then
    _CACHED_JSON=$(cat "$GCM_CACHE_FILE")
    _CACHED_FILES=$(jq -r '.groups[].files[]' <<< "$_CACHED_JSON" 2>/dev/null | sort)
    _PENDING_FILES=$(printf '%s\n' "${ALL_FILES[@]}" | sort)
    if [[ "$_CACHED_FILES" == "$_PENDING_FILES" ]] && jq -e '.groups | length > 0' <<< "$_CACHED_JSON" &>/dev/null; then
        echo "📋 Using cached plan (--reset to re-analyze)"
        JSON_BODY="$_CACHED_JSON"
        NUM_GROUPS=$(jq '.groups | length' <<< "$JSON_BODY")
        _USE_CACHED_PLAN=true
    else
        echo "⚠️  Cache stale, re-analyzing..."
        rm -f "$GCM_CACHE_FILE"
    fi
fi

if ! $_USE_CACHED_PLAN; then

# ── Phase 2: Call LLM ──────────────────────────────────────────────

echo "🔀 Analyzing and grouping changes ($PROVIDER_LABEL)..."

LLM_PROMPT="Analyze these git changes. Group related files into logical commits by semantic relevance.

Output ONLY valid JSON (no markdown fences, no explanation):
{
  \"groups\": [
    {\"files\": [\"path/to/file\"], \"summary\": \"one-line description\", \"commit_message\": \"type(scope): ...\"},
    {\"files\": [\"other/file\"], \"summary\": \"one-line description\", \"commit_message\": null}
  ]
}

Rules:
- Every file from the file list must appear in exactly one group
- Prefer fewer groups (1-3) unless changes are truly unrelated
- commit_message: full conventional commit message ONLY for groups[0], null for all others
- Conventional commit format, first line under 72 chars
- Add a blank line and bullet points for details if there are multiple significant changes
- For renamed files (R status), use the NEW path in your file list

File list:
$(printf '%s\n' "${ALL_FILES[@]}")

Git status:
$STATUS

Diff stats:
$DIFF_STAT

Full diff:
$DIFF_FULL"

LLM_RESPONSE=$(llm_call "$LLM_PROMPT") || true

if [[ -z "$LLM_RESPONSE" ]]; then
    fallback_single_commit "$PROVIDER_LABEL returned empty response. Falling back to single commit."
fi

# ── Phase 3: Parse response ─────────────────────────────────────────

# DEBUG: Log raw response to help diagnose parsing issues
if [[ -n "${DEBUG_GCM:-}" ]]; then
    echo "════════════════════════════════════════════════════════════" >&2
    echo "DEBUG: Raw $PROVIDER_LABEL response:" >&2
    echo "$LLM_RESPONSE" >&2
    echo "════════════════════════════════════════════════════════════" >&2
fi

# Extract JSON: strip fences, grab outermost {...} block, validate with jq
JSON_BODY=$(echo "$LLM_RESPONSE" | sed '/^```/d' | perl -0777 -ne 'print $1 if /(\{.*\})/s' | jq '.' 2>/dev/null) || true

# DEBUG: Log extracted JSON
if [[ -n "${DEBUG_GCM:-}" ]]; then
    echo "DEBUG: Extracted JSON_BODY:" >&2
    echo "$JSON_BODY" >&2
    echo "════════════════════════════════════════════════════════════" >&2
fi

# If extraction failed, try the raw response directly
if [[ -z "$JSON_BODY" ]] || ! jq -e '.groups' <<< "$JSON_BODY" &>/dev/null; then
    JSON_BODY="$LLM_RESPONSE"
fi

# Validate JSON structure
if ! jq -e '.groups | length > 0' <<< "$JSON_BODY" &>/dev/null; then
    fallback_single_commit "Failed to parse $PROVIDER_LABEL response as valid JSON. Falling back to single commit."
fi

NUM_GROUPS=$(jq '.groups | length' <<< "$JSON_BODY")

# ── Phase 3b: Validate files ────────────────────────────────────────

# Build a lookup set of actual changed files
declare -A VALID_FILES=()
for f in "${ALL_FILES[@]}"; do
    VALID_FILES["$f"]=1
done

# Check that every file the LLM suggested actually exists in our change set
ALL_LLM_FILES=$(jq -r '.groups[].files[]' <<< "$JSON_BODY")
INVALID=false
while IFS= read -r hf; do
    if [[ -z "${VALID_FILES[$hf]+x}" ]]; then
        echo "⚠️  $PROVIDER_LABEL suggested unknown file: $hf"
        INVALID=true
    fi
done <<< "$ALL_LLM_FILES"

if $INVALID; then
    fallback_single_commit "$PROVIDER_LABEL hallucinated filenames. Falling back to single commit."
fi

    echo "$JSON_BODY" > "$GCM_CACHE_FILE"

fi  # end of !_USE_CACHED_PLAN

# ── Phase 4: Display groups ─────────────────────────────────────────

echo ""
echo "Found $NUM_GROUPS group(s):"
echo ""

for ((i=0; i<NUM_GROUPS; i++)); do
    summary=$(jq -r ".groups[$i].summary" <<< "$JSON_BODY")
    mapfile -t group_files < <(jq -r ".groups[$i].files[]" <<< "$JSON_BODY")

    if [[ $i -eq 0 ]]; then
        echo "▶ Group $((i+1)) (committing now): $summary"
    else
        echo "  Group $((i+1)) (next run): $summary"
    fi

    for gf in "${group_files[@]}"; do
        echo "    $gf"
    done
    echo ""
done

# Extract group 1 commit message
COMMIT_MSG=$(jq -r '.groups[0].commit_message // empty' <<< "$JSON_BODY")

if [[ -z "$COMMIT_MSG" ]]; then
    fallback_single_commit "No commit message generated for group 1. Falling back to single commit."
fi

echo "💬 Commit message:"
echo "─────────────────────────────"
echo "$COMMIT_MSG"
echo "─────────────────────────────"
echo ""

if $DRY_RUN; then
    echo "[Dry run] Would commit group 1 with the above message"
    remaining=$((${#ALL_FILES[@]} - $(jq '.groups[0].files | length' <<< "$JSON_BODY")))
    if [[ $remaining -gt 0 ]]; then
        echo "📌 $remaining file(s) remaining in other groups — run again after committing"
    fi
    exit 0
fi

# ── Phase 5: Stage & commit ─────────────────────────────────────────

read -r -p "Commit with this message? [Y/n/e(dit)] " response
case "$response" in
    [nN])
        echo "Aborted. No changes staged."
        exit 0
        ;;
    [eE])
        TMPFILE=$(mktemp)
        echo "$COMMIT_MSG" > "$TMPFILE"
        ${EDITOR:-vim} "$TMPFILE"
        COMMIT_MSG=$(cat "$TMPFILE")
        rm "$TMPFILE"
        ;;
esac

# Clean staging area — reset any currently staged changes
git reset HEAD &>/dev/null || true

# Stage only group 1 files
mapfile -t GROUP1_FILES < <(jq -r '.groups[0].files[]' <<< "$JSON_BODY")

git add -- "${GROUP1_FILES[@]}"

git commit -S -m "$COMMIT_MSG"

# Advance cache: drop the committed group, keep remaining groups for next run
if [[ -f "$GCM_CACHE_FILE" ]]; then
    _REMAINING=$(jq '{groups: .groups[1:]}' "$GCM_CACHE_FILE")
    if jq -e '.groups | length > 0' <<< "$_REMAINING" &>/dev/null; then
        echo "$_REMAINING" > "$GCM_CACHE_FILE"
    else
        rm -f "$GCM_CACHE_FILE"
    fi
fi

echo ""
echo "✅ Committed group 1 successfully!"

# Report remaining
remaining=$((${#ALL_FILES[@]} - ${#GROUP1_FILES[@]}))
if [[ $remaining -gt 0 ]]; then
    echo "📌 $remaining file(s) remaining — run gcm again for the next group"
fi
