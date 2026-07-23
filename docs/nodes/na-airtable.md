# na-airtable

Read, create, update, and delete Airtable records.

**Use cases:** airtable, database, spreadsheet

## Inputs

```json
  action: string enum: [list, get, create, update, delete] (required)
  base_id: string (required)
    Airtable Base ID
  fields: object
    Record fields (required for create/update)
  filter_by_formula: string
    Airtable formula filter (list)
  max_records: integer default: 100
    Max records to return (list)
  record_id: string
    Record ID (required for get/update/delete)
  sort_direction: string enum: [asc, desc] default: asc
    Sort direction (list)
  sort_field: string
    Field to sort by (list)
  table_name: string (required)
    Table name
```

## Outputs

```
  count: integer
  ok: boolean
  record: object
  records: array
```

## Secrets

  - `NGALIR_SECRET_TOKEN`

## Credentials

  - ID: `airtable_token`
    Label: Airtable Personal Access Token
    Auth: api_key
    Field: token (password, required)

## Properties

  - **Streaming:** (none)
  - **Idempotent:** False

## See also

  - [google-sheets](google-sheets.md)
  - [csv](csv.md)
