use na_contract::{exit_code, fail, print_manifest, read_input, Example, Manifest};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-xml".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Parse XML documents into JSON or generate XML from JSON.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["read", "write"], "description": "read (parse) or write (serialize) XML" },
                "path": { "type": "string", "description": "file path (required for read; optional for write)" },
                "xml": { "type": "string", "description": "inline XML string (alternative to path for read)" },
                "root_name": { "type": "string", "default": "root", "description": "root element name for write" },
                "item_name": { "type": "string", "default": "item", "description": "element name for array items in write" },
                "data": { "type": "object", "description": "JSON data to serialize (required for write)" }
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
        credentials: vec![],
        streaming: false,
        idempotent: true,
        output_mode: None,
        use_cases: vec![
            "xml".into(),
            "etl".into(),
            "parse".into(),
            "serialize".into(),
        ],
        examples: vec![
            Example {
                input: serde_json::json!({"action": "read", "xml": "<root><item id=\"1\"><name>Alice</name></item></root>"}),
                output: serde_json::json!({"result": {"item": [{"@id": "1", "name": "Alice"}]}, "count": 1}),
            },
            Example {
                input: serde_json::json!({"action": "write", "root_name": "catalog", "data": {"book": {"@id": "1", "title": "XML Guide"}}}),
                output: serde_json::json!({"written": true}),
            },
        ],
        see_also: vec!["jsonpath".into(), "yaml".into()],
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
    let xml_str = input["xml"]
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
                "provide 'xml' string or 'path' for read action",
            );
        });

    let result = parse_xml_to_json(&xml_str);
    let count = match &result {
        Value::Object(m) => m
            .values()
            .filter_map(|v| v.as_array().map(|a| a.len()))
            .sum::<usize>(),
        _ => 0,
    };
    let output = serde_json::json!({ "result": result, "count": count });
    println!("{output}");
}

fn parse_xml_to_json(xml: &str) -> Value {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut node_stack: Vec<serde_json::Map<String, Value>> = Vec::new();
    let mut name_stack: Vec<String> = Vec::new();
    let mut current_text = String::new();
    let mut root: Option<Value> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                current_text.clear();
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                name_stack.push(name);
                let mut map = serde_json::Map::new();
                for attr in e.attributes().flatten() {
                    let key = format!("@{}", String::from_utf8_lossy(attr.key.as_ref()));
                    let val = String::from_utf8_lossy(&attr.value).to_string();
                    map.insert(key, Value::String(val));
                }
                node_stack.push(map);
            }
            Ok(Event::Empty(e)) => {
                current_text.clear();
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let mut map = serde_json::Map::new();
                for attr in e.attributes().flatten() {
                    let key = format!("@{}", String::from_utf8_lossy(attr.key.as_ref()));
                    let val = String::from_utf8_lossy(&attr.value).to_string();
                    map.insert(key, Value::String(val));
                }
                let val = if map.is_empty() {
                    Value::String(String::new())
                } else {
                    Value::Object(map)
                };
                if let Some(parent) = node_stack.last_mut() {
                    add_to_parent(parent, &name, val);
                } else {
                    root = Some(val);
                }
            }
            Ok(Event::End(_)) => {
                let name = name_stack.pop();
                let mut node = node_stack.pop().unwrap_or_default();
                let trimmed = current_text.trim().to_string();
                current_text.clear();
                if !trimmed.is_empty() {
                    if node.is_empty() {
                        if let Some(parent) = node_stack.last_mut() {
                            if let Some(ref n) = name {
                                add_to_parent(parent, n, Value::String(trimmed));
                            }
                        } else {
                            root = Some(Value::String(trimmed));
                        }
                        buf.clear();
                        continue;
                    }
                    node.insert("#text".to_string(), Value::String(trimmed));
                }
                let val = if node.is_empty() {
                    Value::Null
                } else {
                    Value::Object(node)
                };
                if let Some(ref n) = name {
                    if let Some(parent) = node_stack.last_mut() {
                        add_to_parent(parent, n, val);
                    } else {
                        root = Some(serde_json::json!({ n: val }));
                    }
                }
            }
            Ok(Event::Text(e)) => {
                current_text = e.unescape().unwrap_or_default().to_string();
            }
            Ok(Event::Eof) => break,
            Err(e) => fail(exit_code::GENERIC, format!("XML parse error: {e}")),
            _ => {}
        }
        buf.clear();
    }

    root.unwrap_or(Value::Null)
}

fn add_to_parent(parent: &mut serde_json::Map<String, Value>, key: &str, val: Value) {
    if let Some(existing) = parent.get_mut(key) {
        match existing {
            Value::Array(arr) => arr.push(val),
            Value::Object(_) | Value::String(_) | Value::Number(_) | Value::Bool(_) => {
                let old = std::mem::take(existing);
                *existing = Value::Array(vec![old, val]);
            }
            Value::Null => {
                *existing = val;
            }
        }
    } else {
        parent.insert(key.to_string(), val);
    }
}

fn cmd_write(input: &Value) {
    let root_name = input["root_name"].as_str().unwrap_or("root");
    let item_name = input["item_name"].as_str().unwrap_or("item");
    let data = match input.get("data") {
        Some(d) => d,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'data' field for write action",
        ),
    };

    let xml = value_to_xml(data, root_name, item_name, 0);
    let output_path = input["path"].as_str();

    match output_path {
        Some(path) => {
            std::fs::write(path, &xml).unwrap_or_else(|e| {
                fail(exit_code::GENERIC, format!("write file failed: {e}"));
            });
            let output = serde_json::json!({ "written": true, "path": path });
            println!("{output}");
        }
        None => {
            println!("{xml}");
        }
    }
}

fn value_to_xml(val: &Value, elem_name: &str, item_name: &str, depth: usize) -> String {
    let indent = "  ".repeat(depth);

    match val {
        Value::Object(map) => {
            let mut attrs = String::new();
            let mut children = Vec::new();
            for (k, v) in map {
                if k.starts_with('@') {
                    let attr_name = k.trim_start_matches('@');
                    attrs.push_str(&format!(" {}=\"{}\"", attr_name, json_to_attr(v)));
                } else if k == "#text" {
                    return format!(
                        "{indent}<{elem_name}{attrs}>{}</{elem_name}>",
                        json_to_text(v)
                    );
                } else {
                    children.push((k.clone(), v.clone()));
                }
            }
            if children.is_empty() {
                format!("{indent}<{elem_name}{attrs} />")
            } else {
                let inner: String = children
                    .iter()
                    .map(|(k, v)| value_to_xml(v, k, item_name, depth + 1))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("{indent}<{elem_name}{attrs}>\n{inner}\n{indent}</{elem_name}>")
            }
        }
        Value::Array(arr) => arr
            .iter()
            .map(|v| value_to_xml(v, item_name, item_name, depth))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::String(s) => format!("{indent}<{elem_name}>{}</{elem_name}>", escaped_xml(s)),
        Value::Number(n) => format!("{indent}<{elem_name}>{n}</{elem_name}>"),
        Value::Bool(b) => format!("{indent}<{elem_name}>{b}</{elem_name}>"),
        Value::Null => format!("{indent}<{elem_name} />"),
    }
}

fn json_to_attr(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn json_to_text(v: &Value) -> String {
    match v {
        Value::String(s) => escaped_xml(s),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        _ => serde_json::to_string(v).unwrap_or_default(),
    }
}

fn escaped_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn xml_bin() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-xml");
        p
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-xml");
        assert!(!m.version.is_empty());
        assert!(m.inputs.get("required").is_some());
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(xml_bin())
            .arg("--describe")
            .output()
            .expect("spawn na-xml --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-xml"));
    }

    #[test]
    fn test_parse_simple_xml() {
        let bin = xml_bin();
        let input = serde_json::json!({
            "action": "read",
            "xml": "<root><item id=\"1\"><name>Alice</name><age>30</age></item></root>"
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
        assert!(result["result"]["root"]["item"].is_object());
    }

    #[test]
    fn test_generate_xml() {
        let bin = xml_bin();
        let input = serde_json::json!({
            "action": "write",
            "root_name": "catalog",
            "data": {"book": {"@id": "1", "title": "XML Guide"}}
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
        assert!(stdout.contains("catalog") || stdout.contains("written"));
    }

    #[test]
    fn test_invalid_action() {
        let bin = xml_bin();
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
    fn test_escaped_xml() {
        assert_eq!(escaped_xml("a & b < c > d"), "a &amp; b &lt; c &gt; d");
    }
}
