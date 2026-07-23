# na-excel

Read and write Excel (.xlsx) files with sheet and range selection.

**Use cases:** excel, xlsx, spreadsheet

## Inputs

```json
  action: string enum: [read, write] (required)
    read or write Excel
  columns: array
    column names for write
  path: string (required)
    file path (.xlsx)
  range: string
    cell range like A1:C10 (default: all)
  rows: array
    rows to write (required for write)
  sheet: string
    sheet name or 0-based index (default: first sheet)
```

## Outputs

```
  count: integer
  path: string
  sheet: string
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
  - [google-sheets](google-sheets.md)
