use std::path::PathBuf;

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

    let names = checked
        .program
        .modules
        .iter()
        .map(|module| module.name.clone())
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
  `Hi ${u.name}, you are ${u.age}`
}

export fn older(u: User) -> Int {
  u.age + 1
}

export fn label(u: User) -> Str {
  match u.role {
    Role.Admin => "admin"
    Role.Member(n) => `member-${n}`
    Role.Guest => "guest"
  }
}

export fn anonymous() -> User {
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
    assert!(user.contains("export function greet(u: User): string"));
    assert!(user.contains("lt.iadd(u.age, 1n)"), "{user}");
    assert!(user.contains("export function anonymous(): User"));
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
    assert!(validator.contains("export function check(u: m1.User)"));
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
    assert!(validator.contains("return m1.greet(valid);"), "{validator}");
    assert!(validator.contains("users.map((u: m1.User) => check(u))"));
    assert!(validator.contains("default:"));
}

#[test]
fn try_operator_returns_early() {
    let src = r#"
package main {

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
package main {

fn total(xs: List<Int>) -> Int {
  xs.fold(0, fn(acc, x) => acc + x)
}

fn names(xs: List<Str>) -> Str {
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

#[test]
fn to_string_uses_the_display_runtime() {
    let src = r#"
package a {

fn f(x: Int) -> Str {
  x.to_string()
}

}
"#;
    let files = emit(&[("a.lt", src)], "a.lt");
    let module = find(&files, "src/a.ts");
    assert!(module.contains("return lt.disp(x);"), "{module}");
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
fn generic_enum_nullary_variant_is_not_generic() {
    let files = emit(&[("a.lt", GENERIC_ENUM)], "a.lt");
    let module = find(&files, "src/a.ts");
    assert!(
        module.contains("export const MaybeBox_Empty: MaybeBox<any>"),
        "{module}"
    );
    assert!(!module.contains("MaybeBox_Empty<T>"), "{module}");
    assert!(
        module.contains("export function MaybeBox_Value<T>(_0: T): MaybeBox<T>"),
        "{module}"
    );
}
