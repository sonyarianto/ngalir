# AxisFlow — Architecture Specification

> Status: DRAFT / under discussion. Tool like n8n, built in Rust.
> Core idea: flow nodes are standalone CLI binaries (`af-*`). Flows are composed
> from an AI prompt instead of a visual builder (visual builder deferred).

## High-level layers

1. **Node Contract** — uniform interface every `af-*` binary must implement.
2. **Flow Spec** — declarative YAML/JSON DAG produced by the AI Planner.
3. **Orchestrator** — Rust engine that validates and executes the Flow Spec.
4. **AI Planner** — turns a natural-language prompt into a Flow Spec.
5. **Credential Layer (`af-vault`)** — secure secret injection at runtime.

See `docs/node-contract.md` for the detailed Node Contract.

## Design principles

- Every node is independently runnable, testable, and discoverable.
- The AI never executes code; it only *generates* a Flow Spec that the
  Orchestrator validates against node manifests before running.
- Secrets never travel through `argv` (visible in `ps`); they are injected
  by the Orchestrator via `af-vault` into the node's environment.
- Fail fast: schema/contract violations are caught before execution.

## Repository layout

Cargo workspace (resolver 2):

```
axisflow/
  Cargo.toml                 # workspace + shared workspace.package
  docs/
    ARCHITECTURE.md          # this file
    node-contract.md         # Node Contract v1
    flow-spec.md             # Flow Spec v1 (locked decisions)
  crates/
    af-contract/             # shared lib: Manifest, exit_code, helpers
    af-echo/                 # sample node (implements the contract)
    af-orchestrator/         # engine crate; produces the `axisflow` binary
  examples/
    echo-demo.yaml           # minimal 2-node flow for smoke testing
```

The product entry-point binary is **`axisflow`** (the crate is `af-orchestrator`,
its `[[bin]]` name is `axisflow`). Run a flow with `axisflow run <flow.yaml>`;
it spawns the `af-*` node binaries internally.

## Naming conventions

Locked 2026-07-07:

- **CLI binaries (nodes & services): `af-<name>`** — `af-vault`, `af-db`,
  `af-http`, `af-llm`. Hyphen (not underscore) because these are typed on the
  command line. The Orchestrator resolves a flow node's `use: <name>` to the
  binary `af-<name>`.
- **Infrastructure library crates: `af-<name>`** — `af-contract`.
- **Product entry-point binary: `axisflow`** — the main CLI (crate
  `af-orchestrator`, `[[bin]] name = "axisflow"`). It is the umbrella product,
  not a node, so it intentionally does not carry the `af-` prefix. Usage:
  `axisflow run <flow.yaml>`.
- **A "node" vs a "service" is a role, not a name.** `af-vault` is credential
  *storage* — it is invoked by the Orchestrator to resolve `vault://` refs, and
  is NOT placed as a step in the Flow Spec DAG. Role is declared by usage /
  manifest, not encoded in the binary name.
- Crate names use hyphens; within Rust code the path becomes the underscore
  form (`af_contract`, `af_echo`).

## Status (2026-07-07)

- **Node Contract v1**: LOCKED (subprocess model, JSON Schema, NDJSON, semver,
  standard exit codes).
- **Flow Spec v1**: LOCKED decisions (YAML, dot-path wiring, `when:` branching,
  bounded concurrency, structured logging).
- **Scaffold**: building & running. `axisflow` executes `af-echo` end-to-end
  (`examples/echo-demo.yaml`) with JSON piping between nodes.
- **Schema validation**: pre-flight (binary discovery, manifest parse, required-input
  check) + runtime (JSON Schema validation via `jsonschema` crate). Fail-fast
  enforced — invalid types, missing binaries, and missing required inputs are
  all caught *before* node execution.

## Open questions (deferred)

- A. Subprocess (default, LOCKED for v1) vs compiled plugin (wasm/Rust lib) — v2.
- B. YAML (LOCKED for v1) vs JSON-only.
- C. AI model: local vs API; which LLM — needed for AI Planner layer.
- D. Inter-node data transport for large payloads (stdin/stdout vs shared file/pipe) — v2.
