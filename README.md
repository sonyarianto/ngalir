# Ngalir

**Flow automation engine, built in Rust.** Nodes are standalone CLI
binaries (`na-*`); flows are declarative YAML DAGs executed by `ngalir`.
Production-ready: containerised, observable via Prometheus, supports subflows,
streaming, checkpoint/resume, and AI-powered workflow generation.

## Quick start

```bash
# Build everything
cargo build

# List available nodes
PATH=target/debug:$PATH ./target/debug/ngalir nodes

# Run the echo demo
PATH=target/debug:$PATH ./target/debug/ngalir run examples/echo-demo.yaml
```

## Concepts

- **Flow Spec** — a YAML/JSON file describing a DAG of nodes. See `docs/flow-spec.md`.
- **Node** — a standalone CLI binary named `na-<name>` that reads JSON on stdin
  and writes JSON on stdout. See `docs/node-contract.md`.
- **Orchestrator** (`ngalir` binary) — validates & executes a Flow Spec,
  spawning node subprocesses in topological order with bounded concurrency.
- **Subflow** — reuse a flow as a node via `use: "@subflow.yaml"`; node IDs are
  automatically namespaced to prevent collisions.
- **Output modes** — nodes can emit NDJSON lines to stdout (default) or write
  to a temp file (`output_mode: "file"`) for large payloads.
- **Checkpoint** — `--state-dir` enables atomic checkpoint/resume across
  flow executions.

## CLI

```
ngalir <COMMAND>

Commands:
  run        Execute a Flow Spec               ngalir run flow.yaml
  nodes      List all na-* on PATH             ngalir nodes
  validate   Validate without running          ngalir validate flow.yaml
  generate   Generate a flow from a prompt     ngalir generate "fetch API → email result"
  skills     List node skills registry (JSON)  ngalir skills | jq .
  init-node  Scaffold a new node crate         ngalir init-node
  completion Generate shell completions        ngalir completion bash
  help       Print help

Run flags:
  --input JSON       Seed __request__ with initial data
  --state-dir PATH   Enable checkpoint / resume
  --metrics-port N   Expose /metrics on :N
```

## Included nodes

Run `ngalir nodes` to list all `na-*` binaries on your `PATH`. Nodes are
discovered dynamically — add new ones by placing `na-<name>` anywhere on
`PATH` or `NGALIR_NODE_PATH`.

See [docs/NODES.md](docs/NODES.md) for detailed descriptions of every node.

## Writing a flow

```yaml
version: 1
name: etl-demo
nodes:
  - id: src
    use: db-postgres
    with:
      connection: vault://db/prod
      query: "SELECT id, amount FROM orders WHERE day = current_date"
  - id: transform
    use: jsonpath
    inputs:
      data: src.rows
    with:
      filter: "[] | {id, amount}"
  - id: notify
    use: email
    inputs:
      to: ops@example.com
      subject: "ETL done"
      body: "{{ transform.result | length }} rows processed"
    when: "{{ src.rows | length > 0 }}"
```

## Subflows

Reuse common patterns by referencing external flow files:

```yaml
nodes:
  - id: fetch-orders
    use: "@subflows/http-fetch.yaml"
    with:
      url: "https://api.example.com/orders"
```

Subflow node IDs are prefixed (`fetch-orders.node_id`). Exit nodes (`exit: true`)
create passthrough outputs on the parent. Subflows can be nested.

## Observability

All daemon services expose Prometheus metrics and health endpoints:

| Service | Metrics port | Endpoints |
|---|---|---|
| `na-webhook` | 9091 (configurable) | `/health`, `/metrics` |
| `na-schedule` | 9092 (configurable) | `/health`, `/metrics` |
| `ngalir` (opt-in) | `--metrics-port N` | `/health`, `/metrics` |

Metrics include flow/node execution counts by status, flow durations, and
trigger events.

## Credentials & Secrets

Ngalir stores credentials in a structured vault (`~/.ngalir/vault.json` or
`NGALIR_VAULT_FILE`). Managed via:

- **Web UI** — `/credentials` page: list, add, test, delete credentials
- **CLI** — `na-vault --list/--get/--create/--update/--delete`
- **REST API** — `GET/POST/PUT/DELETE /api/credentials`

Each credential is typed by a `CredentialSpec` declared in the node's manifest,
which drives dynamic UI forms, OAuth buttons, and test-connection flows.

Reference credentials in flows via `vault://<credential_id>`:

```yaml
nodes:
  - id: query
    use: db-postgres
    with:
      connection: vault://db/prod
      query: "SELECT * FROM users"
  - id: sheets
    use: google-sheets
    with:
      credentials: vault://my_sa_key
      spreadsheet_id: "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms"
```

### Encryption at rest

Set `NGALIR_VAULT_KEY` to a base64-encoded 32-byte key to enable AES-256-GCM
encryption of the vault file. Without it, the vault is plain JSON (development).

```bash
# Generate a key
openssl rand -base64 32

# Use it
export NGALIR_VAULT_KEY="<base64-key>"
```

## Docker

```bash
# Build the image
docker build -t ngalir/ngalir .

# Run the CLI
docker run --rm ngalir/ngalir --help

# Run a flow (mount flows directory)
docker run --rm -v /path/to/flows:/flows ngalir/ngalir run /flows/my-flow.yaml

# Daemon services with Prometheus
docker compose up -d webhook schedule
# webhook:8080, webhook metrics:9091, schedule metrics:9092
```

`docker compose up` starts the web UI server (port 8080), webhook daemon, and
schedule daemon with persistent volumes and metrics ports exposed.

## Building a custom node

```bash
ngalir init-node
```

The interactive scaffold generates a complete `crates/na-<name>/` crate with:

- `Cargo.toml` with proper dependencies
- `src/main.rs` implementing the Node Contract (manifest, secrets, credentials,
  input/output schemas, test skeleton)
- Auto-registers as a workspace member in `Cargo.toml`

See `docs/node-contract.md` for the binary protocol. Minimal example:
`crates/na-echo/src/main.rs`.

## Environment

| Variable | Purpose |
|---|---|
| `NGALIR_NODE_PATH` | Colon-separated directories to search for `na-*` binaries |
| `NGALIR_VAULT_FILE` | Path to vault JSON file (default `~/.ngalir/vault.json`) |
| `NGALIR_VAULT_KEY` | Base64-encoded 32-byte AES-256-GCM key for vault encryption |
| `NGALIR_OUTPUT_DIR` | Temp directory for file-mode output (set by orchestrator) |
| `NGALIR_SECRET_*` | Env vars prefixed with `NGALIR_SECRET_` are injected as secrets |

## Roadmap

See [docs/ROADMAP.md](docs/ROADMAP.md).

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Node Contract](docs/node-contract.md)
- [Flow Spec](docs/flow-spec.md)
- [Roadmap](docs/ROADMAP.md)

## License

MIT
