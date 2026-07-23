# na-jq

Extract a value from JSON via dot-path syntax (e.g. rows.0.name).

**Use cases:** general

## Inputs

```json
  data: any (required)
    The JSON value to query
  filter: string (required)
    dot-path, e.g. rows.0.name
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

  (none)
