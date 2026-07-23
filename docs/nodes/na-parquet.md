# na-parquet

Read Apache Parquet files. Streaming read emits one NDJSON row per line.

**Use cases:** parquet, analytics, columnar, etl

## Inputs

```json
  action: string enum: [read] (required)
    read Parquet file
  columns: array
    columns to read (default: all)
  path: string (required)
    file path (required)
```

## Outputs

```
  columns: array
  count: integer
  result: any
    parsed rows
```

## Secrets

  (none)

## Credentials

  (none)

## Properties

  - **Streaming:** streaming, idempotent
  - **Idempotent:** True

## See also

  - [csv](csv.md)
  - [excel](excel.md)
