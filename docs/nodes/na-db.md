# na-db

Execute SQL queries against PostgreSQL databases.

**Use cases:** general

## Inputs

```json
  connection: string (required)
    PostgreSQL DSN or vault:// ref
  query: string (required)
    SQL query to execute
```

## Outputs

```
  row_count: integer
  rows: array
```

## Secrets

  - `NGALIR_SECRET_CONNECTION`

## Credentials

  (none)

## Properties

  - **Streaming:** (none)
  - **Idempotent:** False

## See also

  (none)
