# na-db-sqlite

Execute SQL queries against SQLite databases.

**Use cases:** database, sql, sqlite, query

## Inputs

```json
  connection: string (required)
    SQLite file path or vault:// ref
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

  - [db-postgres](db-postgres.md)
  - [db-mysql](db-mysql.md)
