#!/usr/bin/env bash
# Generate docs/nodes/<name>.md from each crate's --describe output.
# Only processes crates listed in workspace Cargo.toml (no stale binaries).
set -euo pipefail

BIN_DIR="${1:-target/debug}"
OUT_DIR="docs/nodes"
ROOT_CARGO="${2:-Cargo.toml}"

mkdir -p "$OUT_DIR"

# Read workspace member crates, filter to na-* (skip orchestrator, contract)
mapfile -t CRATES < <(grep 'crates/na-' "$ROOT_CARGO" | sed 's/.*"crates\/\(.*\)".*/\1/' | grep -v -E '^(na-orchestrator|na-contract)$' || true)

generate_doc() {
  local bin="$1"
  local name
  name=$(basename "$bin")

  local manifest
  manifest=$("$bin" --describe 2>/dev/null) || {
    echo "  skipping $name (no --describe)"
    return
  }

  local description
  description=$(echo "$manifest" | python3 -c "import sys,json; print(json.load(sys.stdin).get('description',''))")
  local use_cases
  use_cases=$(echo "$manifest" | python3 -c "
import sys,json
m = json.load(sys.stdin)
uc = m.get('use_cases', [])
print(', '.join(uc) if uc else 'general')
" 2>/dev/null || echo "general")

  local file="$OUT_DIR/$name.md"

  {
    echo "# $name"
    echo
    echo "$description"
    echo
    echo "**Use cases:** $use_cases"
    echo

    echo "## Inputs"
    echo
    echo '```json'
    echo "$manifest" | python3 -c "
import sys, json
m = json.load(sys.stdin)
inputs = m.get('inputs', {})
props = inputs.get('properties', {})
req = inputs.get('required', [])
for p, s in props.items():
    req_mark = ' (required)' if p in req else ''
    desc = s.get('description', '')
    enum = s.get('enum')
    default = s.get('default')
    extra = ''
    if enum:
        extra = f\" enum: [{', '.join(str(e) for e in enum)}]\"
    if default is not None:
        extra += f\" default: {default}\"
    print(f\"  {p}: {s.get('type', 'any')}{extra}{req_mark}\")
    if desc:
        print(f\"    {desc}\")
"
    echo '```'
    echo

    echo "## Outputs"
    echo
    echo '```'
    echo "$manifest" | python3 -c "
import sys, json
m = json.load(sys.stdin)
outputs = m.get('outputs', {})
props = outputs.get('properties', {})
for p, s in props.items():
    desc = s.get('description', '')
    print(f\"  {p}: {s.get('type', 'any')}\")
    if desc:
        print(f\"    {desc}\")
"
    echo '```'
    echo

    local secrets
    secrets=$(echo "$manifest" | python3 -c "
import sys, json
m = json.load(sys.stdin)
s = m.get('secrets', [])
if s:
    for sec in s:
        print(f\"  - \`NGALIR_SECRET_{sec.upper()}\`\")
else:
    print('  (none)')
")
    echo "## Secrets"
    echo
    echo "$secrets"
    echo

    local creds
    creds=$(echo "$manifest" | python3 -c "
import sys, json
m = json.load(sys.stdin)
creds = m.get('credentials', [])
if creds:
    for c in creds:
        print(f\"  - ID: \`{c.get('id')}\`\")
        print(f\"    Label: {c.get('label')}\")
        print(f\"    Auth: {c.get('auth_type')}\")
        for f in c.get('fields', []):
            print(f\"    Field: {f.get('key')} ({f.get('input_type')}, {'required' if f.get('required') else 'optional'})\")
        oauth = c.get('oauth')
        if oauth:
            print(f\"    OAuth authorize URL: {oauth.get('authorize_url')}\")
            print(f\"    OAuth token URL: {oauth.get('token_url')}\")
            scopes = ', '.join(oauth.get('scopes', []))
            if scopes:
                print(f\"    OAuth scopes: {scopes}\")
else:
    print('  (none)')
")
    echo "## Credentials"
    echo
    echo "$creds"
    echo

    local extra
    extra=$(echo "$manifest" | python3 -c "
import sys, json
m = json.load(sys.stdin)
parts = []
if m.get('streaming'):
    parts.append('streaming')
if m.get('idempotent'):
    parts.append('idempotent')
if m.get('output_mode'):
    parts.append(f\"output_mode: {m['output_mode']}\")
print(', '.join(parts) if parts else '(none)')
")
    echo "## Properties"
    echo
    echo "  - **Streaming:** $extra"
    echo "  - **Idempotent:** $(echo "$manifest" | python3 -c "import sys,json; print(json.load(sys.stdin).get('idempotent', False))" 2>/dev/null || echo false)"
    echo

    local see_also
    see_also=$(echo "$manifest" | python3 -c "
import sys, json
m = json.load(sys.stdin)
sa = m.get('see_also', [])
if sa:
    for ref in sa:
        print(f\"  - [{ref}]({ref}.md)\")
else:
    print('  (none)')
")
    echo "## See also"
    echo
    echo "$see_also"

  } > "$file"

  echo "  generated $file"
}

echo "Generating node docs from workspace members..."
for crate in "${CRATES[@]}"; do
  bin="$BIN_DIR/$crate"
  if [ -f "$bin" ] && [ -x "$bin" ]; then
    generate_doc "$bin"
  else
    echo "  warning: $crate binary not found in $BIN_DIR (run 'cargo build' first)"
  fi
done

echo "Done. $(ls "$OUT_DIR"/*.md 2>/dev/null | wc -l) docs in $OUT_DIR/"
