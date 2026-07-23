use na_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-csv".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Read and write CSV files with configurable delimiter, headers, and encoding."
            .to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["read", "write"], "description": "read or write CSV" },
                "path": { "type": "string", "description": "file path (required for read; optional for write — omit for stdout)" },
                "delimiter": { "type": "string", "default": ",", "description": "field delimiter character" },
                "has_headers": { "type": "boolean", "default": true, "description": "first row is header" },
                "columns": { "type": "array", "items": { "type": "string" }, "description": "column names for write (inferred from JSON keys if omitted)" },
                "rows": { "type": "array", "items": { "type": "object" }, "description": "rows to write (required for write)" }
            },
            "required": ["action"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" },
                "columns": { "type": "array", "items": { "type": "string" } },
                "path": { "type": "string" }
            }
        }),
        secrets: vec![],
        credentials: vec![],
        streaming: true,
        idempotent: true,
        output_mode: None,
        use_cases: vec!["csv".into(), "etl".into(), "import".into(), "export".into()],
        examples: vec![na_contract::Example {
            input: serde_json::json!({"action": "read", "path": "data.csv"}),
            output: serde_json::json!({"count": 3, "columns": ["name", "age"]}),
        }],
        see_also: vec!["excel".into(), "google-sheets".into()],
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
    let path = input["path"].as_str().filter(|s| !s.is_empty());
    let delimiter = input["delimiter"]
        .as_str()
        .and_then(|s| s.as_bytes().first().copied())
        .unwrap_or(b',');
    let has_headers = input["has_headers"].as_bool().unwrap_or(true);

    match action {
        "read" => cmd_read(path, delimiter, has_headers),
        "write" => cmd_write(path, delimiter, has_headers, &input),
        _ => fail(exit_code::INVALID_INPUT, "action must be 'read' or 'write'"),
    }
}

fn cmd_read(path: Option<&str>, delimiter: u8, has_headers: bool) {
    let p = path.unwrap_or_else(|| {
        fail(
            exit_code::INVALID_INPUT,
            "'path' is required for read action",
        )
    });
    let file = std::fs::File::open(p).unwrap_or_else(|e| {
        fail(exit_code::GENERIC, format!("open failed: {e}"));
    });

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(has_headers)
        .flexible(true)
        .from_reader(file);

    let headers: Vec<String> = match reader.headers() {
        Ok(h) => h.iter().map(|s| s.to_string()).collect(),
        Err(_) => vec![],
    };

    let mut count = 0u64;
    for result in reader.records() {
        match result {
            Ok(record) => {
                let row = if has_headers && !headers.is_empty() {
                    let mut map = serde_json::Map::new();
                    for (i, field) in record.iter().enumerate() {
                        let idx_str = i.to_string();
                        let key = headers.get(i).map(|s| s.as_str()).unwrap_or(&idx_str);
                        map.insert(key.to_string(), Value::String(field.to_string()));
                    }
                    Value::Object(map)
                } else {
                    let vals: Vec<Value> = record
                        .iter()
                        .map(|f| Value::String(f.to_string()))
                        .collect();
                    Value::Array(vals)
                };
                println!(
                    "{}",
                    serde_json::to_string(&row).unwrap_or_else(|e| fail(
                        exit_code::GENERIC,
                        format!("serialize failed: {e}")
                    ))
                );
                count += 1;
            }
            Err(e) => {
                fail(
                    exit_code::GENERIC,
                    format!("CSV parse error at row {}: {e}", count + 1),
                );
            }
        }
    }

    if count == 0 {
        let empty = if has_headers {
            let mut map = serde_json::Map::new();
            for h in &headers {
                map.insert(h.clone(), Value::Null);
            }
            Value::Object(map)
        } else {
            Value::Array(vec![])
        };
        println!("{}", serde_json::to_string(&empty).unwrap());
    }
}

fn cmd_write(path: Option<&str>, delimiter: u8, has_headers: bool, input: &Value) {
    let rows = match input.get("rows").and_then(Value::as_array) {
        Some(r) => r,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'rows' array for write action",
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

    let result = write_csv(path, delimiter, has_headers, rows, &columns);
    match result {
        Ok(count) => {
            // Only output summary JSON to stdout when writing to a file
            // (when writing to stdout, the CSV data itself is the output)
            if path.is_some() {
                let out = serde_json::json!({
                    "written": true,
                    "count": count,
                    "columns": columns,
                    "path": path,
                });
                println!("{out}");
            }
        }
        Err(e) => fail(exit_code::GENERIC, e),
    }
}

fn write_csv(
    path: Option<&str>,
    delimiter: u8,
    has_headers: bool,
    rows: &[Value],
    columns: &[String],
) -> Result<u64, String> {
    match path {
        None => {
            let mut wtr = csv::WriterBuilder::new()
                .delimiter(delimiter)
                .has_headers(has_headers)
                .from_writer(std::io::stdout());
            write_all_rows(&mut wtr, rows, columns, has_headers)?;
            Ok(rows.len() as u64)
        }
        Some(p) => {
            if let Some(parent) = std::path::Path::new(p).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let mut wtr = csv::WriterBuilder::new()
                .delimiter(delimiter)
                .has_headers(has_headers)
                .from_path(p)
                .map_err(|e| format!("create file failed: {e}"))?;
            write_all_rows(&mut wtr, rows, columns, has_headers)?;
            wtr.flush().map_err(|e| format!("flush error: {e}"))?;
            Ok(rows.len() as u64)
        }
    }
}

fn write_all_rows<W: std::io::Write>(
    wtr: &mut csv::Writer<W>,
    rows: &[Value],
    columns: &[String],
    has_headers: bool,
) -> Result<(), String> {
    if has_headers {
        wtr.write_record(columns).map_err(|e| e.to_string())?;
    }
    for row in rows {
        let vals: Vec<String> = match row {
            Value::Object(obj) => columns
                .iter()
                .map(|col| obj.get(col).map(value_to_string).unwrap_or_default())
                .collect(),
            Value::Array(arr) => arr.iter().map(value_to_string).collect(),
            _ => vec![value_to_string(row)],
        };
        wtr.write_record(&vals).map_err(|e| e.to_string())?;
    }
    Ok(())
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

    fn csv_bin() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-csv");
        p
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-csv");
        assert!(!m.version.is_empty());
        assert!(!m.description.is_empty());
        assert!(m.streaming);
        assert!(m.idempotent);
        assert!(m.inputs.get("required").is_some());
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(csv_bin())
            .arg("--describe")
            .output()
            .expect("spawn na-csv --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-csv"));
        assert!(stdout.contains("\"streaming\": true"));
    }

    #[test]
    fn test_read_csv_from_stdin() {
        let bin = csv_bin();
        let csv_data = "name,age\nAlice,30\nBob,25\n";
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_read_stdin.csv");
        std::fs::write(&file_path, csv_data).unwrap();

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

        let lines: Vec<&str> = std::str::from_utf8(&output.stdout)
            .unwrap()
            .lines()
            .collect();
        assert_eq!(lines.len(), 2, "expected 2 NDJSON lines, got: {lines:?}");
        let row1: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(row1["name"], "Alice");
        assert_eq!(row1["age"], "30");
        let row2: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(row2["name"], "Bob");
        assert_eq!(row2["age"], "25");

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_read_csv_no_headers() {
        let bin = csv_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_no_headers.csv");
        std::fs::write(&file_path, "Alice,30\nBob,25\n").unwrap();

        let input = serde_json::json!({
            "action": "read",
            "path": file_path.to_string_lossy(),
            "has_headers": false
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
        // Without headers, rows are arrays
        let row1: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(row1[0], "Alice");
        assert_eq!(row1[1], "30");

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_read_csv_tab_delimiter() {
        let bin = csv_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_tsv.csv");
        std::fs::write(&file_path, "name\tage\nAlice\t30\nBob\t25\n").unwrap();

        let input = serde_json::json!({
            "action": "read",
            "path": file_path.to_string_lossy(),
            "delimiter": "\t"
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

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_read_csv_from_file() {
        let bin = csv_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_read.csv");
        std::fs::write(&file_path, "name,age\nAlice,30\nBob,25\n").unwrap();

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

        let lines: Vec<&str> = std::str::from_utf8(&output.stdout)
            .unwrap()
            .lines()
            .collect();
        assert_eq!(lines.len(), 2, "expected 2 NDJSON lines, got: {lines:?}");
        let row1: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(row1["name"], "Alice");
        assert_eq!(row1["age"], "30");

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_write_csv_to_file() {
        let bin = csv_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_write.csv");
        let _ = std::fs::remove_file(&file_path);

        let input = serde_json::json!({
            "action": "write",
            "path": file_path.to_string_lossy(),
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

        let written = std::fs::read_to_string(&file_path).unwrap();
        let expected = "age,name\n30,Alice\n25,Bob\n";
        assert_eq!(written, expected);

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_write_csv_tab_delimiter() {
        let bin = csv_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_write_tsv.csv");
        let _ = std::fs::remove_file(&file_path);

        let input = serde_json::json!({
            "action": "write",
            "path": file_path.to_string_lossy(),
            "delimiter": "\t",
            "rows": [
                {"name": "Alice", "age": "30"}
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

        let written = std::fs::read_to_string(&file_path).unwrap();
        let expected = "age\tname\n30\tAlice\n";
        assert_eq!(written, expected);

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_write_csv_without_path_goes_to_stdout() {
        let bin = csv_bin();
        // Write to stdout (no path)
        let input = serde_json::json!({
            "action": "write",
            "rows": [
                {"x": "1", "y": "2"}
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
        // When writing to stdout (no path), CSV data goes to stdout directly
        let expected = "x,y\n1,2\n";
        assert_eq!(stdout, expected);
    }

    #[test]
    fn test_write_rejects_missing_rows() {
        let bin = csv_bin();
        let input = serde_json::json!({"action": "write"});
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
    fn test_read_csv_empty_file() {
        let bin = csv_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_empty.csv");
        std::fs::write(&file_path, "").unwrap();

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
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.trim().is_empty(),
            "expected at least one NDJSON line for empty CSV"
        );
        let val: Value = serde_json::from_str(stdout.trim()).unwrap();
        // With headers but no data, we output a row with nulls
        assert_eq!(val["name"], Value::Null);

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_value_to_string_coverage() {
        assert_eq!(value_to_string(&Value::String("hello".into())), "hello");
        assert_eq!(
            value_to_string(&Value::Number(
                serde_json::json!(42).as_number().unwrap().clone()
            )),
            "42"
        );
        assert_eq!(value_to_string(&Value::Bool(true)), "true");
        assert_eq!(value_to_string(&Value::Null), "");
        assert_eq!(value_to_string(&Value::Array(vec![])), "[]");
    }

    #[test]
    fn test_invalid_path() {
        let bin = csv_bin();
        let input = serde_json::json!({
            "action": "read",
            "path": "/nonexistent/path.csv"
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
}
