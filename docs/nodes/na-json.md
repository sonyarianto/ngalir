# na-json

Read, write, and transform JSON documents.

**Use cases:** json, transform, etl, data

## Inputs

```json
  action: string enum: [read, write, pick, omit, merge] (required)
    read (parse JSON string/file), write (serialize), pick (select fields), omit (remove fields), merge (deep merge objects)
  data: any
    data to process (write/pick/omit/merge)
  json: string
    inline JSON string (read action)
  keys: array
    field names to pick or omit
  objects: array
    array of objects to merge (merge action)
  path: string
    file path for read/write
```

## Outputs

```
  count: integer
  result: any
    transformed JSON result
```

## Secrets

  (none)

## Credentials

  (none)

## Properties

  - **Streaming:** idempotent
  - **Idempotent:** True

## See also

  - [yaml](yaml.md)
  - [jsonpath](jsonpath.md)
  - [xml](xml.md)
