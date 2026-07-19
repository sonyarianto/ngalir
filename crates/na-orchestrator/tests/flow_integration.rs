//! Integration tests: end-to-end flow execution.

use std::path::{Path, PathBuf};
use std::process::Command;

fn target_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("current exe");
    let dir = exe.parent().expect("exe parent"); // …/target/debug/deps/
    let dir = dir.parent().expect("deps parent"); // …/target/debug/
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
        path = format!(
            "{}{}{}",
            path,
            if cfg!(windows) { ";" } else { ":" },
            existing
        );
    }
    path
}

fn demo_flow() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/echo-demo.yaml")
        .to_string_lossy()
        .into()
}

#[test]
fn echo_demo_flow_succeeds() {
    let output = Command::new(ngalir_bin())
        .arg("run")
        .arg(demo_flow())
        .env("PATH", node_path())
        .output()
        .expect("ngalir process should start");

    assert!(
        output.status.success(),
        "ngalir failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn flow_with_validation_error_fails() {
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        tmp,
        r#"
version: 1
name: test-bad
nodes:
  - id: x
    use: echo
    with:
      message: 42
"#
    )
    .unwrap();

    let output = Command::new(ngalir_bin())
        .arg("run")
        .arg(tmp.path())
        .env("PATH", node_path())
        .output()
        .expect("ngalir should start");

    assert!(
        !output.status.success(),
        "expected validation error, got success"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("schema validation failed"),
        "expected schema validation error, got: {}",
        stderr
    );
}
