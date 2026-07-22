# Ngalir Roadmap — From Prototype to Production

## Status Today (Jul 2026)

Ngalir is a **production-ready low-code platform** suitable for unattended ETL pipelines,
webhook-triggered workflows, scheduled batch jobs, and credential-aware API integrations
with an n8n-class web UI.

**What works:**

- ✅ Flow Spec parsing (YAML/JSON)
- ✅ DAG execution with topological ordering and bounded concurrency
- ✅ Node discovery via `PATH` / `NGALIR_NODE_PATH`
- ✅ Pre-flight validation (required inputs, JSON Schema)
- ✅ Vault secret resolution & env-var injection
- ✅ CLI: `run` (with `--input`, `--state-dir`), `nodes`, `validate`, `generate`, `optimize`, `skills`, `serve`
- ✅ Structured logging (tracing, JSON, stderr)
- ✅ Cycle detection with DFS
- ✅ Retry with exponential backoff
- ✅ Rhai expression engine for `when:` and `{{ }}` interpolation
- ✅ 200+ unit + integration tests across all crates
- ✅ `ngalir generate` — AI flow generation from natural language prompts
- ✅ `ngalir optimize` — AI flow optimization with cost estimation and retry suggestions
- ✅ 20 na-* node binaries + ngalir orchestrator, all containerised: echo, file, http, jsonpath, vault, db-postgres, db-mysql, db-sqlite, webhook, schedule, email, csv, excel, google-sheets, llm, xml, yaml, parquet, fixedwidth, html
- ✅ Data Processing phases: CSV, Excel, Google Sheets, XML, YAML, Parquet, Fixed-Width, and HTML nodes complete
- ✅ NDJSON streaming output for long-running nodes
- ✅ Checkpoint / resume with atomic state files
- ✅ Secret env var injection (`NGALIR_SECRET_*`)
- ✅ Trigger nodes: webhook (HTTP server), schedule (cron daemon), email (SMTP)
- ✅ Per-provider DB split (postgres / mysql / sqlite)
- ✅ Web UI: Svelte 5 flow editor with drag-and-drop, real-time execution via WebSocket, step-through debugging, snapshot comparison
- ✅ Canvas UX: wire management (click/delete), zoom & pan, auto-layout (dagre), undo/redo (50-step), keyboard shortcuts
- ✅ Advanced canvas: multi-select & group operations (rubber-band, shift-click, group move/delete/duplicate), wire reconnection (drag endpoints to rewire), live port discovery (node manifests from `/api/skills`), sticky notes (editable text, colors, resize), native YAML import/export
- ✅ Structured credential management: `CredentialSpec` in node manifests, typed credentials with dynamic UI forms, `--test-connection` mode
- ✅ Vault migration: structured credential store with AES-256-GCM at-rest encryption, CRUD via CLI/API/UI
- ✅ Credential API: REST endpoints for CRUD + test-connection (`/api/credentials`)
- ✅ Web UI credentials page: list, add, test, delete; credential dropdown in flow editor PropertyPanel

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

### 4.4 Large payload streaming ✅ (Complete)

**File-based output transport to avoid in-memory buffering:**

- `Manifest.output_mode: Option<String>` — `"file"` enables file-based transport
- Orchestrator creates a temp output dir per flow execution
- Sets `NGALIR_OUTPUT_DIR` env var for nodes with `output_mode: "file"`
- Node writes output to file in that dir, emits file path to stdout
- Orchestrator reads `__file__`-tagged output via `resolve_file_output()`:
  - Handles both bare path strings and `{"__file__": "/path"}` objects
  - Recursively resolves nested file references
- **`na-file`** updated to use `output_mode: "file"`:
  - Large reads write to temp file instead of stdout pipe
  - Orchestrator reads from file on disk, reducing pipe memory pressure
- All existing nodes get `output_mode: None` (defaults to stdout transport)
- `Manifest` deserialization backward-compatible (serde default for new field)

### 4.5 Flow composition (subflows / includes) ✅ (Complete)

**Subflow inlining implemented:**
- `node.use: "@subflow.yaml"` syntax — loads subflow YAML relative to parent flow
- Node ID namespacing: `parent.subnode_id` prefixing prevents ID collisions
- Input mapping: parent inputs mapped to subflow entry nodes by local ID
- Exit node handling: `exit: true` nodes in subflow create passthrough echo nodes
- Recursive expansion: nested subflows resolved depth-first
- `check_cycles()` called after expansion on the flattened node list
- Removed unused `is_subflow()` helper, added `exit: false` to all test fixtures

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

### 6.1 Node Skills Registry ✅ (Complete)

**Structured LLM-readable metadata for all nodes:**
- Manifest extended with `use_cases: Vec<String>` (tags like `["csv", "etl", "import"]`),
  `examples: Vec<Example>` (sample input/output pairs), and `see_also: Vec<String>`
  (related node names)
- All 15 `na-*` node manifests updated with skills metadata
- `ngalir skills` CLI command outputs full JSON registry (all manifests)
- Backward compatible: new fields are `#[serde(default)]`, old manifests still parse
- Clippy clean, all tests pass

**Effort:** 1-2 days. ✅

### 6.2 AI Flow Generator ✅ (Complete)

**`ngalir generate` command implemented:**
- Takes natural language prompt, calls `na-llm` with Node Skills Registry as context
- `--edit path/to/flow.yaml` mode amends an existing flow
- `--model` flag to choose LLM (default: gpt-4o)
- `--output path.yaml` to write to file (default: stdout)
- Uses `NGALIR_SECRET_API_KEY` env var or `api_key` input field for auth
- Extracts YAML from ```yaml code blocks in LLM response
- Clippy clean, all tests pass

**Effort:** 3-4 days. ✅

### 6.3 Web UI (Flow Editor) ✅ (Complete)

**Problem:** No visual interface for building or monitoring workflows.

**Target:**
- Standalone web app (Svelte 5 + Vite) served by `ngalir serve` (default :8080)
- Graph editor: drag-and-drop nodes, connect inputs/outputs visually
- Real-time flow execution view: node status (pending/running/done/failed), logs
- Flow library: save, load, share flows
- Communication: WebSocket for live updates, REST API for CRUD (nodes, skills, run, snapshots)

**Effort:** 2-3 weeks. ✅

**Implemented:**
- Svelte 5 + Vite + TypeScript + Tailwind CSS v4 scaffolded in `ui/`
- Flow editor: Toolbar, NodePalette, FlowCanvas, NodeBlock, PropertyPanel
- Drag & drop nodes, selection, property editing, export/import flow JSON
- Run / Step buttons with real-time status dots via WebSocket
- Node preview panel (input/output/error)
- Step-through execution with Continue/Stop controls
- Snapshots API for comparing flow runs

### 6.4 Flow Preview & Debug ✅ (Complete)

**Problem:** Users write flows but can't inspect intermediate data without
running the full pipeline.

**Target:**
- Inline preview: select a node, run it with sample input, see output inline
- Step-through mode: execute flow one node at a time, inspect outputs
- Snapshot comparison: run flow on two different inputs, diff the results
- Integrates with Web UI (6.3) for visual debugging

**Effort:** 4-5 days. ✅

**Implemented:**
- Step-through execution: `step: true` on POST `/api/run`, StepCommand via WS (`continue`/`stop`), waits between batches
- Node preview: PropertyPanel shows input/output/error per node, `node_input_ready` event emitted before node runs
- Snapshot diffing: in-memory snapshot store, `/api/snapshots` lists runs, `/api/snapshots/diff?from=0&to=1` compares node outputs
- UI: Step button + Continue/Stop controls in Toolbar

### 6.5 AI-Powered Flow Optimization ✅ (Complete)

**Problem:** Users build flows that work but may be inefficient or suboptimal.

**Target:**
- `ngalir optimize flow.yaml` — AI suggests improvements (parallelism, caching, node choice)
- Cost estimation: estimate API costs for flows using `na-http`, `na-llm`, `na-google-sheets`
- Automatic retry configuration: AI sets sensible retry policies based on node types

**Effort:** 3-5 days. ✅

**Implemented:**
- `ngalir optimize <flow>` CLI command: analyzes flow, runs AI (na-llm) for optimization suggestions
- `estimate_node_cost()` heuristic per node type (llm=80, db=30, http=20, etc.)
- Auto-retry detection: flags idempotent nodes missing `on_error`, vault users without retry

### 6.6 Canvas UX & Productivity ✅ (Complete)

**Problem:** The UI flow editor lacked essential canvas interactions — no way to delete wires, zoom/pan, auto-layout, undo/redo, or keyboard shortcuts.

**Target:**
- Wire management: click-to-select, delete wires, auto-cleanup on node delete
- Canvas zoom & pan: scroll wheel zoom centered on cursor, middle-click/ctrl-drag to pan
- Auto-layout: dagre-based algorithm arranges nodes topologically
- Undo/redo: 50-deep history stack with keyboard shortcuts and toolbar buttons
- Keyboard shortcuts: Delete/Backspace (remove), Escape (deselect), Ctrl+S (save), Ctrl+Z/Y (undo/redo)

**Effort:** 1-2 days. ✅

**Implemented:**
- Wires are clickable via transparent SVG hit paths (stroke-width 14); selected wires glow with purple shadow and thicker stroke
- Drag from output port creates bezier wire; drop on input port completes connection; drop on empty space cancels
- `selectedWireId` state + selectWire/removeWire functions with undo support
- `panX`, `panY`, `zoom` state with CSS transform on canvas content container
- `screenToCanvas()` coordinate conversion for node dragging and wire creation at any zoom level
- Dagre layout: `autoLayout()` computes left-to-right positions from wire topology; Layout button in toolbar
- Undo/redo: `snapshot()` deep-copies nodes/wires; `pushUndo()` before each mutation; 50-entry stack with Ctrl+Z/Y or ↩/↪ buttons
- Keyboard handler on `<svelte:window>` covers Delete, Escape, Ctrl+S, Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y

---

## Phase 7: Advanced Canvas Interactions (Weeks 13-14) ✅ (Complete)

**All 5 sub-items implemented in `ui/`:**

### 7.1 Multi-select & Group Operations ✅
- Rubber-band selection: drag on empty canvas to select nodes in rectangle
- Shift-click to add/remove nodes from multi-selection
- Group move, delete, duplicate (Ctrl+D)

### 7.2 Wire Reconnection ✅
- Grab endpoint circles on selected wire and drag to a different port
- Releasing on a valid port rewires; releasing on empty space removes wire
- Bezier curve follows cursor during reconnection

### 7.3 Live Node Port Discovery ✅
- Fetches `/api/skills` on mount for node manifests with real input/output schemas
- Displays actual port names (e.g. `path`, `delimiter` for CSV) from manifest
- Unconnected ports shown dimmed; PropertyPanel shows manifest port info

### 7.4 Sticky Notes / Canvas Annotations ✅
- `NoteBlock` component with editable text, 6 color choices, resize
- Draggable, selectable, persists in flow YAML/JSON as `notes` array
- PropertyPanel shows note properties when selected

### 7.5 Native YAML Import/Export ✅
- `js-yaml` dependency added
- Export dropdown offers YAML and JSON; file open auto-detects format
- Positions and notes preserved in roundtrip

**Effort:** 9-13 days. ✅

---

## Phase 8: Additional Data Nodes (Week 15) ✅ (Complete)

**5 new data format node crates implemented in `crates/`:**

### 8.1 `na-xml` — XML Parser/Generator ✅
- Parse XML to JSON using `quick-xml` v0.36
- Generate XML from JSON with configurable root element name
- Handles attributes (`@attr`), text content (`#text`), nested elements, arrays of same-named siblings
- Self-closing empty elements, escaped text content
- 6 unit tests covering: parse, generate, escaped content, manifest, describe, invalid action

### 8.2 `na-yaml` — YAML Parser/Generator ✅
- Parse YAML to JSON using `serde_yaml` v0.9
- Generate YAML from JSON
- File read/write and stdin/stdout support
- 8 unit tests covering: read from string, read from file, write to stdout, write to file, manifest, describe, invalid action, missing input

### 8.3 `na-parquet` — Apache Parquet Reader ✅
- Read Parquet files to JSON rows using Apache Arrow `parquet` v54
- Column name override support
- 5 unit tests covering: manifest, describe, field-to-JSON conversion for all primitive types, error handling for nonexistent files

### 8.4 `na-fixedwidth` — Fixed-Width Text Parser ✅
- Parse fixed-width text files with column definitions (start, width)
- Generate fixed-width text from JSON rows with automatic padding
- Optional `has_headers` support for reading
- 8 unit tests covering: read, write, read with headers, extract field, pad field, manifest, describe, invalid action

### 8.5 `na-html` — HTML Table Extractor ✅
- CSS selector-based extraction using `scraper` v0.22
- HTML table parsing to JSON rows
- Read from inline HTML string or file path
- 8 unit tests covering: CSS selector extraction, HTML table parsing, manifest, describe, invalid action, missing input, nonexistent file, escaped output

**Effort:** 4-6 days. ✅

---

## Phase 9: Additional Data Nodes — JSON, ZIP (Week 16) ✅ (Complete)

**2 more data format node crates:**

### 9.1 `na-json` — JSON Transformer ✅
- Read JSON from file or stdin, write to file or stdout
- Optional pretty-print, optional array extraction from a path
- 8 unit tests covering: read, write, pretty-print, array extraction, manifest, describe, invalid action, missing input

### 9.2 `na-zip` — ZIP/Gzip Compressor ✅
- Compress files/directories to ZIP or individual files to Gzip
- List archive contents
- Decompress ZIP and Gzip archives
- 8 unit tests covering: compress/decompress roundtrip for ZIP and Gzip, list contents, manifest, describe, invalid action

**Effort:** 2-3 days. ✅

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

### After Phase 5 + Phase 8 (Data Processing) ✅

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
