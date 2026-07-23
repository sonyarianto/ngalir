# na-file

Read from or write to local files.

**Use cases:** file, io, storage

## Inputs

```json
  action: string enum: [read, write] (required)
  content: string
    content to write (required for write)
  path: string (required)
```

## Outputs

```
  bytes: integer
  content: string
```

## Secrets

  (none)

## Credentials

  (none)

## Properties

  - **Streaming:** output_mode: file
  - **Idempotent:** False

## See also

  - [csv](csv.md)
  - [excel](excel.md)
