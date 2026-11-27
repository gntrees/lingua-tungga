use lt_syntax::{format_file, parse};

fn fmt(src: &str) -> String {
    let out = parse(src);
    assert!(
        out.diagnostics.is_empty(),
        "expected clean parse, got {:#?}",
        out.diagnostics
    );
    format_file(&out.file)
}

fn assert_stable(src: &str) {
    let once = fmt(src);
    let twice = fmt(&once);
    assert_eq!(once, twice, "formatter is not idempotent");
}

#[test]
fn formats_a_struct_and_fn_canonically() {
    let src =
        "package users {\n      export struct User {\n  name: Str\n  age:Int\n  }\n  export fn greet(u: User)->Str{`Hi ${u.name}`}\n}\n";
    let expected = "\
package users {
  export struct User {
    name: Str
    age: Int
  }

  export fn greet(u: User) -> Str {
    `Hi ${u.name}`
  }
}
";
    assert_eq!(fmt(src), expected);
}

#[test]
fn formats_enum_compactly() {
    let src = "package a {\nexport enum Role{\nAdmin\n\n\nMember(Int)\n Guest\n}\n}\n";
    let expected = "\
package a {
  export enum Role {
    Admin
    Member(Int)
    Guest
  }
}
";
    assert_eq!(fmt(src), expected);
}

#[test]
fn empty_bodies_collapse() {
    let src = "package a {}\n\npackage b { struct Empty {} }\n";
    let expected = "\
package a {}

package b {
  struct Empty {}
}
";
    assert_eq!(fmt(src), expected);
}

#[test]
fn imports_then_blank_line() {
    let src =
        "import b from \"./b.lt\"\nimport a { A } from \"./a.lt\"\npackage c { let x: Int = 1 }\n";
    let expected = "\
import b from \"./b.lt\"
import a { A } from \"./a.lt\"

package c {
  let x: Int = 1
}
";
    assert_eq!(fmt(src), expected);
}

#[test]
fn preserves_operator_grouping() {
    let src = "package a {\n  fn f() -> Int {\n    (1 + 2) * 3\n  }\n}\n";
    let out = fmt(src);
    assert!(out.contains("(1 + 2) * 3"), "{out}");
    assert_stable(src);
}

#[test]
fn breaks_long_calls_and_lists() {
    let long_args = (0..12)
        .map(|n| format!("argument_{n}"))
        .collect::<Vec<_>>()
        .join(", ");
    let src = format!("package a {{\n  fn f() {{\n    do_something({long_args})\n  }}\n}}\n");
    let out = fmt(&src);
    assert!(out.contains("do_something(\n"), "{out}");
    assert!(out.contains("argument_11,\n"), "{out}");
    assert_stable(&src);
}

#[test]
fn jsx_inline_and_multiline() {
    let inline = "package v {\n  fn f() -> Element {\n    <Element tag=\"div\" />\n  }\n}\n";
    let out = fmt(inline);
    assert!(out.contains("<Element tag=\"div\" />"), "{out}");

    let nested = "package v {\n  fn f() -> Element {\n    <Element tag=\"div\"><Element tag=\"span\">{name}</Element><Element tag=\"span\">{age}</Element></Element>\n  }\n}\n";
    let out = fmt(nested);
    assert!(out.contains("<Element tag=\"div\">\n"), "{out}");
    assert_stable(nested);
}

#[test]
fn match_arms_and_patterns_round_trip() {
    let src = "\
package a {
  fn f() -> Int {
    match r {
      Role.Member(n) => n
      _ => -1
    }
  }
}
";
    assert_eq!(fmt(src), src);
    assert_stable(src);
}

#[test]
fn template_escaping_round_trips() {
    let src = "package a {\n  fn f() -> Str {\n    `a\\nb \\${not} ${x}`\n  }\n}\n";
    let once = fmt(src);
    let twice = fmt(&once);
    assert_eq!(once, twice);
    assert!(once.contains("\\${not}"), "{once}");
}

#[test]
fn tagged_template_has_no_space() {
    let src = "package a {\n  fn f() -> Q {\n    sql.query`id=${id}`\n  }\n}\n";
    assert!(fmt(src).contains("sql.query`id=${id}`"));
}

#[test]
fn healthy_program_is_stable() {
    let src = r#"
import models { User } from "./models/user.lt"

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

fn loop_it(xs: List<Int>) -> Int {
  let mut total = 0
  for x in xs {
    if x > 0 {
      total = total + x
    } else {
      total = total - 1
    }
  }
  while total > 100 {
    total = total - 1
  }
  total
}

fn view(u: User) -> Element {
  <Element tag="div" class_name="user">
    <Element tag="span">{u.name}</Element>
  </Element>
}

test "checks negative age" {
  expect.true(check(User(name: "a", age: -1, role: Role.Guest, address: None)).is_err())
}
}
"#;
    assert_stable(src);
    let once = fmt(src);
    assert!(once.contains("if u.age < 0 {"), "{once}");
    assert!(once.contains("} else {"), "{once}");
    assert!(once.contains("test \"checks negative age\" {"), "{once}");
}

#[test]
fn comments_are_reported_by_the_lexer() {
    let out = lt_syntax::tokenize_full("// hello\npackage a { let x: Int = 1 }\n");
    assert_eq!(out.comments.len(), 1);
    let span = out.comments[0];
    assert_eq!(
        span.text("// hello\npackage a { let x: Int = 1 }\n"),
        "// hello"
    );
}

fn fmt_comments(src: &str) -> String {
    let lexed = lt_syntax::tokenize_full(src);
    assert!(lexed.errors.is_empty(), "{:#?}", lexed.errors);
    let out = parse(src);
    assert!(out.diagnostics.is_empty(), "{:#?}", out.diagnostics);
    lt_syntax::format_file_with_comments(&out.file, src, &lexed.comments)
        .unwrap_or_else(|error| panic!("{} at {}:{}", error.message, error.line, error.col))
}

#[test]
fn keeps_comments_on_items_and_members() {
    let src = "\
// file header
import b from \"./b.lt\"

// the package
package a {
  // a struct
  struct A {
    // a member
    x: Int
  }

  // a function
  export fn f(a: A) -> Int {
    // body comment
    a.x
  }
}
";
    let once = fmt_comments(src);
    assert!(once.contains("// file header"), "{once}");
    assert!(once.contains("// the package"), "{once}");
    assert!(once.contains("  // a struct"), "{once}");
    assert!(once.contains("    // a member"), "{once}");
    assert!(once.contains("  // a function"), "{once}");
    assert!(once.contains("    // body comment"), "{once}");
    assert_eq!(
        once,
        fmt_comments(&once),
        "comment formatting must be idempotent"
    );
}

#[test]
fn keeps_comments_before_enum_variants_and_match_arms() {
    let src = "\
package a {
  enum Role {
    // admins
    Admin
    // everyone else
    Guest
  }

  fn label(r: Role) -> Str {
    match r {
      // the admin branch
      Role.Admin => \"a\"
      // the guest branch
      Role.Guest => \"g\"
    }
  }
}
";
    let once = fmt_comments(src);
    assert!(once.contains("    // admins\n    Admin"), "{once}");
    assert!(once.contains("      // the admin branch"), "{once}");
    assert_eq!(once, fmt_comments(&once));
}

#[test]
fn keeps_trailing_comments_before_closing_braces() {
    let src = "\
package a {
  struct A {
    x: Int
  }

  fn f(a: A) -> Int {
    a.x
    // last thought
  }
  // end of package
}
";
    let once = fmt_comments(src);
    assert!(once.contains("// last thought"), "{once}");
    assert!(once.contains("// end of package"), "{once}");
    assert_eq!(once, fmt_comments(&once));
}

#[test]
fn normalizes_block_comments() {
    let src = "\
/*
   multi
   line
*/
package a {
  struct A {
    x: Int
  }
}
";
    let once = fmt_comments(src);
    assert!(once.starts_with("/*\n * multi\n * line\n */\n"), "{once}");
    assert_eq!(once, fmt_comments(&once));
}

#[test]
fn rejects_inline_comments() {
    let src = "package a { let x: Int = 1 } // trailing\n";
    let lexed = lt_syntax::tokenize_full(src);
    let out = parse(src);
    let error = lt_syntax::format_file_with_comments(&out.file, src, &lexed.comments).unwrap_err();
    assert!(error.message.contains("inline comments"), "{error:?}");
}

#[test]
fn several_packages_in_one_file_are_separated_by_one_blank_line() {
    let src = "package a{struct A{x:Int}}package b{struct B{y:Int}}";
    let expected = "\
package a {
  struct A {
    x: Int
  }
}

package b {
  struct B {
    y: Int
  }
}
";
    assert_eq!(fmt(src), expected);
}
