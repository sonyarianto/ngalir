# na-http

Make HTTP requests (GET / POST / PUT / DELETE / PATCH).

**Use cases:** http, api, webhook

## Inputs

```json
  body: any
  headers: object
  method: string enum: [GET, POST, PUT, DELETE, PATCH] default: GET
  url: string (required)
```

## Outputs

```
  body: any
  headers: object
  status: integer
```

## Secrets

  - `NGALIR_SECRET_BODY`

## Credentials

  (none)

## Properties

  - **Streaming:** (none)
  - **Idempotent:** False

## See also

  (none)
