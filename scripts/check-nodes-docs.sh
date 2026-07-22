#!/usr/bin/env bash
set -euo pipefail
missing=()
for crate in crates/na-*; do
  [ -f "$crate/src/main.rs" ] || continue
  name=$(basename "$crate")
  [ "$name" = "na-orchestrator" ] && continue
  if ! grep -q "\`$name\`" docs/NODES.md; then
    missing+=("$name")
  fi
done
if [ ${#missing[@]} -gt 0 ]; then
  echo "Missing from docs/NODES.md:"
  printf '  - %s\n' "${missing[@]}"
  exit 1
fi
