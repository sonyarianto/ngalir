# Nodes

Ngalir discovers nodes dynamically — any `na-<name>` binary on `PATH` or `NGALIR_NODE_PATH` is available. Run `ngalir nodes` to list what's installed or `ngalir search <keyword>` to search the remote registry.

`ngalir install <name>` downloads and installs a node binary from the latest GitHub release.

Detailed documentation for each node (inputs, outputs, secrets, credentials, use cases) is available in the [GitHub docs/nodes/](https://github.com/sonyarianto/ngalir/tree/main/docs/nodes). The machine-readable registry is at [registry.json](https://github.com/sonyarianto/ngalir/blob/main/docs/registry.json).

## Core

| Node | Description |
|---|---|
| [na-echo](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-echo.md) | Echo a message (reference / test node) |
| [na-file](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-file.md) | File read / write |
| [na-http](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-http.md) | HTTP client (GET / POST / PUT / DELETE / PATCH) |
| [na-jsonpath](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-jsonpath.md) | JSON path extractor with jq-compatible filtering |
| [na-vault](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-vault.md) | Credential storage (resolves `vault://` refs) |

## Database

| Node | Description |
|---|---|
| [na-db-postgres](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-db-postgres.md) | PostgreSQL query execution |
| [na-db-mysql](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-db-mysql.md) | MySQL query execution |
| [na-db-sqlite](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-db-sqlite.md) | SQLite query execution |

## Data Processing

| Node | Description |
|---|---|
| [na-csv](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-csv.md) | Streaming CSV processor (read / write) |
| [na-excel](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-excel.md) | Excel (.xlsx) processor (read / write, sheet & range selection) |
| [na-google-sheets](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-google-sheets.md) | Google Sheets processor (read / append, OAuth2) |
| [na-xml](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-xml.md) | XML parser / generator |
| [na-yaml](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-yaml.md) | YAML parser / generator |
| [na-parquet](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-parquet.md) | Apache Parquet reader |
| [na-fixedwidth](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-fixedwidth.md) | Fixed-width text parser / generator |
| [na-html](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-html.md) | HTML table extractor & CSS selector scraper |
| [na-json](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-json.md) | JSON transform: read, write, pick, omit, merge |
| [na-zip](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-zip.md) | Archive compressor / decompressor (zip, gzip) |

## AI

| Node | Description |
|---|---|
| [na-llm](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-llm.md) | LLM chat completions (OpenAI / Anthropic / compatible) |

## Integrations

| Node | Description |
|---|---|
| [na-slack](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-slack.md) | Slack messaging (post message, read channel history, OAuth2) |
| [na-telegram](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-telegram.md) | Telegram bot (send message, get updates) |
| [na-discord](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-discord.md) | Discord messaging (webhook, bot token, read messages) |
| [na-notion](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-notion.md) | Notion API (query database, get page, create/update pages) |
| [na-stripe](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-stripe.md) | Stripe API (list/create customers, list/create/retrieve payments) |
| [na-s3](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-s3.md) | S3-compatible object storage (read/write/list/delete) |
| [na-airtable](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-airtable.md) | Airtable (list/get/create/update/delete records) |
| [na-twilio](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-twilio.md) | Twilio SMS / WhatsApp messaging |

## Triggers / Daemons

| Node | Description |
|---|---|
| [na-webhook](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-webhook.md) | HTTP server that triggers flow execution |
| [na-schedule](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-schedule.md) | Cron-based flow scheduler |
| [na-email](https://github.com/sonyarianto/ngalir/blob/main/docs/nodes/na-email.md) | SMTP email sender |

## Observability

All daemon services expose Prometheus metrics and health endpoints:

| Service | Metrics port | Endpoints |
|---|---|---|
| `na-webhook` | 9091 (configurable) | `/health`, `/metrics` |
| `na-schedule` | 9092 (configurable) | `/health`, `/metrics` |
| `ngalir` (opt-in) | `--metrics-port N` | `/health`, `/metrics` |
