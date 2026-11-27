use std::path::PathBuf;

use lt_hir::{check_entries, MapLoader};

fn run(files: &[(&str, &str)], entry: &str) -> Vec<String> {
    let mut loader = MapLoader::new();
    for (path, source) in files {
        loader.add(path, source);
    }
    let out = check_entries(&[PathBuf::from(entry)], &mut loader);
    out.diagnostics
        .iter()
        .map(|file| file.diagnostic.message.clone())
        .collect()
}

fn assert_ok(files: &[(&str, &str)], entry: &str) {
    let messages = run(files, entry);
    assert!(messages.is_empty(), "unexpected diagnostics: {messages:#?}");
}

fn assert_has(files: &[(&str, &str)], entry: &str, needle: &str) {
    let messages = run(files, entry);
    assert!(
        messages.iter().any(|message| message.contains(needle)),
        "expected a diagnostic containing `{needle}`, got {messages:#?}"
    );
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
    `Hi ${self.name}`
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
fn user_example_typechecks() {
    assert_ok(&[("user.lt", USER)], "user.lt");
}

#[test]
fn imported_class_typechecks() {
    let validator = r#"
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
    assert_ok(
        &[("user.lt", USER), ("validator.lt", validator)],
        "validator.lt",
    );
}

#[test]
fn import_cycle_is_reported() {
    let a = "import B from \"./b.lt\"\npub class A { b: Str }\n";
    let b = "import A from \"./a.lt\"\npub class B { a: Str }\n";
    assert_has(&[("a.lt", a), ("b.lt", b)], "a.lt", "import cycle");
}

#[test]
fn remote_import_needs_the_package_manager() {
    let src = "import User from \"https://raw.githubusercontent.com/x/y/v1/src/user.lt\"\npub class A { u: Str }\n";
    assert_has(&[("a.lt", src)], "a.lt", "package manager");
}

#[test]
fn import_paths_must_be_relative() {
    let src = "import User from \"user.lt\"\npub class A { u: Str }\n";
    assert_has(&[("a.lt", src)], "a.lt", "must start with `./`");
}

#[test]
fn private_class_cannot_be_imported() {
    let provider = "class Secret { x: Int }\n";
    let consumer = "import Secret from \"./secret.lt\"\npub class A { s: Secret }\n";
    assert_has(
        &[("secret.lt", provider), ("a.lt", consumer)],
        "a.lt",
        "unknown type `Secret`",
    );
}

#[test]
fn unknown_type_is_reported() {
    assert_has(
        &[("a.lt", "pub class A { x: Missing }\n")],
        "a.lt",
        "unknown type `Missing`",
    );
}

#[test]
fn duplicate_field_is_reported() {
    assert_has(
        &[("a.lt", "pub class A { x: Int\n x: Int }\n")],
        "a.lt",
        "duplicate field `x`",
    );
}

#[test]
fn duplicate_class_is_reported() {
    assert_has(
        &[("a.lt", "pub class A { x: Int }\npub class A { y: Int }\n")],
        "a.lt",
        "declared more than once",
    );
}

#[test]
fn construction_needs_named_arguments() {
    let src = r#"
pub class A {
  x: Int
  static fn make() -> A {
    A(1)
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "named arguments");
}

#[test]
fn missing_and_extra_fields_are_reported() {
    let missing = r#"
pub class A {
  x: Int
  y: Int
  static fn make() -> A {
    A(x: 1)
  }
}
"#;
    assert_has(&[("a.lt", missing)], "a.lt", "missing field `y`");

    let extra = r#"
pub class A {
  x: Int
  static fn make() -> A {
    A(x: 1, z: 2)
  }
}
"#;
    assert_has(&[("a.lt", extra)], "a.lt", "has no field `z`");
}

#[test]
fn wrong_field_type_is_reported() {
    let src = r#"
pub class A {
  x: Int
  static fn make() -> A {
    A(x: "nope")
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "expected `Int`");
}

#[test]
fn match_must_be_exhaustive() {
    let src = r#"
pub enum Color {
  Red
  Green
}

pub class A {
  fn name(c: Color) -> Str {
    match c {
      Color.Red => "red"
    }
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "not exhaustive");
}

#[test]
fn question_mark_needs_matching_return_type() {
    let src = r#"
pub class A {
  fn maybe(x: Option<Int>) -> Result<Int, Str> {
    x?
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "`?` on an `Option`");
}

#[test]
fn question_mark_propagates_result_error_types() {
    let src = r#"
pub class A {
  fn inner() -> Result<Int, Str> { Ok(1) }
  fn outer() -> Result<Int, Str> {
    let value = A.inner()?
    Ok(value + 1)
  }
}
"#;
    assert_ok(&[("a.lt", src)], "a.lt");
}

#[test]
fn jsx_unknown_attribute_is_reported() {
    let src = r#"
pub class Element {
  tag: Str
  children: List<Element>
}

pub class View {
  fn render() -> Element {
    <Element tag="div" class="x" />
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "as JSX attribute");
}

#[test]
fn jsx_children_need_a_children_field() {
    let src = r#"
pub class Element {
  tag: Str
}

pub class View {
  fn render() -> Element {
    <Element tag="div"><Element tag="span" /></Element>
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "declares no `children` field");
}

#[test]
fn jsx_round_trip_typechecks() {
    let src = r#"
pub class Element {
  tag: Str
  class_name: Str
  children: List<Element>
}

pub class View {
  fn render() -> Element {
    <Element tag="div" class_name="user">
      <Element tag="span" class_name="" />
    </Element>
  }
}
"#;
    assert_ok(&[("a.lt", src)], "a.lt");
}

#[test]
fn expect_is_only_for_tests() {
    let src = r#"
pub class A {
  fn f() -> Bool {
    Expect.true(true)
    true
  }
}
"#;
    assert_has(
        &[("a.lt", src)],
        "a.lt",
        "only available inside a `test class`",
    );
}

#[test]
fn expect_inside_test_class_is_fine() {
    let src = r#"
test class T {
  test fn works() {
    Expect.equal(1, 1)
  }
}
"#;
    assert_ok(&[("a.lt", src)], "a.lt");
}

#[test]
fn assigning_to_immutable_binding_is_reported() {
    let src = r#"
pub class A {
  fn f() {
    let x = 1
    x = 2
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "immutable");
}

#[test]
fn mixed_int_and_float_arithmetic_is_reported() {
    let src = r#"
pub class A {
  fn f() -> Int {
    1 + 2.0
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "arithmetic needs");
}

#[test]
fn int_and_float_are_not_interchangeable() {
    let src = r#"
pub class A {
  fn f() -> Float {
    1
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "expected `Float`");
}

#[test]
fn builtin_method_arity_is_checked() {
    let src = r#"
pub class A {
  fn f(xs: List<Int>) -> Int {
    xs.get()
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "expects 1 argument");
}

#[test]
fn higher_order_methods_need_lambdas() {
    let src = r#"
pub class A {
  fn f(xs: List<Int>) -> List<Str> {
    xs.map(1)
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "needs a lambda");
}

#[test]
fn lambda_bodies_are_checked() {
    let src = r#"
pub class A {
  fn f(xs: List<Int>) -> List<Str> {
    xs.map(fn(x) => x + 1)
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "expected `List<Str>`");
}

#[test]
fn generic_class_infers_type_arguments() {
    let src = r#"
pub class Box<T> {
  value: T
  pub fn get(self) -> T { self.value }
}

pub class A {
  static fn make() -> Box<Int> {
    Box(value: 1)
  }

  static fn unwrap(b: Box<Int>) -> Int {
    b.get()
  }
}
"#;
    assert_ok(&[("a.lt", src)], "a.lt");
}

#[test]
fn generic_class_requires_inferable_arguments() {
    let src = r#"
pub class Box<T> {
  value: Int
}

pub class A {
  static fn make() -> Box<Int> {
    Box(value: 1)
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "cannot infer type parameter");
}

#[test]
fn for_requires_a_list() {
    let src = r#"
pub class A {
  fn f() {
    for x in 1 {
      x
    }
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "`for` needs a `List`");
}

#[test]
fn return_type_mismatch_is_reported() {
    let src = r#"
pub class A {
  fn f() -> Int {
    "text"
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "expected `Int`");
}

#[test]
fn static_method_cannot_use_class_generics() {
    let src = r#"
pub class Box<T> {
  value: T
  static fn empty() -> Option<T> {
    None
  }
}
"#;
    assert_has(
        &[("a.lt", src)],
        "a.lt",
        "cannot use the class type parameters",
    );
}

#[test]
fn enum_construction_and_matching_typecheck() {
    let src = r#"
pub enum Shape {
  Circle(Float)
  Square(Float)
}

pub class A {
  fn area(s: Shape) -> Float {
    match s {
      Shape.Circle(r) => Math.pi() * r * r
      Shape.Square(side) => side * side
    }
  }
}
"#;
    assert_ok(&[("a.lt", src)], "a.lt");
}

#[test]
fn method_on_wrong_receiver_type_is_reported() {
    let src = r#"
pub class A {
  fn f() -> Int {
    "text".unknown_method()
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "no method `unknown_method`");
}

#[test]
fn question_mark_inside_lambda_is_rejected() {
    let src = r#"
pub class A {
  fn inner(x: Int) -> Result<Int, Str> { Ok(x) }

  fn outer(xs: List<Int>) -> Result<List<Int>, Str> {
    Ok(xs.map(fn(x) => A.inner(x)?))
  }
}
"#;
    assert_has(
        &[("a.lt", src)],
        "a.lt",
        "`?` cannot be used inside a lambda",
    );
}

#[test]
fn tagged_template_interpolations_are_typechecked() {
    let src = r#"
pub class Sql {
  static fn tag(parts: List<Str>, values: List<Str>) -> Str {
    parts.join("|")
  }
}

pub class A {
  fn f() -> Str {
    Sql.tag`id = ${missing}`
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "unknown variable `missing`");
}

#[test]
fn template_cannot_interpolate_unit() {
    let src = r#"
pub class A {
  static fn side_effect() {}

  fn f() -> Str {
    `${A.side_effect()}`
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "cannot be interpolated");
}

#[test]
fn tagged_template_cannot_interpolate_unit() {
    let src = r#"
pub class Sql {
  static fn tag(parts: List<Str>, values: List<Str>) -> Str {
    parts.join("|")
  }
}

pub class A {
  static fn side_effect() {}

  fn f() -> Str {
    Sql.tag`x ${A.side_effect()}`
  }
}
"#;
    assert_has(&[("a.lt", src)], "a.lt", "found `Unit`");
}

#[test]
fn tagged_template_values_must_match_the_declared_type() {
    let src = r#"
pub class Sql {
  static fn tag(parts: List<Str>, values: List<Int>) -> Str {
    parts.join("|")
  }
}

pub class A {
  fn f() -> Str {
    Sql.tag`x ${"text"}`
  }
}
"#;
    assert_has(
        &[("a.lt", src)],
        "a.lt",
        "expected `Int` in this tagged template value, found `Str`",
    );
}

#[test]
fn tagged_template_does_not_display_convert_values() {
    let src = r#"
pub class Sql {
  static fn tag(parts: List<Str>, values: List<Str>) -> Str {
    parts.join("|")
  }
}

pub class A {
  fn f() -> Str {
    Sql.tag`x ${1}`
  }
}
"#;
    assert_has(
        &[("a.lt", src)],
        "a.lt",
        "expected `Str` in this tagged template value, found `Int`",
    );
}

#[test]
fn tagged_template_accepts_option_values() {
    let src = r#"
pub class Sql {
  static fn tag(parts: List<Str>, values: List<Option<Int>>) -> Str {
    parts.join("|")
  }
}

pub class A {
  fn f() -> Str {
    Sql.tag`a ${Some(1)} b ${None}`
  }
}
"#;
    assert_ok(&[("a.lt", src)], "a.lt");
}

#[test]
fn tagged_template_does_not_implicitly_wrap_some() {
    let src = r#"
pub class Sql {
  static fn tag(parts: List<Str>, values: List<Option<Int>>) -> Str {
    parts.join("|")
  }
}

pub class A {
  fn f() -> Str {
    Sql.tag`a ${1}`
  }
}
"#;
    assert_has(
        &[("a.lt", src)],
        "a.lt",
        "expected `Option<Int>` in this tagged template value, found `Int`",
    );
}

#[test]
fn tagged_template_accepts_a_user_enum_umbrella() {
    let src = r#"
pub enum SqlArg {
  I(Int)
  S(Str)
}

pub class Sql {
  static fn tag(parts: List<Str>, values: List<SqlArg>) -> Str {
    parts.join("|")
  }
}

pub class A {
  fn f() -> Str {
    let id = 7
    Sql.tag`id = ${SqlArg.I(id)} name = ${SqlArg.S("bo")}`
  }
}
"#;
    assert_ok(&[("a.lt", src)], "a.lt");
}
