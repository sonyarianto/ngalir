# na-yaml

Parse YAML documents into JSON and serialize JSON to YAML.

**Use cases:** yaml, config, etl, serialize

## Inputs

```json
  action: string enum: [read, write] (required)
    read (parse) or write (serialize) YAML
  data: any
    JSON data to serialize (required for write)
  path: string
    file path (required for read; optional for write — omit for stdout)
  yaml: string
    inline YAML string (alternative to path for read)
```

## Outputs

```
  count: integer
  result: any
    parsed JSON result (read) or write confirmation (write)
```

## Secrets

  (none)

## Credentials

  (none)

## Properties

  - **Streaming:** idempotent
  - **Idempotent:** True

## See also

  - [xml](xml.md)
  - [csv](csv.md)
  - [jsonpath](jsonpath.md)
