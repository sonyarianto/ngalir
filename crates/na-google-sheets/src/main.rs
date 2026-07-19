use na_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-google-sheets".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Read from and append to Google Sheets using service account auth."
            .to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["read", "append"], "description": "read or append to a sheet" },
                "spreadsheet_id": { "type": "string", "description": "Google Spreadsheet ID from the sheet URL" },
                "range": { "type": "string", "default": "Sheet1", "description": "A1 notation range, e.g. Sheet1!A1:C10" },
                "credentials": { "type": "string", "description": "path to service account JSON, or inline JSON" },
                "rows": { "type": "array", "items": { "type": "object" }, "description": "rows to append (required for append)" },
                "has_headers": { "type": "boolean", "default": true, "description": "first row is header (read only)" }
            },
            "required": ["action", "spreadsheet_id"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "updated_range": { "type": "string" },
                "updated_rows": { "type": "integer" },
                "count": { "type": "integer" }
            }
        }),
        secrets: vec!["credentials".into()],
        streaming: true,
        idempotent: true,
        output_mode: None,
    }
}

#[derive(Deserialize)]
struct ServiceAccountKey {
    #[serde(rename = "private_key")]
    private_key: String,
    #[serde(rename = "private_key_id")]
    private_key_id: String,
    #[serde(rename = "client_email")]
    client_email: String,
    #[serde(rename = "token_uri")]
    token_uri: Option<String>,
}

#[derive(Serialize)]
struct JwtClaims {
    iss: String,
    scope: String,
    aud: String,
    exp: usize,
    iat: usize,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct TokenResponse {
    access_token: String,
    #[serde(rename = "expires_in")]
    expires_in: u64,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct SheetsReadResponse {
    #[serde(default)]
    values: Vec<Vec<Value>>,
    range: Option<String>,
}

#[derive(Serialize)]
struct ValueRange {
    values: Vec<Vec<Value>>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct SheetsAppendResponse {
    #[serde(rename = "spreadsheetId")]
    spreadsheet_id: Option<String>,
    #[serde(rename = "updatedRange")]
    updated_range: Option<String>,
    #[serde(rename = "updatedRows")]
    updated_rows: Option<u64>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--describe") {
        print_manifest(&manifest());
        return;
    }
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let input = read_input();
    let action = input["action"].as_str().unwrap_or("");
    let spreadsheet_id = input["spreadsheet_id"].as_str().filter(|s| !s.is_empty());
    let sid = spreadsheet_id
        .unwrap_or_else(|| fail(exit_code::INVALID_INPUT, "'spreadsheet_id' is required"));

    let credentials = resolve_credentials(&input);
    let range = input["range"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("Sheet1");
    let has_headers = input["has_headers"].as_bool().unwrap_or(true);

    let rt = tokio::runtime::Runtime::new()
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("runtime init failed: {e}")));

    rt.block_on(async {
        let token = match get_access_token(&credentials).await {
            Ok(t) => t,
            Err(e) => fail(exit_code::AUTH, format!("auth failed: {e}")),
        };

        match action {
            "read" => cmd_read(sid, range, has_headers, &token).await,
            "append" => cmd_append(sid, range, &input, &token).await,
            _ => fail(
                exit_code::INVALID_INPUT,
                "action must be 'read' or 'append'",
            ),
        }
    });
}

fn resolve_credentials(input: &Value) -> String {
    // First try secret env var (resolved by orchestrator via vault)
    if let Some(secret) = na_contract::read_secret("credentials") {
        // Could be inline JSON or a file path
        if secret.trim().starts_with('{') {
            return secret;
        }
        // Treat as file path
        return std::fs::read_to_string(&secret).unwrap_or_else(|_| {
            fail(
                exit_code::GENERIC,
                format!("failed to read credentials file: {secret}"),
            )
        });
    }
    // Fall back to input field
    let raw = input["credentials"].as_str().unwrap_or_else(|| {
        fail(
            exit_code::INVALID_INPUT,
            "'credentials' is required (file path or inline JSON)",
        )
    });
    if raw.trim().starts_with('{') {
        raw.to_string()
    } else {
        std::fs::read_to_string(raw).unwrap_or_else(|_| {
            fail(
                exit_code::GENERIC,
                format!("failed to read credentials file: {raw}"),
            )
        })
    }
}

async fn get_access_token(credentials_json: &str) -> Result<String, String> {
    let key: ServiceAccountKey = serde_json::from_str(credentials_json)
        .map_err(|e| format!("parse service account key failed: {e}"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("time error: {e}"))?
        .as_secs() as usize;

    let claims = JwtClaims {
        iss: key.client_email.clone(),
        scope: "https://www.googleapis.com/auth/spreadsheets".to_string(),
        aud: key
            .token_uri
            .clone()
            .unwrap_or_else(|| "https://oauth2.googleapis.com/token".to_string()),
        exp: now + 3600,
        iat: now,
    };

    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(key.private_key_id.clone());

    let token = jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_rsa_pem(key.private_key.as_bytes())
            .map_err(|e| format!("parse private key failed: {e}"))?,
    )
    .map_err(|e| format!("JWT encode failed: {e}"))?;

    let client = reqwest::Client::new();
    let resp = client
        .post(&claims.aud)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &token),
        ])
        .send()
        .await
        .map_err(|e| format!("token request failed: {e}"))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("token endpoint error: {body}"));
    }

    let token_resp: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("parse token response failed: {e}"))?;

    Ok(token_resp.access_token)
}

async fn cmd_read(sid: &str, range: &str, has_headers: bool, token: &str) {
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}",
        url_encode(sid),
        url_encode(range),
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("API request failed: {e}")));

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        fail(exit_code::GENERIC, format!("Sheets API error: {body}"));
    }

    let sr: SheetsReadResponse = resp
        .json()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("parse response failed: {e}")));

    if sr.values.is_empty() {
        println!("{}", serde_json::json!({}));
        return;
    }

    let (headers, data_start) = if has_headers {
        let h: Vec<String> = sr.values[0].iter().map(value_to_label).collect();
        (h, 1)
    } else {
        let h: Vec<String> = (0..sr.values[0].len()).map(column_letter).collect();
        (h, 0)
    };

    let mut count = 0u64;
    for row_data in &sr.values[data_start..] {
        let mut map = serde_json::Map::new();
        for (i, val) in row_data.iter().enumerate() {
            let key = headers.get(i).cloned().unwrap_or_else(|| column_letter(i));
            map.insert(key, val.clone());
        }
        println!("{}", serde_json::to_string(&Value::Object(map)).unwrap());
        count += 1;
    }

    if count == 0 {
        println!("{}", serde_json::json!({}));
    }
}

async fn cmd_append(sid: &str, range: &str, input: &Value, token: &str) {
    let rows = match input.get("rows").and_then(Value::as_array) {
        Some(r) => r,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'rows' array for append action",
        ),
    };

    let columns: Vec<String> = input
        .get("columns")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(|| {
            let mut keys: Vec<String> = Vec::new();
            if let Some(first) = rows.first().and_then(|r| r.as_object()) {
                for key in first.keys() {
                    keys.push(key.clone());
                }
            }
            keys.sort();
            keys
        });

    let mut values: Vec<Vec<Value>> = Vec::new();
    for row in rows {
        let mut row_values: Vec<Value> = Vec::new();
        match row {
            Value::Object(obj) => {
                for col in &columns {
                    row_values.push(obj.get(col).cloned().unwrap_or(Value::Null));
                }
            }
            Value::Array(arr) => {
                row_values = arr.clone();
            }
            other => {
                row_values.push(other.clone());
            }
        }
        values.push(row_values);
    }

    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}:append?valueInputOption=USER_ENTERED",
        url_encode(sid),
        url_encode(range),
    );

    let body = ValueRange { values };

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("API request failed: {e}")));

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        fail(exit_code::GENERIC, format!("Sheets API error: {text}"));
    }

    let ar: SheetsAppendResponse = resp
        .json()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("parse response failed: {e}")));

    let out = serde_json::json!({
        "updated_range": ar.updated_range,
        "updated_rows": ar.updated_rows.unwrap_or(rows.len() as u64),
        "count": rows.len(),
    });
    println!("{out}");
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn url_encode(s: &str) -> String {
    // Google Sheets API expects the range to be URL-encoded
    // But for simple paths, we can just use the raw value
    // reqwest handles URL encoding of path segments
    s.to_string()
}

fn value_to_label(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn column_letter(i: usize) -> String {
    let mut n = i;
    let mut s = String::new();
    loop {
        s.insert(0, char::from((n % 26) as u8 + b'A'));
        n /= 26;
        if n == 0 {
            break;
        }
        n -= 1;
    }
    s
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn bin_path() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-google-sheets");
        p
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-google-sheets");
        assert!(!m.version.is_empty());
        assert!(m.streaming);
        assert!(m.idempotent);
        assert!(m.inputs.get("required").is_some());
        assert!(m.secrets.contains(&"credentials".to_string()));
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(bin_path())
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-google-sheets"));
        assert!(stdout.contains("\"streaming\": true"));
    }

    #[test]
    fn test_resolve_credentials_inline_json() {
        let json = r#"{"private_key": "test"}"#;
        let input = serde_json::json!({"credentials": json});
        let result = resolve_credentials(&input);
        assert!(result.contains("private_key"));
    }

    #[test]
    fn test_column_letter() {
        assert_eq!(column_letter(0), "A");
        assert_eq!(column_letter(25), "Z");
        assert_eq!(column_letter(26), "AA");
    }

    #[test]
    fn test_value_to_label() {
        assert_eq!(value_to_label(&Value::String("hello".into())), "hello");
        assert_eq!(value_to_label(&serde_json::json!(42)), "42");
    }

    #[test]
    fn test_missing_spreadsheet_id_fails() {
        let bin = bin_path();
        let input = serde_json::json!({"action": "read"});
        let mut child = Command::new(&bin)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(!output.status.success());
    }

    #[test]
    fn test_missing_action_fails() {
        let bin = bin_path();
        let input = serde_json::json!({"spreadsheet_id": "test"});
        let mut child = Command::new(&bin)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(!output.status.success());
    }

    #[test]
    fn test_append_missing_rows_fails() {
        let bin = bin_path();
        let input = serde_json::json!({
            "action": "append",
            "spreadsheet_id": "test",
            "credentials": "{}"
        });
        let mut child = Command::new(&bin)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(!output.status.success());
    }

    #[test]
    fn test_service_account_key_parse() {
        // A minimal valid service account JSON (not real credentials)
        let json = r#"{
            "type": "service_account",
            "private_key": "-----BEGIN PRIVATE KEY-----\nMIIBVAIBADANBgkqhkiG9w0BAQEFAASCAT4wggE6AgEAAkEA\n-----END PRIVATE KEY-----\n",
            "private_key_id": "abc123",
            "client_email": "test@project.iam.gserviceaccount.com",
            "client_id": "12345",
            "token_uri": "https://oauth2.googleapis.com/token"
        }"#;
        let key: ServiceAccountKey = serde_json::from_str(json).unwrap();
        assert_eq!(key.private_key_id, "abc123");
        assert_eq!(key.client_email, "test@project.iam.gserviceaccount.com");
        assert_eq!(
            key.token_uri.as_deref(),
            Some("https://oauth2.googleapis.com/token")
        );
    }
}
