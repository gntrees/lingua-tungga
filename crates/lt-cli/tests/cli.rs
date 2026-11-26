use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_lt")
}

fn temp_dir(tag: &str) -> PathBuf {
    let unique = format!(
        "lt-cli-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn fmt_rewrites_file_in_place() {
    let dir = temp_dir("fmt");
    let file = dir.join("a.lt");
    std::fs::write(&file, "pub class A{name:Str}\n").unwrap();

    let status = Command::new(bin()).arg("fmt").arg(&file).status().unwrap();
    assert!(status.success());

    let formatted = std::fs::read_to_string(&file).unwrap();
    assert_eq!(formatted, "pub class A {\n  name: Str\n}\n");

    // Idempotent: a second run leaves the file untouched.
    let status = Command::new(bin())
        .arg("fmt")
        .arg("--check")
        .arg(&file)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn fmt_check_reports_unformatted_file_without_touching_it() {
    let dir = temp_dir("check");
    let file = dir.join("a.lt");
    let original = "pub class A{name:Str}\n";
    std::fs::write(&file, original).unwrap();

    let output = Command::new(bin())
        .arg("fmt")
        .arg("--check")
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
}

#[test]
fn fmt_refuses_to_delete_comments() {
    let dir = temp_dir("comments");
    let file = dir.join("a.lt");
    let original = "// keep me\npub class A{name:Str}\n";
    std::fs::write(&file, original).unwrap();

    let output = Command::new(bin()).arg("fmt").arg(&file).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot preserve comments"), "{stderr}");
}

#[test]
fn check_reports_diagnostics() {
    let dir = temp_dir("broken");
    let file = dir.join("bad.lt");
    std::fs::write(&file, "pub class A { name Str }\n").unwrap();

    let output = Command::new(bin())
        .arg("check")
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("bad.lt"), "{stderr}");
    assert!(stderr.contains("error:"), "{stderr}");
}

#[test]
fn check_walks_directories_recursively() {
    let dir = temp_dir("walk");
    let nested = dir.join("src/models");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(dir.join("root.lt"), "pub class A { x: Int }\n").unwrap();
    std::fs::write(nested.join("user.lt"), "pub class User { name: Str }\n").unwrap();

    let output = Command::new(bin()).arg("check").arg(&dir).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), 2, "{stdout}");
}

#[test]
fn ast_dumps_without_error() {
    let dir = temp_dir("ast");
    let file = dir.join("a.lt");
    std::fs::write(&file, "pub class A { x: Int }\n").unwrap();

    let output = Command::new(bin())
        .arg("ast")
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ClassDecl"), "{stdout}");
}

#[test]
fn examples_typecheck() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let output = Command::new(bin())
        .arg("check")
        .arg(&root)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
