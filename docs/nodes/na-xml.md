# na-xml

Parse XML documents into JSON or generate XML from JSON.

**Use cases:** xml, etl, parse, serialize

## Inputs

```json
  action: string enum: [read, write] (required)
    read (parse) or write (serialize) XML
  data: object
    JSON data to serialize (required for write)
  item_name: string default: item
    element name for array items in write
  path: string
    file path (required for read; optional for write)
  root_name: string default: root
    root element name for write
  xml: string
    inline XML string (alternative to path for read)
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

  - [jsonpath](jsonpath.md)
  - [yaml](yaml.md)
