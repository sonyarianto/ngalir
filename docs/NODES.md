# Nodes

Ngalir discovers nodes dynamically — any `na-<name>` binary on `PATH` or
`NGALIR_NODE_PATH` is available. Run `ngalir nodes` to list what's installed
or `ngalir skills` to get the full machine-readable registry (JSON).

Detailed documentation for each node (inputs, outputs, secrets, credentials,
use cases) is available in [nodes/](nodes/).

## Core

| Node | Description |
|---|---|
| [na-echo](nodes/na-echo.md) | Echo a message (reference / test node) |
| [na-file](nodes/na-file.md) | File read / write |
| [na-http](nodes/na-http.md) | HTTP client (GET / POST / PUT / DELETE / PATCH) |
| [na-jsonpath](nodes/na-jsonpath.md) | JSON path extractor with jq-compatible filtering (`.[]`, slices, pipes) |
| [na-vault](nodes/na-vault.md) | Credential storage (resolves `vault://` refs) |

## Database

| Node | Description |
|---|---|
| [na-db-postgres](nodes/na-db-postgres.md) | PostgreSQL query execution |
| [na-db-mysql](nodes/na-db-mysql.md) | MySQL query execution |
| [na-db-sqlite](nodes/na-db-sqlite.md) | SQLite query execution |

## Data processing

| Node | Description |
|---|---|
| [na-csv](nodes/na-csv.md) | Streaming CSV processor (read / write) |
| [na-excel](nodes/na-excel.md) | Excel (.xlsx) processor (read / write, sheet & range selection) |
| [na-google-sheets](nodes/na-google-sheets.md) | Google Sheets processor (read / append, OAuth2) |
| [na-xml](nodes/na-xml.md) | XML parser / generator (attributes, nested elements, arrays) |
| [na-yaml](nodes/na-yaml.md) | YAML parser / generator (read from string/file, write to stdout/file) |
| [na-parquet](nodes/na-parquet.md) | Apache Parquet reader (column name override, all primitive types) |
| [na-fixedwidth](nodes/na-fixedwidth.md) | Fixed-width text parser / generator (column definitions with start/width, optional headers) |
| [na-html](nodes/na-html.md) | HTML table extractor & CSS selector scraper (extract text/attributes, parse tables to NDJSON) |
| [na-json](nodes/na-json.md) | JSON transform: read, write (pretty), pick (select fields), omit (remove fields), merge (deep merge objects) |
| [na-zip](nodes/na-zip.md) | Archive compressor / decompressor: zip (multi-file) and gzip (single file), list archive contents |

## AI

| Node | Description |
|---|---|
| [na-llm](nodes/na-llm.md) | LLM chat completions (OpenAI / Anthropic / compatible) |

## Integrations

| Node | Description |
|---|---|
| [na-slack](nodes/na-slack.md) | Slack messaging (post message, read channel history, OAuth2) |
| [na-telegram](nodes/na-telegram.md) | Telegram bot (send message, get updates) |
| [na-discord](nodes/na-discord.md) | Discord messaging (webhook, bot token, read messages) |
| [na-notion](nodes/na-notion.md) | Notion API (query database, get page, create/update pages, append blocks) |
| [na-stripe](nodes/na-stripe.md) | Stripe API (list/create customers, list/create/retrieve payments) |
| [na-s3](nodes/na-s3.md) | S3-compatible object storage (read/write/list/delete, AWS SigV4) |
| [na-airtable](nodes/na-airtable.md) | Airtable (list/get/create/update/delete records) |
| [na-twilio](nodes/na-twilio.md) | Twilio SMS / WhatsApp messaging |

## Triggers / daemons

| Node | Description |
|---|---|
| [na-webhook](nodes/na-webhook.md) | HTTP server that triggers flow execution |
| [na-schedule](nodes/na-schedule.md) | Cron-based flow scheduler |
| [na-email](nodes/na-email.md) | SMTP email sender |

## Building a custom node

Use `ngalir init-node` for an interactive scaffold that generates a complete
`crates/na-<name>/` crate with Cargo.toml, main.rs, manifest, and test
skeleton. See [Node Contract](node-contract.md) for the binary protocol.
