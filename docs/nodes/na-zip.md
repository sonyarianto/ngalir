# na-zip

Compress and decompress archives (zip, gzip).

**Use cases:** zip, archive, compress, etl

## Inputs

```json
  action: string enum: [compress, decompress, list] (required)
    compress (create archive), decompress (extract), list (list entries)
  files: array
    files to compress: array of {path, name?} objects or string paths
  format: string enum: [zip, gzip]
    archive format (default: zip)
  output: string
    output directory for decompress, or output path for compress
  path: string
    path to archive file
```

## Outputs

```
  count: integer
  entries: array
    list of archive entries
  output: string
    output path or directory
```

## Secrets

  (none)

## Credentials

  (none)

## Properties

  - **Streaming:** (none)
  - **Idempotent:** False

## See also

  - [file](file.md)
  - [http](http.md)
