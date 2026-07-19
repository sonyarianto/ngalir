use std::path::PathBuf;
use std::process::Command;

fn target_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("current exe");
    let dir = exe.parent().expect("exe parent");
    let dir = dir.parent().expect("deps parent");
    dir.to_path_buf()
}

fn node_bin(name: &str) -> PathBuf {
    let mut p = target_dir();
    p.push(format!("na-{}{}", name, std::env::consts::EXE_SUFFIX));
    assert!(p.exists(), "na-{} not found at {}", name, p.display());
    p
}

fn node_path() -> String {
    let mut path = target_dir().to_string_lossy().to_string();
    if let Ok(existing) = std::env::var("PATH") {
        path = format!("{}:{}", path, existing);
    }
    path
}

fn run_with_stdin(bin: &PathBuf, args: &[&str], stdin: &str) -> std::process::Output {
    use std::io::Write;
    let mut child = Command::new(bin)
        .args(args)
        .env("PATH", node_path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

// ── na-echo tests ──────────────────────────────────────────────────────────

#[test]
fn test_echo_describe() {
    let output = Command::new(node_bin("echo"))
        .arg("--describe")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("na-echo"));
    assert!(stdout.contains("message"));
}

#[test]
fn test_echo_version() {
    let output = Command::new(node_bin("echo"))
        .arg("--version")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).is_empty());
}

#[test]
fn test_echo_roundtrip() {
    let output = run_with_stdin(&node_bin("echo"), &[], r#"{"message": "hello ngalir"}"#);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["echo"], "hello ngalir");
}

#[test]
fn test_echo_missing_message() {
    let output = run_with_stdin(&node_bin("echo"), &[], r#"{}"#);
    assert!(!output.status.success());
}

// ── na-file tests ──────────────────────────────────────────────────────────

#[test]
fn test_file_describe() {
    let output = Command::new(node_bin("file"))
        .arg("--describe")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("na-file"));
    assert!(stdout.contains("read"));
    assert!(stdout.contains("write"));
}

#[test]
fn test_file_version() {
    let output = Command::new(node_bin("file"))
        .arg("--version")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).is_empty());
}

#[test]
fn test_file_write_then_read() {
    use std::path::Path;
    let tmp = Path::new("/tmp/test-ngalir-file-write.txt");
    let _ = std::fs::remove_file(tmp);

    // Write
    let content = "test content 42";
    let write_input = serde_json::json!({
        "action": "write",
        "path": tmp.to_str().unwrap(),
        "content": content
    });
    let out = run_with_stdin(&node_bin("file"), &[], &write_input.to_string());
    assert!(
        out.status.success(),
        "write failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let write_resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(write_resp["bytes"], content.len() as u64);

    // Read back
    let read_input = serde_json::json!({
        "action": "read",
        "path": tmp.to_str().unwrap(),
    });
    let out = run_with_stdin(&node_bin("file"), &[], &read_input.to_string());
    assert!(
        out.status.success(),
        "read failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let read_resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(read_resp["content"], content);
    assert_eq!(read_resp["bytes"], content.len() as u64);

    std::fs::remove_file(tmp).ok();
}

#[test]
fn test_file_write_creates_dirs() {
    let tmp = std::path::Path::new("/tmp/ngalir-test/subdir/test-file.txt");
    let _ = std::fs::remove_dir_all("/tmp/ngalir-test");

    let write_input = serde_json::json!({
        "action": "write",
        "path": tmp.to_str().unwrap(),
        "content": "nested dir test"
    });
    let out = run_with_stdin(&node_bin("file"), &[], &write_input.to_string());
    assert!(
        out.status.success(),
        "write+mkdir failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(tmp.exists());
    std::fs::remove_dir_all("/tmp/ngalir-test").ok();
}

#[test]
fn test_file_missing_action() {
    let input = serde_json::json!({"path": "/tmp/x.txt"});
    let out = run_with_stdin(&node_bin("file"), &[], &input.to_string());
    assert!(!out.status.success());
}

// ── na-jsonpath tests ──────────────────────────────────────────────────────

#[test]
fn test_jsonpath_describe() {
    let output = Command::new(node_bin("jsonpath"))
        .arg("--describe")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("na-jsonpath"));
    assert!(stdout.contains("data"));
    assert!(stdout.contains("filter"));
}

#[test]
fn test_jsonpath_version() {
    let output = Command::new(node_bin("jsonpath"))
        .arg("--version")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).is_empty());
}

#[test]
fn test_jsonpath_extract_nested() {
    let input = serde_json::json!({
        "data": {"a": {"b": [{"name": "alice"}]}},
        "filter": "a.b.0.name"
    });
    let out = run_with_stdin(&node_bin("jsonpath"), &[], &input.to_string());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["result"], "alice");
}

#[test]
fn test_jsonpath_filter_dot_returns_all() {
    let input = serde_json::json!({
        "data": {"x": 1, "y": 2},
        "filter": "."
    });
    let out = run_with_stdin(&node_bin("jsonpath"), &[], &input.to_string());
    assert!(out.status.success());
    let resp: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(resp["result"]["x"], 1);
    assert_eq!(resp["result"]["y"], 2);
}

// ── na-vault tests ─────────────────────────────────────────────────────────

#[test]
fn test_vault_describe() {
    let output = Command::new(node_bin("vault"))
        .arg("--describe")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("na-vault"));
}

#[test]
fn test_vault_version() {
    let output = Command::new(node_bin("vault"))
        .arg("--version")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).is_empty());
}

// ── na-db-postgres tests ───────────────────────────────────────────────────

#[test]
fn test_db_postgres_describe() {
    let output = Command::new(node_bin("db-postgres"))
        .arg("--describe")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("na-db-postgres"));
    assert!(stdout.contains("connection"));
    assert!(stdout.contains("query"));
}

#[test]
fn test_db_postgres_version() {
    let output = Command::new(node_bin("db-postgres"))
        .arg("--version")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).is_empty());
}

// ── na-db-mysql tests ──────────────────────────────────────────────────────

#[test]
fn test_db_mysql_describe() {
    let output = Command::new(node_bin("db-mysql"))
        .arg("--describe")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("na-db-mysql"));
    assert!(stdout.contains("connection"));
}

#[test]
fn test_db_mysql_version() {
    let output = Command::new(node_bin("db-mysql"))
        .arg("--version")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).is_empty());
}

// ── na-db-sqlite tests ─────────────────────────────────────────────────────

#[test]
fn test_db_sqlite_describe() {
    let output = Command::new(node_bin("db-sqlite"))
        .arg("--describe")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("na-db-sqlite"));
    assert!(stdout.contains("connection"));
}

#[test]
fn test_db_sqlite_version() {
    let output = Command::new(node_bin("db-sqlite"))
        .arg("--version")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).is_empty());
}

// ── na-http tests ──────────────────────────────────────────────────────────

#[test]
fn test_http_describe() {
    let output = Command::new(node_bin("http"))
        .arg("--describe")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("na-http"));
    assert!(stdout.contains("url"));
    assert!(stdout.contains("method"));
}

#[test]
fn test_http_version() {
    let output = Command::new(node_bin("http"))
        .arg("--version")
        .env("PATH", node_path())
        .output()
        .expect("spawn");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).is_empty());
}

#[test]
fn test_http_missing_url() {
    let input = serde_json::json!({"method": "GET"});
    let out = run_with_stdin(&node_bin("http"), &[], &input.to_string());
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("missing"));
}
