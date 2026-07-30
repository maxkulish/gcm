#!/usr/bin/env bash
set -euo pipefail

# Resolve repo root for portability (works from any subdirectory or CI).
REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

# Isolated target directory to avoid stale artifacts from default-feature
# builds causing false positives.
export CARGO_TARGET_DIR=target/surface_check

echo "==> Building docs with --lib --no-default-features (isolated target dir)"
cargo doc --lib --no-default-features

DOC_DIR="$CARGO_TARGET_DIR/doc/gcm"

# Forbidden types that must NOT appear in the public library surface.
# These are commit-domain types or binary-only facades.
FORBIDDEN=(
  "trait.Provider.html"
  "struct.ConflictHunk.html"
  "struct.ResolveContext.html"
  "struct.Resolution.html"
  "struct.HunkResolution.html"
  "struct.ResolveReport.html"
  "struct.RoundReport.html"
  "struct.FinishReport.html"
)

FAILED=0
for f in "${FORBIDDEN[@]}"; do
  if [ -f "$DOC_DIR/$f" ]; then
    echo "FAIL: forbidden type leaked into public surface: $f"
    FAILED=1
  fi
done

if [ "$FAILED" -eq 1 ]; then
  echo ""
  echo "One or more commit-domain types are publicly exported."
  echo "Check src/lib.rs to ensure binary-only modules are not included."
  exit 1
fi

echo "OK: no forbidden types in public library surface"
