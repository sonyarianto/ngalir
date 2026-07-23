use na_contract::{
    exit_code, fail, print_manifest, read_input, AuthType, CredentialField, CredentialSpec,
    Manifest,
};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-airtable".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Read, create, update, and delete Airtable records.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["list", "get", "create", "update", "delete"] },
                "base_id": { "type": "string", "description": "Airtable Base ID" },
                "table_name": { "type": "string", "description": "Table name" },
                "record_id": { "type": "string", "description": "Record ID (required for get/update/delete)" },
                "fields": { "type": "object", "description": "Record fields (required for create/update)" },
                "max_records": { "type": "integer", "default": 100, "description": "Max records to return (list)" },
                "filter_by_formula": { "type": "string", "description": "Airtable formula filter (list)" },
                "sort_field": { "type": "string", "description": "Field to sort by (list)" },
                "sort_direction": { "type": "string", "enum": ["asc", "desc"], "default": "asc", "description": "Sort direction (list)" }
            },
            "required": ["action", "base_id", "table_name"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "records": { "type": "array" },
                "count": { "type": "integer" },
                "record": { "type": "object" },
                "ok": { "type": "boolean" }
            }
        }),
        secrets: vec!["token".into()],
        credentials: vec![CredentialSpec {
            id: "airtable_token".into(),
            label: "Airtable Personal Access Token".into(),
            auth_type: AuthType::ApiKey,
            fields: vec![CredentialField {
                key: "token".into(),
                label: "Personal Access Token".into(),
                input_type: "password".into(),
                required: true,
            }],
            oauth: None,
        }],
        streaming: false,
        idempotent: false,
        output_mode: None,
        use_cases: vec!["airtable".into(), "database".into(), "spreadsheet".into()],
        examples: vec![],
        see_also: vec!["google-sheets".into(), "csv".into()],
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--describe") {
        print_manifest(&manifest());
        return;
    }
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    run().await;
}

async fn run() {
    let input = read_input();
    let action = input["action"].as_str().unwrap_or("");
    if action.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'action' field");
    }
    let base_id = input["base_id"].as_str().unwrap_or("");
    if base_id.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'base_id'");
    }
    let table_name = input["table_name"].as_str().unwrap_or("");
    if table_name.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'table_name'");
    }

    let token = match na_contract::read_secret("token") {
        Some(t) => t,
        None => fail(
            exit_code::AUTH,
            "missing Airtable token (set NGALIR_SECRET_TOKEN)",
        ),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    let base_url = format!("https://api.airtable.com/v0/{}/{}", base_id, table_name);

    match action {
        "list" => cmd_list(&client, &base_url, &token, &input).await,
        "get" => cmd_get(&client, &base_url, &token, &input).await,
        "create" => cmd_create(&client, &base_url, &token, &input).await,
        "update" => cmd_update(&client, &base_url, &token, &input).await,
        "delete" => cmd_delete(&client, &base_url, &token, &input).await,
        _ => fail(
            exit_code::INVALID_INPUT,
            format!("unknown action '{}'", action),
        ),
    }
}

fn build_headers(token: &str) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    headers
}

async fn cmd_list(client: &reqwest::Client, base_url: &str, token: &str, input: &Value) {
    let max_records = input["max_records"].as_u64().unwrap_or(100);
    let mut url = format!("{base_url}?maxRecords={max_records}");

    if let Some(filter) = input["filter_by_formula"].as_str() {
        if !filter.is_empty() {
            url.push_str(&format!("&filterByFormula={}", urlencode(filter)));
        }
    }
    if let Some(sort_field) = input["sort_field"].as_str() {
        if !sort_field.is_empty() {
            let direction = input["sort_direction"].as_str().unwrap_or("asc");
            url.push_str(&format!(
                "&sort[0][field]={sort_field}&sort[0][direction]={direction}"
            ));
        }
    }

    let resp = client
        .get(&url)
        .headers(build_headers(token))
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("API request failed: {e}")));

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if status >= 400 {
        let error = body["error"]["message"].as_str().unwrap_or("unknown");
        fail(
            exit_code::GENERIC,
            format!("Airtable error ({}): {error}", status),
        );
    }

    let records = body["records"].as_array().cloned().unwrap_or_default();
    let count = records.len();
    let output = serde_json::json!({
        "records": records,
        "count": count,
    });
    println!("{output}");
}

async fn cmd_get(client: &reqwest::Client, base_url: &str, token: &str, input: &Value) {
    let record_id = input["record_id"].as_str().unwrap_or("");
    if record_id.is_empty() {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'record_id' for get action",
        );
    }

    let url = format!("{base_url}/{record_id}");
    let resp = client
        .get(&url)
        .headers(build_headers(token))
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("API request failed: {e}")));

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if status >= 400 {
        let error = body["error"]["message"].as_str().unwrap_or("unknown");
        fail(
            exit_code::GENERIC,
            format!("Airtable error ({}): {error}", status),
        );
    }

    println!("{}", body);
}

async fn cmd_create(client: &reqwest::Client, base_url: &str, token: &str, input: &Value) {
    let fields = input.get("fields").and_then(Value::as_object).cloned();
    let fields = match fields {
        Some(f) => f,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'fields' object for create action",
        ),
    };

    let payload = serde_json::json!({
        "fields": fields,
        "returnFieldsByFieldId": false,
    });

    let resp = client
        .post(base_url)
        .headers(build_headers(token))
        .json(&payload)
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("API request failed: {e}")));

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if status >= 400 {
        let error = body["error"]["message"].as_str().unwrap_or("unknown");
        fail(
            exit_code::GENERIC,
            format!("Airtable error ({}): {error}", status),
        );
    }

    println!("{}", body);
}

async fn cmd_update(client: &reqwest::Client, base_url: &str, token: &str, input: &Value) {
    let record_id = input["record_id"].as_str().unwrap_or("");
    if record_id.is_empty() {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'record_id' for update action",
        );
    }
    let fields = input.get("fields").and_then(Value::as_object).cloned();
    let fields = match fields {
        Some(f) => f,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'fields' object for update action",
        ),
    };

    let url = format!("{base_url}/{record_id}");
    let payload = serde_json::json!({
        "fields": fields,
    });

    let resp = client
        .patch(&url)
        .headers(build_headers(token))
        .json(&payload)
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("API request failed: {e}")));

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if status >= 400 {
        let error = body["error"]["message"].as_str().unwrap_or("unknown");
        fail(
            exit_code::GENERIC,
            format!("Airtable error ({}): {error}", status),
        );
    }

    println!("{}", body);
}

async fn cmd_delete(client: &reqwest::Client, base_url: &str, token: &str, input: &Value) {
    let record_id = input["record_id"].as_str().unwrap_or("");
    if record_id.is_empty() {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'record_id' for delete action",
        );
    }

    let url = format!("{base_url}/{record_id}");
    let resp = client
        .delete(&url)
        .headers(build_headers(token))
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("API request failed: {e}")));

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if status >= 400 {
        let error = body["error"]["message"].as_str().unwrap_or("unknown");
        fail(
            exit_code::GENERIC,
            format!("Airtable error ({}): {error}", status),
        );
    }

    let output = serde_json::json!({
        "ok": true,
        "deleted": body["deleted"].as_bool().unwrap_or(false),
        "record_id": body["id"].as_str().unwrap_or(""),
    });
    println!("{output}");
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-airtable");
        assert!(!m.version.is_empty());
        assert_eq!(m.credentials.len(), 1);
        assert_eq!(m.credentials[0].id, "airtable_token");
        assert_eq!(m.credentials[0].auth_type, AuthType::ApiKey);
    }

    #[test]
    fn test_describe_output() {
        use std::process::Command;
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-airtable");
        let output = Command::new(&p)
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-airtable"));
    }

    #[test]
    fn test_urlencode() {
        assert_eq!(urlencode("hello world"), "hello%20world");
        assert_eq!(urlencode("foo/bar"), "foo%2Fbar");
    }

    #[tokio::test]
    async fn test_list_success() {
        let mock_server = MockServer::start().await;
        let base_url = format!("{}/v0/app123/Table1", mock_server.uri());
        Mock::given(method("GET"))
            .and(path("/v0/app123/Table1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "records": [{"id": "rec1", "fields": {"Name": "Alice"}}]
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"max_records": 10});
        cmd_list(&client, &base_url, "test-token", &input).await;
    }

    #[tokio::test]
    async fn test_create_success() {
        let mock_server = MockServer::start().await;
        let base_url = format!("{}/v0/app123/Table1", mock_server.uri());
        Mock::given(method("POST"))
            .and(path("/v0/app123/Table1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "rec_new",
                "fields": {"Name": "Bob"}
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"fields": {"Name": "Bob"}});
        cmd_create(&client, &base_url, "test-token", &input).await;
    }
}
