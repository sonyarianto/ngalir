#!/usr/bin/env bash
# Generate docs/registry.json — the node registry manifest.
# Only processes crates listed in workspace Cargo.toml (no stale binaries).
set -euo pipefail

BIN_DIR="${1:-target/debug}"
OUT="${2:-docs/registry.json}"
ROOT_CARGO="${3:-Cargo.toml}"

# Read workspace member crates, filter to na-* (skip orchestrator, contract)
CRATES=$(grep 'crates/na-' "$ROOT_CARGO" | sed 's/.*"crates\/\(.*\)".*/\1/' | grep -v -E '^(na-orchestrator|na-contract)$' | tr '\n' ' ')

python3 -c "
import json, subprocess, sys, os

bins = sys.argv[1].split()
entries = []
for b in bins:
    binpath = os.path.join(sys.argv[2], b)
    if not os.access(binpath, os.X_OK):
        print(f'  warning: {b} binary not found in {sys.argv[2]} (run cargo build first)', file=sys.stderr)
        continue
    try:
        out = subprocess.check_output([binpath, '--describe'], stderr=subprocess.DEVNULL, timeout=10)
        m = json.loads(out)
        entries.append({
            'name': m['name'],
            'version': m['version'],
            'description': m.get('description', ''),
            'use_cases': m.get('use_cases', []),
            'repo': 'https://github.com/sonyarianto/ngalir',
        })
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired, json.JSONDecodeError, OSError) as e:
        print(f'  warning: {b} --describe failed: {e}', file=sys.stderr)

entries.sort(key=lambda x: x['name'])
with open(sys.argv[3], 'w') as f:
    json.dump(entries, f, indent=2)
print(f'generated {sys.argv[3]} ({len(entries)} entries)')
" "$CRATES" "$BIN_DIR" "$OUT"
