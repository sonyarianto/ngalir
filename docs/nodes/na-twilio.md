# na-twilio

Send SMS and WhatsApp messages via Twilio API.

**Use cases:** twilio, sms, whatsapp, notification

## Inputs

```json
  action: string enum: [send_sms, send_whatsapp] default: send_sms
  body: string (required)
    Message body text
  from: string (required)
    Twilio phone number (E.164 format, e.g. +1234567890)
  to: string (required)
    Recipient phone number (E.164 format)
```

## Outputs

```
  ok: boolean
  sid: string
    Twilio message SID
  status: string
    Message status
```

## Secrets

  - `NGALIR_SECRET_ACCOUNT_SID`
  - `NGALIR_SECRET_AUTH_TOKEN`

## Credentials

  - ID: `twilio_credentials`
    Label: Twilio API Credentials
    Auth: custom
    Field: account_sid (text, required)
    Field: auth_token (password, required)

## Properties

  - **Streaming:** (none)
  - **Idempotent:** False

## See also

  - [email](email.md)
