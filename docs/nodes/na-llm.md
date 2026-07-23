# na-llm

LLM chat completions via OpenAI / Anthropic compatible API.

**Use cases:** ai, llm, chat, generation

## Inputs

```json
  api_base: string default: https://api.openai.com/v1
    API base URL for compatible backends
  api_key: string
    API key (or use NGALIR_SECRET_API_KEY)
  max_tokens: integer default: 4096
    Maximum tokens in response
  messages: array
    Chat messages array (OpenAI format)
  model: string default: gpt-4o
    Model name (e.g. gpt-4o, claude-3-opus, gemini-pro)
  prompt: string
    Shortcut: single user message (alternative to messages)
  temperature: number default: 1.0
    Sampling temperature (0-2)
```

## Outputs

```
  content: string
  model: string
  usage: object
```

## Secrets

  - `NGALIR_SECRET_API_KEY`

## Credentials

  (none)

## Properties

  - **Streaming:** streaming, idempotent
  - **Idempotent:** True

## See also

  (none)
