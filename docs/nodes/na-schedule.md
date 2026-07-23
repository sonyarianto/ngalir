# na-schedule

Cron-like timer that executes a flow on a schedule.

**Use cases:** trigger, cron, schedule, timer

## Inputs

```json
  cron: string (required)
    Cron expression (e.g. '0 * * * * *')
  flow: string (required)
  input: object default: {}
```

## Outputs

```
  triggered: integer
```

## Secrets

  (none)

## Credentials

  (none)

## Properties

  - **Streaming:** streaming
  - **Idempotent:** False

## See also

  - [webhook](webhook.md)
