# na-telegram

Telegram Bot node: send messages and get updates.

**Use cases:** telegram, messaging, bot

## Inputs

```json
  action: string enum: [send_message, get_updates] (required)
  chat_id: string (required)
  limit: integer default: 100
  offset: integer
  parse_mode: string enum: [MarkdownV2, HTML]
  text: string
```

## Outputs

```
  count: integer
  message_id: integer
  ok: boolean
  updates: array
```

## Secrets

  - `NGALIR_SECRET_TOKEN`

## Credentials

  - ID: `telegram_bot_token`
    Label: Telegram Bot Token
    Auth: api_key
    Field: bot_token (password, required)

## Properties

  - **Streaming:** (none)
  - **Idempotent:** False

## See also

  (none)
