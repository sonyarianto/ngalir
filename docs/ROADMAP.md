# Ngalir Roadmap — From Prototype to Usable MVP

## Status Today (Jul 2026)

Ngalir is a **functional prototype**. The core idea works — subprocess DAG with
JSON pipelining — but several critical gaps prevent real-world use.

**What works:**

- Flow Spec parsing (YAML/JSON)
- DAG execution with topological ordering and bounded concurrency
- Node discovery via `PATH` / `NGALIR_NODE_PATH`
- Pre-flight validation (required inputs, JSON Schema)
- Vault secret resolution
- CLI: `run`, `nodes`, `validate`
- Structured logging (tracing)

**What's broken or missing:**

| Gap | Why It Matters |
|-----|----------------|
| `when:` only checks literal `"false"` | No conditional branching |
| `na-jq` is a dot-path navigator, not jq | Docs promise jq syntax that fails at runtime |
| `na-http` silently nulls non-JSON responses | Broken for any non-JSON API |
| Secret injection uses stdin, not env vars | Contradicts documented security design |
| No cycle detection | Confusing error on cyclic flows |
| No retry backoff | Immediate retry hammers downstream services |
| 0 tests on all 6 node crates | No confidence in basic functionality |
| In-memory data transport only | OOM on large payloads |

---

## Phase 0: Quick Wins (Week 1)

Low-effort fixes that remove immediate sharp edges.

### 0.1 Align docs with reality

- `docs/flow-spec.md`: Remove jq syntax examples (`.[] | {id, amount}`) that
  don't work. Replace with actual dot-path examples.
- `docs/node-contract.md`: Update secret injection section to match current
  implementation (stdin-based), or commit to fixing it (see Phase 1).
- `docs/ARCHITECTURE.md`: Remove mention of `na-llm`. Add explicit status
  badges for each feature (✅ done / 🟡 partial / ❌ missing).

### 0.2 Fix `na-http` non-JSON handling

**Problem:** Line 101 — `resp.json().await.unwrap_or(Value::Null)` silently
returns null for any non-JSON response (HTML, plain text, XML).

**Fix:**

```rust
// Try JSON first, fall back to text
let resp_body: Value = if let Ok(json) = resp.json().await {
    json
} else {
    let text = resp.text().await.unwrap_or_default();
    Value::String(text)
};
```

**Effort:** 10 minutes, 3 lines changed.

### 0.3 Add cycle detection

**Problem:** DAG loop in line 308-320 bails with generic "cycle or unresolved
dependency" with no indication of which nodes form the cycle.

**Fix:** Add a simple DFS visited-set before execution to detect and report
cycles with the node IDs involved.

```rust
fn detect_cycle(nodes: &[NodeSpec]) -> Result<()> {
    // DFS with 3-color marking (white/gray/black)
    // Report cycle path on detection
}
```

**Effort:** ~30 lines, 30 minutes.

### 0.4 Add exponential backoff to retry

**Problem:** `on_error: retry(N)` loops immediately with no delay.

**Fix:**

```rust
if attempt > 0 {
    let delay = Duration::from_millis(100 * 2u64.pow(attempt - 1));
    tokio::time::sleep(delay).await;
}
```

**Effort:** 3 lines, 5 minutes.

### 0.5 Rename `na-jq` to `na-jsonpath` (OR align docs)

**Problem:** The name `na-jq` implies full jq support. Users will try
`.[] | {id, amount}` and get `Value::Null`.

Two options — pick one:

**Option A (Recommended):** Rename binary + manifest to `na-jsonpath`.
- Change `name: "na-jq"` → `name: "na-jsonpath"` in manifest output
- Rename Cargo package + directory to `na-jsonpath`
- Update docs to show dot-path examples: `filter: "rows.0.name"`
- **Effort:** 15 minutes.

**Option B:** Implement basic jq-like filtering:
- Support `.[]` for array iteration
- Support `|` pipe for chaining
- Support `{id, amount}` object reconstruction
- **Effort:** 3-5 days (significant). Defer to Phase 2.

---

## Phase 1: Expression Engine (Week 2)

The single biggest gap. Without `{{ }}` expressions, flows are linear sequences
with no conditional logic and no dynamic value construction.

### 1.1 Choose an expression engine

Three viable options:

| Approach | Pros | Cons | Effort |
|----------|------|------|--------|
| **Rhai** (embedded scripting) | Full expression language, already Rust-native, small footprint | ~40 deps, ~500KB binary bloat | 2-3 days |
| **MiniJinja** (Mustache/Jinja) | Familiar syntax, Jinja-compatible | Heavier dep tree | 2-3 days |
| **Custom minimal parser** | Zero deps, exact fit, maximal control | Need to write & debug parser | 3-5 days |

**Recommendation: Rhai.** It's the sweet spot — provides boolean expressions
for `when:` and string interpolation for `with:` values, with minimal
integration effort. The syntax (`>`, `<`, `==`, `&&`, `||`) is intuitive.

### 1.2 Implement `when:` evaluation

In `execute_node()` (line 432-436 of `na-orchestrator/src/main.rs`), replace:

```rust
// Before (stub)
if let Some(cond) = &node.when {
    if cond.trim() == "false" {
        info!(when = "false", "node skipped");
        return Ok((node.id.clone(), Value::Null));
    }
}
```

With:

```rust
// After
if let Some(cond) = &node.when {
    let engine = rhai::Engine::new();
    // Inject upstream outputs as variables
    for (id, val) in outputs.iter() {
        engine.set_var(id, val.clone());
    }
    let result: bool = engine.eval(cond)?;
    if !result {
        info!(when = cond, "node skipped");
        return Ok((node.id.clone(), Value::Null));
    }
}
```

### 1.3 Implement template interpolation in `with:` values

In `build_input()` (line 365-377), scan string values for `{{ }}` patterns and
replace them with resolved upstream values.

```rust
fn interpolate(template: &str, outputs: &HashMap<String, Value>) -> String {
    // Regex: {{ <id>.<field> }} -> resolve from outputs
    RE.replace_all(template, |caps: &Captures| {
        resolve_ref(&caps[1], outputs).to_string()
    })
}
```

### 1.4 Tests for expression engine

- `when: "true"` → node runs
- `when: "false"` → node skips
- `when: "{{ a.count > 5 }}"` → evaluated against upstream output
- Template interpolation with `with:` values

---

## Phase 2: Node Hardening (Week 2-3)

### 2.1 Add tests for all node crates

Each node crate is <120 lines — adding unit tests is a 1-hour task each.

Minimum bar per node:

- **na-echo:** Test `--describe` output, test echo round-trip.
- **na-http:** Test manifest validity, test error on missing URL (no actual HTTP).
- **na-jq / na-jsonpath:** Test `resolve_path()` for dot-path, array index, nested.
- **na-file:** Test read/write round-trip with tempfile.
- **na-db:** Test manifest validity (no actual DB connection).
- **na-vault:** Test vault file parsing, test `vault://` key resolution.
- **na-orchestrator:** 2 integration tests exist — add a test for `when:` skip,
  retry exhaustion, cycle detection.

**Effort:** ~6 hours across all crates.

### 2.2 Fix secret env var injection

**Problem:** `docs/node-contract.md` says secrets are injected as
`NGALIR_SECRET_<NAME>` env vars. The orchestrator instead resolves `vault://`
refs in the input JSON before sending to stdin. This means secrets travel in
the JSON pipe, visible in process logs.

**Fix (align code with docs):**

1. In `execute_node()`, after vault resolution, extract secrets from the input
   JSON and set them as child process env vars instead:

```rust
let secrets = resolve_vault_refs_returning_secrets(&mut input).await?;
let mut cmd = Command::new(&bin.binary);
for (name, val) in &secrets {
    cmd.env(format!("NGALIR_SECRET_{}", name.to_uppercase()), val);
}
cmd.stdin(std::process::Stdio::piped())
   .stdout(std::process::Stdio::piped())
   .stderr(std::process::Stdio::piped());
```

2. Update `na-contract` to provide a helper `read_secret(name)` that reads
   `NGALIR_SECRET_<NAME>` from env, so nodes don't need to call
   `std::env::var()` manually.

**Effort:** Full day (touches contract, orchestrator, vault resolution, docs).

### 2.3 `na-jq` basic jq-like implementation (if not renamed)

If you chose Option A (rename) in Phase 0, skip this. If Option B, implement:

- Array map: `.[]` → yield each element
- Object reconstruction: `{id, amount}` → select fields
- Pipe: `filter: ".[] \| {id, name}"`
- Array slice: `.[0:5]`

**Effort:** 3-5 days.

### 2.4 `na-db` type coverage

Add missing PostgreSQL type support to `value_at()`:

```rust
// Add these before the final Value::Null return
if let Ok(v) = row.try_get::<Option<i16>, _>(col) { ... }
if let Ok(v) = row.try_get::<Option<f32>, _>(col) { ... }
if let Ok(v) = row.try_get::<Option<u32>, _>(col) { ... }
if let Ok(v) = row.try_get::<Option<u64>, _>(col) { ... }
if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(col) { ... }
// JSON/B, timestamptz, numeric, uuid, bytea
```

**Effort:** 30 minutes.

---

## Phase 3: Real-World Use Cases (Week 3-4)

### 3.1 Checkpoint / resume

Add optional state persistence so the orchestrator can resume after a crash.

**Design:**

```
state/
  <flow-name>-<timestamp>.json  # checkpoint file
```

- After each node completes, write its output to a checkpoint JSON file
- On restart with `--resume`, skip nodes whose output is already checkpointed
- Keep it optional (disabled by default, opt-in via `--state-dir` flag)

**Effort:** 2-3 days.

### 3.2 NDJSON streaming

The `Manifest` has a `streaming: bool` field but the orchestrator ignores it.

**Fix:**

- If a node's manifest says `streaming: true`, the orchestrator reads stdout
  line-by-line (NDJSON) instead of as a single JSON blob
- Each line gets forwarded as a separate execution to downstream nodes
- Required for `na-http` to stream large responses, or for `na-db` to stream
  large result sets

**Effort:** 2-3 days.

### 3.3 Add trigger/headless nodes

To make flows useful without manual `ngalir run`:

- **na-webhook**: Simple HTTP server that accepts a POST and starts a flow
- **na-schedule**: Cron-like timer that triggers a flow on a schedule
- **na-email**: (placeholder) Send email via SMTP

These turn Ngalir from a CLI demo into something that can run unattended.

**Effort:** 4-5 days per node, ~2 weeks total.

---

## Summary: Two Paths Forward

### Path A: Stabilize (2 weeks) — Recommended next

Focus on making existing features reliable and usable.

| Step | Effort | Impact |
|------|--------|--------|
| 0.1 Align docs | 2h | Removes misleading promises |
| 0.2 Fix na-http non-JSON | 10min | Unblocks HTTP APIs |
| 0.3 Cycle detection | 30min | Better error messages |
| 0.4 Retry backoff | 5min | Production hygiene |
| 0.5 Rename na-jq → na-jsonpath | 15min | Honest naming |
| 1.0 Expression engine (rhai) | 2-3d | **Enables conditional flows** |
| 2.1 Node tests | 6h | Safety net |
| 2.2 Fix secret injection | 1d | Security + docs alignment |
| 2.4 na-db type coverage | 30min | DB compatibility |
| **Total** | **~7 days** | |

After Path A, Ngalir can handle:

```
DB query -> JSON transform -> HTTP POST
HTTP GET  -> conditionally write to file
Echo input with vault secrets
Linear multi-step pipelines
```

### Path B: Expand (4-6 weeks)

Add new capabilities after Path A.

| Step | Effort |
|------|--------|
| Checkpoint / resume | 2-3d |
| NDJSON streaming | 2-3d |
| na-webhook trigger | 4-5d |
| na-schedule trigger | 4-5d |
| na-email node | 4-5d |
| na-jq full jq impl (if not renamed) | 3-5d |

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
    use: db
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

### After Phase 3 (Real-World)

Scheduled ETL pipelines, webhook-triggered workflows, email notifications —
all running unattended with crash recovery.
