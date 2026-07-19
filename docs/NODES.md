# Nodes

Ngalir discovers nodes dynamically — any `na-<name>` binary on `PATH` or
`NGALIR_NODE_PATH` is available. Run `ngalir nodes` to list what's installed
or `ngalir skills` to get the full machine-readable registry (JSON).

## Core

| Node | Description |
|---|---|
| `na-echo` | Echo a message (reference / test node) |
| `na-file` | File read / write |
| `na-http` | HTTP client (GET / POST / PUT / DELETE / PATCH) |
| `na-jsonpath` | JSON path extractor with jq-compatible filtering (`.[]`, slices, pipes) |
| `na-vault` | Credential storage (resolves `vault://` refs) |

## Database

| Node | Description |
|---|---|
| `na-db-postgres` | PostgreSQL query execution |
| `na-db-mysql` | MySQL query execution |
| `na-db-sqlite` | SQLite query execution |

## Data processing

| Node | Description |
|---|---|
| `na-csv` | Streaming CSV processor (read / write) |
| `na-excel` | Excel (.xlsx) processor (read / write, sheet & range selection) |
| `na-google-sheets` | Google Sheets processor (read / append, OAuth2) |

## AI

| Node | Description |
|---|---|
| `na-llm` | LLM chat completions (OpenAI / Anthropic / compatible) |

## Triggers / daemons

| Node | Description |
|---|---|
| `na-webhook` | HTTP server that triggers flow execution |
| `na-schedule` | Cron-based flow scheduler |
| `na-email` | SMTP email sender |

## Building a custom node

See [Node Contract](node-contract.md) for the binary protocol. Name your
binary `na-<name>`, implement `--describe` / `--version` / stdin JSON
execution, and place it on `PATH`.
