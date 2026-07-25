//! Ngalir File node.
//!
//! Read from or write to local files.
//! Uses `NGALIR_OUTPUT_DIR` for output path resolution when set.

use na_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use std::path::PathBuf;

/// Return the capability manifest for `na-file`.
///
/// Registers actions: read, write. Supports file output mode.
fn manifest() -> Manifest {
    Manifest {
        name: "na-file".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Read from or write to local files.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["read", "write"] },
                "path": { "type": "string" },
                "content": { "type": "string", "description": "content to write (required for write)" }
            },
            "required": ["action", "path"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "content": { "type": "string" },
                "bytes": { "type": "integer" }
            }
        }),
        secrets: vec![],
        credentials: vec![],
        streaming: false,
        idempotent: false,
        output_mode: Some("file".into()),
        use_cases: vec!["file".into(), "io".into(), "storage".into()],
        examples: vec![],
        see_also: vec!["csv".into(), "excel".into()],
    }
}

/// Resolve the output file path from `NGALIR_OUTPUT_DIR`, if set.
fn output_file_path() -> Option<PathBuf> {
    std::env::var("NGALIR_OUTPUT_DIR")
        .ok()
        .map(|d| PathBuf::from(d).join("output.json"))
}

/// Write a JSON value to the output path (if configured) or stdout.
fn write_output(val: serde_json::Value) {
    if let Some(out_path) = output_file_path() {
        let json = serde_json::to_string(&val).expect("serialize");
        std::fs::write(&out_path, &json).unwrap_or_else(|e| {
            fail(exit_code::GENERIC, format!("write output file failed: {e}"));
        });
        println!("\"{}\"", out_path.display());
    } else {
        println!("{val}");
    }
}

/// Entry point: dispatch `--describe`, `--version`, `read`, or `write`.
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
    let path = input["path"].as_str().unwrap_or("");

    if path.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'path'");
    }

    match action {
        "read" => {
            let content = std::fs::read_to_string(path).unwrap_or_else(|e| {
                fail(exit_code::GENERIC, format!("read failed: {e}"));
            });
            let bytes = content.len();
            let out = serde_json::json!({"content": content, "bytes": bytes});
            write_output(out);
        }
        "write" => {
            let content = input["content"].as_str().unwrap_or("");
            if let Some(parent) = std::path::Path::new(path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(path, content).unwrap_or_else(|e| {
                fail(exit_code::GENERIC, format!("write failed: {e}"));
            });
            let out = serde_json::json!({"bytes": content.len()});
            write_output(out);
        }
        _ => fail(exit_code::INVALID_INPUT, "action must be 'read' or 'write'"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn file_bin() -> PathBuf {
        let exe = std::env::current_exe().expect("current exe");
        let dir = exe.parent().expect("exe parent");
        let mut p = dir.parent().expect("deps parent").to_path_buf();
        p.push("na-file");
        p
    }

    fn run(input: serde_json::Value) -> (bool, String, String) {
        let mut child = Command::new(file_bin())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn na-file");
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
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        )
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-file");
        let required = m.inputs.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("action")));
        assert!(required.contains(&serde_json::json!("path")));
        assert!(m.secrets.is_empty());
        assert_eq!(m.output_mode, Some("file".into()));
    }

    #[test]
    fn test_manifest_has_read_write_actions() {
        let m = manifest();
        let actions = m.inputs["properties"]["action"]["enum"].as_array().unwrap();
        let vals: Vec<&str> = actions.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(vals.contains(&"read"));
        assert!(vals.contains(&"write"));
    }

    #[test]
    fn test_write_then_read_roundtrip() {
        let pid = std::process::id();
        let dir = std::env::temp_dir();
        let file_path = dir.join(format!("ngalir_rt_{pid}.txt"));
        let _ = std::fs::remove_file(&file_path);

        let (ok, stdout, stderr) = run(serde_json::json!({
            "action": "write",
            "path": file_path.to_string_lossy(),
            "content": "hello world"
        }));
        assert!(ok, "write failed: stderr={stderr}");
        let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(result["bytes"], 11);

        let written = std::fs::read_to_string(&file_path).unwrap_or_else(|e| {
            panic!(
                "file not found after write: {e}, path={}",
                file_path.display()
            )
        });
        assert_eq!(written, "hello world");

        let (ok, stdout, stderr) = run(serde_json::json!({
            "action": "read",
            "path": file_path.to_string_lossy()
        }));
        assert!(ok, "read failed: stderr={stderr}");
        let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(result["content"], "hello world");
        assert_eq!(result["bytes"], 11);

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_write_creates_parent_dirs() {
        let base_dir = std::env::temp_dir().join("ngalir_test_nested");
        let file_path = base_dir.join("sub").join("output.txt");
        let _ = std::fs::remove_dir_all(&base_dir);

        let (ok, stdout, _) = run(serde_json::json!({
            "action": "write",
            "path": file_path.to_string_lossy(),
            "content": "nested"
        }));
        assert!(ok, "write should create parent dirs");
        let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(result["bytes"], 6);

        let written = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(written, "nested");

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn test_read_missing_path() {
        let (ok, _, stderr) = run(serde_json::json!({
            "action": "read"
        }));
        assert!(!ok, "missing path should fail");
        assert!(stderr.contains("missing"), "stderr: {stderr}");
    }

    #[test]
    fn test_read_nonexistent_file() {
        let (ok, _, stderr) = run(serde_json::json!({
            "action": "read",
            "path": "/tmp/ngalir_nonexistent_file_xyz.txt"
        }));
        assert!(!ok, "nonexistent file should fail");
        assert!(stderr.contains("failed"), "stderr: {stderr}");
    }

    #[test]
    fn test_invalid_action() {
        let (ok, _, stderr) = run(serde_json::json!({
            "action": "invalid",
            "path": "/tmp/x"
        }));
        assert!(!ok, "invalid action should fail");
        assert!(stderr.contains("must be"), "stderr: {stderr}");
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(file_bin())
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-file"));
    }
}
