//! Smoke test: start `ngalir serve`, hit API and static-file endpoints.

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

fn target_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("current exe");
    let dir = exe.parent().expect("exe parent");
    let dir = dir.parent().expect("deps parent");
    dir.to_path_buf()
}

fn ngalir_bin() -> PathBuf {
    let mut p = target_dir();
    p.push(format!("ngalir{}", std::env::consts::EXE_SUFFIX));
    assert!(p.exists(), "ngalir binary not found at {}", p.display());
    p
}

fn node_path() -> String {
    let mut path = target_dir().to_string_lossy().to_string();
    if let Ok(existing) = std::env::var("PATH") {
        path = format!("{path}:{existing}");
    }
    path
}

struct TempDirGuard(tempfile::TempDir);

impl TempDirGuard {
    fn new(dir: tempfile::TempDir) -> Self {
        Self(dir)
    }
}

fn start_server(port: u16) -> Child {
    let guard = TempDirGuard::new(tempfile::tempdir().expect("tempdir"));
    fs::write(
        guard.0.path().join("index.html"),
        "<html><body>ngalir-ui</body></html>",
    )
    .expect("write index.html");

    let child = Command::new(ngalir_bin())
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--ui-dir")
        .arg(guard.0.path())
        .env("PATH", node_path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("ngalir serve should spawn");

    // Keep ui_dir alive until server exits
    std::mem::forget(guard);
    child
}

async fn wait_for_server(base: &str) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("reqwest client");

    for _ in 0..30 {
        if client
            .get(format!("{base}/api/health"))
            .send()
            .await
            .is_ok()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("server did not start within 6s on {base}");
}

#[tokio::test]
async fn api_health_returns_ok() {
    let port = 9876;
    let mut child = start_server(port);
    let base = format!("http://127.0.0.1:{port}");

    wait_for_server(&base).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/api/health"))
        .send()
        .await
        .expect("GET /api/health");
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "OK");

    child.kill().expect("kill server");
    child.wait().expect("wait");
}

#[tokio::test]
async fn static_files_serve_index_html() {
    let port = 9877;
    let mut child = start_server(port);
    let base = format!("http://127.0.0.1:{port}");

    wait_for_server(&base).await;

    let client = reqwest::Client::new();
    let resp = client.get(&base).send().await.expect("GET /");
    assert_eq!(resp.status(), 200);
    assert!(resp.text().await.unwrap().contains("ngalir-ui"));

    child.kill().expect("kill server");
    child.wait().expect("wait");
}

#[tokio::test]
async fn api_routes_not_swallowed_by_static_files() {
    let port = 9878;
    let mut child = start_server(port);
    let base = format!("http://127.0.0.1:{port}");

    wait_for_server(&base).await;

    let client = reqwest::Client::new();

    // /api/health must return the handler response, NOT index.html
    let resp = client
        .get(format!("{base}/api/health"))
        .send()
        .await
        .expect("GET /api/health");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert_eq!(body, "OK", "api route must not return index.html");

    // /api/nodes must return valid JSON, not HTML
    let resp = client
        .get(format!("{base}/api/nodes"))
        .send()
        .await
        .expect("GET /api/nodes");
    assert_eq!(resp.status(), 200);
    let nodes: Vec<serde_json::Value> = resp.json().await.expect("nodes JSON");
    assert!(!nodes.is_empty(), "should list at least one node");

    // Unknown routes → 404 (not index.html SPA fallback)
    let resp = client
        .get(format!("{base}/no-such-path"))
        .send()
        .await
        .expect("GET /no-such-path");
    assert_eq!(resp.status(), 404);

    child.kill().expect("kill server");
    child.wait().expect("wait");
}
