# na-webhook

HTTP server that executes a flow on each POST request.

**Use cases:** trigger, http, webhook, server

## Inputs

```json
  flow: string (required)
  path: string default: /
  port: integer default: 8080
```

## Outputs

```
  server: string
```

## Secrets

  (none)

## Credentials

  (none)

## Properties

  - **Streaming:** streaming
  - **Idempotent:** False

## See also

  - [schedule](schedule.md)
