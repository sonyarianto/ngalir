# na-slack

Slack messaging node: post messages and read channel history.

**Use cases:** slack, messaging, chat

## Inputs

```json
  action: string enum: [post_message, read_history] (required)
    Action to perform
  channel: string (required)
    Slack channel ID or name
  count: integer default: 10
    Number of messages to retrieve (read_history only)
  text: string
    Message text (required for post_message)
```

## Outputs

```
  count: integer
  messages: array
  ok: boolean
  ts: string
    Timestamp of posted message
```

## Secrets

  - `NGALIR_SECRET_TOKEN`

## Credentials

  - ID: `slack_api`
    Label: Slack API
    Auth: oauth2
    Field: access_token (password, required)
    OAuth authorize URL: https://slack.com/oauth/v2/authorize
    OAuth token URL: https://slack.com/api/oauth.v2.access
    OAuth scopes: chat:write, channels:history

## Properties

  - **Streaming:** (none)
  - **Idempotent:** False

## See also

  (none)
