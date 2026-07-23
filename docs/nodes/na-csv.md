# na-csv

Read and write CSV files with configurable delimiter, headers, and encoding.

**Use cases:** csv, etl, import, export

## Inputs

```json
  action: string enum: [read, write] (required)
    read or write CSV
  columns: array
    column names for write (inferred from JSON keys if omitted)
  delimiter: string default: ,
    field delimiter character
  has_headers: boolean default: True
    first row is header
  path: string
    file path (required for read; optional for write — omit for stdout)
  rows: array
    rows to write (required for write)
```

## Outputs

```
  columns: array
  count: integer
  path: string
```

## Secrets

  (none)

## Credentials

  (none)

## Properties

  - **Streaming:** streaming, idempotent
  - **Idempotent:** True

## See also

  - [excel](excel.md)
  - [google-sheets](google-sheets.md)
