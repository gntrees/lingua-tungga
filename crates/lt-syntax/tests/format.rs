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
fn formats_a_class_canonically() {
    let src = "pub class User {\n      name: Str\n  age:Int\n  pub fn greet(self)->Str{`Hi ${self.name}`}\n}\n";
    let expected = "\
pub class User {
  name: Str
  age: Int

  pub fn greet(self) -> Str {
    `Hi ${self.name}`
  }
}
";
    assert_eq!(fmt(src), expected);
}

#[test]
fn formats_enum_compactly() {
    let src = "pub enum Role{\nAdmin\n\n\nMember(Int)\n Guest\n}\n";
    let expected = "\
pub enum Role {
  Admin
  Member(Int)
  Guest
}
";
    assert_eq!(fmt(src), expected);
}

#[test]
fn empty_bodies_collapse() {
    let src = "pub class A {\n}\n\npub enum B {\n}\n\ntest class T {\n}\n";
    let expected = "pub class A {}\n\npub enum B {}\n\ntest class T {}\n";
    assert_eq!(fmt(src), expected);
}

#[test]
fn imports_then_blank_line() {
    let src = "import B from \"./b.lt\"\nimport A from \"./a.lt\"\npub class C { x: Int }\n";
    let expected = "\
import B from \"./b.lt\"
import A from \"./a.lt\"

pub class C {
  x: Int
}
";
    assert_eq!(fmt(src), expected);
}

#[test]
fn preserves_operator_grouping() {
    let src = "pub class A {\n  fn f(self) -> Int {\n    (1 + 2) * 3\n  }\n}\n";
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
    let src = format!("pub class A {{\n  fn f(self) {{\n    do_something({long_args})\n  }}\n}}\n");
    let out = fmt(&src);
    assert!(out.contains("do_something(\n"), "{out}");
    assert!(out.contains("argument_11,\n"), "{out}");
    assert_stable(&src);
}

#[test]
fn jsx_inline_and_multiline() {
    let inline = "pub class V {\n  fn f(self) -> Element {\n    <Element tag=\"div\" />\n  }\n}\n";
    let out = fmt(inline);
    assert!(out.contains("<Element tag=\"div\" />"), "{out}");

    let nested = "pub class V {\n  fn f(self) -> Element {\n    <Element tag=\"div\"><Element tag=\"span\">{name}</Element><Element tag=\"span\">{age}</Element></Element>\n  }\n}\n";
    let out = fmt(nested);
    assert!(out.contains("<Element tag=\"div\">\n"), "{out}");
    assert_stable(nested);
}

#[test]
fn match_arms_and_patterns_round_trip() {
    let src = "\
pub class A {
  fn f(self) -> Int {
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
    let src = "pub class A {\n  fn f(self) -> Str {\n    `a\\nb \\${not} ${x}`\n  }\n}\n";
    let once = fmt(src);
    let twice = fmt(&once);
    assert_eq!(once, twice);
    assert!(once.contains("\\${not}"), "{once}");
}

#[test]
fn tagged_template_has_no_space() {
    let src = "pub class A {\n  fn f(self) -> Q {\n    Sql.query`id=${id}`\n  }\n}\n";
    assert!(fmt(src).contains("Sql.query`id=${id}`"));
}

#[test]
fn healthy_program_is_stable() {
    let src = r#"
import User from "./user.lt"

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
}
"#;
    assert_stable(src);
    let once = fmt(src);
    assert!(once.contains("if u.age < 0 {"), "{once}");
    assert!(once.contains("} else {"), "{once}");
}

#[test]
fn comments_are_reported_by_the_lexer() {
    let out = lt_syntax::tokenize_full("// hello\npub class A { x: Int }\n");
    assert_eq!(out.comments.len(), 1);
    let span = out.comments[0];
    assert_eq!(span.text("// hello\npub class A { x: Int }\n"), "// hello");
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
import B from \"./b.lt\" // inline is rejected later, keep out

// the class
pub class A {
  // a field
  x: Int

  // a method
  pub fn f(self) -> Int {
    // body comment
    self.x
  }
}
";
    let src = src.replace(
        "import B from \"./b.lt\" // inline is rejected later, keep out",
        "import B from \"./b.lt\"",
    );
    let once = fmt_comments(&src);
    assert!(once.contains("// file header"), "{once}");
    assert!(once.contains("// the class"), "{once}");
    assert!(once.contains("  // a field"), "{once}");
    assert!(once.contains("  // a method"), "{once}");
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
pub enum Role {
  // admins
  Admin
  // everyone else
  Guest
}

pub class A {
  fn label(self, r: Role) -> Str {
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
    assert!(once.contains("  // admins\n  Admin"), "{once}");
    assert!(once.contains("      // the admin branch"), "{once}");
    assert_eq!(once, fmt_comments(&once));
}

#[test]
fn keeps_trailing_comments_before_closing_braces() {
    let src = "\
pub class A {
  x: Int

  pub fn f(self) -> Int {
    self.x
    // last thought
  }
  // end of class
}
";
    let once = fmt_comments(src);
    assert!(once.contains("// last thought"), "{once}");
    assert!(once.contains("// end of class"), "{once}");
    assert_eq!(once, fmt_comments(&once));
}

#[test]
fn normalizes_block_comments() {
    let src = "\
/*
   multi
   line
*/
pub class A {
  x: Int
}
";
    let once = fmt_comments(src);
    assert!(once.starts_with("/*\n * multi\n * line\n */\n"), "{once}");
    assert_eq!(once, fmt_comments(&once));
}

#[test]
fn rejects_inline_comments() {
    let src = "pub class A { x: Int } // trailing\n";
    let lexed = lt_syntax::tokenize_full(src);
    let out = parse(src);
    let error = lt_syntax::format_file_with_comments(&out.file, src, &lexed.comments).unwrap_err();
    assert!(error.message.contains("inline comments"), "{error:?}");
}
