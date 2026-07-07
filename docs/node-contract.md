# Node Contract

Every AxisFlow node is a standalone Rust binary named `af-<name>` (e.g.
`af-data`, `af-vault`, `af-http`, `af-llm`). All nodes share one uniform
interface so the Orchestrator and AI Planner can treat them generically.

## 1. Modes (subcommands)

| Invocation            | Purpose                                                              |
|-----------------------|----------------------------------------------------------------------|
| `af-data` / `run`     | Execute the node. Reads input JSON, writes output JSON/NDJSON.       |
| `af-data --describe`  | Print the **capability manifest** (JSON) for discovery & validation. |
| `af-data --version`   | Print version string (semver).                                       |

The manifest is the single source of truth the AI Planner uses to know what a
node can do, and the Orchestrator uses to validate a Flow Spec before running.

## 2. Capability manifest (`--describe`)

```json
{
  "name": "af-data",
  "version": "0.1.0",
  "description": "Query relational databases",
  "inputs": {
    "type": "object",
    "properties": {
      "query":      { "type": "string", "description": "SQL query" },
      "connection": { "type": "string", "description": "vault://ref" }
    },
    "required": ["query"]
  },
  "outputs": {
    "type": "object",
    "properties": {
      "rows":      { "type": "array" },
      "row_count": { "type": "integer" }
    }
  },
  "secrets":    ["connection"],
  "streaming":  false,
  "idempotent": true
}
```

Fields:
- `inputs` / `outputs` — JSON Schema describing the node's data contract.
- `secrets` — list of input names that are credentials; resolved via `af-vault`
  and injected into the environment, never passed as `argv`.
- `streaming` — if `true`, output is NDJSON (one JSON object per line).
- `idempotent` — hints the Orchestrator whether safe to retry on failure.

## 3. Execution protocol

- **Input**: a single JSON object on **stdin** (or `--input <file>`).
  Secrets are *not* embedded here; they are injected as env vars by the
  Orchestrator (see Credential Layer).
- **Output**: a single JSON object on **stdout** on success. If `streaming`,
  stdout is NDJSON (one record per line), which the Orchestrator can forward
  record-by-record to the next node.
- **Errors**: written as JSON to **stderr** (`{"error": "...", "code": 2}`),
  process exits with the matching code below.

### Exit codes

| Code | Meaning                | Orchestrator behavior        |
|------|------------------------|------------------------------|
| 0    | success                | continue                     |
| 1    | generic error          | stop / mark flow failed      |
| 2    | retryable (transient)  | retry with backoff           |
| 3    | auth / credential      | resolve via `af-vault`, retry|
| 4    | invalid input / schema | fail fast                    |
| 10+  | domain-specific errors | node-defined                 |

## 4. Credential Layer (`af-vault`)

- A node declares secret inputs via `secrets` in its manifest.
- The Orchestrator asks `af-vault` to resolve each `vault://...` reference and
  injects the value into the child process environment as
  `AXISFLOW_SECRET_<NAME>`.
- Nodes read secrets from env, never from `argv` or the input JSON body.
- This keeps secrets out of process listings, logs, and the Flow Spec file.

## 5. Inter-node data transport

Default: Orchestrator captures a node's stdout and pipes it as stdin to the
next node (in-memory JSON). Open question (D): for large payloads (e.g. millions
of rows) we may need a shared temp file / named pipe addressed by a path
injected via env, instead of buffering in memory.

## Open questions for this layer

- **A. Subprocess vs plugin**: spawn `af-*` as a child process (simple,
  isolated, matches the vision) vs compile nodes as wasm/Rust plugins (faster,
  no spawn overhead, but more complex tooling). Proposal: subprocess by
  default, plugin as an optional optimization later.
- **Manifest format**: reuse JSON Schema for `inputs`/`outputs` (validators
  already exist in Rust, e.g. `jsonschema`) — good default?
- **Error schema**: standardize `{"error","code","retryable"}` or keep minimal?
- **Streaming granularity**: per-record NDJSON, or support Server-Sent-Events
  style for the future visual builder?
