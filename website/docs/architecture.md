# Architecture

Ngalir is a flow automation engine built around three core concepts:

- **Flow Spec** — a YAML/JSON file describing a DAG of nodes
- **Node** — a standalone CLI binary named `na-<name>` that reads JSON on stdin and writes JSON on stdout
- **Orchestrator** (`ngalir` binary) — validates & executes a Flow Spec, spawning node subprocesses in topological order with bounded concurrency

Data flows between nodes via JSON pipes. Each node's manifest declares its inputs, outputs, secrets, and credentials using JSON Schema, enabling pre-flight validation and dynamic UI rendering.

For the full architecture document, see [ARCHITECTURE.md on GitHub](https://github.com/sonyarianto/ngalir/blob/main/docs/ARCHITECTURE.md).
