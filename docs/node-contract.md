# Node Contract

Every Ngalir node is a standalone Rust binary named `na-<name>` (e.g.
`na-echo`, `na-vault`, `na-http`, `na-jsonpath`). All nodes share one uniform
interface so the Orchestrator can treat every node generically.

## 1. Modes (subcommands)

| Invocation            | Purpose                                                              |
|-----------------------|----------------------------------------------------------------------|
| `na-echo` / `run`     | Execute the node. Reads input JSON, writes output JSON/NDJSON.       |
| `na-echo --describe`  | Print the **capability manifest** (JSON) for discovery & validation. |
| `na-echo --version`   | Print version string (semver).                                       |

The manifest is the single source of truth the Orchestrator uses to discover
node capabilities and validate a Flow Spec before running.

## 2. Capability manifest (`--describe`)

```json
{
  "name": "na-echo",
  "version": "0.1.0",
  "description": "Sample node demonstrating the contract",
  "inputs": {
    "type": "object",
    "properties": {
      "message":   { "type": "string", "description": "Message to echo" }
    },
    "required": ["message"]
  },
  "outputs": {
    "type": "object",
    "properties": {
      "echo":      { "type": "string" }
    }
  },
  "secrets":    [],
  "streaming":  false,
  "idempotent": true
}
```

Fields:
- `inputs` / `outputs` — JSON Schema describing the node's data contract.
- `secrets` — list of input names that are credentials; resolved via `na-vault`
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
| 3    | auth / credential      | resolve via `na-vault`, retry|
| 4    | invalid input / schema | fail fast                    |
| 10+  | domain-specific errors | node-defined                 |

## 4. Credential Layer (`na-vault`)

- A node declares secret inputs via `secrets` in its manifest.
- The Orchestrator asks `na-vault` to resolve each `vault://...` reference and
  injects the value into the child process environment as
  `NGALIR_SECRET_<NAME>` (planned — currently resolved into stdin JSON).
- Nodes read secrets from env, never from `argv` or the input JSON body.
- This keeps secrets out of process listings, logs, and the Flow Spec file.

## 5. Inter-node data transport

Default: Orchestrator captures a node's stdout and pipes it as stdin to the
next node (in-memory JSON). Open question (D): for large payloads (e.g. millions
of rows) we may need a shared temp file / named pipe addressed by a path
injected via env, instead of buffering in memory.

## Open questions for this layer

- **A. Subprocess vs plugin**: spawn `na-*` as a child process (simple,
  isolated, matches the vision) vs compile nodes as wasm/Rust plugins (faster,
  no spawn overhead, but more complex tooling). Proposal: subprocess by
  default, plugin as an optional optimization later.
- **Manifest format**: reuse JSON Schema for `inputs`/`outputs` (validators
  already exist in Rust, e.g. `jsonschema`) — good default?
- **Error schema**: standardize `{"error","code","retryable"}` or keep minimal?
- **Streaming granularity**: per-record NDJSON, or support Server-Sent-Events
  style for the future visual builder?
