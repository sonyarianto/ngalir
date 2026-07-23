# na-jsonpath

Extract / transform JSON via jq-compatible filters (.[] | {id, name}, .[0:5], etc.)

**Use cases:** transform, filter, json, jq

## Inputs

```json
  data: any (required)
    The JSON value to query
  filter: string (required)
    jq-compatible filter, e.g. rows[].name or .[] | {id, name}
```

## Outputs

```
  result: any
```

## Secrets

  (none)

## Credentials

  (none)

## Properties

  - **Streaming:** idempotent
  - **Idempotent:** True

## See also

  - [echo](echo.md)
