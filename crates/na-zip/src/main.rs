use na_contract::{exit_code, fail, print_manifest, read_input, Example, Manifest};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::Path;

fn manifest() -> Manifest {
    Manifest {
        name: "na-zip".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Compress and decompress archives (zip, gzip).".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["compress", "decompress", "list"],
                    "description": "compress (create archive), decompress (extract), list (list entries)"
                },
                "format": {
                    "type": "string",
                    "enum": ["zip", "gzip"],
                    "description": "archive format (default: zip)"
                },
                "path": { "type": "string", "description": "path to archive file" },
                "output": { "type": "string", "description": "output directory for decompress, or output path for compress" },
                "files": {
                    "type": "array",
                    "description": "files to compress: array of {path, name?} objects or string paths"
                }
            },
            "required": ["action"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "entries": { "type": "array", "description": "list of archive entries" },
                "count": { "type": "integer" },
                "output": { "type": "string", "description": "output path or directory" }
            }
        }),
        secrets: vec![],
        credentials: vec![],
        streaming: false,
        idempotent: false,
        output_mode: None,
        use_cases: vec![
            "zip".into(),
            "archive".into(),
            "compress".into(),
            "etl".into(),
        ],
        examples: vec![
            Example {
                input: serde_json::json!({"action": "list", "path": "/data/archive.zip"}),
                output: serde_json::json!({"entries": ["file1.csv", "file2.json"], "count": 2}),
            },
            Example {
                input: serde_json::json!({"action": "compress", "files": [{"path": "/data/file.csv", "name": "data/file.csv"}], "output": "/tmp/out.zip"}),
                output: serde_json::json!({"output": "/tmp/out.zip", "count": 1}),
            },
        ],
        see_also: vec!["file".into(), "http".into()],
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
    let format = input["format"].as_str().unwrap_or("zip");

    match action {
        "compress" => cmd_compress(&input, format),
        "decompress" => cmd_decompress(&input, format),
        "list" => cmd_list(&input, format),
        _ => fail(
            exit_code::INVALID_INPUT,
            "action must be 'compress', 'decompress', or 'list'",
        ),
    }
}

fn cmd_compress(input: &Value, format: &str) {
    let files = match input.get("files").and_then(Value::as_array) {
        Some(f) => f,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'files' array for compress action",
        ),
    };
    let output_path = input["output"].as_str().unwrap_or_else(|| {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'output' path for compress action",
        )
    });

    match format {
        "zip" => compress_zip(files, output_path),
        "gzip" => compress_gzip(files, output_path),
        _ => fail(exit_code::INVALID_INPUT, "format must be 'zip' or 'gzip'"),
    }
}

fn compress_zip(files: &[Value], output_path: &str) {
    let file = std::fs::File::create(output_path)
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("create zip failed: {e}")));
    let mut zip_writer = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut count = 0u64;
    for entry in files {
        let (src, name) = match entry {
            Value::Object(m) => {
                let path = m.get("path").and_then(Value::as_str).unwrap_or_else(|| {
                    fail(
                        exit_code::INVALID_INPUT,
                        "each file entry needs a 'path' field",
                    )
                });
                let name = m.get("name").and_then(Value::as_str).unwrap_or(path);
                (path, name)
            }
            Value::String(s) => (s.as_str(), s.as_str()),
            _ => fail(
                exit_code::INVALID_INPUT,
                "file entry must be a string or object",
            ),
        };

        let mut f = std::fs::File::open(src)
            .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("open '{src}' failed: {e}")));
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("read '{src}' failed: {e}")));

        zip_writer.start_file(name, options).unwrap_or_else(|e| {
            fail(
                exit_code::GENERIC,
                format!("zip start_file '{name}' failed: {e}"),
            )
        });
        zip_writer.write_all(&buf).unwrap_or_else(|e| {
            fail(
                exit_code::GENERIC,
                format!("zip write '{name}' failed: {e}"),
            )
        });
        count += 1;
    }

    zip_writer
        .finish()
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("zip finish failed: {e}")));

    let output = serde_json::json!({ "output": output_path, "count": count });
    println!("{output}");
}

fn compress_gzip(files: &[Value], output_path: &str) {
    if files.len() != 1 {
        fail(
            exit_code::INVALID_INPUT,
            "gzip supports exactly one input file",
        );
    }

    let src = match &files[0] {
        Value::Object(m) => m
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_else(|| fail(exit_code::INVALID_INPUT, "file entry needs a 'path' field")),
        Value::String(s) => s.as_str(),
        _ => fail(
            exit_code::INVALID_INPUT,
            "file entry must be a string or object",
        ),
    };

    let mut f = std::fs::File::open(src)
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("open '{src}' failed: {e}")));
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("read '{src}' failed: {e}")));

    let out = std::fs::File::create(output_path).unwrap_or_else(|e| {
        fail(
            exit_code::GENERIC,
            format!("create '{output_path}' failed: {e}"),
        )
    });
    let mut encoder = flate2::write::GzEncoder::new(out, flate2::Compression::default());
    encoder
        .write_all(&buf)
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("gzip write failed: {e}")));
    encoder
        .finish()
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("gzip finish failed: {e}")));

    let output = serde_json::json!({ "output": output_path, "count": 1 });
    println!("{output}");
}

fn cmd_decompress(input: &Value, format: &str) {
    let path = input["path"].as_str().unwrap_or_else(|| {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'path' for decompress action",
        )
    });
    let output_dir = input["output"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let p = Path::new(path);
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("extracted");
            format!("./{stem}")
        });

    match format {
        "zip" => decompress_zip(path, &output_dir),
        "gzip" => decompress_gzip(path, &output_dir),
        _ => fail(exit_code::INVALID_INPUT, "format must be 'zip' or 'gzip'"),
    }
}

fn decompress_zip(path: &str, output_dir: &str) {
    let file = std::fs::File::open(path)
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("open '{path}' failed: {e}")));
    let mut archive = zip::ZipArchive::new(file)
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("read zip '{path}' failed: {e}")));

    let count = archive.len();
    std::fs::create_dir_all(output_dir).unwrap_or_else(|e| {
        fail(
            exit_code::GENERIC,
            format!("create dir '{output_dir}' failed: {e}"),
        )
    });

    let mut extracted: Vec<String> = Vec::new();
    for i in 0..count {
        let mut entry = archive
            .by_index(i)
            .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("read entry {i} failed: {e}")));
        let name = entry.name().to_string();
        let out_path = Path::new(output_dir).join(&name);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).unwrap_or_else(|e| {
                fail(
                    exit_code::GENERIC,
                    format!("create dir '{:?}' failed: {e}", out_path),
                )
            });
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).unwrap_or_else(|e| {
                    fail(
                        exit_code::GENERIC,
                        format!("create dir '{:?}' failed: {e}", parent),
                    )
                });
            }
            let mut out = std::fs::File::create(&out_path).unwrap_or_else(|e| {
                fail(
                    exit_code::GENERIC,
                    format!("create '{:?}' failed: {e}", out_path),
                )
            });
            std::io::copy(&mut entry, &mut out).unwrap_or_else(|e| {
                fail(exit_code::GENERIC, format!("extract '{name}' failed: {e}"))
            });
        }
        extracted.push(name);
    }

    let output = serde_json::json!({ "output": output_dir, "entries": extracted, "count": count });
    println!("{output}");
}

fn decompress_gzip(path: &str, output_dir: &str) {
    let file = std::fs::File::open(path)
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("open '{path}' failed: {e}")));
    let mut decoder = flate2::read::GzDecoder::new(file);
    let mut buf = Vec::new();
    decoder
        .read_to_end(&mut buf)
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("gzip decompress failed: {e}")));

    std::fs::create_dir_all(output_dir).unwrap_or_else(|e| {
        fail(
            exit_code::GENERIC,
            format!("create dir '{output_dir}' failed: {e}"),
        )
    });

    let p = Path::new(path);
    let out_name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let out_path = Path::new(output_dir).join(out_name);

    std::fs::write(&out_path, &buf).unwrap_or_else(|e| {
        fail(
            exit_code::GENERIC,
            format!("write '{:?}' failed: {e}", out_path),
        )
    });

    let output = serde_json::json!({
        "output": output_dir,
        "entries": [out_name],
        "count": 1
    });
    println!("{output}");
}

fn cmd_list(input: &Value, format: &str) {
    let path = input["path"]
        .as_str()
        .unwrap_or_else(|| fail(exit_code::INVALID_INPUT, "missing 'path' for list action"));

    match format {
        "zip" => list_zip(path),
        "gzip" => {
            let entries = vec![path.to_string()];
            let output = serde_json::json!({ "entries": entries, "count": 1 });
            println!("{output}");
        }
        _ => fail(exit_code::INVALID_INPUT, "format must be 'zip' or 'gzip'"),
    }
}

fn list_zip(path: &str) {
    let file = std::fs::File::open(path)
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("open '{path}' failed: {e}")));
    let mut archive = zip::ZipArchive::new(file)
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("read zip '{path}' failed: {e}")));

    let entries: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            archive.by_index(i).ok().map(|e| {
                let name = e.name().to_string();
                if e.is_dir() {
                    format!("{name}/")
                } else {
                    name
                }
            })
        })
        .collect();

    let count = entries.len();
    let output = serde_json::json!({ "entries": entries, "count": count });
    println!("{output}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn zip_bin() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-zip");
        p
    }

    fn run(input: Value) -> (bool, String, String) {
        let mut child = Command::new(zip_bin())
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
            String::from_utf8_lossy(&output.stderr).to_string(),
        )
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-zip");
        assert!(!m.version.is_empty());
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(zip_bin())
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-zip"));
    }

    #[test]
    fn test_compress_decompress_zip_roundtrip() {
        let dir = std::env::temp_dir();
        let src = dir.join("ngalir_test_zip_src.txt");
        let arc = dir.join("ngalir_test_zip.zip");
        let out = dir.join("ngalir_test_zip_out");

        std::fs::write(&src, "hello world").unwrap();
        let _ = std::fs::remove_dir_all(&out);
        let _ = std::fs::remove_file(&arc);

        let (ok, stdout, _) = run(serde_json::json!({
            "action": "compress",
            "format": "zip",
            "files": [{"path": src.to_string_lossy(), "name": "data/hello.txt"}],
            "output": arc.to_string_lossy()
        }));
        assert!(ok, "compress failed: {stdout}");
        let result: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(result["count"], 1);

        let (ok, stdout, _) = run(serde_json::json!({
            "action": "decompress",
            "format": "zip",
            "path": arc.to_string_lossy(),
            "output": out.to_string_lossy()
        }));
        assert!(ok, "decompress failed: {stdout}");

        let extracted = out.join("data/hello.txt");
        let content = std::fs::read_to_string(&extracted).unwrap();
        assert_eq!(content, "hello world");

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&arc);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn test_compress_gzip_roundtrip() {
        let dir = std::env::temp_dir();
        let src = dir.join("ngalir_test_gzip_src.txt");
        let arc = dir.join("ngalir_test_gzip.gz");
        let out = dir.join("ngalir_test_gzip_out");

        std::fs::write(&src, "gzip test data").unwrap();
        let _ = std::fs::remove_dir_all(&out);
        let _ = std::fs::remove_file(&arc);

        let (ok, stdout, _) = run(serde_json::json!({
            "action": "compress",
            "format": "gzip",
            "files": [src.to_string_lossy()],
            "output": arc.to_string_lossy()
        }));
        assert!(ok, "compress gzip failed: {stdout}");

        let (ok, stdout, _) = run(serde_json::json!({
            "action": "decompress",
            "format": "gzip",
            "path": arc.to_string_lossy(),
            "output": out.to_string_lossy()
        }));
        assert!(ok, "decompress gzip failed: {stdout}");

        let extracted = out.join("ngalir_test_gzip");
        let content = std::fs::read_to_string(&extracted).unwrap();
        assert_eq!(content, "gzip test data");

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&arc);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn test_list_zip() {
        let dir = std::env::temp_dir();
        let arc = dir.join("ngalir_test_list.zip");
        let _ = std::fs::remove_file(&arc);

        let file = std::fs::File::create(&arc).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = zip::write::FileOptions::<()>::default();
        zip_writer.start_file("a.csv", options).unwrap();
        zip_writer.write_all(b"a,b").unwrap();
        zip_writer.start_file("b.json", options).unwrap();
        zip_writer.write_all(b"{}").unwrap();
        zip_writer.finish().unwrap();

        let (ok, stdout, _) = run(serde_json::json!({
            "action": "list",
            "format": "zip",
            "path": arc.to_string_lossy()
        }));
        assert!(ok, "list failed: {stdout}");
        let result: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(result["count"], 2);
        let entries = result["entries"].as_array().unwrap();
        let names: Vec<&str> = entries.iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"a.csv"));
        assert!(names.contains(&"b.json"));

        let _ = std::fs::remove_file(&arc);
    }

    #[test]
    fn test_list_gzip() {
        let dir = std::env::temp_dir();
        let arc = dir.join("ngalir_test_list.gz");
        let _ = std::fs::remove_file(&arc);

        let out = std::fs::File::create(&arc).unwrap();
        let mut encoder = flate2::write::GzEncoder::new(out, flate2::Compression::default());
        encoder.write_all(b"test").unwrap();
        encoder.finish().unwrap();

        let (ok, stdout, _) = run(serde_json::json!({
            "action": "list",
            "format": "gzip",
            "path": arc.to_string_lossy()
        }));
        assert!(ok, "list gzip failed: {stdout}");
        let result: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(result["count"], 1);

        let _ = std::fs::remove_file(&arc);
    }

    #[test]
    fn test_invalid_action() {
        let (ok, _, _) = run(serde_json::json!({"action": "invalid"}));
        assert!(!ok);
    }

    #[test]
    fn test_compress_missing_files() {
        let (ok, _, _) = run(serde_json::json!({
            "action": "compress",
            "output": "/tmp/x.zip"
        }));
        assert!(!ok);
    }
}
