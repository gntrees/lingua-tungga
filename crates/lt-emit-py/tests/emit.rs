use std::path::{Path, PathBuf};

use lt_emit_py::{emit_package, emit_package_with_tests, EmittedFile};
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
fn emits_a_python_package() {
    let files = emit(&[("user.lt", USER)], "user.lt", false);
    assert!(files
        .iter()
        .any(|file| file.path == Path::new("lt_runtime.py")));
    assert!(files
        .iter()
        .any(|file| file.path == Path::new("pyproject.toml")));
    assert!(files
        .iter()
        .any(|file| file.path == Path::new("gen/__init__.py")));

    let module = find(&files, "gen/user.py");
    assert!(module.contains("from __future__ import annotations"));
    assert!(module.contains("import lt_runtime as lt"));
    assert!(module.contains("from dataclasses import dataclass"));
    assert!(module.contains("@dataclass(frozen=True)\nclass Role:"));
    assert!(module.contains("role_admin = Role(\"Admin\", (), 0)"));
    assert!(module.contains("def role_member(_0: int) -> Role:"));
    assert!(module.contains("@dataclass(frozen=True)\nclass User:"));
    assert!(module.contains("    name: str\n"));
    assert!(module.contains("    def greet(self) -> str:"));
    assert!(module.contains("lt.iadd(self.age, 1)"));
    assert!(module.contains("@staticmethod"));
    assert!(module.contains("def to_display(self) -> str:"));
    assert!(module.contains("if __match1.tag == \"Member\":"));
}

#[test]
fn imports_between_modules_are_absolute() {
    let files = emit(
        &[("user.lt", USER), ("validator.lt", VALIDATOR)],
        "validator.lt",
        false,
    );
    let validator = find(&files, "gen/validator.py");
    assert!(validator.contains("import gen.user as m1"), "{validator}");
    assert!(validator.contains("def check(u: m1.User) -> lt.Res"), "{validator}");
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
fn try_option_and_result_use_tags() {
    let src = r#"
pub class A {
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
    let number = A.parse("42")?
    Ok(number)
  }
}
"#;
    let files = emit(&[("main.lt", src)], "main.lt", false);
    let main = find(&files, "gen/main.py");
    assert!(main.contains("if __try1.tag == \"none\":"), "{main}");
    assert!(main.contains("return __try1"), "{main}");
    assert!(main.contains(".tag == \"err\":"), "{main}");
    assert!(main.contains(".values[0]"), "{main}");
}

#[test]
fn tests_are_emitted_to_pytest_files() {
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
    let tests = find(&files, "tests/test_suite.py");
    assert!(tests.contains("import lt_runtime as lt"), "{tests}");
    assert!(tests.contains("import gen.suite as m0"), "{tests}");
    assert!(tests.contains("def test_adds() -> None:"), "{tests}");
    assert!(tests.contains("lt.expect_equal(m0.Adder.add(1, 2), 3)"), "{tests}");

    // `emit_package` alone never emits tests.
    let library = emit(&[("suite.lt", src)], "suite.lt", false);
    assert!(!library
        .iter()
        .any(|file| file.path.starts_with("tests/")));
}

#[test]
fn nested_modules_get_init_files() {
    let files = emit(
        &[
            ("models/user.lt", USER),
            ("app.lt", "import User from \"./models/user.lt\"\n\npub class App {\n  user: User\n}\n"),
        ],
        "app.lt",
        false,
    );
    for path in [
        "gen/__init__.py",
        "gen/models/__init__.py",
        "gen/models/user.py",
        "gen/app.py",
    ] {
        assert!(
            files.iter().any(|file| file.path == Path::new(path)),
            "missing {path}"
        );
    }
    let app = find(&files, "gen/app.py");
    assert!(app.contains("import gen.models.user as m1"), "{app}");
}

#[test]
fn emission_is_deterministic() {
    let first = emit(&[("user.lt", USER)], "user.lt", true);
    let second = emit(&[("user.lt", USER)], "user.lt", true);
    assert_eq!(
        first.iter().map(|f| (f.path.clone(), f.contents.clone())).collect::<Vec<_>>(),
        second.iter().map(|f| (f.path.clone(), f.contents.clone())).collect::<Vec<_>>()
    );
}
