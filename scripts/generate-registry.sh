#!/usr/bin/env bash
# Generate docs/registry.json — the node registry manifest.
set -euo pipefail

BIN_DIR="${1:-target/debug}"
OUT="${2:-docs/registry.json}"

python3 -c "
import json, subprocess, sys, os, glob

bins = sorted(glob.glob(os.path.join(sys.argv[1], 'na-*')))
entries = []
for b in bins:
    if not os.access(b, os.X_OK):
        continue
    try:
        out = subprocess.check_output([b, '--describe'], stderr=subprocess.DEVNULL, timeout=10)
        m = json.loads(out)
        entries.append({
            'name': m['name'],
            'version': m['version'],
            'description': m.get('description', ''),
            'use_cases': m.get('use_cases', []),
            'repo': 'https://github.com/sonyarianto/ngalir',
        })
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired, json.JSONDecodeError, OSError):
        pass

entries.sort(key=lambda x: x['name'])
with open(sys.argv[2], 'w') as f:
    json.dump(entries, f, indent=2)
print(f'generated {sys.argv[2]} ({len(entries)} entries)')
" "$BIN_DIR" "$OUT"
