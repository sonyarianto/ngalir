use na_contract::{print_manifest, read_input, Example, Manifest};
use serde_json::Value;

enum PathSegment<'a> {
    Key(&'a str),
    ArrayIter,
    ArraySlice(Option<isize>, Option<isize>),
}

fn manifest() -> Manifest {
    Manifest {
        name: "na-jsonpath".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description:
            "Extract / transform JSON via jq-compatible filters (.[] | {id, name}, .[0:5], etc.)"
                .to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "data": { "description": "The JSON value to query" },
                "filter": { "type": "string", "description": "jq-compatible filter, e.g. rows[].name or .[] | {id, name}" }
            },
            "required": ["data", "filter"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": { "result": {} }
        }),
        secrets: vec![],
        credentials: vec![],
        streaming: false,
        idempotent: true,
        output_mode: None,
        use_cases: vec![
            "transform".into(),
            "filter".into(),
            "json".into(),
            "jq".into(),
        ],
        examples: vec![Example {
            input: serde_json::json!({"items": [{"id": 1}, {"id": 2}]}),
            output: serde_json::json!([{"id": 1}, {"id": 2}]),
        }],
        see_also: vec!["echo".into()],
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
    let data = input.get("data").unwrap_or(&Value::Null).clone();
    let filter = input.get("filter").and_then(Value::as_str).unwrap_or(".");

    let result = eval_filter(&data, filter);
    println!("{}", serde_json::json!({"result": result}));
}

fn eval_filter(data: &Value, filter: &str) -> Value {
    let mut filter = filter.trim();
    if let Some(rest) = filter.strip_prefix('$') {
        let rest = rest.trim();
        filter = if rest.is_empty() { "." } else { rest };
    }
    if filter.is_empty() || filter == "." {
        return data.clone();
    }
    let pipeline: Vec<&str> = filter.split('|').map(|s| s.trim()).collect();
    let mut current = data.clone();
    for expr in pipeline {
        current = eval_stage(&current, expr);
    }
    current
}

fn eval_stage(data: &Value, expr: &str) -> Value {
    let expr = expr.trim();
    if expr.is_empty() || expr == "." {
        return data.clone();
    }

    if let Some(body) = expr.strip_prefix('.') {
        if body == "[]" {
            return match data {
                Value::Array(arr) => Value::Array(arr.clone()),
                _ => Value::Array(vec![]),
            };
        }
        if body.starts_with('[') && body.ends_with(']') {
            let inner = &body[1..body.len() - 1];
            if let Some(slice) = parse_slice(inner) {
                return slice_array(data, slice.0, slice.1);
            }
        }
        return resolve_path(data, body);
    }

    if expr.starts_with('[') && expr.ends_with(']') {
        let inner = &expr[1..expr.len() - 1];
        if let Some(slice) = parse_slice(inner) {
            return slice_array(data, slice.0, slice.1);
        }
        if let Ok(idx) = inner.parse::<usize>() {
            return match data {
                Value::Array(arr) => arr.get(idx).cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            };
        }
    }

    if expr.starts_with('{') && expr.ends_with('}') {
        let inner = expr[1..expr.len() - 1].trim();
        return construct_objects(data, inner);
    }

    resolve_path(data, expr)
}

fn parse_slice(s: &str) -> Option<(Option<isize>, Option<isize>)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some((a, b)) = s.split_once(':') {
        let start = a.trim();
        let end = b.trim();
        let s = if start.is_empty() {
            None
        } else {
            Some(start.parse::<isize>().ok()?)
        };
        let e = if end.is_empty() {
            None
        } else {
            Some(end.parse::<isize>().ok()?)
        };
        return Some((s, e));
    }
    None
}

fn slice_array(data: &Value, start: Option<isize>, end: Option<isize>) -> Value {
    match data {
        Value::Array(arr) => {
            let len = arr.len() as isize;
            let s = start.map_or(0, |v| if v < 0 { (len + v).max(0) } else { v.min(len) }) as usize;
            let e = end.map_or(len, |v| if v < 0 { (len + v).max(0) } else { v.min(len) }) as usize;
            Value::Array(arr[s..e].to_vec())
        }
        _ => Value::Array(vec![]),
    }
}

fn parse_dot_path_segments(path: &str) -> Vec<PathSegment<'_>> {
    let mut segments = Vec::new();
    for part in path.split('.') {
        if part.is_empty() {
            continue;
        }
        if let Some(key) = part.strip_suffix("[]") {
            segments.push(PathSegment::Key(key));
            segments.push(PathSegment::ArrayIter);
        } else if let Some(rest) = part.strip_suffix(']') {
            if let Some((key, slice_str)) = rest.split_once('[') {
                if let Some(slice) = parse_slice(slice_str) {
                    segments.push(PathSegment::Key(key));
                    segments.push(PathSegment::ArraySlice(slice.0, slice.1));
                } else {
                    segments.push(PathSegment::Key(part));
                }
            } else {
                segments.push(PathSegment::Key(part));
            }
        } else {
            segments.push(PathSegment::Key(part));
        }
    }
    segments
}

fn resolve_path(value: &Value, path: &str) -> Value {
    if path.is_empty() || path == "." {
        return value.clone();
    }
    let segments = parse_dot_path_segments(path);
    let mut current = value.clone();
    for seg in segments {
        current = match seg {
            PathSegment::Key(key) => match current {
                Value::Object(ref obj) => obj.get(key).cloned().unwrap_or(Value::Null),
                Value::Array(ref arr) => {
                    if let Ok(idx) = key.parse::<usize>() {
                        arr.get(idx).cloned().unwrap_or(Value::Null)
                    } else {
                        Value::Null
                    }
                }
                _ => Value::Null,
            },
            PathSegment::ArrayIter => match current {
                Value::Array(arr) => Value::Array(arr),
                _ => Value::Array(vec![]),
            },
            PathSegment::ArraySlice(start, end) => slice_array(&current, start, end),
        };
    }
    current
}

fn construct_objects(data: &Value, fields_spec: &str) -> Value {
    let fields: Vec<&str> = fields_spec
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let keys: Vec<(&str, Option<&str>)> = fields
        .iter()
        .map(|f| {
            if let Some((key, val_expr)) = f.split_once(':') {
                (key.trim(), Some(val_expr.trim()))
            } else {
                (f.trim(), None)
            }
        })
        .collect();

    match data {
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|item| build_object(item, &keys)).collect())
        }
        _ => build_object(data, &keys),
    }
}

fn build_object(data: &Value, keys: &[(&str, Option<&str>)]) -> Value {
    let mut map = serde_json::Map::new();
    for (key, val_expr) in keys {
        let val = match val_expr {
            Some(expr) => eval_stage(data, expr),
            None => resolve_path(data, key),
        };
        map.insert(key.to_string(), val);
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-jsonpath");
        assert!(m
            .inputs
            .get("required")
            .unwrap()
            .as_array()
            .unwrap()
            .contains(&json!("data")));
        assert!(m
            .inputs
            .get("required")
            .unwrap()
            .as_array()
            .unwrap()
            .contains(&json!("filter")));
        assert!(m.idempotent);
    }

    #[test]
    fn test_dot_returns_self() {
        let v = json!({"a": 1});
        assert_eq!(eval_filter(&v, "."), v);
        assert_eq!(eval_filter(&v, ""), v);
    }

    #[test]
    fn test_nested_object() {
        let v = json!({"a": {"b": {"c": 42}}});
        assert_eq!(eval_filter(&v, ".a.b.c"), json!(42));
    }

    #[test]
    fn test_array_index() {
        let v = json!({"rows": [{"name": "alice"}, {"name": "bob"}]});
        assert_eq!(eval_filter(&v, "rows.0.name"), json!("alice"));
        assert_eq!(eval_filter(&v, ".rows.0.name"), json!("alice"));
    }

    #[test]
    fn test_missing_returns_null() {
        let v = json!({"a": 1});
        assert_eq!(eval_filter(&v, "a.b.c"), Value::Null);
        assert_eq!(eval_filter(&v, "x.y"), Value::Null);
    }

    #[test]
    fn test_out_of_bounds() {
        let v = json!([1, 2, 3]);
        assert_eq!(eval_filter(&v, "5"), Value::Null);
        assert_eq!(eval_filter(&v, ".0"), json!(1));
    }

    #[test]
    fn test_array_iter() {
        let v = json!([{"id": 1}, {"id": 2}]);
        assert_eq!(eval_filter(&v, ".[]"), v);
        assert_eq!(eval_filter(&v, ".[]"), json!([{"id": 1}, {"id": 2}]));
    }

    #[test]
    fn test_array_slice() {
        let v = json!([10, 20, 30, 40, 50]);
        assert_eq!(eval_filter(&v, ".[0:3]"), json!([10, 20, 30]));
        assert_eq!(eval_filter(&v, ".[2:]"), json!([30, 40, 50]));
        assert_eq!(eval_filter(&v, ".[:2]"), json!([10, 20]));
        assert_eq!(eval_filter(&v, ".[1:4]"), json!([20, 30, 40]));
    }

    #[test]
    fn test_negative_slice() {
        let v = json!([10, 20, 30, 40, 50]);
        assert_eq!(eval_filter(&v, ".[-2:]"), json!([40, 50]));
        assert_eq!(eval_filter(&v, ".[:-2]"), json!([10, 20, 30]));
    }

    #[test]
    fn test_object_reconstruction() {
        let v = json!({"id": 1, "name": "alice", "age": 30});
        assert_eq!(
            eval_filter(&v, "{id, name}"),
            json!({"id": 1, "name": "alice"})
        );
    }

    #[test]
    fn test_object_reconstruction_with_expr() {
        let v = json!({"id": 1, "name": "alice", "age": 30});
        assert_eq!(
            eval_filter(&v, "{user_id: .id, full_name: .name}"),
            json!({"user_id": 1, "full_name": "alice"})
        );
    }

    #[test]
    fn test_pipe_array_iter_to_object() {
        let v = json!([
            {"id": 1, "name": "alice", "age": 30},
            {"id": 2, "name": "bob", "age": 25}
        ]);
        assert_eq!(
            eval_filter(&v, ".[] | {id, name}"),
            json!([
                {"id": 1, "name": "alice"},
                {"id": 2, "name": "bob"}
            ])
        );
    }

    #[test]
    fn test_pipe_slice_to_object() {
        let v = json!([
            {"id": 1, "name": "alice"},
            {"id": 2, "name": "bob"},
            {"id": 3, "name": "charlie"}
        ]);
        assert_eq!(
            eval_filter(&v, ".[0:2] | {id}"),
            json!([
                {"id": 1},
                {"id": 2}
            ])
        );
    }

    #[test]
    fn test_nested_pipeline() {
        let v = json!({
            "items": [
                {"product": "A", "price": 10, "qty": 2},
                {"product": "B", "price": 15, "qty": 3}
            ]
        });
        assert_eq!(
            eval_filter(&v, ".items[] | {product}"),
            json!([
                {"product": "A"},
                {"product": "B"}
            ])
        );
    }

    #[test]
    fn test_empty_array_iter_on_non_array() {
        let v = json!({"a": 1});
        assert_eq!(eval_filter(&v, ".[]"), json!([]));
    }

    #[test]
    fn test_slice_out_of_bounds() {
        let v = json!([1, 2, 3]);
        assert_eq!(eval_filter(&v, ".[5:10]"), json!([]));
        assert_eq!(eval_filter(&v, ".[0:100]"), json!([1, 2, 3]));
    }

    #[test]
    fn test_object_reconstruction_single_field() {
        let v = json!({"a": {"x": 1}, "b": {"y": 2}});
        assert_eq!(eval_filter(&v, "{a}"), json!({"a": {"x": 1}}));
    }

    #[test]
    fn test_object_reconstruction_nested_dotpath() {
        let v = json!([{"name": "alice", "meta": {"score": 100}}, {"name": "bob", "meta": {"score": 200}}]);
        assert_eq!(
            eval_filter(&v, ".[] | {name, score: .meta.score}"),
            json!([
                {"name": "alice", "score": 100},
                {"name": "bob", "score": 200}
            ])
        );
    }

    #[test]
    fn test_dollar_root_identity() {
        let v = json!({"a": 1});
        assert_eq!(eval_filter(&v, "$"), v);
    }

    #[test]
    fn test_dollar_dot_path() {
        let v = json!({"a": {"b": 42}});
        assert_eq!(eval_filter(&v, "$.a.b"), json!(42));
    }

    #[test]
    fn test_dollar_bracket_index() {
        let v = json!([10, 20, 30]);
        assert_eq!(eval_filter(&v, "$[0]"), json!(10));
    }

    #[test]
    fn test_dollar_dot_side_by_side() {
        let v = json!({"title": "hello"});
        assert_eq!(eval_filter(&v, "$.title"), eval_filter(&v, ".title"));
        assert_eq!(eval_filter(&v, "$"), eval_filter(&v, "."));
    }

    #[test]
    fn test_whitespace_after_dollar() {
        let v = json!({"a": 1});
        assert_eq!(eval_filter(&v, "$ .a"), json!(1));
    }
}
