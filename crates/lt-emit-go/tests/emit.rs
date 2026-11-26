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
            module
                .path
                .strip_prefix(&root)
                .unwrap()
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/")
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
pub enum Role {
  Admin
  Member(Int)
}

pub class User {
  name: Str
  age: Int
  role: Role

  pub fn greet(self) -> Str {
    `Hi ${self.name}`
  }

  pub fn older(self) -> Int {
    self.age + 1
  }

  pub fn label(self) -> Str {
    match self.role {
      Role.Admin => "admin"
      Role.Member(level) => `member-${level}`
    }
  }

  static fn anonymous() -> User {
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
    assert!(module.contains("func NewUser(name string, age int64, role lt.Variant) User"), "{module}");
    assert!(module.contains("func (self User) Greet() string"), "{module}");
    assert!(module.contains("\"Hi \" + lt.Disp(self.Name)"), "{module}");
    assert!(module.contains("func UserAnonymous() User"), "{module}");
    assert!(module.contains("func (self User) LtEq(other any) bool"), "{module}");
    assert!(module.contains("var RoleAdmin = lt.Variant{Tag: \"Admin\""), "{module}");
    assert!(module.contains("func RoleMember(_0 int64) lt.Variant"), "{module}");
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
    assert!(validator.contains("func ValidatorCheck(u m1.User) lt.Variant"), "{validator}");
}

const VALIDATOR: &str = r#"
import User from "./user.lt"

pub class Validator {
  pub fn check(u: User) -> Result<User, Str> {
    if u.age < 0 {
      return Err("age must be >= 0")
    }
    Ok(u)
  }

  pub fn describe(u: User) -> Str {
    match Validator.check(u) {
      Ok(valid) => valid.greet()
      Err(message) => `invalid: ${message}`
    }
  }

  pub fn all(users: List<User>) -> List<Result<User, Str>> {
    users.map(fn(u) => Validator.check(u))
  }
}
"#;

#[test]
fn option_and_result_use_variants() {
    let src = r#"
pub class A {
  fn go(value: Option<Int>) -> Option<Int> {
    let inner = value?
    Some(inner + 1)
  }

  fn parse(text: Str) -> Result<Int, Str> {
    match text.parse_int() {
      Some(value) => Ok(value)
      None => Err("bad")
    }
  }

  fn run(text: Str) -> Result<Int, Str> {
    let value = A.parse(text)?
    Ok(value)
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
pub class A {
  static fn total(xs: List<Int>) -> Int {
    xs.fold(0, fn(acc, x) => acc + x)
  }

  static fn doubled(xs: List<Int>) -> List<Int> {
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
pub class Adder {
  static fn add(a: Int, b: Int) -> Int { a + b }
}

test class AdderTest {
  test fn adds() {
    Expect.equal(Adder.add(1, 2), 3)
  }
}
"#;
    let files = emit(&[("suite.lt", src)], "suite.lt", true);
    let tests = find(&files, "gen/suite/suite_test.go");
    assert!(tests.contains("package suite_test"), "{tests}");
    assert!(tests.contains("func TestAdds(t *testing.T)"), "{tests}");
    assert!(tests.contains("lt.ExpectEqual(m0.AdderAdd(int64(1), int64(2)), int64(3))"), "{tests}");

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
