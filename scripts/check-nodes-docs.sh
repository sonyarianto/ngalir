#!/usr/bin/env bash
set -euo pipefail
missing=()
missing_docs=()
for crate in crates/na-*; do
  [ -f "$crate/src/main.rs" ] || continue
  name=$(basename "$crate")
  [ "$name" = "na-orchestrator" ] && continue
  if ! grep -Eq "\[$name\]" docs/NODES.md; then
    missing+=("$name")
  fi
  if [ ! -f "docs/nodes/$name.md" ]; then
    missing_docs+=("$name")
  fi
done
rc=0
if [ ${#missing[@]} -gt 0 ]; then
  echo "Missing from docs/NODES.md:"
  printf '  - %s\n' "${missing[@]}"
  rc=1
fi
if [ ${#missing_docs[@]} -gt 0 ]; then
  echo "Missing from docs/nodes/:"
  printf '  - %s\n' "${missing_docs[@]}"
  echo "Run: scripts/generate-node-docs.sh target/debug"
  rc=1
fi
exit $rc
