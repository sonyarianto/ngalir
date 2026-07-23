# Flow Spec

A Flow Spec is a YAML or JSON file that describes a directed acyclic graph (DAG) of nodes.

**Top-level fields:**
- `version` — schema version (currently `1`)
- `name` — flow name
- `nodes` — array of node specifications

**Node specification:**
- `id` — unique node identifier within the flow
- `use` — node type (e.g., `http`, `db-postgres`) or subflow path (`@subflow.yaml`)
- `with` — configuration parameters for the node
- `inputs` — data wiring from upstream node outputs
- `when` — optional Rhai expression for conditional execution

See [flow-spec.md on GitHub](https://github.com/sonyarianto/ngalir/blob/main/docs/flow-spec.md) for the complete specification.
