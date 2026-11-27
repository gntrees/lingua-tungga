use std::path::{Path, PathBuf};

use lt_emit_ts::{emit_package, EmittedFile};
use lt_hir::{check_entries, MapLoader};
use lt_ir::lower_program;

fn emit(files: &[(&str, &str)], entry: &str) -> Vec<EmittedFile> {
    let mut loader = MapLoader::new();
    for (path, source) in files {
        loader.add(path, source);
    }
    let checked = check_entries(&[PathBuf::from(entry)], &mut loader);
    assert!(checked.is_ok(), "type errors: {:#?}", checked.diagnostics);

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
    assert!(diagnostics.is_empty(), "lowering errors: {diagnostics:#?}");
    emit_package(&ir)
}

fn find<'a>(files: &'a [EmittedFile], path: &str) -> &'a str {
    files
        .iter()
        .find(|file| file.path == std::path::Path::new(path))
        .unwrap_or_else(|| panic!("missing emitted file `{path}`"))
        .contents
        .as_str()
}

const USER: &str = r#"
pub enum Role {
  Admin
  Member(Int)
  Guest
}

pub class User {
  name: Str
  age: Int
  role: Role

  pub fn greet(self) -> Str {
    `Hi ${self.name}, you are ${self.age}`
  }

  pub fn older(self) -> Int {
    self.age + 1
  }

  pub fn label(self) -> Str {
    match self.role {
      Role.Admin => "admin"
      Role.Member(n) => `member-${n}`
      Role.Guest => "guest"
    }
  }

  static fn anonymous() -> User {
    User(name: "?", age: 0, role: Role.Guest)
  }
}
"#;

#[test]
fn emits_a_typed_package() {
    let files = emit(&[("user.lt", USER)], "user.lt");
    assert!(files
        .iter()
        .any(|f| f.path == std::path::Path::new("package.json")));
    assert!(files
        .iter()
        .any(|f| f.path == std::path::Path::new("tsconfig.json")));
    assert!(files
        .iter()
        .any(|f| f.path == std::path::Path::new("src/user.ts")));

    let runtime = find(&files, "_lingua_rt.ts");
    assert!(runtime.contains("export function disp"));
    assert!(runtime.contains("export class LList"));
    assert!(runtime.contains("BigInt.asIntN(64"));

    let user = find(&files, "src/user.ts");
    assert!(user.contains("export type Role ="));
    assert!(user.contains("export const Role_Admin"));
    assert!(user.contains("export function Role_Member(_0: bigint): Role"));
    assert!(user.contains("export class User"));
    assert!(user.contains("readonly age: bigint;"));
    assert!(user.contains("greet(): string"));
    assert!(user.contains("lt.iadd(this.age, 1n)"), "{user}");
    assert!(user.contains("static anonymous(): User"));
    assert!(user.contains("new User({ name: \"?\", age: 0n, role: Role_Guest })"));
    assert!(user.contains("__lt_eq(other: unknown): boolean"));
    assert!(user.contains("switch (__match1.tag)"));
    assert!(user.contains("import * as lt from \"../_lingua_rt\";"));
}

#[test]
fn imports_between_modules_are_relative() {
    let files = emit(
        &[("user.lt", USER), ("validator.lt", VALIDATOR)],
        "validator.lt",
    );
    let validator = find(&files, "src/validator.ts");
    assert!(
        validator.contains("import * as m1 from \"./user\";"),
        "{validator}"
    );
    assert!(validator.contains("static check(u: m1.User)"));
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
fn option_and_result_use_runtime_tags() {
    let files = emit(
        &[("user.lt", USER), ("validator.lt", VALIDATOR)],
        "validator.lt",
    );
    let validator = find(&files, "src/validator.ts");
    assert!(
        validator.contains("return lt.Err(\"age must be >= 0\");"),
        "{validator}"
    );
    assert!(validator.contains("case \"ok\": {"), "{validator}");
    assert!(validator.contains("case \"err\": {"), "{validator}");
    assert!(validator.contains("return valid.greet();"), "{validator}");
    assert!(validator.contains("users.map((u: m1.User) => Validator.check(u))"));
    assert!(validator.contains("default:"));
}

#[test]
fn try_operator_returns_early() {
    let src = r#"
pub class Inner {
  fn run() -> Result<Int, Str> { Ok(1) }
}

pub class Outer {
  fn go(x: Option<Int>) -> Option<Int> {
    let value = x?
    Some(value + 1)
  }
}
"#;
    let files = emit(&[("main.lt", src)], "main.lt");
    let main = find(&files, "src/main.ts");
    assert!(
        main.contains("if (__try1.tag === \"none\") return __try1;"),
        "{main}"
    );
    assert!(main.contains("__try1.values[0]"), "{main}");
}

#[test]
fn errors_and_lists_render() {
    let src = r#"
pub class A {
  static fn total(xs: List<Int>) -> Int {
    xs.fold(0, fn(acc, x) => acc + x)
  }

  static fn names(xs: List<Str>) -> Str {
    xs.join(", ")
  }
}
"#;
    let files = emit(&[("main.lt", src)], "main.lt");
    let main = find(&files, "src/main.ts");
    assert!(
        main.contains("xs.fold(0n, (acc: bigint, x: bigint) => lt.iadd(acc, x))"),
        "{main}"
    );
    assert!(main.contains("xs.join(\", \")"), "{main}");
}

#[test]
fn emission_is_deterministic() {
    let first = emit(&[("user.lt", USER)], "user.lt");
    let second = emit(&[("user.lt", USER)], "user.lt");
    assert_eq!(
        first.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
        second.iter().map(|f| f.path.clone()).collect::<Vec<_>>()
    );
    for (left, right) in first.iter().zip(second.iter()) {
        assert_eq!(left.contents, right.contents);
    }
}
