use std::path::PathBuf;

use lt_hir::{check_entries, MapLoader};

/// Wrap a package body in `package <name> { … }`.
fn pkg(name: &str, body: &str) -> String {
    format!("package {name} {{\n{body}\n}}\n")
}

/// A file whose imports precede the package block.
fn file(imports: &str, name: &str, body: &str) -> String {
    format!("{imports}\n\npackage {name} {{\n{body}\n}}\n")
}

fn run(files: &[(&str, String)], entry: &str) -> Vec<String> {
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

fn assert_ok(files: &[(&str, String)], entry: &str) {
    let messages = run(files, entry);
    assert!(messages.is_empty(), "unexpected diagnostics: {messages:#?}");
}

fn assert_has(files: &[(&str, String)], entry: &str, needle: &str) {
    let messages = run(files, entry);
    assert!(
        messages.iter().any(|message| message.contains(needle)),
        "expected a diagnostic containing `{needle}`, got {messages:#?}"
    );
}

fn one(name: &str, body: &str) -> Vec<(&'static str, String)> {
    // Callers pass a fixed file name; leaks are fine in a test binary.
    let path: &'static str = Box::leak(format!("{name}.lt").into_boxed_str());
    vec![(path, pkg(name, body))]
}

const USER: &str = r#"
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
    Role.Member(n) => `member-${n}`
    Role.Guest => "guest"
  }
}

export fn anonymous() -> User {
  User(name: "?", age: 0, role: Role.Guest)
}
"#;

#[test]
fn package_with_struct_enum_and_functions() {
    assert_ok(&[("models.lt", pkg("models", USER))], "models.lt");
}

#[test]
fn receiver_sugar_calls_package_function() {
    let body = r#"
export struct Point {
  x: Int
  y: Int
}

export fn len(p: Point) -> Int {
  p.x + p.y
}

export fn use_it(p: Point) -> Int {
  p.len()
}
"#;
    assert_ok(&one("geo", body), "geo.lt");
}

#[test]
fn receiver_function_must_take_its_type_first() {
    let body = r#"
export struct Point {
  x: Int
}

export fn len(n: Int) -> Int {
  n
}

export fn use_it(p: Point) -> Int {
  p.len()
}
"#;
    assert_has(&one("geo", body), "geo.lt", "has no function `len`");
}

#[test]
fn construction_requires_named_arguments() {
    let body = r#"
export struct Point {
  x: Int
}

export fn origin() -> Point {
  Point(0)
}
"#;
    assert_has(&one("geo", body), "geo.lt", "named arguments");
}

#[test]
fn construction_requires_every_field() {
    let body = r#"
export struct Point {
  x: Int
  y: Int
}

export fn origin() -> Point {
  Point(x: 0)
}
"#;
    assert_has(&one("geo", body), "geo.lt", "missing field `y`");
}

#[test]
fn construction_rejects_unknown_field() {
    let body = r#"
export struct Point {
  x: Int
}

export fn origin() -> Point {
  Point(x: 0, z: 1)
}
"#;
    assert_has(&one("geo", body), "geo.lt", "has no field `z`");
}

#[test]
fn duplicate_field_is_reported() {
    let body = r#"
export struct Point {
  x: Int
  x: Str
}
"#;
    assert_has(&one("geo", body), "geo.lt", "duplicate field `x`");
}

#[test]
fn one_namespace_per_package() {
    let body = r#"
export struct Data {
  value: Int
}

export fn data(d: Data) -> Int {
  d.value
}
"#;
    assert_has(&one("shapes", body), "shapes.lt", "declared more than once");
}

#[test]
fn type_names_are_pascal_case() {
    let body = "export struct user {\n  name: Str\n}\n";
    assert_has(&one("shapes", body), "shapes.lt", "PascalCase");
}

#[test]
fn value_names_are_snake_case() {
    let body = "export fn DoThing() -> Int {\n  1\n}\n";
    assert_has(&one("shapes", body), "shapes.lt", "snake_case");
}

#[test]
fn destructured_import_requires_export() {
    let files = [
        (
            "models.lt",
            pkg("models", "struct User {\n  name: Str\n}"),
        ),
        (
            "app.lt",
            file(
                "import models { User } from \"./models.lt\"",
                "app",
                "export fn f(u: User) -> Str {\n  u.name\n}",
            ),
        ),
    ];
    assert_has(&files, "app.lt", "unknown type `User`");
}

#[test]
fn destructured_import_is_enough() {
    let files = [
        ("models.lt", pkg("models", USER)),
        (
            "app.lt",
            file(
                "import models { User, greet } from \"./models.lt\"",
                "app",
                "export fn hello(u: User) -> Str {\n  greet(u)\n}",
            ),
        ),
    ];
    assert_ok(&files, "app.lt");
}

#[test]
fn qualified_paths_reach_types_functions_and_bindings() {
    let files = [
        (
            "models.lt",
            pkg(
                "models",
                "export let origin: Str = \"local\"\n\nexport struct User {\n  name: Str\n}",
            ),
        ),
        (
            "app.lt",
            file(
                "import models from \"./models.lt\"",
                "app",
                "export fn make() -> models.User {\n  models.User(name: models.origin)\n}",
            ),
        ),
    ];
    assert_ok(&files, "app.lt");
}

#[test]
fn private_binding_is_not_reachable_by_qualifier() {
    let files = [
        ("models.lt", pkg("models", "let origin: Str = \"local\"")),
        (
            "app.lt",
            file(
                "import models from \"./models.lt\"",
                "app",
                "export fn read() -> Str {\n  models.origin\n}",
            ),
        ),
    ];
    assert_has(&files, "app.lt", "private to package `models`");
}

#[test]
fn import_must_name_a_package_that_exists() {
    let files = [
        ("a.lt", pkg("alpha", "export fn f() -> Int {\n  1\n}")),
        (
            "b.lt",
            file("import missing { f } from \"./a.lt\"", "beta", ""),
        ),
    ];
    assert_has(&files, "b.lt", "does not declare `package missing`");
}

#[test]
fn package_names_are_unique_per_build() {
    let files = [
        ("a.lt", pkg("shared", "export fn f() -> Int {\n  1\n}")),
        ("b.lt", pkg("shared", "export fn g() -> Int {\n  2\n}")),
        (
            "c.lt",
            file(
                "import shared { f } from \"./a.lt\"\nimport shared from \"./b.lt\"",
                "app",
                "export fn h() -> Int {\n  f()\n}",
            ),
        ),
    ];
    assert_has(&files, "c.lt", "already declared");
}

#[test]
fn to_string_override_is_used_by_interpolation() {
    let body = r#"
export struct Price {
  amount: Int
}

export fn to_string(p: Price) -> Str {
  `${p.amount}`
}

export fn show(p: Price) -> Str {
  `price: ${p}`
}
"#;
    assert_ok(&one("money", body), "money.lt");
}

#[test]
fn non_exhaustive_match_is_reported() {
    let body = r#"
export enum Role {
  Admin
  Guest
}

export fn label(r: Role) -> Str {
  match r {
    Role.Admin => "admin"
  }
}
"#;
    assert_has(&one("models", body), "models.lt", "not exhaustive");
}

#[test]
fn generic_struct_is_usable() {
    let body = r#"
export struct Box<T> {
  value: T
}

export fn unbox(b: Box<Int>) -> Int {
  b.value
}
"#;
    assert_ok(&one("box", body), "box.lt");
}

#[test]
fn package_level_bindings_are_inferred() {
    let body = r#"
let limit = 10

export fn at_limit(n: Int) -> Bool {
  n >= limit
}
"#;
    assert_ok(&one("config", body), "config.lt");
}

#[test]
fn bindings_are_immutable() {
    let files = [
        ("config.lt", pkg("config", "export let limit = 10")),
        (
            "app.lt",
            file(
                "import config { limit } from \"./config.lt\"",
                "app",
                "export fn bump() -> Int {\n  limit = 11\n  limit\n}",
            ),
        ),
    ];
    assert_has(&files, "app.lt", "immutable");
}

#[test]
fn expect_is_only_available_in_tests() {
    let body = r#"
export fn f() -> Int {
  expect.eq(1, 1)
  0
}
"#;
    assert_has(&one("app", body), "app.lt", "only available inside");
}

#[test]
fn test_declaration_uses_expect() {
    let body = r#"
export fn double(n: Int) -> Int {
  n * 2
}

test "doubles" {
  expect.eq(double(2), 4)
}
"#;
    assert_ok(&one("app", body), "app.lt");
}

#[test]
fn tagged_template_tag_is_a_package_function() {
    let body = r#"
export fn query(parts: List<Str>, values: List<Int>) -> Str {
  "q"
}

export fn find(id: Int) -> Str {
  query`select ${id}`
}
"#;
    assert_ok(&one("sql", body), "sql.lt");
}

#[test]
fn jsx_tag_is_a_struct() {
    let body = r#"
export struct Element {
  tag: Str
  children: List<Str>
}

export fn render() -> Element {
  <Element tag="div">hello</Element>
}
"#;
    assert_ok(&one("view", body), "view.lt");
}

#[test]
fn jsx_rejects_unknown_tag() {
    let body = r#"
export fn render() -> Int {
  <Missing tag="div" />
  0
}
"#;
    assert_has(&one("view", body), "view.lt", "unknown struct `Missing`");
}

#[test]
fn assigning_to_a_non_mut_local_is_reported() {
    let body = r#"
export fn f() -> Int {
  let x = 1
  x = 2
  x
}
"#;
    assert_has(&one("app", body), "app.lt", "immutable");
}

#[test]
fn struct_field_types_are_checked() {
    let body = r#"
export struct Point {
  x: Int
}

export fn bad() -> Point {
  Point(x: "no")
}
"#;
    assert_has(&one("geo", body), "geo.lt", "expected `Int`");
}
