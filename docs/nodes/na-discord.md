# na-discord

Send messages to Discord via webhook or bot token.

**Use cases:** discord, chat, notification

## Inputs

```json
  action: string enum: [send_webhook, send_bot, get_messages] (required)
  avatar_url: string
    Override avatar URL (webhook only)
  channel_id: string
    Discord channel ID (required for send_bot/get_messages)
  content: string
    Message content
  limit: integer default: 50
    Message limit for get_messages
  username: string
    Override username (webhook only)
  webhook_url: string
    Discord webhook URL (required for send_webhook)
```

## Outputs

```
  count: integer
  message_id: string
  messages: array
  ok: boolean
```

## Secrets

  - `NGALIR_SECRET_TOKEN`

## Credentials

  - ID: `discord_bot_token`
    Label: Discord Bot Token
    Auth: api_key
    Field: token (password, required)

## Properties

  - **Streaming:** (none)
  - **Idempotent:** False

## See also

  - [slack](slack.md)
  - [telegram](telegram.md)
