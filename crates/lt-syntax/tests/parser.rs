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

fn first_class(file: &File) -> &ClassDecl {
    file.items
        .iter()
        .find_map(|item| match item {
            Item::Class(c) => Some(c),
            _ => None,
        })
        .expect("expected a class")
}

fn body_stmts(class: &ClassDecl, method: &str) -> Vec<Stmt> {
    class
        .methods
        .iter()
        .find(|m| m.name == method)
        .unwrap_or_else(|| panic!("no method {method}"))
        .body
        .stmts
        .clone()
}

const USERS: &str = r#"
import Address from "./address.lt"

pub enum Role {
  Admin
  Member(Int)
  Guest
}

pub class User {
  name: Str
  age: Int
  role: Role
  address: Address?

  pub fn greet(self) -> Str {
    `Hi ${self.name}, you are ${self.age}`
  }

  pub fn label(self) -> Str {
    match self.role {
      Role.Admin => "admin"
      Role.Member(n) => `member-${n}`
      Role.Guest => "guest"
    }
  }

  pub fn city(self) -> Str {
    match self.address {
      Some(a) => a.city
      None => "-"
    }
  }

  static fn anonymous() -> User {
    User(name: "?", age: 0, role: Role.Guest, address: None)
  }
}
"#;

#[test]
fn empty_file() {
    let file = parse_ok("");
    assert!(file.imports.is_empty());
    assert!(file.items.is_empty());
}

#[test]
fn imports() {
    let file = parse_ok(
        "import User from \"./user.lt\"\n\
         import Post from \"https://raw.githubusercontent.com/foo/bar/v1.2.0/src/post.lt\"\n",
    );
    assert_eq!(file.imports.len(), 2);
    assert_eq!(file.imports[0].name, "User");
    assert_eq!(file.imports[0].path, "./user.lt");
    assert!(file.imports[1].path.starts_with("https://"));
}

#[test]
fn class_with_fields_and_methods() {
    let file = parse_ok(
        r#"
pub class User {
  name: Str
  age: Int

  pub fn greet(self) -> Str { "hi" }

  static fn unknown() -> Str { "?" }
}
"#,
    );
    let class = first_class(&file);
    assert!(class.is_pub);
    assert_eq!(class.name, "User");
    assert_eq!(class.fields.len(), 2);
    assert_eq!(class.fields[0].name, "name");
    assert_eq!(class.fields[0].ty.name(), "Str");
    assert_eq!(class.methods.len(), 2);

    let greet = &class.methods[0];
    assert!(greet.has_self);
    assert!(greet.is_pub);
    assert!(!greet.is_static);
    assert_eq!(greet.ret.as_ref().map(Type::name), Some("Str".to_string()));

    let unknown = &class.methods[1];
    assert!(unknown.is_static);
    assert!(!unknown.has_self);
}

#[test]
fn optional_type_sugar() {
    let file = parse_ok("pub class A { maybe: Int? }");
    let class = first_class(&file);
    assert!(class.fields[0].ty.optional);
    assert_eq!(class.fields[0].ty.name(), "Int");
}

#[test]
fn generic_class_and_type_arguments() {
    let file = parse_ok(
        r#"
pub class Box<T> {
  value: T
  all: List<T>
  pub fn get(self) -> T { self.value }
}
"#,
    );
    let class = first_class(&file);
    assert_eq!(class.generics, vec!["T".to_string()]);
    assert_eq!(class.fields[1].ty.path, vec!["List".to_string()]);
    assert_eq!(class.fields[1].ty.args.len(), 1);
}

#[test]
fn enum_with_payloads() {
    let file = parse_ok(
        r#"
pub enum Shape {
  Circle(Float)
  Rect(Float, Float)
  Point
}
"#,
    );
    let Item::Enum(decl) = &file.items[0] else {
        panic!("expected enum");
    };
    assert_eq!(decl.variants.len(), 3);
    assert_eq!(decl.variants[0].name, "Circle");
    assert_eq!(decl.variants[0].payload.len(), 1);
    assert_eq!(decl.variants[1].payload.len(), 2);
    assert!(decl.variants[2].payload.is_empty());
}

#[test]
fn test_class() {
    let file = parse_ok(
        r#"
test class UserTest {
  test fn greet_format() {
    Expect.equal(1, 1)
  }

  fn helper() -> Int { 1 }
}
"#,
    );
    let Item::TestClass(decl) = &file.items[0] else {
        panic!("expected test class");
    };
    assert_eq!(decl.name, "UserTest");
    assert_eq!(decl.tests.len(), 1);
    assert_eq!(decl.tests[0].name, "greet_format");
    assert_eq!(decl.helpers.len(), 1);
}

#[test]
fn let_bindings_with_and_without_types() {
    let file = parse_ok(
        r#"
pub class A {
  fn f(self) {
    let a = 1
    let b: Int = 2
    let mut c = 3
  }
}
"#,
    );
    let stmts = body_stmts(first_class(&file), "f");
    match &stmts[0] {
        Stmt::Let { mutable, name, ty, .. } => {
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
pub class A {
  fn f(self) {
    let mut x = 1
    x = 2
  }
}
"#,
    );
    let stmts = body_stmts(first_class(&file), "f");
    assert!(matches!(&stmts[1], Stmt::Assign { name, .. } if name == "x"));
}

#[test]
fn if_else_chain_and_loops() {
    let file = parse_ok(
        r#"
pub class A {
  fn f(self, xs: List<Int>) -> Int {
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
    let stmts = body_stmts(first_class(&file), "f");
    assert!(matches!(&stmts[1], Stmt::For { var, .. } if var == "x"));
    assert!(matches!(&stmts[2], Stmt::While { .. }));
}

#[test]
fn precedence_and_associativity() {
    let file = parse_ok(
        r#"
pub class A {
  fn f(self) -> Int {
    1 + 2 * 3 - 4
  }
}
"#,
    );
    let stmts = body_stmts(first_class(&file), "f");
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
pub class A {
  fn f(self) {
    x = a
    -b
  }
}
"#,
    );
    let stmts = body_stmts(first_class(&file), "f");
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
pub class A {
  fn f(self) -> Int {
    g(
      1 +
      2
    )
  }
}
"#,
    );
    let stmts = body_stmts(first_class(&file), "f");
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
pub class A {
  fn f(self) {
    User(name: "a", age: 1)
    Math.max(1, 2)
  }
}
"#,
    );
    let stmts = body_stmts(first_class(&file), "f");
    let Stmt::Expr(Expr::Call { args: Args::Named(items), .. }) = &stmts[0] else {
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
pub class A {
  fn f(self) -> Str {
    `a ${x} b`
  }
}
"#,
    );
    let stmts = body_stmts(first_class(&file), "f");
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
pub class A {
  fn f(self) -> Query {
    Sql.query`id=${id}`
  }
}
"#,
    );
    let stmts = body_stmts(first_class(&file), "f");
    let Stmt::Expr(Expr::TaggedTemplate { tag, parts, exprs, .. }) = &stmts[0] else {
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
pub class V {
  fn f(self) -> Element {
    <Element tag="div" class_name="user">
      <Element tag="span">{name}</Element>
    </Element>
  }
}
"#,
    );
    let stmts = body_stmts(first_class(&file), "f");
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
pub class A {
  fn f(self) -> Int {
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
    let stmts = body_stmts(first_class(&file), "f");
    let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] else {
        panic!("expected match");
    };
    assert_eq!(arms.len(), 4);
    assert!(matches!(&arms[0].pattern, Pattern::Variant { path, args, .. }
        if path == &vec!["Role".to_string(), "Member".to_string()] && args.len() == 1));
    assert!(matches!(&arms[2].pattern, Pattern::NoneLit { .. }));
    assert!(matches!(&arms[3].pattern, Pattern::Wildcard { .. }));
}

#[test]
fn lambdas() {
    let file = parse_ok(
        r#"
pub class A {
  fn f(self, xs: List<Int>) {
    xs.map(fn(x) => x + 1)
    xs.filter(fn(x) => { x })
  }
}
"#,
    );
    let stmts = body_stmts(first_class(&file), "f");
    let Stmt::Expr(Expr::Call { args: Args::Positional(items), .. }) = &stmts[0] else {
        panic!("expected call");
    };
    let Expr::Lambda { body, .. } = &items[0] else {
        panic!("expected lambda");
    };
    assert!(matches!(body, LambdaBody::Expr(_)));

    let Stmt::Expr(Expr::Call { args: Args::Positional(items), .. }) = &stmts[1] else {
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
pub class A {
  fn f(self) -> Result<Int, Str> {
    validate(u)?
  }
}
"#,
    );
    let stmts = body_stmts(first_class(&file), "f");
    let Stmt::Expr(Expr::Try { value, .. }) = &stmts[0] else {
        panic!("expected try expr");
    };
    assert!(matches!(value.as_ref(), Expr::Call { .. }));
}

#[test]
fn full_program_parses_cleanly() {
    let file = parse_ok(USERS);
    assert_eq!(file.imports.len(), 1);
    assert_eq!(file.items.len(), 2);
    let class = first_class(&file);
    assert_eq!(class.fields.len(), 4);
    assert_eq!(class.methods.len(), 4);
}

#[test]
fn validation_example_parses_cleanly() {
    let src = r#"
import User from "./models/user.lt"

pub class Validator {
  pub fn check(u: User) -> Result<User, Str> {
    if u.age < 0 {
      return Err("age must be >= 0")
    }
    if u.name.len() == 0 {
      return Err("name required")
    }
    Ok(u)
  }

  pub fn check_all(users: List<User>) -> Result<List<User>, Str> {
    users.map(fn(u) => Validator.check(u))
  }
}
"#;
    let file = parse_ok(src);
    let class = first_class(&file);
    assert_eq!(class.methods.len(), 2);
    assert!(!class.methods[0].has_self);
}

#[test]
fn error_missing_field_colon() {
    let out = parse("pub class A { name Str }");
    assert!(!out.diagnostics.is_empty());
    assert!(
        out.diagnostics[0].message.contains("expected"),
        "{:?}",
        out.diagnostics[0]
    );
}

#[test]
fn error_unclosed_class_body() {
    let out = parse("pub class A {\n  name: Str\n");
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
    let out = parse("import User \"x.lt\"");
    assert!(!out.diagnostics.is_empty());
}

#[test]
fn error_does_not_panic_on_garbage() {
    let out = parse("@@@@ class { fn } } }");
    assert!(!out.diagnostics.is_empty());
}

#[test]
fn diagnostic_renders_with_location() {
    let src = "pub class A {\n  name Str\n}";
    let out = parse(src);
    let rendered = out.diagnostics[0].render("main.lt", src);
    assert!(rendered.contains("main.lt:2:8"), "{rendered}");
}
