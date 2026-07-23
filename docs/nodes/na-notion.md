# na-notion

Query Notion databases, create/update pages, append blocks.

**Use cases:** notion, database, wiki

## Inputs

```json
  action: string enum: [query_database, get_page, create_page, update_page, append_block] (required)
  children: array
    Blocks to append (required for append_block)
  database_id: string
    Notion database ID (required for query_database)
  filter: object
    Database query filter
  page_id: string
    Notion page ID (required for get_page/update_page/append_block)
  page_size: integer default: 100
  properties: object
    Page properties (required for create_page/update_page)
  sorts: array
    Database query sorts
```

## Outputs

```
  count: integer
  has_more: boolean
  ok: boolean
  page: object
  results: array
```

## Secrets

  - `NGALIR_SECRET_TOKEN`

## Credentials

  - ID: `notion_token`
    Label: Notion Integration Token
    Auth: api_key
    Field: token (password, required)

## Properties

  - **Streaming:** (none)
  - **Idempotent:** False

## See also

  - [airtable](airtable.md)
  - [google-sheets](google-sheets.md)
