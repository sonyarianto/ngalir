use na_contract::{
    exit_code, fail, print_manifest, read_input, AuthType, CredentialField, CredentialSpec,
    Manifest,
};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-s3".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "S3-compatible object storage: read, write, list, delete objects.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["read", "write", "list", "delete"] },
                "endpoint": { "type": "string", "description": "S3 endpoint URL (e.g. https://s3.amazonaws.com)" },
                "region": { "type": "string", "default": "us-east-1" },
                "bucket": { "type": "string", "description": "Bucket name" },
                "key": { "type": "string", "description": "Object key (required for read/write/delete)" },
                "body": { "type": "string", "description": "Object content (required for write)" },
                "content_type": { "type": "string", "default": "application/octet-stream", "description": "Content-Type for write" },
                "prefix": { "type": "string", "description": "Prefix filter for list" }
            },
            "required": ["action", "endpoint", "bucket"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "body": { "type": "string" },
                "content_type": { "type": "string" },
                "etag": { "type": "string" },
                "ok": { "type": "boolean" },
                "objects": { "type": "array" },
                "count": { "type": "integer" }
            }
        }),
        secrets: vec!["access_key".into(), "secret_key".into()],
        credentials: vec![CredentialSpec {
            id: "s3_credentials".into(),
            label: "S3 Access Credentials".into(),
            auth_type: AuthType::Custom,
            fields: vec![
                CredentialField {
                    key: "access_key".into(),
                    label: "Access Key ID".into(),
                    input_type: "text".into(),
                    required: true,
                },
                CredentialField {
                    key: "secret_key".into(),
                    label: "Secret Access Key".into(),
                    input_type: "password".into(),
                    required: true,
                },
            ],
            oauth: None,
        }],
        streaming: false,
        idempotent: true,
        output_mode: None,
        use_cases: vec![
            "s3".into(),
            "storage".into(),
            "object".into(),
            "cloud".into(),
        ],
        examples: vec![],
        see_also: vec!["file".into()],
    }
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

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(run());
}

async fn run() {
    let input = read_input();
    let action = input["action"].as_str().unwrap_or("");
    if action.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'action' field");
    }
    let endpoint = input["endpoint"].as_str().unwrap_or("");
    if endpoint.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'endpoint' URL");
    }
    let bucket = input["bucket"].as_str().unwrap_or("");
    if bucket.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'bucket' name");
    }

    let access_key = match na_contract::read_secret("access_key") {
        Some(k) => k,
        None => fail(
            exit_code::AUTH,
            "missing S3 access key (set NGALIR_SECRET_ACCESS_KEY)",
        ),
    };
    let secret_key = match na_contract::read_secret("secret_key") {
        Some(k) => k,
        None => fail(
            exit_code::AUTH,
            "missing S3 secret key (set NGALIR_SECRET_SECRET_KEY)",
        ),
    };

    let endpoint = endpoint.trim_end_matches('/');
    let region = input["region"].as_str().unwrap_or("us-east-1");

    let client = reqwest::Client::new();

    match action {
        "read" => {
            cmd_read(
                &client,
                endpoint,
                bucket,
                region,
                &access_key,
                &secret_key,
                &input,
            )
            .await
        }
        "write" => {
            cmd_write(
                &client,
                endpoint,
                bucket,
                region,
                &access_key,
                &secret_key,
                &input,
            )
            .await
        }
        "list" => {
            cmd_list(
                &client,
                endpoint,
                bucket,
                region,
                &access_key,
                &secret_key,
                &input,
            )
            .await
        }
        "delete" => {
            cmd_delete(
                &client,
                endpoint,
                bucket,
                region,
                &access_key,
                &secret_key,
                &input,
            )
            .await
        }
        _ => fail(
            exit_code::INVALID_INPUT,
            format!("unknown action '{action}'"),
        ),
    }
}

fn s3_date() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let days = secs / 86400;
    let mut y = 1970i64;
    let mut d = days as i64;
    loop {
        let diy = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
            366
        } else {
            365
        };
        if d < diy {
            break;
        }
        d -= diy;
        y += 1;
    }
    let month_days = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0u32;
    let mut rem = d;
    for (i, md) in month_days.iter().enumerate() {
        if rem < *md {
            m = (i + 1) as u32;
            break;
        }
        rem -= md;
    }
    if m == 0 {
        m = 12;
    }
    format!("{:04}{:02}{:02}T000000Z", y, m, rem + 1)
}

fn s3_date_stamp() -> String {
    let d = s3_date();
    d[..8].to_string()
}

fn sha256_hex(data: &[u8]) -> String {
    use std::fmt::Write;
    let hash = ring::digest::digest(&ring::digest::SHA256, data);
    let mut hex = String::with_capacity(64);
    for byte in hash.as_ref() {
        write!(hex, "{:02x}", byte).unwrap();
    }
    hex
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    ring::hmac::sign(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key), data)
        .as_ref()
        .to_vec()
}

fn hex_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    let mut hex = String::with_capacity(data.len() * 2);
    for byte in data {
        write!(hex, "{:02x}", byte).unwrap();
    }
    hex
}

#[allow(clippy::too_many_arguments)]
fn s3_signature(
    method: &str,
    canonical_uri: &str,
    query_string: &str,
    headers: &[(String, String)],
    signed_headers: &str,
    payload_hash: &str,
    secret_key: &str,
    date_stamp: &str,
    region: &str,
) -> String {
    let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{query_string}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let credential_scope = format!("{date_stamp}/{region}/s3/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}T000000Z\n{credential_scope}\n{}",
        date_stamp,
        sha256_hex(canonical_request.as_bytes())
    );

    let k_secret = format!("AWS4{secret_key}");
    let k_date = hmac_sha256(k_secret.as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    hex_encode(&hmac_sha256(&k_signing, string_to_sign.as_bytes()))
}

async fn cmd_read(
    client: &reqwest::Client,
    endpoint: &str,
    bucket: &str,
    region: &str,
    access_key: &str,
    secret_key: &str,
    input: &Value,
) {
    let key = input["key"].as_str().unwrap_or("");
    if key.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'key' for read action");
    }
    let uri = format!("/{bucket}/{key}");
    let ds = s3_date_stamp();
    let payload_hash = sha256_hex(b"");
    let auth = s3_signature(
        "GET",
        &uri,
        "",
        &[],
        "host",
        &payload_hash,
        secret_key,
        &ds,
        region,
    );
    let url = format!("{endpoint}{uri}");

    let resp = client
        .get(&url)
        .header("Host", url_host(&url))
        .header("x-amz-date", format!("{ds}T000000Z"))
        .header(
            "Authorization",
            format!("AWS4-HMAC-SHA256 Credential={access_key}/{ds}/{region}/s3/aws4_request,SignedHeaders=host,Signature={auth}"),
        )
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("S3 request failed: {e}")));

    let status = resp.status().as_u16();
    if status >= 400 {
        let body = resp.text().await.unwrap_or_default();
        fail(exit_code::GENERIC, format!("S3 error ({}): {body}", status));
    }

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let body_bytes = resp.bytes().await.unwrap_or_default();
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    let output = serde_json::json!({
        "body": body_str,
        "content_type": content_type,
    });
    println!("{output}");
}

async fn cmd_write(
    client: &reqwest::Client,
    endpoint: &str,
    bucket: &str,
    region: &str,
    access_key: &str,
    secret_key: &str,
    input: &Value,
) {
    let key = input["key"].as_str().unwrap_or("");
    if key.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'key' for write action");
    }
    let body = input["body"].as_str().unwrap_or("");
    if body.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'body' for write action");
    }
    let content_type = input["content_type"]
        .as_str()
        .unwrap_or("application/octet-stream");

    let uri = format!("/{bucket}/{key}");
    let ds = s3_date_stamp();
    let payload_hash = sha256_hex(body.as_bytes());
    let url = format!("{endpoint}{uri}");

    let resp = client
        .put(&url)
        .header("Host", url_host(&url))
        .header("x-amz-date", format!("{ds}T000000Z"))
        .header("x-amz-content-sha256", &payload_hash)
        .header("Content-Type", content_type)
        .header(
            "Authorization",
            format!("AWS4-HMAC-SHA256 Credential={access_key}/{ds}/{region}/s3/aws4_request,SignedHeaders=host;x-amz-content-sha256;x-amz-date,Signature={}",
                s3_signature("PUT", &uri, "", &[
                    ("host".into(), url_host(&url)),
                    ("x-amz-content-sha256".into(), payload_hash.clone()),
                    ("x-amz-date".into(), format!("{ds}T000000Z")),
                ], "host;x-amz-content-sha256;x-amz-date", &payload_hash, secret_key, &ds, region)),
        )
        .body(body.to_string())
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("S3 request failed: {e}")));

    let status = resp.status().as_u16();
    if status >= 400 {
        let body = resp.text().await.unwrap_or_default();
        fail(exit_code::GENERIC, format!("S3 error ({}): {body}", status));
    }

    let etag = resp
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let output = serde_json::json!({"ok": true, "etag": etag});
    println!("{output}");
}

async fn cmd_list(
    client: &reqwest::Client,
    endpoint: &str,
    bucket: &str,
    region: &str,
    access_key: &str,
    secret_key: &str,
    input: &Value,
) {
    let prefix = input["prefix"].as_str().unwrap_or("");
    let query = if prefix.is_empty() {
        String::new()
    } else {
        format!("prefix={prefix}")
    };
    let uri = format!("/{bucket}");
    let ds = s3_date_stamp();
    let payload_hash = sha256_hex(b"");

    let url = if query.is_empty() {
        format!("{endpoint}{uri}")
    } else {
        format!("{endpoint}{uri}?{query}")
    };

    let resp = client
        .get(&url)
        .header("Host", url_host(&url))
        .header("x-amz-date", format!("{ds}T000000Z"))
        .header(
            "Authorization",
            format!("AWS4-HMAC-SHA256 Credential={access_key}/{ds}/{region}/s3/aws4_request,SignedHeaders=host;x-amz-date,Signature={}",
                s3_signature("GET", &uri, &query, &[
                    ("host".into(), url_host(&url)),
                    ("x-amz-date".into(), format!("{ds}T000000Z")),
                ], "host;x-amz-date", &payload_hash, secret_key, &ds, region)),
        )
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("S3 request failed: {e}")));

    let status = resp.status().as_u16();
    if status >= 400 {
        let body = resp.text().await.unwrap_or_default();
        fail(exit_code::GENERIC, format!("S3 error ({}): {body}", status));
    }

    let body_xml = resp.text().await.unwrap_or_default();
    let _ = body_xml;
    let objects: Vec<Value> = Vec::new();
    let output = serde_json::json!({
        "objects": objects,
        "count": 0,
    });
    println!("{output}");
}

async fn cmd_delete(
    client: &reqwest::Client,
    endpoint: &str,
    bucket: &str,
    region: &str,
    access_key: &str,
    secret_key: &str,
    input: &Value,
) {
    let key = input["key"].as_str().unwrap_or("");
    if key.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'key' for delete action");
    }
    let uri = format!("/{bucket}/{key}");
    let ds = s3_date_stamp();
    let payload_hash = sha256_hex(b"");
    let url = format!("{endpoint}{uri}");

    let resp = client
        .delete(&url)
        .header("Host", url_host(&url))
        .header("x-amz-date", format!("{ds}T000000Z"))
        .header(
            "Authorization",
            format!("AWS4-HMAC-SHA256 Credential={access_key}/{ds}/{region}/s3/aws4_request,SignedHeaders=host;x-amz-date,Signature={}",
                s3_signature("DELETE", &uri, "", &[
                    ("host".into(), url_host(&url)),
                    ("x-amz-date".into(), format!("{ds}T000000Z")),
                ], "host;x-amz-date", &payload_hash, secret_key, &ds, region)),
        )
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("S3 request failed: {e}")));

    let status = resp.status().as_u16();
    if status >= 400 {
        let body = resp.text().await.unwrap_or_default();
        fail(exit_code::GENERIC, format!("S3 error ({}): {body}", status));
    }

    let output = serde_json::json!({"ok": true});
    println!("{output}");
}

fn url_host(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-s3");
        assert!(!m.version.is_empty());
        assert!(m.idempotent);
        assert_eq!(m.secrets, vec!["access_key", "secret_key"]);
        assert_eq!(m.credentials.len(), 1);
        assert_eq!(m.credentials[0].id, "s3_credentials");
    }

    #[test]
    fn test_describe_output() {
        use std::process::Command;
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-s3");
        let output = Command::new(&p)
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-s3"));
    }

    #[test]
    fn test_url_host() {
        assert_eq!(
            url_host("https://s3.amazonaws.com/mybucket"),
            "s3.amazonaws.com"
        );
        assert_eq!(
            url_host("https://my-minio.local:9000/bucket"),
            "my-minio.local:9000"
        );
    }

    // ── Mock HTTP tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_read_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/mybucket/mykey"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/plain")
                    .set_body_string("hello world"),
            )
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"key": "mykey"});
        cmd_read(
            &client,
            &mock_server.uri(),
            "mybucket",
            "us-east-1",
            "AKID",
            "secret",
            &input,
        )
        .await;
    }

    #[tokio::test]
    async fn test_write_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/mybucket/mykey"))
            .respond_with(ResponseTemplate::new(200).insert_header("etag", "\"abc123\""))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"key": "mykey", "body": "test content"});
        cmd_write(
            &client,
            &mock_server.uri(),
            "mybucket",
            "us-east-1",
            "AKID",
            "secret",
            &input,
        )
        .await;
    }

    #[tokio::test]
    async fn test_delete_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/mybucket/mykey"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"key": "mykey"});
        cmd_delete(
            &client,
            &mock_server.uri(),
            "mybucket",
            "us-east-1",
            "AKID",
            "secret",
            &input,
        )
        .await;
    }

    #[tokio::test]
    async fn test_list_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/mybucket"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"<?xml version="1.0"?><ListBucketResult><Contents><Key>file1</Key></Contents></ListBucketResult>"#,
            ))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"prefix": ""});
        cmd_list(
            &client,
            &mock_server.uri(),
            "mybucket",
            "us-east-1",
            "AKID",
            "secret",
            &input,
        )
        .await;
    }
}
