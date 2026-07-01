#!/usr/bin/env bash
# Publish/update the gcm Homebrew formula on maxkulish/homebrew-tap.
#
# Unlike a locally-built artifact, gcm's release tarballs are produced by CI
# (.github/workflows/release.yml) AFTER a `v*` tag is pushed. So this script:
#   1. waits for that release build to publish the GitHub Release + .sha256 assets
#   2. reads the four per-target SHA256 sums straight from the release assets
#   3. renders the COMPLETE Formula/gcm.rb (creating it on first run)
#   4. pushes it directly to the tap's main branch (only if it changed)
#
# Idempotent: re-running with the same release is a no-op ("already up to date").
# Safe to run standalone for recovery if the formula step failed after a
# successful release build:  make brew-bump
#
# Prerequisites:
#   - `gh` CLI authenticated with push access to maxkulish/homebrew-tap
#   - a pushed `v<version>` tag whose release.yml run produces the 4 tarballs

set -euo pipefail

REPO="maxkulish/gcm"
TAP="maxkulish/homebrew-tap"
FORMULA_PATH="Formula/gcm.rb"
CLASS="Gcm"
BIN="gcm"
ASSET_TIMEOUT="${ASSET_TIMEOUT:-1800}"   # seconds to wait for release assets (30 min)

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

command -v gh  >/dev/null 2>&1 || { echo "Error: gh CLI required (brew install gh)" >&2; exit 1; }

VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')"
DESC="$(grep -m1 '^description' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')"
LICENSE="$(grep -m1 '^license' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')"
HOMEPAGE="$(grep -m1 '^repository' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')"
[ -n "$VERSION" ] || { echo "Error: could not read version from Cargo.toml" >&2; exit 1; }
TAG="v$VERSION"
DL_BASE="https://github.com/$REPO/releases/download/$TAG"

# Ordered: target triple <-> Homebrew (OS, CPU) predicate. Keep in sync with the
# Makefile TARGETS list and .github/workflows/release.yml matrix.
TARGETS=(
  "aarch64-apple-darwin|OS.mac? && Hardware::CPU.arm?"
  "x86_64-apple-darwin|OS.mac? && Hardware::CPU.intel?"
  "aarch64-unknown-linux-musl|OS.linux? && Hardware::CPU.arm?"
  "x86_64-unknown-linux-musl|OS.linux? && Hardware::CPU.intel?"
)

echo "Publishing Homebrew formula for $BIN $TAG"
echo "  repo: $REPO   tap: $TAP   formula: $FORMULA_PATH"

# ── 1. follow the release build (best-effort live progress) ────────────────────
RUN_ID=""
for _ in $(seq 1 20); do
  RUN_ID="$(gh run list -R "$REPO" --workflow=release.yml --branch "$TAG" --limit 1 \
            --json databaseId --jq '.[0].databaseId // empty' 2>/dev/null || true)"
  [ -n "$RUN_ID" ] && break
  sleep 6
done
if [ -n "$RUN_ID" ]; then
  echo "Watching release build (run $RUN_ID) - the release assets below are the real gate ..."
  gh run watch "$RUN_ID" -R "$REPO" --exit-status \
    || echo "Warning: release run $RUN_ID did not report success; verifying assets anyway." >&2
else
  echo "No release run located yet for $TAG; polling the release directly."
fi

# ── 2. wait until all 4 .sha256 assets exist (authoritative) ───────────────────
echo "Waiting for $TAG release assets (need 4 .sha256 files, up to $((ASSET_TIMEOUT/60)) min) ..."
deadline=$(( $(date +%s) + ASSET_TIMEOUT ))
while :; do
  present="$(gh release view "$TAG" -R "$REPO" --json assets \
            --jq '[.assets[].name | select(endswith(".sha256"))] | length' 2>/dev/null || echo 0)"
  [ "${present:-0}" -ge 4 ] && break
  if [ "$(date +%s)" -ge "$deadline" ]; then
    echo "Error: timed out waiting for $TAG assets (${present:-0}/4)." >&2
    echo "       Check the release build: gh run list -R $REPO --workflow=release.yml" >&2
    exit 1
  fi
  echo "  ... ${present:-0}/4 sha256 assets present; retrying in 20s"
  sleep 20
done
echo "All release assets are published."

# ── 3. pull each per-target SHA256 from the release ────────────────────────────
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

declare -a SHA
for i in "${!TARGETS[@]}"; do
  triple="${TARGETS[$i]%%|*}"
  shafile="$BIN-$TAG-$triple.tar.gz.sha256"
  gh release download "$TAG" -R "$REPO" -p "$shafile" -D "$TMP" --clobber >/dev/null 2>&1 \
    || { echo "Error: missing release asset $shafile" >&2; exit 1; }
  hash="$(awk '{print $1}' "$TMP/$shafile")"
  case "$hash" in
    [0-9a-f]*) [ "${#hash}" -eq 64 ] || { echo "Error: bad sha256 for $triple: '$hash'" >&2; exit 1; } ;;
    *) echo "Error: bad sha256 for $triple: '$hash'" >&2; exit 1 ;;
  esac
  SHA[i]="$hash"
  echo "  $triple  ${hash:0:12}..."
done

# ── 4. render the full formula ─────────────────────────────────────────────────
render_formula() {
  cat <<EOF
class $CLASS < Formula
  desc "$DESC"
  homepage "$HOMEPAGE"
  version "$VERSION"
  license "$LICENSE"

EOF
  for i in "${!TARGETS[@]}"; do
    triple="${TARGETS[$i]%%|*}"
    pred="${TARGETS[$i]##*|}"
    kw="elsif"; [ "$i" -eq 0 ] && kw="if"
    cat <<EOF
  $kw $pred
    url "$DL_BASE/$BIN-$TAG-$triple.tar.gz"
    sha256 "${SHA[i]}"
EOF
  done
  cat <<EOF
  end

  def install
    bin.install "$BIN"
  end

  test do
    system "#{bin}/$BIN", "--version"
  end
end
EOF
}

# ── 5. push to the tap (only if changed) ───────────────────────────────────────
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$TMP" "$WORKDIR"' EXIT
echo "Cloning $TAP ..."
gh repo clone "$TAP" "$WORKDIR/tap" -- --depth=1 --quiet
cd "$WORKDIR/tap"

mkdir -p "$(dirname "$FORMULA_PATH")"
render_formula > "$FORMULA_PATH"

git add "$FORMULA_PATH"
# index-vs-HEAD: a brand-new formula stages as an addition (proceed); an
# identical existing formula stages nothing (no-op, idempotent re-run).
if git diff --cached --quiet; then
  echo "Formula already up to date at $TAG - nothing to push."
  exit 0
fi

git -c user.name="${GIT_AUTHOR_NAME:-Max Kulish}" \
    -c user.email="${GIT_AUTHOR_EMAIL:-kma.memo@gmail.com}" \
    commit -q -m "$BIN $TAG"
git push -q origin HEAD

echo ""
echo "Pushed $FORMULA_PATH @ $TAG to $TAP."
echo "Install / upgrade:"
echo "    brew install $TAP/$BIN     # or: brew upgrade $BIN"
