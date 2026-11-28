use lt_syntax::ast::*;
use lt_syntax::parse;

fn parse_ok(src: &str) -> File {
    let out = parse(src);
    assert!(
        out.diagnostics.is_empty(),
        "expected no diagnostics, got:\n{:#?}",
        out.diagnostics
    );
    out.file
}

fn first_package(file: &File) -> &PackageDecl {
    file.packages.first().expect("expected a package")
}

fn first_struct(file: &File) -> &StructDecl {
    first_package(file)
        .items
        .iter()
        .find_map(|item| match item {
            PackageItem::Struct(decl) => Some(decl),
            _ => None,
        })
        .expect("expected a struct")
}

fn package_fn<'a>(package: &'a PackageDecl, name: &str) -> &'a FnDecl {
    package
        .items
        .iter()
        .find_map(|item| match item {
            PackageItem::Fn(decl) if decl.name == name => Some(decl),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no function {name}"))
}

fn body_stmts(package: &PackageDecl, name: &str) -> Vec<Stmt> {
    package_fn(package, name).body.stmts.clone()
}

const USERS: &str = r#"
import common { Address } "./common.lt"

package models {

export enum Role {
  Admin
  Member(Int)
  Guest
}

export struct User {
  name: Str
  age: Int
  role: Role
  address: Address?
}

export fn greet(u: User) -> Str {
  `Hi ${u.name}, you are ${u.age}`
}

export fn label(u: User) -> Str {
  match u.role {
    Role.Admin => "admin"
    Role.Member(n) => `member-${n}`
    Role.Guest => "guest"
  }
}

export fn city(u: User) -> Str {
  match u.address {
    Some(a) => a.city
    None => "-"
  }
}

export fn anonymous() -> User {
  User(name: "?", age: 0, role: Role.Guest, address: None)
}
}
"#;

#[test]
fn empty_file() {
    let file = parse_ok("");
    assert!(file.imports.is_empty());
    assert!(file.packages.is_empty());
}

#[test]
fn imports() {
    let file = parse_ok(
        "import user \"./user.lt\"\n\
         import post { Post as P } \"https://raw.githubusercontent.com/foo/bar/v1.2.0/src/post.lt\"\n",
    );
    assert_eq!(file.imports.len(), 2);
    assert_eq!(file.imports[0].package, "user");
    assert_eq!(file.imports[0].path, "./user.lt");
    assert!(file.imports[0].items.is_empty());
    assert_eq!(file.imports[0].binding(), "user");
    assert_eq!(file.imports[1].items.len(), 1);
    assert_eq!(file.imports[1].items[0].name, "Post");
    assert_eq!(file.imports[1].items[0].binding(), "P");
    assert!(file.imports[1].path.starts_with("https://"));
}

#[test]
fn package_with_struct_and_functions() {
    let file = parse_ok(
        r#"
package models {

export struct User {
  name: Str
  age: Int
}

export fn greet(u: User) -> Str { "hi" }

fn unknown() -> Str { "?" }
}
"#,
    );
    let package = first_package(&file);
    assert_eq!(package.name, "models");

    let user = first_struct(&file);
    assert!(user.is_export);
    assert_eq!(user.name, "User");
    assert_eq!(user.fields.len(), 2);
    assert_eq!(user.fields[0].name, "name");
    assert_eq!(user.fields[0].ty.name(), "Str");

    let greet = package_fn(package, "greet");
    assert!(greet.is_export);
    assert_eq!(greet.params.len(), 1);
    assert_eq!(greet.params[0].name, "u");
    assert_eq!(greet.ret.as_ref().map(Type::name), Some("Str".to_string()));

    let unknown = package_fn(package, "unknown");
    assert!(!unknown.is_export);
    assert!(unknown.params.is_empty());
}

#[test]
fn optional_type_sugar() {
    let file = parse_ok("package a {\n  struct A { maybe: Int? }\n}");
    let decl = first_struct(&file);
    assert!(decl.fields[0].ty.optional);
    assert_eq!(decl.fields[0].ty.name(), "Int");
}

#[test]
fn generic_struct_and_type_arguments() {
    let file = parse_ok(
        r#"
package box {

export struct Box<T> {
  value: T
  all: List<T>
}

export fn get(b: Box<T>) -> T { b.value }
}
"#,
    );
    let decl = first_struct(&file);
    assert_eq!(decl.generics, vec!["T".to_string()]);
    assert_eq!(decl.fields[1].ty.path, vec!["List".to_string()]);
    assert_eq!(decl.fields[1].ty.args.len(), 1);
}

#[test]
fn enum_with_payloads() {
    let file = parse_ok(
        r#"
package shapes {

export enum Shape {
  Circle(Float)
  Rect(Float, Float)
  Point
}
}
"#,
    );
    let PackageItem::Enum(decl) = &first_package(&file).items[0] else {
        panic!("expected enum");
    };
    assert_eq!(decl.variants.len(), 3);
    assert_eq!(decl.variants[0].name, "Circle");
    assert_eq!(decl.variants[0].payload.len(), 1);
    assert_eq!(decl.variants[1].payload.len(), 2);
    assert!(decl.variants[2].payload.is_empty());
}

#[test]
fn test_declaration() {
    let file = parse_ok(
        r#"
package user_test {

test "greet format" {
  expect.eq(1, 1)
}

fn helper() -> Int { 1 }
}
"#,
    );
    let package = first_package(&file);
    let PackageItem::Test(decl) = &package.items[0] else {
        panic!("expected a test declaration");
    };
    assert_eq!(decl.title, "greet format");
    assert!(!decl.body.stmts.is_empty());

    let helper = package_fn(package, "helper");
    assert!(helper.params.is_empty());
}

#[test]
fn let_bindings_with_and_without_types() {
    let file = parse_ok(
        r#"
package a {
  fn f() {
    let a = 1
    let b: Int = 2
    let mut c = 3
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    match &stmts[0] {
        Stmt::Let {
            mutable, name, ty, ..
        } => {
            assert!(!mutable);
            assert_eq!(name, "a");
            assert!(ty.is_none());
        }
        other => panic!("unexpected {other:?}"),
    }
    match &stmts[1] {
        Stmt::Let { ty, .. } => assert_eq!(ty.as_ref().map(Type::name), Some("Int".into())),
        other => panic!("unexpected {other:?}"),
    }
    match &stmts[2] {
        Stmt::Let { mutable, .. } => assert!(mutable),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn assignment_is_its_own_statement() {
    let file = parse_ok(
        r#"
package a {
  fn f() {
    let mut x = 1
    x = 2
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    assert!(matches!(&stmts[1], Stmt::Assign { name, .. } if name == "x"));
}

#[test]
fn if_else_chain_and_loops() {
    let file = parse_ok(
        r#"
package a {
  fn f(xs: List<Int>) -> Int {
    let mut total = 0
    for x in xs {
      if x > 0 {
        total = total + x
      } else if x == 0 {
        total = total
      } else {
        total = total - 1
      }
    }
    while total > 100 {
      total = total - 1
    }
    total
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    assert!(matches!(&stmts[1], Stmt::For { var, .. } if var == "x"));
    assert!(matches!(&stmts[2], Stmt::While { .. }));
}

#[test]
fn precedence_and_associativity() {
    let file = parse_ok(
        r#"
package a {
  fn f() -> Int {
    1 + 2 * 3 - 4
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    let Stmt::Expr(Expr::Binary { op, lhs, rhs, .. }) = &stmts[0] else {
        panic!("expected binary expr, got {:?}", stmts[0]);
    };
    assert_eq!(*op, BinOp::Sub);
    assert!(matches!(rhs.as_ref(), Expr::Int { raw, .. } if raw == "4"));
    let Expr::Binary { op: inner, .. } = lhs.as_ref() else {
        panic!("expected nested binary");
    };
    assert_eq!(*inner, BinOp::Add);
}

#[test]
fn newline_ends_a_statement() {
    let file = parse_ok(
        r#"
package a {
  fn f() {
    x = a
    -b
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    assert_eq!(stmts.len(), 2, "{stmts:#?}");
    assert!(matches!(&stmts[0], Stmt::Assign { .. }));
    assert!(matches!(
        &stmts[1],
        Stmt::Expr(Expr::Unary { op: UnOp::Neg, .. })
    ));
}

#[test]
fn multiline_expressions_continue_inside_parens() {
    let file = parse_ok(
        r#"
package a {
  fn f() -> Int {
    g(
      1 +
      2
    )
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    let Stmt::Expr(Expr::Call { args, .. }) = &stmts[0] else {
        panic!("expected call, got {stmts:?}");
    };
    let Args::Positional(items) = args else {
        panic!("expected positional args");
    };
    assert!(matches!(&items[0], Expr::Binary { op: BinOp::Add, .. }));
}

#[test]
fn named_and_positional_arguments() {
    let file = parse_ok(
        r#"
package a {
  fn f() {
    User(name: "a", age: 1)
    Math.max(1, 2)
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    let Stmt::Expr(Expr::Call {
        args: Args::Named(items),
        ..
    }) = &stmts[0]
    else {
        panic!("expected named args, got {:?}", stmts[0]);
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].name, "name");
    let Stmt::Expr(Expr::Call {
        args: Args::Positional(items),
        ..
    }) = &stmts[1]
    else {
        panic!("expected positional args");
    };
    assert_eq!(items.len(), 2);
}

#[test]
fn template_literal_parts() {
    let file = parse_ok(
        r#"
package a {
  fn f() -> Str {
    `a ${x} b`
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    let Stmt::Expr(Expr::Template { parts, exprs, .. }) = &stmts[0] else {
        panic!("expected template");
    };
    assert_eq!(parts, &vec!["a ".to_string(), " b".to_string()]);
    assert_eq!(exprs.len(), 1);
    assert!(matches!(&exprs[0], Expr::Ident { name, .. } if name == "x"));
}

#[test]
fn tagged_template() {
    let file = parse_ok(
        r#"
package a {
  fn f() -> Query {
    Sql.query`id=${id}`
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    let Stmt::Expr(Expr::TaggedTemplate {
        tag, parts, exprs, ..
    }) = &stmts[0]
    else {
        panic!("expected tagged template");
    };
    assert!(matches!(tag.as_ref(), Expr::Field { name, .. } if name == "query"));
    assert_eq!(parts, &vec!["id=".to_string(), String::new()]);
    assert_eq!(exprs.len(), 1);
}

#[test]
fn jsx_element() {
    let file = parse_ok(
        r#"
package v {
  fn f() -> Element {
    <Element tag="div" class_name="user">
      <Element tag="span">{name}</Element>
    </Element>
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    let Stmt::Expr(Expr::Jsx(root)) = &stmts[0] else {
        panic!("expected jsx");
    };
    assert_eq!(root.name, "Element");
    assert_eq!(root.attrs.len(), 2);
    assert!(matches!(&root.attrs[0].value, JsxAttrValue::Str(s) if s == "div"));
    assert_eq!(root.children.len(), 1);
    let JsxChild::Element(child) = &root.children[0] else {
        panic!("expected nested element");
    };
    assert_eq!(child.name, "Element");
    assert!(matches!(&child.children[0], JsxChild::Expr(_)));
}

#[test]
fn match_arms_and_patterns() {
    let file = parse_ok(
        r#"
package a {
  fn f() -> Int {
    match r {
      Role.Member(n) => n
      Role.Admin => 1
      None => 0
      _ => -1
    }
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] else {
        panic!("expected match");
    };
    assert_eq!(arms.len(), 4);
    assert!(
        matches!(&arms[0].pattern, Pattern::Variant { path, args, .. }
        if path == &vec!["Role".to_string(), "Member".to_string()] && args.len() == 1)
    );
    assert!(matches!(&arms[2].pattern, Pattern::NoneLit { .. }));
    assert!(matches!(&arms[3].pattern, Pattern::Wildcard { .. }));
}

#[test]
fn lambdas() {
    let file = parse_ok(
        r#"
package a {
  fn f(xs: List<Int>) {
    xs.map(fn(x) => x + 1)
    xs.filter(fn(x) => { x })
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    let Stmt::Expr(Expr::Call {
        args: Args::Positional(items),
        ..
    }) = &stmts[0]
    else {
        panic!("expected call");
    };
    let Expr::Lambda { body, .. } = &items[0] else {
        panic!("expected lambda");
    };
    assert!(matches!(body, LambdaBody::Expr(_)));

    let Stmt::Expr(Expr::Call {
        args: Args::Positional(items),
        ..
    }) = &stmts[1]
    else {
        panic!("expected call");
    };
    let Expr::Lambda { body, .. } = &items[0] else {
        panic!("expected lambda");
    };
    assert!(matches!(body, LambdaBody::Block(_)));
}

#[test]
fn try_operator_and_field_chains() {
    let file = parse_ok(
        r#"
package a {
  fn f() -> Result<Int, Str> {
    validate(u)?
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    let Stmt::Expr(Expr::Try { value, .. }) = &stmts[0] else {
        panic!("expected try expr");
    };
    assert!(matches!(value.as_ref(), Expr::Call { .. }));
}

#[test]
fn full_program_parses_cleanly() {
    let file = parse_ok(USERS);
    assert_eq!(file.imports.len(), 1);
    assert_eq!(file.packages.len(), 1);
    let package = first_package(&file);
    assert_eq!(package.name, "models");
    assert_eq!(first_struct(&file).fields.len(), 4);
    let functions = package
        .items
        .iter()
        .filter(|item| matches!(item, PackageItem::Fn(_)))
        .count();
    assert_eq!(functions, 4);
}

#[test]
fn validation_example_parses_cleanly() {
    let src = r#"
import models { User } "./models/user.lt"

package validation {

export fn check(u: User) -> Result<User, Str> {
  if u.age < 0 {
    return Err("age must be >= 0")
  }
  if u.name.len() == 0 {
    return Err("name required")
  }
  Ok(u)
}

export fn check_all(users: List<User>) -> Result<List<User>, Str> {
  users.map(fn(u) => check(u))
}
}
"#;
    let file = parse_ok(src);
    let package = first_package(&file);
    let check = package_fn(package, "check");
    assert_eq!(check.params.len(), 1);
    assert!(check.is_export);
}

#[test]
fn error_missing_field_colon() {
    let out = parse("package a {\n  struct A { name Str }\n}");
    assert!(!out.diagnostics.is_empty());
    assert!(
        out.diagnostics[0].message.contains("expected"),
        "{:?}",
        out.diagnostics[0]
    );
}

#[test]
fn error_unclosed_class_body() {
    let out = parse("package a {\n  struct A {\n    name: Str\n");
    assert!(!out.diagnostics.is_empty());
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("expected `}`")),
        "{:#?}",
        out.diagnostics
    );
}

#[test]
fn error_missing_from_in_import() {
    let out = parse("import User from \"x.lt\"");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("`from` is no longer used")),
        "{:#?}",
        out.diagnostics
    );
}

#[test]
fn import_group_entries_share_one_group() {
    let file = parse_ok(
        "import (\n  post { Post } \"https://example.com/post.lt\"\n\n  user \"user.lt\"\n)\n",
    );
    assert_eq!(file.imports.len(), 2);
    assert_eq!(file.imports[0].package, "post");
    assert_eq!(file.imports[0].group, file.imports[1].group);
    assert!(file.imports[0].group.is_some());
    assert!(!file.imports[0].blank_before);
    assert!(file.imports[1].blank_before);
}

#[test]
fn single_imports_have_no_group() {
    let file = parse_ok("import a \"./a.lt\"\nimport b \"./b.lt\"\n");
    assert_eq!(file.imports.len(), 2);
    assert!(file.imports[0].group.is_none());
    assert!(file.imports[1].group.is_none());
}

#[test]
fn error_dot_import() {
    let out = parse("import . \"./a.lt\"");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("dot import")),
        "{:#?}",
        out.diagnostics
    );
}

#[test]
fn error_blank_import() {
    let out = parse("import _ \"./a.lt\"");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("blank import")),
        "{:#?}",
        out.diagnostics
    );
}

#[test]
fn error_empty_import_list() {
    let out = parse("import a {} \"./a.lt\"");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("empty import list")),
        "{:#?}",
        out.diagnostics
    );
}

#[test]
fn error_empty_import_group() {
    let out = parse("import ()");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("empty import group")),
        "{:#?}",
        out.diagnostics
    );
}

#[test]
fn soft_keywords_can_name_declarations_and_members() {
    let file = parse_ok(
        "import from \"./from.lt\"\n\n\
         package keys {\n\
         \x20 export struct Range {\n\
         \x20   from: Int\n\
         \x20 }\n\
         \n\
         \x20 export fn from(r: Range) -> Int {\n\
         \x20   r.from\n\
         \x20 }\n\
         \n\
         \x20 export fn build() -> Range {\n\
         \x20   let from = 1\n\
         \x20   Range(from: from)\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!(file.imports.len(), 1);
    assert_eq!(file.imports[0].package, "from");
    let package = first_package(&file);
    assert_eq!(package.name, "keys");
    assert_eq!(first_struct(&file).fields[0].name, "from");
    assert!(package_fn(package, "from").is_export);
}

#[test]
fn any_keyword_can_name_a_member() {
    // The position after `.` already proves this is a name, so even words that
    // can begin a statement are accepted there.
    let file = parse_ok(
        "package a {\n  export fn f(b: Box) -> Int {\n    b.match + b.class + b.fn + b.none\n  }\n}\n",
    );
    let package = first_package(&file);
    assert!(package_fn(package, "f").is_export);
}

#[test]
fn reserved_keyword_as_a_name_suggests_an_underscore() {
    let out = parse("package a {\n  export fn match(x: Int) -> Int {\n    x\n  }\n}");
    assert!(
        out.diagnostics.iter().any(|d| d
            .message
            .contains("`match` is a keyword; rename it to `match_`")),
        "{:#?}",
        out.diagnostics
    );
}

#[test]
fn dict_literals_keep_their_entries() {
    let file =
        parse_ok("package a {\n  export fn f() -> Dict<Int> {\n    [\"b\": 2, \"a\": 1]\n  }\n}\n");
    let package = first_package(&file);
    let function = package_fn(package, "f");
    let Stmt::Expr(Expr::Dict { entries, .. }) = &function.body.stmts[0] else {
        panic!("expected a dict literal");
    };
    assert_eq!(entries.len(), 2);
    assert!(matches!(entries[0].0, Expr::Str { .. }));
    assert!(matches!(entries[1].0, Expr::Str { .. }));
}

#[test]
fn empty_dict_is_a_colon_in_brackets() {
    let file = parse_ok("package a {\n  export fn f() -> Dict<Int> {\n    [:]\n  }\n}\n");
    let function = package_fn(first_package(&file), "f");
    let Stmt::Expr(Expr::Dict { entries, .. }) = &function.body.stmts[0] else {
        panic!("expected an empty dict literal");
    };
    assert!(entries.is_empty());
}

#[test]
fn dict_keys_must_be_string_literals() {
    let out = parse("package a {\n  export fn f() -> Dict<Int> {\n    [name: 1]\n  }\n}\n");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("dict keys must be string literals")),
        "{:#?}",
        out.diagnostics
    );
}

#[test]
fn error_does_not_panic_on_garbage() {
    let out = parse("@@@@ class { fn } } }");
    assert!(!out.diagnostics.is_empty());
}

#[test]
fn diagnostic_renders_with_location() {
    let src = "package a {\n  struct A { name Str }\n}";
    let out = parse(src);
    let rendered = out.diagnostics[0].render("main.lt", src);
    assert!(rendered.contains("main.lt:2:"), "{rendered}");
}

#[test]
fn method_chains_continue_across_lines() {
    let file = parse_ok(
        r#"
package a {
  fn f(xs: List<Int>) -> Int {
    xs
      .filter(fn(x) => x > 0)
      .map(fn(x) => x * 2)
      .len()
  }
}
"#,
    );
    let stmts = body_stmts(first_package(&file), "f");
    assert_eq!(stmts.len(), 1, "a chain across newlines is one statement");
    assert!(matches!(&stmts[0], Stmt::Expr(Expr::Call { .. })));
}
