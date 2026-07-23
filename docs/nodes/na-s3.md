# na-s3

S3-compatible object storage: read, write, list, delete objects.

**Use cases:** s3, storage, object, cloud

## Inputs

```json
  action: string enum: [read, write, list, delete] (required)
  body: string
    Object content (required for write)
  bucket: string (required)
    Bucket name
  content_type: string default: application/octet-stream
    Content-Type for write
  endpoint: string (required)
    S3 endpoint URL (e.g. https://s3.amazonaws.com)
  key: string
    Object key (required for read/write/delete)
  prefix: string
    Prefix filter for list
  region: string default: us-east-1
```

## Outputs

```
  body: string
  content_type: string
  count: integer
  etag: string
  objects: array
  ok: boolean
```

## Secrets

  - `NGALIR_SECRET_ACCESS_KEY`
  - `NGALIR_SECRET_SECRET_KEY`

## Credentials

  - ID: `s3_credentials`
    Label: S3 Access Credentials
    Auth: custom
    Field: access_key (text, required)
    Field: secret_key (password, required)

## Properties

  - **Streaming:** idempotent
  - **Idempotent:** True

## See also

  - [file](file.md)
