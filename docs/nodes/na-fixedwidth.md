# na-fixedwidth

Read and write fixed-width text files with configurable column definitions.

**Use cases:** fixedwidth, legacy, mainframe, etl

## Inputs

```json
  action: string enum: [read, write] (required)
    read or write fixed-width
  columns: array (required)
    column definitions (required for read and write)
  has_headers: boolean default: False
    first row is a header line
  path: string
    file path (required for read; optional for write)
  rows: array
    rows to write (required for write)
```

## Outputs

```
  columns: array
  count: integer
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
