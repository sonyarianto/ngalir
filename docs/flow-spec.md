# Flow Spec

Declarative DAG executed by the Ngalir Orchestrator.
Language: **YAML** (human + AI friendly), parsed internally into a typed Rust
struct. JSON is also accepted (superset of YAML).

## Shape

```yaml
version: 1
name: daily-report
description: "Pull rows, transform, email the team"
concurrency: 8
nodes:
  - id: src
    use: na-data            # -> resolves to binary `na-data`
    with:                   # static params (literal values)
      query: "SELECT id, amount FROM orders WHERE day = current_date"
      connection: vault://db/prod
  - id: transform
    use: na-jq
    with:
      filter: ".rows[] | {id, amount}"
    inputs:                 # dynamic wiring from upstream outputs
      data: src.rows        # <upstream_id>.<output_field>
  - id: notify
    use: na-mail
    with:
      to: team@example.com
    inputs:
      body: transform.result
    on_error: stop          # stop | continue | retry(N)
  - id: alert
    use: na-mail
    when: "{{ src.row_count > 1000 }}"   # node only runs if condition true
    inputs:
      body: src.rows
```

## Fields

- `version` — Flow Spec schema version (currently `1`).
- `concurrency` — max parallel node executions (default `8`).
- `nodes[].id` — unique within the flow; referenced by downstream wiring.
- `nodes[].use` — node name; Orchestrator looks up the `na-<use>` binary and
  reads its manifest via `--describe` to validate `with` + `inputs`.
- `nodes[].with` — literal params merged into the node's input JSON.
- `nodes[].inputs` — values wired from upstream node outputs, addressed as
  `<upstream_id>.<output_field>` (dot-path).
- `nodes[].when` — optional condition; node is skipped unless it evaluates true.
- `nodes[].on_error` — `stop` (default), `continue`, or `retry(N)`.

## Execution semantics

- **Topological order**: the Orchestrator builds a DAG from `inputs`/`when`
  wiring and schedules nodes once all upstreams complete.
- **Concurrency**: nodes with no dependency edge run in parallel, bounded by
  `concurrency` (worker pool / semaphore).
- **Validation before run**: every node's resolved input is checked against its
  manifest JSON Schema. Invalid flow fails fast with a clear error.
- **Secrets**: any `vault://` value in `with` is resolved by `na-vault` and
  injected into the node env as `NGALIR_SECRET_*` before spawn.
- **Branching**: via `when:` expression on a node (skips node if false).
- **Loops / iteration**: deferred to v2 (map over streamed arrays).

## Decisions (LOCKED v1)

- **B. Language**: YAML, parsed to the same Rust type as JSON.
- **Wiring**: dot-path `src.rows` (field names are unique per manifest).
- **Branching**: `when:` expression per node.
- **Observability**: every node run emits a structured JSON log line (node id,
  duration, exit code, bytes) — implemented in Orchestrator v1.
- **Resource limits**: bounded concurrency via semaphore (`concurrency`).

## Deferred to v2

- State / resume-after-crash persistence of intermediate results.
- Fan-out iteration over array/streamed outputs (map-style).
- Visual builder (maps Flow Spec <-> graph UI).
