use na_contract::{exit_code, fail, print_manifest, read_input, Example, Manifest};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-yaml".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Parse YAML documents into JSON and serialize JSON to YAML.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["read", "write"], "description": "read (parse) or write (serialize) YAML" },
                "path": { "type": "string", "description": "file path (required for read; optional for write — omit for stdout)" },
                "yaml": { "type": "string", "description": "inline YAML string (alternative to path for read)" },
                "data": { "description": "JSON data to serialize (required for write)" }
            },
            "required": ["action"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "result": { "description": "parsed JSON result (read) or write confirmation (write)" },
                "count": { "type": "integer" }
            }
        }),
        secrets: vec![],
        streaming: false,
        idempotent: true,
        output_mode: None,
        use_cases: vec![
            "yaml".into(),
            "config".into(),
            "etl".into(),
            "serialize".into(),
        ],
        examples: vec![
            Example {
                input: serde_json::json!({"action": "read", "yaml": "name: Alice\nage: 30\n"}),
                output: serde_json::json!({"result": {"name": "Alice", "age": 30}, "count": 2}),
            },
            Example {
                input: serde_json::json!({"action": "write", "data": {"name": "Bob", "age": 25}}),
                output: serde_json::json!({"written": true, "count": 2}),
            },
        ],
        see_also: vec!["xml".into(), "csv".into(), "jsonpath".into()],
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

    let input = read_input();
    let action = input["action"].as_str().unwrap_or("");

    match action {
        "read" => cmd_read(&input),
        "write" => cmd_write(&input),
        _ => fail(exit_code::INVALID_INPUT, "action must be 'read' or 'write'"),
    }
}

fn cmd_read(input: &Value) {
    let yaml_str = input["yaml"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| {
            input["path"]
                .as_str()
                .and_then(|p| std::fs::read_to_string(p).ok())
        })
        .unwrap_or_else(|| {
            fail(
                exit_code::INVALID_INPUT,
                "provide 'yaml' string or 'path' for read action",
            );
        });

    let result: Value = serde_yaml::from_str(&yaml_str).unwrap_or_else(|e| {
        fail(exit_code::GENERIC, format!("YAML parse error: {e}"));
    });

    let count = match &result {
        Value::Object(m) => m.len(),
        Value::Array(a) => a.len(),
        _ => 1,
    };

    let output = serde_json::json!({ "result": result, "count": count });
    println!("{output}");
}

fn cmd_write(input: &Value) {
    let data = match input.get("data") {
        Some(d) => d,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'data' field for write action",
        ),
    };

    let yaml_str = serde_yaml::to_string(data).unwrap_or_else(|e| {
        fail(exit_code::GENERIC, format!("YAML serialize error: {e}"));
    });

    let count = match data {
        Value::Object(m) => m.len(),
        Value::Array(a) => a.len(),
        _ => 1,
    };

    match input["path"].as_str() {
        Some(path) => {
            std::fs::write(path, &yaml_str).unwrap_or_else(|e| {
                fail(exit_code::GENERIC, format!("write file failed: {e}"));
            });
            let output = serde_json::json!({ "written": true, "count": count, "path": path });
            println!("{output}");
        }
        None => {
            print!("{yaml_str}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn yaml_bin() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-yaml");
        p
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-yaml");
        assert!(!m.version.is_empty());
        assert!(m.inputs.get("required").is_some());
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(yaml_bin())
            .arg("--describe")
            .output()
            .expect("spawn na-yaml --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-yaml"));
    }

    #[test]
    fn test_read_yaml_from_string() {
        let bin = yaml_bin();
        let input = serde_json::json!({
            "action": "read",
            "yaml": "name: Alice\nage: 30\n"
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
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["result"]["name"], "Alice");
        assert_eq!(result["result"]["age"], 30);
        assert_eq!(result["count"], 2);
    }

    #[test]
    fn test_write_yaml_to_stdout() {
        let bin = yaml_bin();
        let input = serde_json::json!({
            "action": "write",
            "data": {"name": "Bob", "age": 25}
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
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Bob") || stdout.contains("age:"));
    }

    #[test]
    fn test_read_yaml_from_file() {
        let bin = yaml_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_yaml_read.yaml");
        std::fs::write(&file_path, "items:\n  - a\n  - b\n").unwrap();

        let input = serde_json::json!({
            "action": "read",
            "path": file_path.to_string_lossy()
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
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["result"]["items"][0], "a");

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_write_yaml_to_file() {
        let bin = yaml_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_yaml_write.yaml");
        let _ = std::fs::remove_file(&file_path);

        let input = serde_json::json!({
            "action": "write",
            "path": file_path.to_string_lossy(),
            "data": {"x": 1, "y": 2}
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
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let written = std::fs::read_to_string(&file_path).unwrap();
        assert!(written.contains("x:") || written.contains("y:"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_invalid_action() {
        let bin = yaml_bin();
        let input = serde_json::json!({"action": "invalid"});
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
    fn test_read_missing_input() {
        let bin = yaml_bin();
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
}
