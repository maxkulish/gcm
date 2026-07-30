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
# Format: "<item-kind>.<Name>" where item-kind is one of:
#   struct, enum, trait, type, fn
FORBIDDEN=(
  "trait.Provider"
  "struct.ConflictHunk"
  "struct.ResolveContext"
  "struct.Resolution"
  "enum.HunkResolution"
  "struct.ResolveReport"
  "struct.RoundReport"
  "struct.FinishReport"
)

FAILED=0
for f in "${FORBIDDEN[@]}"; do
  # Recursive search under doc/gcm/ — rustdoc places types in per-module
  # subdirectories (e.g. doc/gcm/provider/facade/trait.Provider.html).
  # Match all item-kind prefixes: struct, enum, trait, type, fn.
  if find "$DOC_DIR" -name "${f}.html" 2>/dev/null | grep -q .; then
    echo "FAIL: forbidden type leaked into public surface: ${f}.html"
    find "$DOC_DIR" -name "${f}.html"
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
