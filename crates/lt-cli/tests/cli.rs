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
    std::fs::write(&file, "package a{struct A{name:Str}}\n").unwrap();

    let status = Command::new(bin()).arg("fmt").arg(&file).status().unwrap();
    assert!(status.success());

    let formatted = std::fs::read_to_string(&file).unwrap();
    assert_eq!(
        formatted,
        "package a {\n  struct A {\n    name: Str\n  }\n}\n"
    );

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
    let original = "package a{struct A{name:Str}}\n";
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
fn fmt_preserves_comments() {
    let dir = temp_dir("comments");
    let file = dir.join("a.lt");
    let original = "// keep me\npackage a{struct A{name:Str}}\n";
    std::fs::write(&file, original).unwrap();

    let output = Command::new(bin()).arg("fmt").arg(&file).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let formatted = std::fs::read_to_string(&file).unwrap();
    assert!(formatted.starts_with("// keep me\n"), "{formatted}");
    assert!(formatted.contains("package a {"), "{formatted}");

    // Idempotent with comments.
    let output = Command::new(bin())
        .arg("fmt")
        .arg("--check")
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn fmt_rejects_inline_comments() {
    let dir = temp_dir("inline-comment");
    let file = dir.join("a.lt");
    let original = "package a { struct A { x: Int } } // trailing\n";
    std::fs::write(&file, original).unwrap();

    let output = Command::new(bin()).arg("fmt").arg(&file).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("inline comments"), "{stderr}");
}

#[test]
fn check_reports_diagnostics() {
    let dir = temp_dir("broken");
    let file = dir.join("bad.lt");
    std::fs::write(&file, "package a {\n  struct A { name Str }\n}\n").unwrap();

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
    std::fs::write(
        dir.join("root.lt"),
        "package root {\n  struct A { x: Int }\n}\n",
    )
    .unwrap();
    std::fs::write(
        nested.join("user.lt"),
        "package user {\n  struct User { name: Str }\n}\n",
    )
    .unwrap();

    let output = Command::new(bin()).arg("check").arg(&dir).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), 2, "{stdout}");
}

#[test]
fn ast_dumps_without_error() {
    let dir = temp_dir("ast");
    let file = dir.join("a.lt");
    std::fs::write(&file, "package a {\n  struct A { x: Int }\n}\n").unwrap();

    let output = Command::new(bin()).arg("ast").arg(&file).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PackageDecl"), "{stdout}");
    assert!(stdout.contains("StructDecl"), "{stdout}");
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

#[test]
fn build_emits_a_typescript_package() {
    let dir = temp_dir("build");
    let entry = dir.join("user.lt");
    std::fs::write(
        &entry,
        "package user {\n  export struct User {\n    name: Str\n  }\n\n  export fn greet(u: User) -> Str {\n    `hi ${u.name}`\n  }\n}\n",
    )
    .unwrap();

    let out = dir.join("out");
    let output = Command::new(bin())
        .arg("build")
        .arg("-t")
        .arg("ts")
        .arg(&entry)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let file = out.join("ts/src/user.ts");
    let contents = std::fs::read_to_string(&file).unwrap();
    assert!(contents.contains("export class User"), "{contents}");
    assert!(contents.contains("import * as lt from \"../_lingua_rt\";"));
    assert!(out.join("ts/_lingua_rt.ts").is_file());
    assert!(out.join("ts/package.json").is_file());
    assert!(out.join("ts/tsconfig.json").is_file());

    // Rebuilding produces byte-identical output.
    let first = contents;
    let output = Command::new(bin())
        .arg("build")
        .arg("-t")
        .arg("ts")
        .arg(&entry)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let second = std::fs::read_to_string(file).unwrap();
    assert_eq!(first, second);
}

#[test]
fn build_refuses_type_errors() {
    let dir = temp_dir("build-bad");
    let entry = dir.join("bad.lt");
    std::fs::write(
        &entry,
        "package bad {\n  export struct A { x: Missing }\n}\n",
    )
    .unwrap();

    let output = Command::new(bin())
        .arg("build")
        .arg("-t")
        .arg("ts")
        .arg(&entry)
        .arg("--out")
        .arg(dir.join("out"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown type"), "{stderr}");
    assert!(!dir.join("out/ts/src/bad.ts").exists());
}

#[test]
fn ir_command_dumps_lowering() {
    let dir = temp_dir("ir");
    let entry = dir.join("a.lt");
    std::fs::write(&entry, "package a {\n  struct A { x: Int }\n}\n").unwrap();

    let output = Command::new(bin()).arg("ir").arg(&entry).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("IrProgram"), "{stdout}");
    assert!(stdout.contains("IrStruct"), "{stdout}");
}

#[test]
fn test_command_emits_vitest_files() {
    let dir = temp_dir("test-emit");
    let entry = dir.join("suite.lt");
    std::fs::write(
        &entry,
        r#"package suite {

export fn add(a: Int, b: Int) -> Int { a + b }

test "adds" {
  expect.eq(add(1, 2), 3)
}

}
"#,
    )
    .unwrap();

    let out = dir.join("out");
    let output = Command::new(bin())
        .arg("test")
        .arg("-t")
        .arg("ts")
        .arg(&entry)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let test_file = out.join("ts/tests/suite.test.ts");
    let contents = std::fs::read_to_string(&test_file).unwrap();
    assert!(
        contents.contains("import { describe, it } from \"vitest\";"),
        "{contents}"
    );
    assert!(contents.contains("it(\"adds\""), "{contents}");

    let package = std::fs::read_to_string(out.join("ts/package.json")).unwrap();
    assert!(package.contains("\"vitest\""), "{package}");
    assert!(out.join("ts/src/suite.ts").is_file());
}

#[test]
fn build_does_not_emit_tests() {
    let dir = temp_dir("build-no-tests");
    let entry = dir.join("suite.lt");
    std::fs::write(
        &entry,
        "package suite {\n  export struct A { x: Int }\n\n  test \"ok\" {\n    expect.eq(1, 1)\n  }\n}\n",
    )
    .unwrap();

    let out = dir.join("out");
    let output = Command::new(bin())
        .arg("build")
        .arg("-t")
        .arg("ts")
        .arg(&entry)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(!out.join("ts/tests/suite.test.ts").exists());
}

#[test]
fn build_python_package() {
    let dir = temp_dir("build-py");
    let entry = dir.join("user.lt");
    std::fs::write(
        &entry,
        "package user {\n  export struct User {\n    name: Str\n  }\n\n  export fn greet(u: User) -> Str {\n    `hi ${u.name}`\n  }\n}\n",
    )
    .unwrap();

    let out = dir.join("out");
    let output = Command::new(bin())
        .arg("build")
        .arg("-t")
        .arg("py")
        .arg(&entry)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(out.join("py/lt_runtime.py").is_file());
    assert!(out.join("py/gen/__init__.py").is_file());
    let module = std::fs::read_to_string(out.join("py/gen/user.py")).unwrap();
    assert!(module.contains("class User:"), "{module}");
    assert!(module.contains("import lt_runtime as _lt"), "{module}");
    assert!(!out.join("py/tests/test_user.py").exists());
}

#[test]
fn test_command_emits_pytest_files_for_python() {
    let dir = temp_dir("test-emit-py");
    let entry = dir.join("suite.lt");
    std::fs::write(
        &entry,
        "package suite {\n  export fn add(a: Int, b: Int) -> Int { a + b }\n\n  test \"adds\" {\n    expect.eq(add(1, 2), 3)\n  }\n}\n",
    )
    .unwrap();

    let out = dir.join("out");
    let output = Command::new(bin())
        .arg("test")
        .arg("-t")
        .arg("py")
        .arg(&entry)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));

    let tests = std::fs::read_to_string(out.join("py/tests/test_suite.py")).unwrap();
    assert!(tests.contains("def test_adds() -> None:"), "{tests}");
    assert!(tests.contains("import gen.suite as m0"), "{tests}");
}

#[test]
fn build_go_package() {
    let dir = temp_dir("build-go");
    let entry = dir.join("user.lt");
    std::fs::write(
        &entry,
        "package user {\n  export struct User {\n    name: Str\n  }\n\n  export fn greet(u: User) -> Str {\n    `hi ${u.name}`\n  }\n}\n",
    )
    .unwrap();

    let out = dir.join("out");
    let output = Command::new(bin())
        .arg("build")
        .arg("-t")
        .arg("go")
        .arg(&entry)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(out.join("go/go.mod").is_file());
    assert!(out.join("go/lt/runtime.go").is_file());
    let module = std::fs::read_to_string(out.join("go/gen/user/user.go")).unwrap();
    assert!(module.contains("package user"), "{module}");
    assert!(
        module.contains("func NewUser(name string) User"),
        "{module}"
    );
    assert!(module.contains("lt \"linguatungga/lt\""), "{module}");
    assert!(!out.join("go/gen/user/user_test.go").exists());
}

#[test]
fn test_command_emits_go_test_files() {
    let dir = temp_dir("test-emit-go");
    let entry = dir.join("suite.lt");
    std::fs::write(
        &entry,
        "package suite {\n  export fn add(a: Int, b: Int) -> Int { a + b }\n\n  test \"adds\" {\n    expect.eq(add(1, 2), 3)\n  }\n}\n",
    )
    .unwrap();

    let out = dir.join("out");
    let output = Command::new(bin())
        .arg("test")
        .arg("-t")
        .arg("go")
        .arg(&entry)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));

    let tests = std::fs::read_to_string(out.join("go/gen/suite/suite_test.go")).unwrap();
    assert!(tests.contains("package suite"), "{tests}");
    assert!(tests.contains("func TestAdds(t *testing.T)"), "{tests}");
}

#[test]
fn local_projects_do_not_get_a_lockfile() {
    let dir = temp_dir("no-lock");
    let entry = dir.join("a.lt");
    std::fs::write(&entry, "package a {\n  struct A { x: Int }\n}\n").unwrap();

    let output = Command::new(bin())
        .arg("build")
        .arg("-t")
        .arg("ts")
        .arg(&entry)
        .arg("--out")
        .arg(dir.join("out"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(
        !dir.join("lt.lock").exists(),
        "no remote imports means no lockfile"
    );
}

#[test]
fn vendor_without_remotes_is_a_noop() {
    let dir = temp_dir("vendor-empty");
    let entry = dir.join("a.lt");
    std::fs::write(&entry, "package a {\n  struct A { x: Int }\n}\n").unwrap();

    let output = Command::new(bin())
        .arg("vendor")
        .arg(&entry)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("vendored 0 package(s)"), "{stdout}");
}
