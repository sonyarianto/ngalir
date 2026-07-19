# Ngalir Roadmap — From Prototype to Production

## Status Today (Jul 2026)

Ngalir is a **usable MVP** suitable for unattended ETL pipelines, webhook-triggered
workflows, and scheduled batch jobs.

**What works:**

- ✅ Flow Spec parsing (YAML/JSON)
- ✅ DAG execution with topological ordering and bounded concurrency
- ✅ Node discovery via `PATH` / `NGALIR_NODE_PATH`
- ✅ Pre-flight validation (required inputs, JSON Schema)
- ✅ Vault secret resolution & env-var injection
- ✅ CLI: `run` (with `--input`, `--state-dir`), `nodes`, `validate`
- ✅ Structured logging (tracing, JSON, stderr)
- ✅ Cycle detection with DFS
- ✅ Retry with exponential backoff
- ✅ Rhai expression engine for `when:` and `{{ }}` interpolation
- ✅ 92 unit + integration tests across all crates
- ✅ 16 node crates: echo, file, http, jsonpath, vault, db-postgres, db-mysql, db-sqlite, webhook, schedule, email, csv, excel
- ✅ NDJSON streaming output for long-running nodes
- ✅ Checkpoint / resume with atomic state files
- ✅ Secret env var injection (`NGALIR_SECRET_*`)
- ✅ Trigger nodes: webhook (HTTP server), schedule (cron daemon), email (SMTP)
- ✅ Per-provider DB split (postgres / mysql / sqlite)

**What could be improved:**

| Gap | Why It Matters |
|-----|----------------|
| `na-jsonpath` is dot-path only, not jq-compatible | Users expect `.[] | {id}` syntax |
| No Docker images or container orchestration | Hard to deploy in production |
| No Prometheus metrics or health endpoints | No observability in production |
| Large payloads held in memory | OOM on files > 100MB |
| No flow composition (subflows / includes) | Duplication across similar flows |
| No release automation | Manual build & publish |
| No `na-llm` node | Requested by early users |
| No Excel or Google Sheets nodes | Can't process two of the three most common business data formats |

---

## Phase 0: Quick Wins ✅ (Complete)

Low-effort fixes that removed immediate sharp edges.

- **0.1** Align docs with reality — removed jq syntax examples, updated node-contract.md, cleaned up ARCHITECTURE.md
- **0.2** Fix `na-http` non-JSON handling — fallback to text on non-JSON responses
- **0.3** Add cycle detection — DFS with 3-color marking, reports cycle path
- **0.4** Add exponential backoff to retry — `delay = 100ms * 2^(attempt-1)`
- **0.5** Rename `na-jq` to `na-jsonpath` — honest naming, updated docs

## Phase 1: Expression Engine ✅ (Complete)

- Rhai engine for `when:` evaluation (`{{ expr }}` syntax, full boolean logic)
- Template interpolation in `with:` values (`{{ id.field }}` → resolved value)
- Tests: when true/false, boolean expressions, template interpolation

## Phase 2: Node Hardening ✅ (Complete)

- **2.1** Tests added for all 12 node crates (6 → 92 tests)
- **2.2** Secret env var injection — `read_secret()` helper, stripped from stdin
- **2.3** Rename (Option A) chosen — na-jq → na-jsonpath; full jq deferred
- **2.4** na-db type coverage — i16, f32, serde_json::Value, etc.
- DB split: na-db → na-db-postgres, na-db-mysql, na-db-sqlite

## Phase 3: Real-World Use Cases ✅ (Complete)

- **3.1** Checkpoint / resume — `--state-dir` flag, atomic state file (tmp + rename)
- **3.2** NDJSON streaming — `read_stream_output()` helper, line-by-line stdout parsing
- **3.3** Trigger / headless nodes — na-webhook, na-schedule, na-email

---

## Phase 4: Production Polish (Weeks 4-6)

Address the remaining gaps between MVP and production-ready system.

### 4.1 jq-compatible filtering for na-jsonpath

**Problem:** `na-jsonpath` only supports dot-path access (`rows.0.name`).
Users migrating from jq expect `.[] | {id, name}` syntax.

**Target:**
- Array iteration: `.[]` → yield each element as a separate output
- Pipe chaining: `.[] | {id, name}` → map + select fields
- Object reconstruction: `{id, amount}` → build new objects from fields
- Array slice: `.[0:5]` → select sub-range
- Keep dot-path as fallback for simple cases

**Effort:** 3-5 days.

### 4.2 Docker images & CI/CD

**Problem:** No container images or automated release pipeline.

**Target:**
- Multi-stage Dockerfile for orchestrator + all node binaries
- `docker-compose.yml` with webhook + schedule daemon configurations
- GitHub Actions CI/CD:
  - Build & test on every push
  - Docker image build & push on tags
  - Release draft with pre-built binaries on version tags
- Version bumps via `cargo release` or similar tooling

**Effort:** 2-3 days.

### 4.3 Observability (metrics & health)

**Problem:** No way to monitor flow execution in production.

**Target:**
- `na-webhook`: `/health` endpoint, `/metrics` (Prometheus) endpoint
- `na-schedule`: Prometheus counters for triggered / succeeded / failed executions
- Orchestrator: emit metrics via `tracing` or dedicated metrics crate
- Flow-level metrics: execution duration, node counts, error rates

**Effort:** 2-3 days.

### 4.4 Large payload streaming

**Problem:** All node output is buffered in memory before passing to
downstream nodes. Files > 100MB cause OOM.

**Target:**
- `Manifest` gains optional `output_mode: "file"` field
- Nodes write large outputs to temp files instead of stdout
- Orchestrator passes file paths instead of in-memory JSON between nodes
- Streaming nodes (webhook, schedule) use temp file transport for >1MB payloads

**Effort:** 3-5 days.

### 4.5 Flow composition (subflows / includes)

**Problem:** No way to reuse common flow patterns (e.g. "HTTP fetch → parse → store").

**Target:**
- `node.use: "@subflow.yaml"` syntax referencing external flow files
- Subflow nodes expose typed inputs/outputs mapped to the subflow's entry/exit nodes
- Validation: recursive schema check
- Namespacing: subflow node outputs prefixed with node ID

**Effort:** 4-5 days.

### 4.6 `na-llm` node

**Problem:** Early users request LLM API integration (OpenAI, Anthropic, local).

**Target:**
- `na-llm` node that calls OpenAI / Anthropic / compatible API
- Configurable model, prompt, temperature, max_tokens
- Supports `messages` array for chat completions
- API key via secrets/env vars (`NGALIR_SECRET_OPENAI_API_KEY`)
- Streaming support for SSE-based LLM responses

**Effort:** 3-4 days.

---

## Phase 5: Data Processing (Weeks 6-8)

Ngalir handles JSON between nodes but can't process the three most common
business data formats: **CSV**, **Excel (.xlsx)**, and **Google Sheets**.
Adding these unlocks real-world ETL scenarios like "download CSV from FTP →
transform → upload to database" or "sync Google Sheet → Excel report daily."

### 5.1 `na-csv` — CSV processor ✅ (Complete)

**Streaming CSV node implemented:**
- **Read CSV** from file → streams rows as NDJSON lines (one per row) to stdout
- **Write CSV** → accepts JSON rows array, writes to file or stdout
- Options: delimiter (comma/tab), `has_headers` (default true), `columns` (auto-inferred from first row, sorted alphabetically)
- `streaming: true` — each row emitted as a separate NDJSON line
- 13 unit tests covering: read/write with file, stdin, stdout, delimiters, headers, error cases
- Error handling: missing path for read, missing rows for write, file I/O errors

### 5.2 `na-excel` — Excel (.xlsx) processor ✅ (Complete)

**Streaming Excel node implemented (calamine + rust_xlsxwriter):**
- **Read** .xlsx files → streams rows as NDJSON (one per line, columns A, B, C...)
- **Write** .xlsx files from JSON rows array, columns auto-inferred and sorted
- Sheet selection by name or 0-based index
- Cell range selection: `A1:C10` syntax (1-indexed, inclusive)
- Type-aware: integers, floats, strings, booleans, dates (→ ISO 8601)
- Whole-number floats auto-converted to integers on read
- 13 unit tests covering: roundtrip, sheet by name/index, range selection, errors, edge cases

**Output example:**
```json
{ "sheets": { "Sheet1": [{ "Name": "Alice", "Amount": 1500 }, ...] },
  "count": 42 }
```

**Effort:** 2-3 days.

### 5.3 `na-sheets` — Google Sheets processor

**Why third:** Cloud-native, enables real-time collaboration flows.
Requires Google Cloud OAuth2 setup (more complex than file-based nodes).

**Target:**
- **Read** a spreadsheet by ID + sheet name → JSON array
- **Write / append** rows to a sheet
- **Create** new spreadsheets
- OAuth2 authentication via service account or OOB flow
- Secret: `NGALIR_SECRET_GOOGLE_CREDENTIALS` for the JSON key
- Rate-limit aware (Google quota: 60 requests/user/second)

**Input example:**
```json
{ "action": "read", "spreadsheet_id": "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms",
  "sheet": "Sheet1", "range": "A:E" }
```

**Output example:**
```json
{ "rows": [{ "Name": "Alice", "Amount": 1500 }, ...], "count": 42 }
```

**Effort:** 3-4 days.

---

## Later: Other Data Nodes (Candidate)

Once the three core formats are done, consider:

| Node | Description | Effort |
|------|-------------|--------|
| `na-xml` | Parse/generate XML (enterprise SOAP/EDI) | 2-3d |
| `na-yaml` | Parse/generate YAML (config files) | 1d |
| `na-parquet` | Apache Parquet read/write (analytics) | 3-4d |
| `na-fixedwidth` | Fixed-width text (legacy mainframe) | 1-2d |
| `na-html` | HTML table extraction (web scraping) | 1-2d |

---

## Use Cases Enabled at Each Phase

### Today (Prototype)

```yaml
# Linear echo — works but trivial
nodes:
  - id: a
    use: echo
    with:
      message: "hello"
  - id: b
    use: echo
    inputs:
      message: a.echo
```

### After Phase 0 (Quick Wins)

Same as today, but `na-http` can read HTML responses and retry has backoff.

### After Phase 1 (Expression Engine)

```yaml
# Conditional pipeline — real use case
nodes:
  - id: fetch
    use: http
    with:
      url: "https://api.example.com/orders"
  - id: notify
    use: http
    with:
      url: "https://hooks.slack.com/..."
      method: POST
    inputs:
      body: fetch.body
    when: "{{ fetch.status == 200 }}"
```

### After Phase 2 (Node Hardening + Secrets)

```yaml
# ETL with credentials — production-like use case
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
      filter: "id.amount"
  - id: upload
    use: http
    with:
      url: "https://api.example.com/bulk"
      method: POST
    inputs:
      body: transform.result
    on_error: retry(3)
```

### After Phase 3 (Real-World) ✅

```yaml
# Scheduled ETL with webhook trigger
nodes:
  - id: webhook
    use: webhook
    with:
      port: 8080
      path: /etl
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
      filter: "id.amount"
  - id: upload
    use: http
    with:
      url: "https://api.example.com/bulk"
      method: POST
    inputs:
      body: transform.result
    on_error: retry(3)
```

### After Phase 4 (Production Polish)

```yaml
# Self-monitoring, containerized pipeline with subflows
include:
  - etl-base: ./subflows/etl-base.yaml

nodes:
  - id: scheduler
    use: schedule
    with:
      cron: "0 */6 * * *"
  - id: fetch-orders
    use: "@etl-base"
    with:
      query: "SELECT * FROM orders WHERE processed = false"
      target: "https://api.example.com/orders"
  - id: notify
    use: llm
    with:
      model: gpt-4
      prompt: "Summarise {{ fetch-orders.rows | length }} new orders."
    inputs:
      api_key: vault://openai/prod
    when: "{{ fetch-orders.rows | length > 0 }}"
  - id: email-report
    use: email
    with:
      to: "ops@example.com"
      subject: "Daily ETL Summary"
    inputs:
      body: notify.text
```

### After Phase 5 (Data Processing) 🔄

**5.1 ✅ CSV — done.**
**5.2 ✅ Excel — done.**

```yaml
# Download CSV from SFTP → clean → load to database → email report
nodes:
  - id: download
    use: csv
    with:
      action: read
      path: "/data/inventory.csv"
      delimiter: ","
  - id: clean
    use: jsonpath
    inputs:
      data: download.rows
    with:
      filter: "rows.*"
  - id: summary
    use: csv
    with:
      action: write
    inputs:
      rows: clean.result
  - id: archive
    use: sheets
    with:
      action: append
      spreadsheet_id: "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms"
      sheet: "Inventory"
    inputs:
      rows: clean.result
  - id: notify
    use: email
    with:
      to: "ops@example.com"
      subject: "Inventory Sync Complete"
      body: "{{ download.count }} rows processed."
```
