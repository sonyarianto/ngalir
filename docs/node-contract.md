# Node Contract

Every Ngalir node is a standalone Rust binary named `na-<name>` (e.g.
`na-echo`, `na-vault`, `na-http`, `na-jsonpath`). All nodes share one uniform
interface so the Orchestrator can treat every node generically.

## 1. Modes (subcommands)

| Invocation                   | Purpose                                                              |
|------------------------------|----------------------------------------------------------------------|
| `na-echo` / `run`            | Execute the node. Reads input JSON, writes output JSON/NDJSON.       |
| `na-echo --describe`         | Print the **capability manifest** (JSON) for discovery & validation. |
| `na-echo --version`          | Print version string (semver).                                       |
| `na-echo --test-connection`  | Verify credential validity (reads credential data from stdin).       |

The manifest is the single source of truth the Orchestrator uses to discover
node capabilities and validate a Flow Spec before running.

## 2. Capability manifest (`--describe`)

```json
{
  "name": "na-echo",
  "version": "0.1.0",
  "description": "Sample node demonstrating the contract",
  "inputs": {
    "type": "object",
    "properties": {
      "message":   { "type": "string", "description": "Message to echo" }
    },
    "required": ["message"]
  },
  "outputs": {
    "type": "object",
    "properties": {
      "echo":      { "type": "string" }
    }
  },
  "secrets":    [],
  "credentials": [],
  "streaming":  false,
  "idempotent": true
}
```

Fields:
- `inputs` / `outputs` — JSON Schema describing the node's data contract.
- `secrets` — legacy list of input names that are credentials; resolved via
  `na-vault` and injected into the environment.
- `credentials` — structured credential specs (replaces `secrets` for richer
  UI forms, OAuth, and validation). See section 6.
- `streaming` — if `true`, output is NDJSON (one JSON object per line).
- `idempotent` — hints the Orchestrator whether safe to retry on failure.

## 3. Execution protocol

- **Input**: a single JSON object on **stdin** (or `--input <file>`).
  Secrets are *not* embedded here; they are injected as env vars by the
  Orchestrator (see Credential Layer).
- **Output**: a single JSON object on **stdout** on success. If `streaming`,
  stdout is NDJSON (one record per line), which the Orchestrator can forward
  record-by-record to the next node.
- **Errors**: written as JSON to **stderr** (`{"error": "...", "code": 2}`),
  process exits with the matching code below.

### Exit codes

| Code | Meaning                | Orchestrator behavior        |
|------|------------------------|------------------------------|
| 0    | success                | continue                     |
| 1    | generic error          | stop / mark flow failed      |
| 2    | retryable (transient)  | retry with backoff           |
| 3    | auth / credential      | resolve via `na-vault`, retry|
| 4    | invalid input / schema | fail fast                    |
| 10+  | domain-specific errors | node-defined                 |

## 4. Credential Layer (`na-vault`)

- A node declares credential requirements via `credentials` in its manifest
  (or legacy `secrets` for backward compatibility).
- The Orchestrator resolves `vault://<credential_id>` references via `na-vault`
  and injects values into the child process environment as
  `NGALIR_SECRET_<NAME>`.
- Nodes read secrets from env, never from `argv` or the input JSON body.
- This keeps secrets out of process listings, logs, and the Flow Spec file.

### `--test-connection` mode

Nodes that require credentials should implement `--test-connection` to verify
that stored credentials are valid. Protocol:

```
echo '{"private_key": "..."}' | na-google-sheets --test-connection
→ {"ok": true, "message": "Service account credentials are valid."}
```

- **Input**: a JSON object containing the credential data (as stored in the
  vault's `data` field).
- **Output**: `{"ok": true/false, "message": "..."}` on stdout.
- Exit code is always `0` — success/failure is communicated via the JSON
  response body.
- The orchestrator's `POST /api/credentials/:id/test` endpoint calls the
  appropriate node's `--test-connection` with the stored credential data.

## 5. Inter-node data transport

Default: Orchestrator captures a node's stdout and pipes it as stdin to the
next node (in-memory JSON). Open question (D): for large payloads (e.g. millions
of rows) we may need a shared temp file / named pipe addressed by a path
injected via env, instead of buffering in memory.

## 6. Credential Spec (`credentials` field)

The `credentials` field in a manifest is an array of `CredentialSpec` objects
that describe the credential types a node accepts. This drives the web UI's
credential forms, OAuth flows, and test-connection logic.

### CredentialSpec

```json
{
  "id": "google_service_account",
  "label": "Google Service Account",
  "auth_type": "custom",
  "fields": [
    {
      "key": "credentials",
      "label": "Service Account JSON",
      "input_type": "textarea",
      "required": true
    }
  ],
  "oauth": null
}
```

Fields:
- `id` — unique slug matching the credential type (e.g. `"slack_api"`).
- `label` — human-readable name shown in UI dropdowns.
- `auth_type` — one of: `"api_key"`, `"basic_auth"`, `"oauth2"`, `"custom"`.
- `fields` — array of `CredentialField` descriptors for form rendering.
- `oauth` — `OAuthConfig` object (required if `auth_type == "oauth2"`).

### AuthType

| Value          | Description                                  |
|----------------|----------------------------------------------|
| `api_key`      | Single API key or token string.              |
| `basic_auth`   | Username + password pair.                    |
| `oauth2`       | OAuth2 authorization code flow.              |
| `custom`       | Arbitrary JSON (e.g. service account key).   |

### CredentialField

- `key` — field name used in credential `data` map.
- `label` — display label in UI form.
- `input_type` — `"text"` (default), `"password"`, `"textarea"`, or `"url"`.
- `required` — whether the field must be filled.

### OAuthConfig

```json
{
  "authorize_url": "https://slack.com/oauth/authorize",
  "token_url": "https://slack.com/api/oauth.token",
  "scopes": ["chat:write"],
  "client_id_env": "NGALIR_SLACK_CLIENT_ID"
}
```

- `authorize_url` — OAuth authorization endpoint (user redirect).
- `token_url` — token exchange endpoint.
- `scopes` — list of OAuth scopes to request.
- `client_id_env` — env var name containing the app's client ID (set
  server-side, not per-user).

### Backward Compatibility

If a manifest uses the legacy `secrets: ["field_name"]` field instead of
`credentials`, the orchestrator treats each secret name as an
`AuthType::ApiKey` spec with a single password field. This ensures all
existing nodes work without modification.

## Open questions for this layer

- **A. Subprocess vs plugin**: spawn `na-*` as a child process (simple,
  isolated, matches the vision) vs compile nodes as wasm/Rust plugins (faster,
  no spawn overhead, but more complex tooling). Proposal: subprocess by
  default, plugin as an optional optimization later.
- **Manifest format**: reuse JSON Schema for `inputs`/`outputs` (validators
  already exist in Rust, e.g. `jsonschema`) — good default?
- **Error schema**: standardize `{"error","code","retryable"}` or keep minimal?
- **Streaming granularity**: per-record NDJSON, or support Server-Sent-Events
  style for the future visual builder?
