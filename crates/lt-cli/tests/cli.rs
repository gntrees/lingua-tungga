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
fn fmt_preserves_comments() {
    let dir = temp_dir("comments");
    let file = dir.join("a.lt");
    let original = "// keep me\npub class A{name:Str}\n";
    std::fs::write(&file, original).unwrap();

    let output = Command::new(bin()).arg("fmt").arg(&file).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let formatted = std::fs::read_to_string(&file).unwrap();
    assert!(formatted.starts_with("// keep me\n"), "{formatted}");
    assert!(formatted.contains("pub class A {"), "{formatted}");

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
    let original = "pub class A { x: Int } // trailing\n";
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

#[test]
fn build_emits_a_typescript_package() {
    let dir = temp_dir("build");
    let entry = dir.join("user.lt");
    std::fs::write(&entry, "pub class User {\n  name: Str\n  pub fn greet(self) -> Str { `hi ${self.name}` }\n}\n").unwrap();

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
    std::fs::write(&entry, "pub class A { x: Missing }\n").unwrap();

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
    std::fs::write(&entry, "pub class A { x: Int }\n").unwrap();

    let output = Command::new(bin()).arg("ir").arg(&entry).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("IrProgram"), "{stdout}");
    assert!(stdout.contains("IrClass"), "{stdout}");
}

#[test]
fn test_command_emits_vitest_files() {
    let dir = temp_dir("test-emit");
    let entry = dir.join("suite.lt");
    std::fs::write(
        &entry,
        r#"
pub class Adder {
  static fn add(a: Int, b: Int) -> Int { a + b }
}

test class AdderTest {
  test fn adds() {
    Expect.equal(Adder.add(1, 2), 3)
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
    assert!(contents.contains("import { describe, it } from \"vitest\";"), "{contents}");
    assert!(contents.contains("describe(\"AdderTest\""), "{contents}");
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
        "pub class A { x: Int }\n\ntest class T {\n  test fn ok() {\n    Expect.equal(1, 1)\n  }\n}\n",
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
        "pub class User {\n  name: Str\n  pub fn greet(self) -> Str { `hi ${self.name}` }\n}\n",
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
    assert!(module.contains("import lt_runtime as lt"), "{module}");
    assert!(!out.join("py/tests/test_user.py").exists());
}

#[test]
fn test_command_emits_pytest_files_for_python() {
    let dir = temp_dir("test-emit-py");
    let entry = dir.join("suite.lt");
    std::fs::write(
        &entry,
        "pub class Adder {\n  static fn add(a: Int, b: Int) -> Int { a + b }\n}\n\ntest class AdderTest {\n  test fn adds() {\n    Expect.equal(Adder.add(1, 2), 3)\n  }\n}\n",
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
        "pub class User {\n  name: Str\n  pub fn greet(self) -> Str { `hi ${self.name}` }\n}\n",
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
    assert!(module.contains("func NewUser(name string) User"), "{module}");
    assert!(module.contains("lt \"linguatungga/lt\""), "{module}");
    assert!(!out.join("go/gen/user/user_test.go").exists());
}

#[test]
fn test_command_emits_go_test_files() {
    let dir = temp_dir("test-emit-go");
    let entry = dir.join("suite.lt");
    std::fs::write(
        &entry,
        "pub class Adder {\n  static fn add(a: Int, b: Int) -> Int { a + b }\n}\n\ntest class AdderTest {\n  test fn adds() {\n    Expect.equal(Adder.add(1, 2), 3)\n  }\n}\n",
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
    assert!(tests.contains("package suite_test"), "{tests}");
    assert!(tests.contains("func TestAdds(t *testing.T)"), "{tests}");
}

#[test]
fn local_projects_do_not_get_a_lockfile() {
    let dir = temp_dir("no-lock");
    let entry = dir.join("a.lt");
    std::fs::write(&entry, "pub class A { x: Int }\n").unwrap();

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
    assert!(!dir.join("lt.lock").exists(), "no remote imports means no lockfile");
}

#[test]
fn vendor_without_remotes_is_a_noop() {
    let dir = temp_dir("vendor-empty");
    let entry = dir.join("a.lt");
    std::fs::write(&entry, "pub class A { x: Int }\n").unwrap();

    let output = Command::new(bin())
        .arg("vendor")
        .arg(&entry)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("vendored 0 package(s)"), "{stdout}");
}
