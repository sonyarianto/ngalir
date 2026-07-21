use na_contract::{exit_code, fail, print_manifest, read_input, Example, Manifest};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-fixedwidth".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Read and write fixed-width text files with configurable column definitions."
            .to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["read", "write"], "description": "read or write fixed-width" },
                "path": { "type": "string", "description": "file path (required for read; optional for write)" },
                "columns": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "column name" },
                            "start": { "type": "integer", "description": "0-based start position" },
                            "width": { "type": "integer", "description": "column width in characters" }
                        },
                        "required": ["name", "start", "width"]
                    },
                    "description": "column definitions (required for read and write)"
                },
                "has_headers": { "type": "boolean", "default": false, "description": "first row is a header line" },
                "rows": { "type": "array", "items": { "type": "object" }, "description": "rows to write (required for write)" }
            },
            "required": ["action", "columns"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" },
                "columns": { "type": "array", "items": { "type": "string" } }
            }
        }),
        secrets: vec![],
        streaming: true,
        idempotent: true,
        output_mode: None,
        use_cases: vec![
            "fixedwidth".into(),
            "legacy".into(),
            "mainframe".into(),
            "etl".into(),
        ],
        examples: vec![Example {
            input: serde_json::json!({
                "action": "read",
                "path": "/data/input.txt",
                "columns": [{"name": "name", "start": 0, "width": 10}, {"name": "age", "start": 10, "width": 3}]
            }),
            output: serde_json::json!({"count": 2, "columns": ["name", "age"]}),
        }],
        see_also: vec!["csv".into(), "excel".into()],
    }
}

#[derive(Debug, Clone)]
struct ColDef {
    name: String,
    start: usize,
    width: usize,
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

    let cols = parse_columns(&input);

    match action {
        "read" => cmd_read(&input, &cols),
        "write" => cmd_write(&input, &cols),
        _ => fail(exit_code::INVALID_INPUT, "action must be 'read' or 'write'"),
    }
}

fn parse_columns(input: &Value) -> Vec<ColDef> {
    let cols = match input.get("columns").and_then(Value::as_array) {
        Some(c) => c,
        None => fail(exit_code::INVALID_INPUT, "missing 'columns' array"),
    };

    let mut result = Vec::new();
    for (i, col) in cols.iter().enumerate() {
        let name = col["name"].as_str().unwrap_or_else(|| {
            fail(
                exit_code::INVALID_INPUT,
                format!("columns[{i}].name is required"),
            )
        });
        let start = col["start"].as_i64().unwrap_or_else(|| {
            fail(
                exit_code::INVALID_INPUT,
                format!("columns[{i}].start is required"),
            )
        }) as usize;
        let width = col["width"].as_i64().unwrap_or_else(|| {
            fail(
                exit_code::INVALID_INPUT,
                format!("columns[{i}].width is required"),
            )
        }) as usize;
        result.push(ColDef {
            name: name.to_string(),
            start,
            width,
        });
    }

    if result.is_empty() {
        fail(exit_code::INVALID_INPUT, "'columns' array is empty");
    }
    result
}

fn cmd_read(input: &Value, cols: &[ColDef]) {
    let has_headers = input["has_headers"].as_bool().unwrap_or(false);

    let content = input["text"]
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
                "provide 'text' string or 'path' for read action",
            );
        });

    let lines: Vec<&str> = content.lines().collect();
    let start_line = if has_headers { 1 } else { 0 };

    for line in lines.iter().skip(start_line) {
        let mut map = serde_json::Map::new();
        for col in cols {
            let val = extract_field(line, col.start, col.width);
            map.insert(col.name.clone(), Value::String(val));
        }
        println!("{}", serde_json::to_string(&Value::Object(map)).unwrap());
    }
}

fn extract_field(line: &str, start: usize, width: usize) -> String {
    line.chars()
        .skip(start)
        .take(width)
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn cmd_write(input: &Value, cols: &[ColDef]) {
    let rows = match input.get("rows").and_then(Value::as_array) {
        Some(r) => r,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'rows' array for write action",
        ),
    };

    let has_headers = input["has_headers"].as_bool().unwrap_or(false);

    let mut output_lines: Vec<String> = Vec::new();

    if has_headers {
        let header: String = cols.iter().map(|c| pad_field(&c.name, c.width)).collect();
        output_lines.push(header);
    }

    for row in rows {
        let line: String = cols
            .iter()
            .map(|c| {
                let val = row.get(&c.name).map(value_to_string).unwrap_or_default();
                pad_field(&val, c.width)
            })
            .collect();
        output_lines.push(line);
    }

    let output_text = output_lines.join("\n");

    match input["path"].as_str() {
        Some(path) => {
            std::fs::write(path, &output_text).unwrap_or_else(|e| {
                fail(exit_code::GENERIC, format!("write file failed: {e}"));
            });
            let output = serde_json::json!({
                "written": true,
                "count": rows.len(),
                "path": path,
            });
            println!("{output}");
        }
        None => {
            println!("{output_text}");
        }
    }
}

fn pad_field(s: &str, width: usize) -> String {
    let trimmed: String = s.chars().take(width).collect();
    format!("{:<width$}", trimmed, width = width)
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn fw_bin() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-fixedwidth");
        p
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-fixedwidth");
        assert!(!m.version.is_empty());
        assert!(m.streaming);
        assert!(m.inputs.get("required").is_some());
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(fw_bin())
            .arg("--describe")
            .output()
            .expect("spawn na-fixedwidth --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-fixedwidth"));
    }

    #[test]
    fn test_read_fixedwidth() {
        let bin = fw_bin();
        let text = "Alice     30 \nBob       25 \n";
        let input = serde_json::json!({
            "action": "read",
            "text": text,
            "columns": [
                {"name": "name", "start": 0, "width": 10},
                {"name": "age", "start": 10, "width": 3}
            ]
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
        let lines: Vec<&str> = std::str::from_utf8(&output.stdout)
            .unwrap()
            .lines()
            .collect();
        assert_eq!(lines.len(), 2);
        let row1: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(row1["name"], "Alice");
        assert_eq!(row1["age"], "30");
    }

    #[test]
    fn test_write_fixedwidth() {
        let bin = fw_bin();
        let input = serde_json::json!({
            "action": "write",
            "columns": [
                {"name": "name", "start": 0, "width": 10},
                {"name": "age", "start": 10, "width": 3}
            ],
            "rows": [
                {"name": "Alice", "age": "30"},
                {"name": "Bob", "age": "25"}
            ]
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
        assert!(stdout.contains("Alice"));
        assert!(stdout.contains("Bob"));
    }

    #[test]
    fn test_read_with_headers() {
        let bin = fw_bin();
        let text = "name      age\nAlice      30 \nBob        25 \n";
        let input = serde_json::json!({
            "action": "read",
            "text": text,
            "has_headers": true,
            "columns": [
                {"name": "name", "start": 0, "width": 10},
                {"name": "age", "start": 10, "width": 3}
            ]
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
        let lines: Vec<&str> = std::str::from_utf8(&output.stdout)
            .unwrap()
            .lines()
            .collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_extract_field() {
        assert_eq!(extract_field("Alice      30 ", 0, 5), "Alice");
        assert_eq!(extract_field("Alice     30 ", 10, 2), "30");
        assert_eq!(extract_field("short", 0, 10), "short");
    }

    #[test]
    fn test_pad_field() {
        assert_eq!(pad_field("Alice", 10), "Alice     ");
        assert_eq!(pad_field("Alice", 3), "Ali");
    }

    #[test]
    fn test_invalid_action() {
        let bin = fw_bin();
        let input = serde_json::json!({"action": "invalid", "columns": [{"name": "x", "start": 0, "width": 1}]});
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
