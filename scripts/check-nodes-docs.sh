#!/usr/bin/env bash
# Pre-commit hook: verify documentation coherence.
set -euo pipefail

ROOT_CARGO="Cargo.toml"

# Read workspace member crates, filter to na-* (skip orchestrator, contract)
CRATES=()
while IFS= read -r line; do
  CRATES+=("$line")
done < <(grep 'crates/na-' "$ROOT_CARGO" | sed 's/.*"crates\/\(.*\)".*/\1/' | grep -v -E '^(na-orchestrator|na-contract)$' || true)

# Files currently in docs/nodes/
DOCS_FILES=()
while IFS= read -r f; do
  name=$(basename "$f" .md)
  DOCS_FILES+=("$name")
done < <(ls docs/nodes/*.md 2>/dev/null || true)

rc=0

# Check 1: every crate has an entry in NODES.md
missing_nodes=()
for name in "${CRATES[@]}"; do
  if ! grep -Eq "\[$name\]" docs/NODES.md; then
    missing_nodes+=("$name")
  fi
done
if [ ${#missing_nodes[@]} -gt 0 ]; then
  echo "Missing from docs/NODES.md:"
  printf '  - %s\n' "${missing_nodes[@]}"
  rc=1
fi

# Check 2: every crate has a per-node doc
missing_docs=()
for name in "${CRATES[@]}"; do
  if [ ! -f "docs/nodes/$name.md" ]; then
    missing_docs+=("$name")
  fi
done
if [ ${#missing_docs[@]} -gt 0 ]; then
  echo "Missing from docs/nodes/:"
  printf '  - %s\n' "${missing_docs[@]}"
  echo "Run: make docs"
  rc=1
fi

# Check 3: every doc file has a matching crate (no phantom docs)
phantom_docs=()
for name in "${DOCS_FILES[@]}"; do
  found=0
  for crate in "${CRATES[@]}"; do
    if [ "$name" = "$crate" ]; then
      found=1
      break
    fi
  done
  if [ "$found" -eq 0 ]; then
    phantom_docs+=("$name")
  fi
done
if [ ${#phantom_docs[@]} -gt 0 ]; then
  echo "Phantom docs (no matching crate):"
  printf '  - docs/nodes/%s.md\n' "${phantom_docs[@]}"
  echo "Delete them or add the crate back."
  rc=1
fi

# Check 4: registry.json exists
if [ ! -f "docs/registry.json" ]; then
  echo "Missing: docs/registry.json"
  echo "Run: make registry"
  rc=1
fi

exit $rc
