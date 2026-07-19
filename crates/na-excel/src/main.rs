use na_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-excel".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Read and write Excel (.xlsx) files with sheet and range selection."
            .to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["read", "write"], "description": "read or write Excel" },
                "path": { "type": "string", "description": "file path (.xlsx)" },
                "sheet": { "type": "string", "description": "sheet name or 0-based index (default: first sheet)" },
                "range": { "type": "string", "description": "cell range like A1:C10 (default: all)" },
                "columns": { "type": "array", "items": { "type": "string" }, "description": "column names for write" },
                "rows": { "type": "array", "items": { "type": "object" }, "description": "rows to write (required for write)" }
            },
            "required": ["action", "path"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" },
                "sheet": { "type": "string" },
                "path": { "type": "string" }
            }
        }),
        secrets: vec![],
        streaming: true,
        idempotent: true,
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

    match action {
        "read" => cmd_read(path, &input),
        "write" => cmd_write(path, &input),
        _ => fail(exit_code::INVALID_INPUT, "action must be 'read' or 'write'"),
    }
}

fn cmd_read(path: Option<&str>, input: &Value) {
    let p = path.unwrap_or_else(|| {
        fail(
            exit_code::INVALID_INPUT,
            "'path' is required for read action",
        )
    });

    use calamine::{open_workbook, Reader};
    let mut workbook: calamine::Xlsx<_> = match open_workbook(p) {
        Ok(wb) => wb,
        Err(e) => fail(exit_code::GENERIC, format!("open workbook failed: {e}")),
    };

    let sheet_spec = input["sheet"].as_str();
    let range_spec = input["range"].as_str();

    let sheet_names = workbook.sheet_names().to_vec();
    let target_sheets = resolve_sheets(&sheet_names, sheet_spec);

    let mut total_count = 0u64;

    for sheet_name in &target_sheets {
        let range = match workbook.worksheet_range(sheet_name) {
            Ok(r) => r,
            Err(e) => fail(
                exit_code::GENERIC,
                format!("read sheet '{sheet_name}' failed: {e}"),
            ),
        };

        let rows: Vec<Vec<calamine::Data>> = range.rows().map(|r| r.to_vec()).collect();
        if rows.is_empty() {
            continue;
        }

        let (row_start, col_start, row_end, col_end) =
            parse_range(range_spec, rows.len(), rows[0].len());

        // Headers from first row (if we start at row 0 and it looks like headers)
        // Actually, Excel doesn't have "headers" like CSV. The first row is data.
        // But users may want to treat the first row as field names.
        // We'll just use indices as field names: col_0, col_1, ... or letter-based A, B, ...
        // More useful: the user can specify columns, or we use letter-based column names.

        let _use_labels = total_count == 0 && target_sheets.len() == 1;

        for r in row_start..=row_end {
            let mut map = serde_json::Map::new();
            for c in col_start..=col_end {
                let cell = rows
                    .get(r)
                    .and_then(|row| row.get(c))
                    .unwrap_or(&calamine::Data::Empty);
                let col_name = column_letter(c);
                map.insert(col_name, calamine_data_to_json(cell));
            }
            if target_sheets.len() > 1 {
                map.insert("__sheet__".to_string(), Value::String(sheet_name.clone()));
            }
            println!("{}", serde_json::to_string(&map).unwrap());
            total_count += 1;
        }

        // If no rows in range, emit empty object
        if row_start > row_end || rows.is_empty() {
            let empty = serde_json::json!({});
            println!("{empty}");
            total_count += 1;
        }
    }

    if total_count == 0 {
        let empty = serde_json::json!({});
        println!("{empty}");
    }
}

fn cmd_write(path: Option<&str>, input: &Value) {
    let p = path.unwrap_or_else(|| {
        fail(
            exit_code::INVALID_INPUT,
            "'path' is required for write action",
        )
    });
    if let Some(parent) = std::path::Path::new(p).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let rows = match input.get("rows").and_then(Value::as_array) {
        Some(r) => r,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'rows' array for write action",
        ),
    };

    let sheet_name = input
        .get("sheet")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("Sheet1");

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

    use rust_xlsxwriter::*;
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    let _ = worksheet.set_name(sheet_name);

    // Write data rows (no header row — columns are field mappings, not Excel headers)
    for (row_idx, row) in rows.iter().enumerate() {
        let excel_row = row_idx as u32;
        match row {
            Value::Object(obj) => {
                for (col_idx, col_name) in columns.iter().enumerate() {
                    let val = obj.get(col_name);
                    write_cell(worksheet, excel_row, col_idx as u16, val);
                }
            }
            Value::Array(arr) => {
                for (col_idx, val) in arr.iter().enumerate() {
                    write_cell(worksheet, excel_row, col_idx as u16, Some(val));
                }
            }
            other => {
                write_cell(worksheet, excel_row, 0, Some(other));
            }
        }
    }

    if let Err(e) = workbook.save(p) {
        fail(exit_code::GENERIC, format!("save workbook failed: {e}"));
    }

    let out = serde_json::json!({
        "written": true,
        "count": rows.len(),
        "sheet": sheet_name,
        "path": p,
    });
    println!("{out}");
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn resolve_sheets(all: &[String], spec: Option<&str>) -> Vec<String> {
    match spec {
        None => all.first().cloned().map(|s| vec![s]).unwrap_or_default(),
        Some(name) => {
            // Try as index first
            if let Ok(idx) = name.parse::<usize>() {
                if idx < all.len() {
                    return vec![all[idx].clone()];
                }
                fail(
                    exit_code::INVALID_INPUT,
                    format!("sheet index {idx} out of range (0..{})", all.len()),
                );
            }
            // Try as name
            if all.iter().any(|s| s == name) {
                return vec![name.to_string()];
            }
            // Maybe user wants a prefix match or partial name?
            let matches: Vec<String> = all.iter().filter(|s| s.contains(name)).cloned().collect();
            if matches.is_empty() {
                fail(
                    exit_code::INVALID_INPUT,
                    format!("sheet '{name}' not found. Available: {}", all.join(", ")),
                );
            }
            matches
        }
    }
}

fn parse_range(
    spec: Option<&str>,
    row_count: usize,
    col_count: usize,
) -> (usize, usize, usize, usize) {
    let Some(spec) = spec else {
        return (
            0,
            0,
            row_count.saturating_sub(1),
            col_count.saturating_sub(1),
        );
    };
    let spec = spec.trim().to_uppercase();

    // Parse "A1:C10" format
    let colon_pos = spec.find(':');
    let (start_s, end_s) = match colon_pos {
        Some(p) => (&spec[..p], &spec[p + 1..]),
        None => (&spec[..], &spec[..]),
    };

    let (start_col, start_row) = parse_cell_ref(start_s);
    let (end_col, end_row) = parse_cell_ref(end_s);

    let row_start = start_row.unwrap_or(0);
    let col_start = start_col.unwrap_or(0);
    let row_end = end_row
        .unwrap_or(row_count.saturating_sub(1))
        .min(row_count.saturating_sub(1));
    let col_end = end_col
        .unwrap_or(col_count.saturating_sub(1))
        .min(col_count.saturating_sub(1));

    (row_start, col_start, row_end, col_end)
}

fn parse_cell_ref(s: &str) -> (Option<usize>, Option<usize>) {
    let mut col_str = String::new();
    let mut row_str = String::new();
    let mut in_col = true;
    for c in s.chars() {
        if c.is_ascii_alphabetic() && in_col {
            col_str.push(c);
        } else if c.is_ascii_digit() {
            in_col = false;
            row_str.push(c);
        } else {
            break;
        }
    }
    let col = if col_str.is_empty() {
        None
    } else {
        Some(column_index(&col_str))
    };
    let row = if row_str.is_empty() {
        None
    } else {
        row_str.parse::<usize>().ok().map(|r| r.saturating_sub(1)) // 1-based → 0-based
    };
    (col, row)
}

fn column_index(s: &str) -> usize {
    let mut idx = 0usize;
    for c in s.chars() {
        idx = idx * 26 + ((c as usize) - ('A' as usize) + 1);
    }
    idx.saturating_sub(1)
}

fn column_letter(i: usize) -> String {
    let mut n = i;
    let mut s = String::new();
    loop {
        s.insert(0, char::from((n % 26) as u8 + b'A'));
        n /= 26;
        if n == 0 {
            break;
        }
        n -= 1;
    }
    s
}

fn calamine_data_to_json(d: &calamine::Data) -> Value {
    use calamine::Data;
    match d {
        Data::Empty => Value::Null,
        Data::String(s) => Value::String(s.clone()),
        Data::Float(f) => {
            if *f == f.floor() && f.is_finite() && f.is_sign_positive() {
                Value::Number(serde_json::Number::from(*f as i64))
            } else {
                serde_json::Number::from_f64(*f)
                    .map(Value::Number)
                    .unwrap_or(Value::String(f.to_string()))
            }
        }
        Data::Int(i) => Value::Number(serde_json::Number::from(*i)),
        Data::Bool(b) => Value::Bool(*b),
        Data::DateTime(dt) => {
            // Convert Excel serial date to ISO string
            Value::String(excel_datetime_to_iso(dt.as_f64()))
        }
        Data::DateTimeIso(s) => Value::String(s.clone()),
        Data::DurationIso(s) => Value::String(s.clone()),
        Data::Error(err) => Value::String(format!("ERR: {err}")),
    }
}

fn excel_datetime_to_iso(serial: f64) -> String {
    // Excel epoch: 1899-12-30 (day 0)
    // This is approximate; real conversion needs leap year + 1900 bug handling
    let whole_days = serial.floor() as i64;
    let frac = serial - whole_days as f64;

    // Simple conversion for common ranges
    let mut y = 1899i64;
    let mut remaining = whole_days;
    // Skip 1900 (Excel's bug: 1900 is a leap year in Excel)
    if remaining >= 60 {
        remaining -= 1; // skip Feb 29, 1900
    }

    // Approximate month/day from day offset (naive, works for most dates up to 2100)
    let days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    // Advance years
    while remaining > 365 {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining >= days_in_year {
            remaining -= days_in_year;
            y += 1;
        } else {
            break;
        }
    }

    let mut m = 0usize;
    let mut d = remaining;
    for (i, &dim) in days_in_month.iter().enumerate() {
        let dim_actual = if i == 1 && is_leap(y) { 29 } else { dim };
        if d >= dim_actual as i64 {
            d -= dim_actual as i64;
        } else {
            m = i + 1;
            break;
        }
    }
    if m == 0 {
        m = 12;
        // shouldn't happen
    }
    let day = d + 1; // 1-based

    let total_seconds = (frac * 86400.0).round() as i64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        y, m, day, hours, minutes, seconds
    )
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn write_cell(worksheet: &mut rust_xlsxwriter::Worksheet, row: u32, col: u16, val: Option<&Value>) {
    let Some(v) = val else { return };
    match v {
        Value::Null => {}
        Value::String(s) => {
            let _ = worksheet.write_string(row, col, s);
        }
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                let _ = worksheet.write_number(row, col, f);
            } else {
                let _ = worksheet.write_string(row, col, n.to_string());
            }
        }
        Value::Bool(b) => {
            let _ = worksheet.write_boolean(row, col, *b);
        }
        Value::Array(_) | Value::Object(_) => {
            let _ = worksheet.write_string(row, col, serde_json::to_string(v).unwrap_or_default());
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn excel_bin() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-excel");
        p
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-excel");
        assert!(!m.version.is_empty());
        assert!(m.streaming);
        assert!(m.idempotent);
        assert!(m.inputs.get("required").is_some());
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(excel_bin())
            .arg("--describe")
            .output()
            .expect("spawn na-excel --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-excel"));
        assert!(stdout.contains("\"streaming\": true"));
    }

    #[test]
    fn test_write_then_read_roundtrip() {
        let bin = excel_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_excel_roundtrip.xlsx");
        let _ = std::fs::remove_file(&file_path);

        // Write
        let write_input = serde_json::json!({
            "action": "write",
            "path": file_path.to_string_lossy(),
            "rows": [
                {"name": "Alice", "age": 30},
                {"name": "Bob", "age": 25}
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
                .write_all(write_input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(
            output.status.success(),
            "write failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Verify file exists
        assert!(file_path.exists(), "written file not found");

        // Read back
        let read_input = serde_json::json!({
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
                .write_all(read_input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(
            output.status.success(),
            "read failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!(lines.len(), 2, "expected 2 NDJSON lines, got: {lines:?}");
        let row1: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(row1["A"], 30, "first row col A: {:?}", row1);
        assert_eq!(row1["B"], "Alice", "first row col B: {:?}", row1);
        let row2: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(row2["A"], 25);
        assert_eq!(row2["B"], "Bob");

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_read_sheet_by_name() {
        let bin = excel_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_excel_sheet.xlsx");
        let _ = std::fs::remove_file(&file_path);

        // Write with custom sheet name
        let write_input = serde_json::json!({
            "action": "write",
            "path": file_path.to_string_lossy(),
            "sheet": "MySheet",
            "rows": [{"val": 42}]
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
                .write_all(write_input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(
            output.status.success(),
            "write: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Read by name
        let read_input = serde_json::json!({
            "action": "read",
            "path": file_path.to_string_lossy(),
            "sheet": "MySheet"
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
                .write_all(read_input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(
            output.status.success(),
            "read: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let row: Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
        assert_eq!(row["A"], 42);

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_read_sheet_by_index() {
        let bin = excel_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_excel_index.xlsx");
        let _ = std::fs::remove_file(&file_path);

        // Write
        let write_input = serde_json::json!({
            "action": "write",
            "path": file_path.to_string_lossy(),
            "rows": [{"x": 1}]
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
                .write_all(write_input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(output.status.success());

        // Read by index 0
        let read_input = serde_json::json!({
            "action": "read",
            "path": file_path.to_string_lossy(),
            "sheet": "0"
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
                .write_all(read_input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(
            output.status.success(),
            "read: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let row: Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
        assert_eq!(row["A"], 1);

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_read_range() {
        let bin = excel_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_excel_range.xlsx");
        let _ = std::fs::remove_file(&file_path);

        // Write 3 rows, 3 cols
        let write_input = serde_json::json!({
            "action": "write",
            "path": file_path.to_string_lossy(),
            "columns": ["a", "b", "c"],
            "rows": [
                {"a": 1, "b": 2, "c": 3},
                {"a": 4, "b": 5, "c": 6},
                {"a": 7, "b": 8, "c": 9}
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
                .write_all(write_input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(
            output.status.success(),
            "write: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        // Read range A2:C2 (just second row)
        let read_input = serde_json::json!({
            "action": "read",
            "path": file_path.to_string_lossy(),
            "range": "A2:C2"
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
                .write_all(read_input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(
            output.status.success(),
            "read: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!(lines.len(), 1, "expected 1 row from range, got: {lines:?}");
        let row: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(row["A"], 4, "A col from range: {:?}", row);
        assert_eq!(row["B"], 5);
        assert_eq!(row["C"], 6);

        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_invalid_path() {
        let bin = excel_bin();
        let input = serde_json::json!({
            "action": "read",
            "path": "/nonexistent/file.xlsx"
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
    fn test_write_missing_rows() {
        let bin = excel_bin();
        let dir = std::env::temp_dir();
        let file_path = dir.join("ngalir_test_excel_no_rows.xlsx");
        let _ = std::fs::remove_file(&file_path);

        let input = serde_json::json!({
            "action": "write",
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
        assert!(!output.status.success());
        let _ = std::fs::remove_file(&file_path);
    }

    #[test]
    fn test_column_letter() {
        assert_eq!(column_letter(0), "A");
        assert_eq!(column_letter(25), "Z");
        assert_eq!(column_letter(26), "AA");
        assert_eq!(column_letter(27), "AB");
    }

    #[test]
    fn test_column_index() {
        assert_eq!(column_index("A"), 0);
        assert_eq!(column_index("Z"), 25);
        assert_eq!(column_index("AA"), 26);
    }

    #[test]
    fn test_parse_cell_ref() {
        let (col, row) = parse_cell_ref("A1");
        assert_eq!(col, Some(0));
        assert_eq!(row, Some(0));

        let (col, row) = parse_cell_ref("C10");
        assert_eq!(col, Some(2));
        assert_eq!(row, Some(9));

        let (col, row) = parse_cell_ref("AA");
        assert_eq!(col, Some(26));
        assert_eq!(row, None);
    }

    #[test]
    fn test_calamine_data_to_json() {
        use calamine::Data;
        assert_eq!(calamine_data_to_json(&Data::Empty), Value::Null);
        assert_eq!(
            calamine_data_to_json(&Data::String("hello".into())),
            Value::String("hello".into())
        );
        assert_eq!(
            calamine_data_to_json(&Data::Float(3.5)),
            serde_json::json!(3.5)
        );
        assert_eq!(calamine_data_to_json(&Data::Int(42)), serde_json::json!(42));
        assert_eq!(calamine_data_to_json(&Data::Bool(true)), Value::Bool(true));
    }

    #[test]
    fn test_resolve_sheets() {
        let all = vec!["Sheet1".into(), "Sheet2".into(), "Data".into()];
        assert_eq!(resolve_sheets(&all, None), vec!["Sheet1"]);
        assert_eq!(resolve_sheets(&all, Some("Sheet2")), vec!["Sheet2"]);
        assert_eq!(resolve_sheets(&all, Some("0")), vec!["Sheet1"]);
        assert_eq!(resolve_sheets(&all, Some("1")), vec!["Sheet2"]);
    }
}
