# Writing Flows

A flow is a YAML file describing a DAG of nodes.

```yaml
version: 1
name: etl-demo
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
      filter: "[] | {id, amount}"
  - id: notify
    use: email
    inputs:
      to: ops@example.com
      subject: "ETL done"
      body: "{{ transform.result | length }} rows processed"
    when: "{{ src.rows | length > 0 }}"
```

## Subflows

Reuse common patterns by referencing external flow files:

```yaml
nodes:
  - id: fetch-orders
    use: "@subflows/http-fetch.yaml"
    with:
      url: "https://api.example.com/orders"
```

## Credentials

Reference credentials in flows via `vault://<credential_id>`:

```yaml
nodes:
  - id: query
    use: db-postgres
    with:
      connection: vault://db/prod
      query: "SELECT * FROM users"
```

## Environment Variables

| Variable | Purpose |
|---|---|
| `NGALIR_NODE_PATH` | Colon-separated directories to search for `na-*` binaries |
| `NGALIR_VAULT_FILE` | Path to vault JSON file (default `~/.ngalir/vault.json`) |
| `NGALIR_VAULT_KEY` | Base64-encoded 32-byte AES-256-GCM key for vault encryption |
| `NGALIR_OUTPUT_DIR` | Temp directory for file-mode output (set by orchestrator) |
| `NGALIR_SECRET_*` | Env vars prefixed with `NGALIR_SECRET_` are injected as secrets |

See [Flow Spec](/docs/flow-spec) for the complete specification.
