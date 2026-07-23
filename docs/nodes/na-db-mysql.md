# na-db-mysql

Execute SQL queries against MySQL databases.

**Use cases:** database, sql, mysql, query

## Inputs

```json
  connection: string (required)
    MySQL DSN or vault:// ref
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
  - [db-sqlite](db-sqlite.md)
