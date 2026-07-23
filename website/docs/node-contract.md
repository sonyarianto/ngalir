# Node Contract

Every Ngalir node is a standalone binary named `na-<name>` that communicates via stdin/stdout using JSON.

**Protocol:**
- `--describe` flag — outputs the node's manifest JSON (name, version, inputs, outputs, secrets, credentials, streaming, idempotent, output_mode, use_cases)
- Stdin — receives input JSON when executed
- Stdout — writes result JSON (or NDJSON lines when `streaming: true`)
- Stderr — reserved for logging/tracing

**Manifest fields:**
- `name`, `version`, `description`
- `inputs` — JSON Schema object defining required and optional fields
- `outputs` — JSON Schema object describing the return shape
- `secrets` — env var names for sensitive values
- `credentials` — structured credential specs (API Key, OAuth2, Basic Auth)
- `streaming` — boolean, enables NDJSON output
- `idempotent` — boolean, safe for retry
- `output_mode` — `"file"` for large payloads (writes to `NGALIR_OUTPUT_DIR`)

See [node-contract.md on GitHub](https://github.com/sonyarianto/ngalir/blob/main/docs/node-contract.md) for the full specification.
