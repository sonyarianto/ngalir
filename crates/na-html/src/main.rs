use na_contract::{exit_code, fail, print_manifest, read_input, Example, Manifest};
use scraper::{Html, Selector};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-html".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Extract data from HTML documents using CSS selectors. Supports table extraction and arbitrary selector queries."
            .to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["extract", "tables"], "description": "extract (CSS selector) or tables (find all HTML tables)" },
                "path": { "type": "string", "description": "file path to HTML file" },
                "html": { "type": "string", "description": "inline HTML string (alternative to path)" },
                "url": { "type": "string", "description": "URL to fetch HTML from" },
                "selector": { "type": "string", "description": "CSS selector for extraction (required for extract action)" },
                "attribute": { "type": "string", "description": "attribute to extract (omit for text content)" },
                "table_index": { "type": "integer", "default": 0, "description": "0-based index of table to extract (tables action only)" },
                "has_headers": { "type": "boolean", "default": true, "description": "first row is header" }
            },
            "required": ["action"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "result": { "description": "extracted data" },
                "count": { "type": "integer" },
                "columns": { "type": "array", "items": { "type": "string" } }
            }
        }),
        secrets: vec![],
        credentials: vec![],
        streaming: true,
        idempotent: true,
        output_mode: None,
        use_cases: vec!["html".into(), "web-scraping".into(), "table-extraction".into(), "etl".into()],
        examples: vec![
            Example {
                input: serde_json::json!({"action": "tables", "html": "<table><tr><th>Name</th><th>Age</th></tr><tr><td>Alice</td><td>30</td></tr></table>"}),
                output: serde_json::json!({"count": 1, "columns": ["Name", "Age"]}),
            },
            Example {
                input: serde_json::json!({"action": "extract", "html": "<div class=\"item\">Hello</div>", "selector": ".item"}),
                output: serde_json::json!({"result": ["Hello"], "count": 1}),
            },
        ],
        see_also: vec!["csv".into(), "xml".into()],
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

    let html_str = resolve_html(&input);

    match action {
        "extract" => cmd_extract(&input, &html_str),
        "tables" => cmd_tables(&input, &html_str),
        _ => fail(
            exit_code::INVALID_INPUT,
            "action must be 'extract' or 'tables'",
        ),
    }
}

fn resolve_html(input: &Value) -> String {
    if let Some(html) = input["html"].as_str() {
        return html.to_string();
    }
    if let Some(path) = input["path"].as_str() {
        return std::fs::read_to_string(path)
            .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("read file failed: {e}")));
    }
    if let Some(url) = input["url"].as_str() {
        // Basic URL fetch via curl-like approach (no external HTTP dep)
        return fetch_url(url);
    }
    fail(exit_code::INVALID_INPUT, "provide 'html', 'path', or 'url'");
}

fn fetch_url(url: &str) -> String {
    // Use a simple system call to curl if available, or fallback
    let output = std::process::Command::new("curl")
        .args(["-s", "-L", url])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        });
    match output {
        Some(body) => body,
        None => {
            // Fallback: try wget
            let output = std::process::Command::new("wget")
                .args(["-q", "-O", "-", url])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        String::from_utf8(o.stdout).ok()
                    } else {
                        None
                    }
                });
            match output {
                Some(body) => body,
                None => fail(
                    exit_code::GENERIC,
                    format!(
                        "failed to fetch URL: {url}. Install curl or wget, or provide inline HTML."
                    ),
                ),
            }
        }
    }
}

fn cmd_extract(input: &Value, html_str: &str) {
    let selector_str = input["selector"].as_str().unwrap_or_else(|| {
        fail(
            exit_code::INVALID_INPUT,
            "'selector' is required for extract action",
        )
    });

    let document = Html::parse_document(html_str);
    let selector = Selector::parse(selector_str).unwrap_or_else(|e| {
        fail(
            exit_code::INVALID_INPUT,
            format!("invalid CSS selector '{selector_str}': {e}"),
        );
    });

    let attr = input["attribute"].as_str();
    let mut results = Vec::new();

    for element in document.select(&selector) {
        let val = match attr {
            Some(a) => element
                .value()
                .attr(a)
                .map(|s| s.to_string())
                .unwrap_or_default(),
            None => element
                .text()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string(),
        };
        results.push(Value::String(val));
    }

    for item in &results {
        println!("{}", serde_json::to_string(item).unwrap());
    }

    if results.is_empty() {
        println!("{}", serde_json::json!({"count": 0}));
    }
}

fn cmd_tables(input: &Value, html_str: &str) {
    let document = Html::parse_document(html_str);
    let table_selector = Selector::parse("table").unwrap();
    let tables: Vec<_> = document.select(&table_selector).collect();

    let table_idx = input["table_index"].as_i64().unwrap_or(0) as usize;
    let has_headers = input["has_headers"].as_bool().unwrap_or(true);

    let table = match tables.get(table_idx) {
        Some(t) => t,
        None => fail(
            exit_code::INVALID_INPUT,
            format!(
                "table index {table_idx} out of range (found {} tables)",
                tables.len()
            ),
        ),
    };

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut columns: Vec<String> = Vec::new();

    // Find all tr elements in the table
    let tr_selector = Selector::parse("tr").unwrap();
    let th_selector = Selector::parse("th").unwrap();
    let td_selector = Selector::parse("td").unwrap();

    for tr_elem in table.select(&tr_selector) {
        let is_header = tr_elem.select(&th_selector).count() > 0;

        if is_header && has_headers && columns.is_empty() {
            columns = tr_elem
                .select(&th_selector)
                .map(|th| th.text().collect::<Vec<_>>().join(" ").trim().to_string())
                .collect();
        } else {
            let cells: Vec<String> = tr_elem
                .select(&td_selector)
                .map(|td| td.text().collect::<Vec<_>>().join(" ").trim().to_string())
                .collect();
            if !cells.is_empty() {
                rows.push(cells);
            }
        }
    }

    if columns.is_empty() && has_headers && !rows.is_empty() {
        // Try to use first row as header
        columns = rows.remove(0);
    }

    if columns.is_empty() {
        // Generate default column names
        let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        columns = (0..max_cols).map(|i| format!("col{}", i + 1)).collect();
    }

    for row in &rows {
        let mut map = serde_json::Map::new();
        for (i, col_name) in columns.iter().enumerate() {
            let val = row.get(i).cloned().unwrap_or_default();
            map.insert(col_name.clone(), Value::String(val));
        }
        println!("{}", serde_json::to_string(&Value::Object(map)).unwrap());
    }

    if rows.is_empty() {
        let empty = columns
            .iter()
            .map(|c| (c.clone(), Value::Null))
            .collect::<serde_json::Map<_, _>>();
        println!("{}", serde_json::to_string(&Value::Object(empty)).unwrap());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn html_bin() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-html");
        p
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-html");
        assert!(!m.version.is_empty());
        assert!(m.streaming);
        assert!(m.inputs.get("required").is_some());
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(html_bin())
            .arg("--describe")
            .output()
            .expect("spawn na-html --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-html"));
    }

    #[test]
    fn test_extract_css_selector() {
        let bin = html_bin();
        let html_str = "<html><body><div class=\"item\">Hello</div><div class=\"item\">World</div></body></html>";
        let input = serde_json::json!({
            "action": "extract",
            "html": html_str,
            "selector": ".item"
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
        let val1: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(val1, "Hello");
        let val2: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(val2, "World");
    }

    #[test]
    fn test_extract_with_attribute() {
        let bin = html_bin();
        let html_str = "<html><a href=\"/page1\">Link 1</a><a href=\"/page2\">Link 2</a></html>";
        let input = serde_json::json!({
            "action": "extract",
            "html": html_str,
            "selector": "a",
            "attribute": "href"
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
        assert_eq!(serde_json::from_str::<Value>(lines[0]).unwrap(), "/page1");
        assert_eq!(serde_json::from_str::<Value>(lines[1]).unwrap(), "/page2");
    }

    #[test]
    fn test_extract_tables() {
        let bin = html_bin();
        let html_str = "\
            <html><body>\
            <table>\
            <tr><th>Name</th><th>Age</th></tr>\
            <tr><td>Alice</td><td>30</td></tr>\
            <tr><td>Bob</td><td>25</td></tr>\
            </table>\
            </body></html>";
        let input = serde_json::json!({
            "action": "tables",
            "html": html_str
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
        assert_eq!(lines.len(), 2, "expected 2 NDJSON rows, got: {lines:?}");
        let row1: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(row1["Name"], "Alice");
        assert_eq!(row1["Age"], "30");
    }

    #[test]
    fn test_extract_tables_no_headers() {
        let bin = html_bin();
        let html_str = "\
            <html><body>\
            <table>\
            <tr><td>Alice</td><td>30</td></tr>\
            <tr><td>Bob</td><td>25</td></tr>\
            </table>\
            </body></html>";
        let input = serde_json::json!({
            "action": "tables",
            "html": html_str,
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
        let row1: Value = serde_json::from_str(lines[0]).unwrap();
        assert!(row1
            .as_object()
            .is_some_and(|m| m.values().any(|v| v == "Alice")));
    }

    #[test]
    fn test_invalid_action() {
        let bin = html_bin();
        let input = serde_json::json!({"action": "invalid", "html": "<html></html>"});
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
    fn test_missing_html_source() {
        let bin = html_bin();
        let input = serde_json::json!({"action": "extract", "selector": "div"});
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
