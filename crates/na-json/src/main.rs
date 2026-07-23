use na_contract::{exit_code, fail, print_manifest, read_input, Example, Manifest};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-json".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Read, write, and transform JSON documents.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write", "pick", "omit", "merge"],
                    "description": "read (parse JSON string/file), write (serialize), pick (select fields), omit (remove fields), merge (deep merge objects)"
                },
                "json": { "type": "string", "description": "inline JSON string (read action)" },
                "path": { "type": "string", "description": "file path for read/write" },
                "data": { "description": "data to process (write/pick/omit/merge)" },
                "keys": { "type": "array", "items": { "type": "string" }, "description": "field names to pick or omit" },
                "objects": { "type": "array", "description": "array of objects to merge (merge action)" }
            },
            "required": ["action"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "result": { "description": "transformed JSON result" },
                "count": { "type": "integer" }
            }
        }),
        secrets: vec![],
        credentials: vec![],
        streaming: false,
        idempotent: true,
        output_mode: None,
        use_cases: vec![
            "json".into(),
            "transform".into(),
            "etl".into(),
            "data".into(),
        ],
        examples: vec![
            Example {
                input: serde_json::json!({"action": "read", "json": "{\"name\":\"Alice\",\"age\":30}"}),
                output: serde_json::json!({"result": {"name": "Alice", "age": 30}, "count": 2}),
            },
            Example {
                input: serde_json::json!({"action": "pick", "data": {"name": "Alice", "age": 30, "id": 1}, "keys": ["name", "age"]}),
                output: serde_json::json!({"result": {"name": "Alice", "age": 30}}),
            },
            Example {
                input: serde_json::json!({"action": "merge", "objects": [{"a": 1}, {"b": 2}]}),
                output: serde_json::json!({"result": {"a": 1, "b": 2}, "count": 2}),
            },
        ],
        see_also: vec!["yaml".into(), "jsonpath".into(), "xml".into()],
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
        "pick" => cmd_pick(&input),
        "omit" => cmd_omit(&input),
        "merge" => cmd_merge(&input),
        _ => fail(
            exit_code::INVALID_INPUT,
            "action must be 'read', 'write', 'pick', 'omit', or 'merge'",
        ),
    }
}

fn cmd_read(input: &Value) {
    let json_str = input["json"]
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
                "provide 'json' string or 'path' for read action",
            );
        });

    let result: Value = serde_json::from_str(&json_str).unwrap_or_else(|e| {
        fail(exit_code::GENERIC, format!("JSON parse error: {e}"));
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

    let json_str = serde_json::to_string_pretty(data).unwrap_or_else(|e| {
        fail(exit_code::GENERIC, format!("JSON serialize error: {e}"));
    });

    let count = match data {
        Value::Object(m) => m.len(),
        Value::Array(a) => a.len(),
        _ => 1,
    };

    match input["path"].as_str() {
        Some(path) => {
            std::fs::write(path, &json_str).unwrap_or_else(|e| {
                fail(exit_code::GENERIC, format!("write file failed: {e}"));
            });
            let output = serde_json::json!({ "written": true, "count": count, "path": path });
            println!("{output}");
        }
        None => {
            print!("{json_str}");
        }
    }
}

fn cmd_pick(input: &Value) {
    let data = match input.get("data") {
        Some(d) => d,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'data' field for pick action",
        ),
    };
    let keys = match input.get("keys").and_then(Value::as_array) {
        Some(k) => k,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'keys' array for pick action",
        ),
    };

    let obj = match data {
        Value::Object(m) => m,
        _ => fail(
            exit_code::INVALID_INPUT,
            "'data' must be an object for pick action",
        ),
    };

    let mut result = serde_json::Map::new();
    for key in keys {
        if let Some(k) = key.as_str() {
            if let Some(v) = obj.get(k) {
                result.insert(k.to_string(), v.clone());
            }
        }
    }

    let output = serde_json::json!({ "result": Value::Object(result) });
    println!("{output}");
}

fn cmd_omit(input: &Value) {
    let data = match input.get("data") {
        Some(d) => d,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'data' field for omit action",
        ),
    };
    let keys = match input.get("keys").and_then(Value::as_array) {
        Some(k) => k,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'keys' array for omit action",
        ),
    };

    let obj = match data {
        Value::Object(m) => m,
        _ => fail(
            exit_code::INVALID_INPUT,
            "'data' must be an object for omit action",
        ),
    };

    let omit_set: Vec<&str> = keys.iter().filter_map(|k| k.as_str()).collect();
    let mut result = serde_json::Map::new();
    for (k, v) in obj {
        if !omit_set.contains(&k.as_str()) {
            result.insert(k.clone(), v.clone());
        }
    }

    let output = serde_json::json!({ "result": Value::Object(result) });
    println!("{output}");
}

fn cmd_merge(input: &Value) {
    let objects = match input.get("objects").and_then(Value::as_array) {
        Some(o) => o,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'objects' array for merge action",
        ),
    };

    let mut result = serde_json::Map::new();
    for obj in objects {
        match obj {
            Value::Object(m) => {
                for (k, v) in m {
                    result.insert(k.clone(), v.clone());
                }
            }
            _ => fail(
                exit_code::INVALID_INPUT,
                "each item in 'objects' must be an object",
            ),
        }
    }

    let count = result.len();
    let output = serde_json::json!({ "result": Value::Object(result), "count": count });
    println!("{output}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn json_bin() -> PathBuf {
        let exe = std::env::current_exe().expect("current exe");
        let dir = exe.parent().expect("exe parent");
        let mut p = dir.parent().expect("deps parent").to_path_buf();
        p.push("na-json");
        p
    }

    fn run(input: Value) -> (bool, String) {
        let mut child = Command::new(json_bin())
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
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).to_string(),
        )
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-json");
        assert!(!m.version.is_empty());
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(json_bin())
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-json"));
    }

    #[test]
    fn test_read_json_string() {
        let (ok, stdout) = run(serde_json::json!({
            "action": "read",
            "json": "{\"name\":\"Alice\",\"age\":30}"
        }));
        assert!(ok);
        let result: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(result["result"]["name"], "Alice");
        assert_eq!(result["result"]["age"], 30);
        assert_eq!(result["count"], 2);
    }

    #[test]
    fn test_read_json_file() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_json_read.json");
        std::fs::write(&file_path, r#"{"items":[1,2,3]}"#).unwrap();
        let (ok, stdout) = run(serde_json::json!({
            "action": "read",
            "path": file_path.to_string_lossy()
        }));
        assert!(ok);
        let result: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(result["result"]["items"][0], 1);
        assert_eq!(result["count"], 1);
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_write_json_stdout() {
        let (ok, stdout) = run(serde_json::json!({
            "action": "write",
            "data": {"name": "Bob", "age": 25}
        }));
        assert!(ok);
        let parsed: Value = serde_json::from_str(&stdout).unwrap();
        assert!(parsed.is_object());
    }

    #[test]
    fn test_write_json_file() {
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_json_write.json");
        let _ = std::fs::remove_file(&file_path);
        let (ok, _stdout) = run(serde_json::json!({
            "action": "write",
            "path": file_path.to_string_lossy(),
            "data": {"x": 1, "y": 2}
        }));
        assert!(ok);
        let written = std::fs::read_to_string(&file_path).unwrap();
        assert!(written.contains("x"));
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_pick_fields() {
        let (ok, stdout) = run(serde_json::json!({
            "action": "pick",
            "data": {"name": "Alice", "age": 30, "id": 1},
            "keys": ["name", "age"]
        }));
        assert!(ok);
        let result: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(result["result"]["name"], "Alice");
        assert_eq!(result["result"]["age"], 30);
        assert!(result["result"].get("id").is_none());
    }

    #[test]
    fn test_omit_fields() {
        let (ok, stdout) = run(serde_json::json!({
            "action": "omit",
            "data": {"name": "Alice", "age": 30, "id": 1},
            "keys": ["id"]
        }));
        assert!(ok);
        let result: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(result["result"]["name"], "Alice");
        assert_eq!(result["result"]["age"], 30);
        assert!(result["result"].get("id").is_none());
    }

    #[test]
    fn test_merge_objects() {
        let (ok, stdout) = run(serde_json::json!({
            "action": "merge",
            "objects": [{"a": 1, "b": 2}, {"c": 3}]
        }));
        assert!(ok);
        let result: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(result["result"]["a"], 1);
        assert_eq!(result["result"]["b"], 2);
        assert_eq!(result["result"]["c"], 3);
        assert_eq!(result["count"], 3);
    }

    #[test]
    fn test_invalid_action() {
        let (ok, _stdout) = run(serde_json::json!({"action": "invalid"}));
        assert!(!ok);
    }

    #[test]
    fn test_pick_missing_keys() {
        let (ok, _stdout) = run(serde_json::json!({
            "action": "pick",
            "data": {"a": 1}
        }));
        assert!(!ok);
    }

    #[test]
    fn test_omit_missing_keys() {
        let (ok, _stdout) = run(serde_json::json!({
            "action": "omit",
            "data": {"a": 1}
        }));
        assert!(!ok);
    }
}
