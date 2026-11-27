use std::path::{Path, PathBuf};

use lt_emit_go::{emit_package, emit_package_with_tests, EmittedFile};
use lt_hir::{check_entries, MapLoader};
use lt_ir::lower_program;

fn emit(files: &[(&str, &str)], entry: &str, with_tests: bool) -> Vec<EmittedFile> {
    let mut loader = MapLoader::new();
    for (path, source) in files {
        loader.add(path, source);
    }
    let checked = check_entries(&[PathBuf::from(entry)], &mut loader);
    assert!(checked.is_ok(), "{:#?}", checked.diagnostics);

    let root = Path::new(entry)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let names = checked
        .program
        .modules
        .iter()
        .map(|module| {
            let base = module.path.strip_prefix(&root).unwrap();
            match base.parent().filter(|dir| !dir.as_os_str().is_empty()) {
                Some(dir) => format!("{}/{}", dir.to_string_lossy(), module.name),
                None => module.name.clone(),
            }
        })
        .collect();

    let (ir, diagnostics) = lower_program(&checked.program, &checked.types, names);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    if with_tests {
        emit_package_with_tests(&ir)
    } else {
        emit_package(&ir)
    }
}

fn find<'a>(files: &'a [EmittedFile], path: &str) -> &'a str {
    files
        .iter()
        .find(|file| file.path == Path::new(path))
        .unwrap_or_else(|| panic!("missing emitted file `{path}`"))
        .contents
        .as_str()
}

const USER: &str = r#"
package user {

export enum Role {
  Admin
  Member(Int)
  Guest
}

export struct User {
  name: Str
  age: Int
  role: Role
}

export fn greet(u: User) -> Str {
  `Hi ${u.name}`
}

export fn older(u: User) -> Int {
  u.age + 1
}

export fn label(u: User) -> Str {
  match u.role {
    Role.Admin => "admin"
    Role.Member(level) => `member-${level}`
  }
}

export fn anonymous() -> User {
  User(name: "?", age: 0, role: Role.Admin)
}

}
"#;

#[test]
fn emits_a_go_package() {
    let files = emit(&[("user.lt", USER)], "user.lt", false);
    assert!(files.iter().any(|f| f.path == Path::new("go.mod")));
    assert!(files.iter().any(|f| f.path == Path::new("lt/runtime.go")));

    let module = find(&files, "gen/user/user.go");
    assert!(module.contains("package user"), "{module}");
    assert!(module.contains("type User struct {"), "{module}");
    assert!(module.contains("\tName string"), "{module}");
    assert!(module.contains("\tRole lt.Variant"), "{module}");
    assert!(
        module.contains("func NewUser(name string, age int64, role lt.Variant) User"),
        "{module}"
    );
    assert!(module.contains("func Greet(u User) string"), "{module}");
    assert!(module.contains("\"Hi \" + lt.Disp(u.Name)"), "{module}");
    assert!(module.contains("func Anonymous() User"), "{module}");
    assert!(
        module.contains("func (self User) LtEq(other any) bool"),
        "{module}"
    );
    assert!(
        module.contains("var RoleAdmin = lt.Variant{Tag: \"Admin\""),
        "{module}"
    );
    assert!(
        module.contains("func RoleMember(_0 int64) lt.Variant"),
        "{module}"
    );
    assert!(module.contains("switch __match1.Tag {"), "{module}");
}

#[test]
fn imports_between_modules_use_go_import_paths() {
    let files = emit(
        &[("user.lt", USER), ("validator.lt", VALIDATOR)],
        "validator.lt",
        false,
    );
    let validator = find(&files, "gen/validator/validator.go");
    assert!(
        validator.contains("m1 \"linguatungga/gen/user\""),
        "{validator}"
    );
    assert!(
        validator.contains("func Check(u m1.User) lt.Variant"),
        "{validator}"
    );
}

const VALIDATOR: &str = r#"
import user { User } from "./user.lt"

package validator {

export fn check(u: User) -> Result<User, Str> {
  if u.age < 0 {
    return Err("age must be >= 0")
  }
  Ok(u)
}

export fn describe(u: User) -> Str {
  match check(u) {
    Ok(valid) => valid.greet()
    Err(message) => `invalid: ${message}`
  }
}

export fn all(users: List<User>) -> List<Result<User, Str>> {
  users.map(fn(u) => check(u))
}

}
"#;

#[test]
fn option_and_result_use_variants() {
    let src = r#"
package main {

fn go(value: Option<Int>) -> Option<Int> {
  let inner = value?
  Some(inner + 1)
}

fn parse(text: Str) -> Result<Int, Str> {
  match text.parse_int() {
    Some(value) => Ok(value)
    None => Err("bad number")
  }
}

fn run() -> Result<Int, Str> {
  let number = parse("42")?
  Ok(number)
}

}
"#;
    let files = emit(&[("main.lt", src)], "main.lt", false);
    let main = find(&files, "gen/main/main.go");
    assert!(main.contains("if __try1.Tag == \"none\" {"), "{main}");
    assert!(main.contains(".Tag == \"err\" {"), "{main}");
}

#[test]
fn lambdas_carry_parameter_and_result_types() {
    let src = r#"
package main {

fn total(xs: List<Int>) -> Int {
  xs.fold(0, fn(acc, x) => acc + x)
}

fn doubled(xs: List<Int>) -> List<Int> {
  xs.map(fn(x) => x * 2)
}

}
"#;
    let files = emit(&[("main.lt", src)], "main.lt", false);
    let main = find(&files, "gen/main/main.go");
    assert!(main.contains("func(acc int64, x int64) int64"), "{main}");
    assert!(main.contains("func(x int64) int64"), "{main}");
    assert!(main.contains("lt.FoldList[int64, int64]("), "{main}");
    assert!(main.contains("lt.MapList[int64, int64]("), "{main}");
}

#[test]
fn tests_become_go_test_functions() {
    let src = r#"
package suite {

export fn add(a: Int, b: Int) -> Int { a + b }

test "adds" {
  expect.eq(add(1, 2), 3)
}

}
"#;
    let files = emit(&[("suite.lt", src)], "suite.lt", true);
    let tests = find(&files, "gen/suite/suite_test.go");
    assert!(tests.contains("package suite"), "{tests}");
    assert!(tests.contains("func TestAdds(t *testing.T)"), "{tests}");
    assert!(
        tests.contains("lt.ExpectEqual(Add(int64(1), int64(2)), int64(3))"),
        "{tests}"
    );

    let library = emit(&[("suite.lt", src)], "suite.lt", false);
    assert!(!library.iter().any(|f| f.path.ends_with("suite_test.go")));
}

#[test]
fn emission_is_deterministic() {
    let first = emit(&[("user.lt", USER)], "user.lt", true);
    let second = emit(&[("user.lt", USER)], "user.lt", true);
    assert_eq!(
        first
            .iter()
            .map(|f| (f.path.clone(), f.contents.clone()))
            .collect::<Vec<_>>(),
        second
            .iter()
            .map(|f| (f.path.clone(), f.contents.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn to_string_uses_the_display_runtime() {
    let src = r#"
package a {

fn f(x: Int) -> Str {
  x.to_string()
}

}
"#;
    let files = emit(&[("a.lt", src)], "a.lt", false);
    let module = find(&files, "gen/a/a.go");
    assert!(module.contains("return lt.Disp(x)"), "{module}");
}

const GENERIC_ENUM: &str = r#"
package a {

export enum MaybeBox<T> {
  Empty
  Value(T)
}

fn make() -> MaybeBox<Int> {
  MaybeBox.Value(7)
}

fn unwrap(value: MaybeBox<Int>) -> Int {
  match value {
    MaybeBox.Empty => 0
    MaybeBox.Value(n) => n
  }
}

}
"#;

#[test]
fn generic_enum_constructors_declare_type_parameters() {
    let files = emit(&[("a.lt", GENERIC_ENUM)], "a.lt", false);
    let module = find(&files, "gen/a/a.go");
    assert!(
        module.contains("func MaybeBoxValue[T any](_0 T) lt.Variant"),
        "{module}"
    );
    assert!(
        module.contains("MaybeBoxValue[int64](int64(7))"),
        "{module}"
    );
    assert!(
        module.contains("var MaybeBoxEmpty = lt.Variant{Tag: \"Empty\""),
        "{module}"
    );
}
