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
- ✅ 15 na-* node binaries + ngalir orchestrator, all containerised: echo, file, http, jsonpath, vault, db-postgres, db-mysql, db-sqlite, webhook, schedule, email, csv, excel, google-sheets, llm
- ✅ Data Processing phase: CSV, Excel, and Google Sheets nodes complete
- ✅ NDJSON streaming output for long-running nodes
- ✅ Checkpoint / resume with atomic state files
- ✅ Secret env var injection (`NGALIR_SECRET_*`)
- ✅ Trigger nodes: webhook (HTTP server), schedule (cron daemon), email (SMTP)
- ✅ Per-provider DB split (postgres / mysql / sqlite)

**What could be improved:**

| Gap | Why It Matters |
|-----|----------------|
| `na-jsonpath` was dot-path only, now jq-compatible | Upgraded with `.[]`, slices, pipes, object reconstruction |
| No Docker images or container orchestration | Dockerfile + docker-compose + CI/CD pipeline ready |
| No Prometheus metrics or health endpoints | /health + /metrics on webhook, schedule, orchestrator |
| Large payloads held in memory | OOM on files > 100MB |
| No flow composition (subflows / includes) | Duplication across similar flows |
| No release automation | Manual build & publish |
| No Web UI or AI workflow generation | Steep learning curve for non-devs |

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

### 4.1 jq-compatible filtering for na-jsonpath ✅ (Complete)

**Upgraded from simple dot-path to jq-compatible pipeline filter:**
- Array iteration: `.[]` → returns array of elements (or `[]` on non-array)
- Array slicing: `.[0:5]`, `.[-2:]`, `.[:3]`, `.[2:]` with negative index support
- Pipe chaining: `.[] | {id, name}` → map + select fields across pipeline stages
- Object reconstruction: `{key1, key2}` and `{new_key: .nested.key}` syntax
- Compound paths: `items[]`, `items[0:3]`, `items[].name` parsed as key + array operator
- Backward compatible: all existing dot-path syntax still works
- 18 unit tests (was 6) covering: pipeline stages, object reconstruction, array slices with negative indices, nested dot-paths in reconstruction

### 4.2 Docker images & CI/CD ✅ (Complete)

**Multi-stage Dockerfile, docker-compose, and CI/CD pipeline:**
- `Dockerfile`: multi-stage build (rust:latest → debian:trixie-slim), copies all 16 binaries to `/usr/local/bin/`, minimal 247 MB image
- `docker-compose.yml`: webhook (port 8080) and schedule daemon services with persistent volumes
- `.github/workflows/ci.yml`: lint (fmt + clippy) → test → (on tag) Docker push + release draft with tarball
- `README.md` updated with Docker usage examples

**Docker image contains all 15 `na-*` node binaries + `ngalir` orchestrator, ready for flow execution and daemon deployment.**

### 4.3 Observability (metrics & health) ✅ (Complete)

**Prometheus metrics and health endpoints added to all daemon services:**

- **`na-webhook`**: 
  - `/health` (200 OK) and `/metrics` (Prometheus text format) endpoints
  - `na_webhook_flow_executions_total{status}` counter (success / failed / spawn_failed / wait_failed)
  - Separate metrics server on port 9091 (`--metrics-port`)
- **`na-schedule`**: 
  - Embedded metrics HTTP server with `/health` and `/metrics`
  - `na_schedule_triggers_total{status}` counter (triggered / succeeded / failed / spawn_failed / wait_failed)
  - `--metrics-port` (default 9092)
- **Orchestrator (`ngalir`)**:
  - `ngalir_flow_executions_total{status}` counter (started / completed)
  - `ngalir_node_executions_total{node_type,status}` counter (success / failed)
  - Optional `--metrics-port` flag to expose Prometheus HTTP endpoint
  - Structured `tracing` events with `metric = "flow.duration"`, `duration_ms`, `node_count`, `error_count`
- Added `prometheus` crate to webhook, schedule, and orchestrator

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

### 4.6 `na-llm` node ✅ (Complete)

**LLM chat completion node implemented:**
- OpenAI / Anthropic / compatible API via `/chat/completions` endpoint
- Configurable: model, messages array, prompt shortcut, temperature, max_tokens, api_base
- API key via `api_key` input field or `NGALIR_SECRET_API_KEY` env var (vault integration)
- `streaming: true` — outputs single JSON with content, model, and usage stats
- 8 unit tests covering: manifest, describe, messages/prompt building, request serialization, input validation

---

## Phase 6: AI-Native Workflow Studio (Weeks 8-12)

This phase transforms Ngalir from a DAG workflow engine into an **AI-native
workflow studio** where users describe what they want in natural language and
the system generates, visualizes, and runs the workflow.

### 6.1 Node Skills Registry

**Problem:** Each node has a `Manifest` (name, description, input/output schema)
but no structured metadata that an LLM can reason about when composing flows.

**Target:**
- Extend Manifest with `use_cases: Vec<String>` (tags like `["csv", "etl", "import"]`)
- Add `examples: Vec<{input, output}>` — sample JSON pairs showing typical usage
- Add `see_also: Vec<String>` — related node names (e.g. csv → excel, google-sheets)
- Ship a `ngalir skills` CLI command that outputs the full skills registry as JSON
- The registry becomes the context prompt for AI flow generation

**Effort:** 1-2 days.

### 6.2 AI Flow Generator

**Problem:** Users must manually write YAML flows. A natural-language interface
would lower the barrier dramatically.

**Target:**
- New `ngalir generate` command: takes a natural language prompt, outputs a `.yaml` flow
- Uses `na-llm` internally (or configurable LLM provider via env var)
- Context: the Node Skills Registry (6.1) is injected as system prompt
- Iterative refinement: `ngalir generate --edit prompt2` amends an existing flow
- Output can be piped directly into `ngalir run --flow -`

**Example:**
```
ngalir generate "download orders.csv from SFTP, filter rows where amount > 100, email summary to ops@example.com"
```

**Effort:** 3-4 days.

### 6.3 Web UI (Flow Editor)

**Problem:** No visual interface for building or monitoring workflows.

**Target:**
- Standalone web app (React / Svelte) served by `ngalir serve` (or separate `ngalir-ui` binary)
- Graph editor: drag-and-drop nodes, connect inputs/outputs visually
- Real-time flow execution view: node status (pending/running/done/failed), logs
- Flow library: save, load, share flows
- Authentication: optional basic auth or OAuth2 proxy
- Communication: WebSocket for live updates, REST API for CRUD

**Effort:** 2-3 weeks.

### 6.4 Flow Preview & Debug

**Problem:** Users write flows but can't inspect intermediate data without
running the full pipeline.

**Target:**
- Inline preview: select a node, run it with sample input, see output inline
- Step-through mode: execute flow one node at a time, inspect outputs
- Snapshot comparison: run flow on two different inputs, diff the results
- Integrates with Web UI (6.3) for visual debugging

**Effort:** 4-5 days.

### 6.5 AI-Powered Flow Optimization

**Problem:** Users build flows that work but may be inefficient or suboptimal.

**Target:**
- `ngalir optimize flow.yaml` — AI suggests improvements (parallelism, caching, node choice)
- Cost estimation: estimate API costs for flows using `na-http`, `na-llm`, `na-google-sheets`
- Automatic retry configuration: AI sets sensible retry policies based on node types

**Effort:** 3-5 days.

---

## Phase 5: Data Processing (Weeks 6-8) ✅ (Complete)

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

### 5.3 `na-google-sheets` — Google Sheets processor ✅ (Complete)

**Streaming Google Sheets node implemented (jsonwebtoken + reqwest):**
- **Read** a spreadsheet by ID + sheet name/range → NDJSON rows (one per line)
- **Append** rows to a sheet from JSON rows array
- OAuth2 service account authentication via `jsonwebtoken` (RS256 JWT assertion)
- Credentials resolved via env var `NGALIR_SECRET_CREDENTIALS` (vault integration) or file path / inline JSON
- `has_headers: true` (default) treats first row as field names; `false` uses A, B, C... column labels
- `range` supports A1 notation (e.g. `Sheet1!A1:C10`)
- Streaming output for read (`streaming: true`)
- 9 unit tests covering manifest, credential resolution, input validation
  "sheet": "Sheet1", "range": "A:E" }
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
**5.3 ✅ Google Sheets — done.**

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
