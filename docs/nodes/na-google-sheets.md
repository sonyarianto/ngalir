# na-google-sheets

Read from and append to Google Sheets using service account auth.

**Use cases:** sheets, spreadsheet, google

## Inputs

```json
  action: string enum: [read, append] (required)
    read or append to a sheet
  credentials: string
    path to service account JSON, or inline JSON
  has_headers: boolean default: True
    first row is header (read only)
  range: string default: Sheet1
    A1 notation range, e.g. Sheet1!A1:C10
  rows: array
    rows to append (required for append)
  spreadsheet_id: string (required)
    Google Spreadsheet ID from the sheet URL
```

## Outputs

```
  count: integer
  updated_range: string
  updated_rows: integer
```

## Secrets

  - `NGALIR_SECRET_CREDENTIALS`

## Credentials

  - ID: `google_service_account`
    Label: Google Service Account
    Auth: custom
    Field: credentials (textarea, required)

## Properties

  - **Streaming:** streaming, idempotent
  - **Idempotent:** True

## See also

  - [csv](csv.md)
  - [excel](excel.md)
