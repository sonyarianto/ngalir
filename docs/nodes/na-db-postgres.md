# na-db-postgres

Execute SQL queries against PostgreSQL databases.

**Use cases:** database, sql, postgresql, query

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

  - [db-mysql](db-mysql.md)
  - [db-sqlite](db-sqlite.md)
