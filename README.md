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
  skills     List node skills registry (JSON)  ngalir skills
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

## Secrets (vault)

Write secrets to a JSON file (default `~/.ngalir/vault.json` or
`NGALIR_VAULT_FILE`):

```json
{
  "api_key": "sk-xxx",
  "db/prod": "postgresql://user:pass@host/db"
}
```

Reference them in flows via `vault://` or `NGALIR_SECRET_*` env vars:

```yaml
nodes:
  - id: query
    use: db-postgres
    with:
      connection: vault://db/prod
      query: "SELECT * FROM users"
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

`docker compose up` starts the webhook (port 8080) and schedule daemon with
persistent volumes and metrics ports exposed.

## Building a custom node

1. Implement the Node Contract: `--describe` (manifest), `--version`, and
   stdin/stdout JSON execution.
2. Name your binary `na-<name>`.
3. Put it on `PATH` or `NGALIR_NODE_PATH`.

Minimal example: see `crates/na-echo/src/main.rs`.

## Environment

| Variable | Purpose |
|---|---|
| `NGALIR_NODE_PATH` | Colon-separated directories to search for `na-*` binaries |
| `NGALIR_VAULT_FILE` | Path to vault JSON file (default `~/.ngalir/vault.json`) |
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
