# Ngalir — Architecture Specification

> Status: v1 — production-grade scaffold. Tool like n8n, built in Rust.
> Core idea: flow nodes are standalone CLI binaries (`na-*`). Flows are
> declarative YAML DAGs executed by the `ngalir` Orchestrator.

## High-level layers

1. **Node Contract** — uniform interface every `na-*` binary must implement.
2. **Flow Spec** — declarative YAML/JSON DAG; the single source of truth.
3. **Orchestrator** — Rust engine that validates and executes the Flow Spec.
4. **Credential Layer (`na-vault`)** — secure secret injection at runtime.

See `docs/node-contract.md` for the detailed Node Contract.

## Design principles

- Every node is independently runnable, testable, and discoverable.
- Secrets never travel through `argv` (visible in `ps`); they are injected
  by the Orchestrator via `na-vault` into the node's environment.
- Fail fast: schema/contract violations are caught before execution.

## Repository layout

Cargo workspace (resolver 2):

```
ngalir/
  Cargo.toml                 # workspace + shared workspace.package
  docs/
    ARCHITECTURE.md          # this file
    node-contract.md         # Node Contract v1
    flow-spec.md             # Flow Spec v1 (locked decisions)
  crates/
    na-contract/             # shared lib: Manifest, exit_code, helpers
    na-echo/                 # sample node (implements the contract)
    na-orchestrator/         # engine crate; produces the `ngalir` binary
    na-vault/                # credential storage (resolves vault:// refs)
  examples/
    echo-demo.yaml           # minimal 2-node flow for smoke testing
```

The product entry-point binary is **`ngalir`** (the crate is `na-orchestrator`,
its `[[bin]]` name is `ngalir`). Run a flow with `ngalir run <flow.yaml>`;
it spawns the `na-*` node binaries internally.

## Naming conventions

Locked 2026-07-07:

- **CLI binaries (nodes & services): `na-<name>`** — `na-vault`, `na-db`,
  `na-http`, `na-jsonpath`. Hyphen (not underscore) because these are typed on the
  command line. The Orchestrator resolves a flow node's `use: <name>` to the
  binary `na-<name>`.
- **Infrastructure library crates: `na-<name>`** — `na-contract`.
- **Product entry-point binary: `ngalir`** — the main CLI (crate
  `na-orchestrator`, `[[bin]] name = "ngalir"`). It is the umbrella product,
  not a node, so it intentionally does not carry the `na-` prefix. Usage:
  `ngalir run <flow.yaml>`.
- **A "node" vs a "service" is a role, not a name.** `na-vault` is credential
  *storage* — it is invoked by the Orchestrator to resolve `vault://` refs, and
  is NOT placed as a step in the Flow Spec DAG. Role is declared by usage /
  manifest, not encoded in the binary name.
- Crate names use hyphens; within Rust code the path becomes the underscore
  form (`na_contract`, `na_echo`).

## Status (2026-07-07)

- **Node Contract v1**: LOCKED (subprocess model, JSON Schema, NDJSON, semver,
  standard exit codes).
- **Flow Spec v1**: LOCKED decisions (YAML, dot-path wiring, `when:` branching,
  bounded concurrency, structured logging).
- **Scaffold**: building & running. `ngalir` executes `na-echo` end-to-end
  (`examples/echo-demo.yaml`) with JSON piping between nodes.
- **Schema validation**: pre-flight (binary discovery, manifest parse, required-input
  check) + runtime (JSON Schema validation via `jsonschema` crate). Fail-fast
  enforced — invalid types, missing binaries, and missing required inputs are
  all caught *before* node execution.
- **Structured logging**: `tracing` + `tracing-subscriber` with JSON output.
  Every flow & node run emits a structured span with timing, exit code, and
  diagnostics. One JSON log-line per event (Logstash/Elasticsearch friendly).
- **`na-vault`**: credential storage binary that reads a JSON key-value file
  (default `~/.ngalir/vault.json` or `NGALIR_VAULT_FILE`). Orchestrator
  resolves `vault://<key>` refs at runtime before spawning each node.

## Open questions (deferred)

- A. Subprocess (default, LOCKED for v1) vs compiled plugin (wasm/Rust lib) — v2.
- B. YAML (LOCKED for v1) vs JSON-only.
- C. Inter-node data transport for large payloads (stdin/stdout vs shared file/pipe) — v2.
