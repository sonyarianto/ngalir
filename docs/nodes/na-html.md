# na-html

Extract data from HTML documents using CSS selectors. Supports table extraction and arbitrary selector queries.

**Use cases:** html, web-scraping, table-extraction, etl

## Inputs

```json
  action: string enum: [extract, tables] (required)
    extract (CSS selector) or tables (find all HTML tables)
  attribute: string
    attribute to extract (omit for text content)
  has_headers: boolean default: True
    first row is header
  html: string
    inline HTML string (alternative to path)
  path: string
    file path to HTML file
  selector: string
    CSS selector for extraction (required for extract action)
  table_index: integer default: 0
    0-based index of table to extract (tables action only)
  url: string
    URL to fetch HTML from
```

## Outputs

```
  columns: array
  count: integer
  result: any
    extracted data
```

## Secrets

  (none)

## Credentials

  (none)

## Properties

  - **Streaming:** streaming, idempotent
  - **Idempotent:** True

## See also

  - [csv](csv.md)
  - [xml](xml.md)
