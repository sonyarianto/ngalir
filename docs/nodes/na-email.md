# na-email

Sends an email via SMTP.

**Use cases:** email, notify, smtp

## Inputs

```json
  body: string (required)
    Email body (plain text)
  password: string
    SMTP password (optional)
  smtp_host: string default: localhost
  smtp_port: integer default: 25
  subject: string (required)
    Email subject
  to: string (required)
    Recipient email address
  username: string
    SMTP username (optional)
```

## Outputs

```
  message_id: string
  sent: boolean
```

## Secrets

  - `NGALIR_SECRET_PASSWORD`

## Credentials

  (none)

## Properties

  - **Streaming:** idempotent
  - **Idempotent:** True

## See also

  (none)
