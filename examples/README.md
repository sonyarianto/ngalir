# Examples

Run any example with one command. All examples require ngalir + node binaries on your PATH.

```bash
# From the repo root, build everything first:
cargo build

# Then run with NGALIR_NODE_PATH pointing to target/debug/:
NGALIR_NODE_PATH=target/debug ngalir run examples/<filename>
```

| Example | Description |
|---|---|
| [`echo-demo.yaml`](echo-demo.yaml) | Two echo nodes that each output a message. |
| [`when-demo.yaml`](when-demo.yaml) | Conditional execution — an echo node runs only when another node's output satisfies a condition. |
| [`echo-vault.yaml`](echo-vault.yaml) | Reference a secret from the vault via `vault://` URI. |
| [`file-demo.yaml`](file-demo.yaml) | Write a file and read it back in the same flow. |
| [`jsonpath-demo.yaml`](jsonpath-demo.yaml) | Extract a field from structured data using JSONPath. |
| [`api-demo.yaml`](api-demo.yaml) | Pipeline: HTTP GET → JSONPath extraction → Echo. Requires internet access. |

## Environment

- `NGALIR_NODE_PATH=target/debug` — path to node binaries if they are not on `PATH`
- `NGALIR_VAULT_FILE=examples/vault.json` — vault file for `echo-vault.yaml`
