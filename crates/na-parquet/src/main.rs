use na_contract::{exit_code, fail, print_manifest, read_input, Example, Manifest};
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::{Field, Row};
use serde_json::Value;
use std::fs::File;

fn manifest() -> Manifest {
    Manifest {
        name: "na-parquet".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Read Apache Parquet files. Streaming read emits one NDJSON row per line."
            .to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["read"], "description": "read Parquet file" },
                "path": { "type": "string", "description": "file path (required)" },
                "columns": { "type": "array", "items": { "type": "string" }, "description": "columns to read (default: all)" }
            },
            "required": ["action", "path"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "result": { "description": "parsed rows" },
                "count": { "type": "integer" },
                "columns": { "type": "array", "items": { "type": "string" } }
            }
        }),
        secrets: vec![],
        streaming: true,
        idempotent: true,
        output_mode: None,
        use_cases: vec![
            "parquet".into(),
            "analytics".into(),
            "columnar".into(),
            "etl".into(),
        ],
        examples: vec![Example {
            input: serde_json::json!({"action": "read", "path": "/data/file.parquet"}),
            output: serde_json::json!({"count": 100, "columns": ["name", "age"]}),
        }],
        see_also: vec!["csv".into(), "excel".into()],
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
        _ => fail(exit_code::INVALID_INPUT, "action must be 'read'"),
    }
}

fn cmd_read(input: &Value) {
    let path = input["path"].as_str().unwrap_or_else(|| {
        fail(
            exit_code::INVALID_INPUT,
            "'path' is required for read action",
        )
    });

    let file = File::open(path).unwrap_or_else(|e| {
        fail(exit_code::GENERIC, format!("open failed: {e}"));
    });

    let reader = SerializedFileReader::new(file).unwrap_or_else(|e| {
        fail(exit_code::GENERIC, format!("read parquet failed: {e}"));
    });

    let selected_cols: Option<Vec<String>> =
        input.get("columns").and_then(Value::as_array).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

    let metadata = reader.metadata();
    let file_metadata = metadata.file_metadata();
    let schema = file_metadata.schema();
    let fields = schema.get_fields();

    let col_names: Vec<String> = fields
        .iter()
        .filter_map(|f| {
            let name = f.name().to_string();
            match &selected_cols {
                Some(cols) if !cols.contains(&name) => None,
                _ => Some(name),
            }
        })
        .collect();

    let row_iter = reader.get_row_iter(None).unwrap_or_else(|e| {
        fail(
            exit_code::GENERIC,
            format!("create row iterator failed: {e}"),
        );
    });

    let mut row_count = 0u64;
    for result in row_iter {
        match result {
            Ok(row) => {
                let json_row = row_to_json(&row, &col_names);
                println!("{}", serde_json::to_string(&json_row).unwrap());
                row_count += 1;
            }
            Err(e) => {
                fail(exit_code::GENERIC, format!("read row failed: {e}"));
            }
        }
    }

    if row_count == 0 {
        let empty = col_names
            .iter()
            .map(|c| (c.clone(), Value::Null))
            .collect::<serde_json::Map<_, _>>();
        println!("{}", serde_json::to_string(&Value::Object(empty)).unwrap());
    }
}

fn row_to_json(row: &Row, col_names: &[String]) -> Value {
    let mut map = serde_json::Map::new();

    for (idx, (name, field)) in row.get_column_iter().enumerate() {
        if idx < col_names.len() {
            map.insert(col_names[idx].clone(), field_to_json(field));
        } else {
            map.insert(name.clone(), field_to_json(field));
        }
    }

    Value::Object(map)
}

fn field_to_json(field: &Field) -> Value {
    match field {
        Field::Null => Value::Null,
        Field::Bool(b) => Value::Bool(*b),
        Field::Byte(b) => Value::Number(serde_json::json!(b).as_number().unwrap().clone()),
        Field::Short(s) => Value::Number(serde_json::json!(s).as_number().unwrap().clone()),
        Field::Int(i) => Value::Number(serde_json::json!(i).as_number().unwrap().clone()),
        Field::Long(l) => Value::Number(serde_json::json!(l).as_number().unwrap().clone()),
        Field::UByte(b) => Value::Number(serde_json::json!(b).as_number().unwrap().clone()),
        Field::UShort(s) => Value::Number(serde_json::json!(s).as_number().unwrap().clone()),
        Field::UInt(i) => Value::Number(serde_json::json!(i).as_number().unwrap().clone()),
        Field::ULong(l) => Value::Number(serde_json::json!(l).as_number().unwrap().clone()),
        Field::Float(f) => Value::Number(serde_json::json!(f).as_number().unwrap().clone()),
        Field::Double(d) => Value::Number(serde_json::json!(d).as_number().unwrap().clone()),
        Field::Float16(h) => {
            Value::Number(serde_json::json!(h.to_f32()).as_number().unwrap().clone())
        }
        Field::Str(s) => Value::String(s.clone()),
        Field::Bytes(b) => Value::String(format!("{b:?}")),
        Field::Decimal(d) => Value::String(format!("{d:?}")),
        Field::Date(i) => Value::Number(serde_json::json!(i).as_number().unwrap().clone()),
        Field::TimestampMillis(ms) => {
            Value::Number(serde_json::json!(ms).as_number().unwrap().clone())
        }
        Field::TimestampMicros(us) => {
            Value::Number(serde_json::json!(us).as_number().unwrap().clone())
        }
        Field::ListInternal(list) => {
            let arr: Vec<Value> = list.elements().iter().map(field_to_json).collect();
            Value::Array(arr)
        }
        Field::MapInternal(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map.entries() {
                let key = field_to_json(k);
                let val = field_to_json(v);
                if let Value::String(s) = key {
                    obj.insert(s, val);
                }
            }
            Value::Object(obj)
        }
        Field::Group(row) => {
            let mut obj = serde_json::Map::new();
            for (name, field) in row.get_column_iter() {
                obj.insert(name.clone(), field_to_json(field));
            }
            Value::Object(obj)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn parquet_bin() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-parquet");
        p
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-parquet");
        assert!(!m.version.is_empty());
        assert!(m.streaming);
        assert!(m.inputs.get("required").is_some());
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(parquet_bin())
            .arg("--describe")
            .output()
            .expect("spawn na-parquet --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-parquet"));
    }

    #[test]
    fn test_field_to_json() {
        assert_eq!(field_to_json(&Field::Null), Value::Null);
        assert_eq!(field_to_json(&Field::Bool(true)), Value::Bool(true));
        assert_eq!(field_to_json(&Field::Int(42)), serde_json::json!(42));
        assert_eq!(
            field_to_json(&Field::Str("hello".into())),
            Value::String("hello".into())
        );
    }

    #[test]
    fn test_read_nonexistent_file() {
        let bin = parquet_bin();
        let input = serde_json::json!({
            "action": "read",
            "path": "/nonexistent/file.parquet"
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

    #[test]
    fn test_invalid_action() {
        let bin = parquet_bin();
        let input = serde_json::json!({"action": "write", "path": "/tmp/test.parquet"});
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
