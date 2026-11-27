use lt_syntax::{tokenize, TokenKind};

fn kinds(src: &str) -> Vec<TokenKind> {
    let (tokens, errors) = tokenize(src);
    assert!(errors.is_empty(), "unexpected lex errors: {errors:#?}");
    tokens
        .into_iter()
        .map(|t| t.kind)
        .filter(|k| *k != TokenKind::Eof)
        .collect()
}

fn text(s: &str) -> TokenKind {
    TokenKind::TemplateText(s.to_string())
}

#[test]
fn integers_with_separators() {
    assert_eq!(kinds("1 1_000_000 42"), vec![TokenKind::Int; 3]);
}

#[test]
fn floats_and_exponents() {
    assert_eq!(
        kinds("3.14 1.0e10 2.5E-3"),
        vec![TokenKind::Float, TokenKind::Float, TokenKind::Float]
    );
    // `1.` is not a float: the dot is field access.
    assert_eq!(
        kinds("1 .x"),
        vec![TokenKind::Int, TokenKind::Dot, TokenKind::Ident("x".into())]
    );
}

#[test]
fn punctuation_and_arrows() {
    assert_eq!(
        kinds("-> => == != ! <= >= < > && || ? . , : = + - * / %"),
        vec![
            TokenKind::Arrow,
            TokenKind::FatArrow,
            TokenKind::EqEq,
            TokenKind::BangEq,
            TokenKind::Bang,
            TokenKind::LtEq,
            TokenKind::GtEq,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::AmpAmp,
            TokenKind::PipePipe,
            TokenKind::Question,
            TokenKind::Dot,
            TokenKind::Comma,
            TokenKind::Colon,
            TokenKind::Eq,
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
        ]
    );
}

#[test]
fn keywords() {
    assert_eq!(
        kinds(
            "pub class test enum fn static self let mut return if else for in while match \
             import from true false None"
        ),
        vec![
            TokenKind::Pub,
            TokenKind::Class,
            TokenKind::Test,
            TokenKind::Enum,
            TokenKind::Fn,
            TokenKind::Static,
            TokenKind::SelfKw,
            TokenKind::Let,
            TokenKind::Mut,
            TokenKind::Return,
            TokenKind::If,
            TokenKind::Else,
            TokenKind::For,
            TokenKind::In,
            TokenKind::While,
            TokenKind::Match,
            TokenKind::Import,
            TokenKind::From,
            TokenKind::True,
            TokenKind::False,
            TokenKind::NoneKw,
        ]
    );
}

#[test]
fn identifiers_are_not_keywords_by_prefix() {
    assert_eq!(
        kinds("publisher classy _x _"),
        vec![
            TokenKind::Ident("publisher".into()),
            TokenKind::Ident("classy".into()),
            TokenKind::Ident("_x".into()),
            TokenKind::Underscore,
        ]
    );
}

#[test]
fn string_escapes() {
    assert_eq!(
        kinds(r#""a\nb\t\\\"\`\$\u{1F600}""#),
        vec![TokenKind::Str("a\nb\t\\\"`$😀".into())]
    );
}

#[test]
fn comments_are_skipped() {
    assert_eq!(
        kinds("// line\nlet x = 1 /* inline */ + 2"),
        vec![
            TokenKind::Let,
            TokenKind::Ident("x".into()),
            TokenKind::Eq,
            TokenKind::Int,
            TokenKind::Plus,
            TokenKind::Int,
        ]
    );
}

#[test]
fn empty_template() {
    assert_eq!(
        kinds("``"),
        vec![TokenKind::TemplateStart, TokenKind::TemplateEnd]
    );
}

#[test]
fn template_with_interpolation() {
    assert_eq!(
        kinds("`a ${x} b`"),
        vec![
            TokenKind::TemplateStart,
            text("a "),
            TokenKind::InterpStart,
            TokenKind::Ident("x".into()),
            TokenKind::InterpEnd,
            text(" b"),
            TokenKind::TemplateEnd,
        ]
    );
}

#[test]
fn nested_template() {
    assert_eq!(
        kinds("`a ${`b ${x}` } c`"),
        vec![
            TokenKind::TemplateStart,
            text("a "),
            TokenKind::InterpStart,
            TokenKind::TemplateStart,
            text("b "),
            TokenKind::InterpStart,
            TokenKind::Ident("x".into()),
            TokenKind::InterpEnd,
            TokenKind::TemplateEnd,
            TokenKind::InterpEnd,
            text(" c"),
            TokenKind::TemplateEnd,
        ]
    );
}

#[test]
fn template_escaped_dollar_is_text() {
    assert_eq!(
        kinds(r#"`cost: \${x}`"#),
        vec![
            TokenKind::TemplateStart,
            text("cost: ${x}"),
            TokenKind::TemplateEnd,
        ]
    );
}

#[test]
fn tagged_template() {
    assert_eq!(
        kinds("Sql.query`id=${id}`"),
        vec![
            TokenKind::Ident("Sql".into()),
            TokenKind::Dot,
            TokenKind::Ident("query".into()),
            TokenKind::TemplateStart,
            text("id="),
            TokenKind::InterpStart,
            TokenKind::Ident("id".into()),
            TokenKind::InterpEnd,
            TokenKind::TemplateEnd,
        ]
    );
}

#[test]
fn jsx_self_closing() {
    assert_eq!(
        kinds(r#"let e = <Element tag="div" />"#),
        vec![
            TokenKind::Let,
            TokenKind::Ident("e".into()),
            TokenKind::Eq,
            TokenKind::JsxTagOpen,
            TokenKind::Ident("Element".into()),
            TokenKind::Ident("tag".into()),
            TokenKind::Eq,
            TokenKind::Str("div".into()),
            TokenKind::JsxTagSelfClose,
        ]
    );
}

#[test]
fn jsx_with_expression_child() {
    assert_eq!(
        kinds("let e = <Element tag=\"div\">{x}</Element>"),
        vec![
            TokenKind::Let,
            TokenKind::Ident("e".into()),
            TokenKind::Eq,
            TokenKind::JsxTagOpen,
            TokenKind::Ident("Element".into()),
            TokenKind::Ident("tag".into()),
            TokenKind::Eq,
            TokenKind::Str("div".into()),
            TokenKind::JsxTagEnd,
            TokenKind::LBrace,
            TokenKind::Ident("x".into()),
            TokenKind::RBrace,
            TokenKind::JsxTagCloseOpen,
            TokenKind::Ident("Element".into()),
            TokenKind::JsxTagEnd,
        ]
    );
}

#[test]
fn jsx_text_child_and_nesting() {
    assert_eq!(
        kinds("x = <A>hello<B/></A>"),
        vec![
            TokenKind::Ident("x".into()),
            TokenKind::Eq,
            TokenKind::JsxTagOpen,
            TokenKind::Ident("A".into()),
            TokenKind::JsxTagEnd,
            TokenKind::JsxText("hello".into()),
            TokenKind::JsxTagOpen,
            TokenKind::Ident("B".into()),
            TokenKind::JsxTagSelfClose,
            TokenKind::JsxTagCloseOpen,
            TokenKind::Ident("A".into()),
            TokenKind::JsxTagEnd,
        ]
    );
}

#[test]
fn generic_arguments_are_not_jsx() {
    assert_eq!(
        kinds("let x: List<Int> = y"),
        vec![
            TokenKind::Let,
            TokenKind::Ident("x".into()),
            TokenKind::Colon,
            TokenKind::Ident("List".into()),
            TokenKind::Lt,
            TokenKind::Ident("Int".into()),
            TokenKind::Gt,
            TokenKind::Eq,
            TokenKind::Ident("y".into()),
        ]
    );
}

#[test]
fn comparison_is_not_jsx() {
    assert_eq!(
        kinds("a < b"),
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::Lt,
            TokenKind::Ident("b".into()),
        ]
    );
}

#[test]
fn jsx_is_allowed_after_return() {
    let ks = kinds("return <A/>");
    assert_eq!(ks[0], TokenKind::Return);
    assert_eq!(ks[1], TokenKind::JsxTagOpen);
}

#[test]
fn jsx_expression_with_nested_object_like_braces() {
    // `{ a { b } }` in an expression position must track brace depth.
    assert_eq!(
        kinds("x = <A>{ a { b } }</A>"),
        vec![
            TokenKind::Ident("x".into()),
            TokenKind::Eq,
            TokenKind::JsxTagOpen,
            TokenKind::Ident("A".into()),
            TokenKind::JsxTagEnd,
            TokenKind::LBrace,
            TokenKind::Ident("a".into()),
            TokenKind::LBrace,
            TokenKind::Ident("b".into()),
            TokenKind::RBrace,
            TokenKind::RBrace,
            TokenKind::JsxTagCloseOpen,
            TokenKind::Ident("A".into()),
            TokenKind::JsxTagEnd,
        ]
    );
}

#[test]
fn tracks_line_and_column() {
    let (tokens, errors) = tokenize("let x = 1\nlet y = 2");
    assert!(errors.is_empty());
    let second_let = tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Let)
        .nth(1)
        .expect("second let");
    assert_eq!((second_let.line, second_let.col), (2, 1));
    let one = &tokens[3];
    assert_eq!(one.kind, TokenKind::Int);
    assert_eq!((one.line, one.col), (1, 9));
}

#[test]
fn empty_source_has_only_eof() {
    let (tokens, errors) = tokenize("");
    assert!(errors.is_empty());
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, TokenKind::Eof);
}

#[test]
fn semicolon_is_lexed_so_the_parser_can_reject_it() {
    assert_eq!(
        kinds("let x = 1;"),
        vec![
            TokenKind::Let,
            TokenKind::Ident("x".into()),
            TokenKind::Eq,
            TokenKind::Int,
            TokenKind::Semicolon,
        ]
    );
}

#[test]
fn error_unterminated_string() {
    let (_, errors) = tokenize("let s = \"abc");
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].message.contains("unterminated string"));
    assert_eq!((errors[0].line, errors[0].col), (1, 9));
}

#[test]
fn error_invalid_escape() {
    let (_, errors) = tokenize(r#""a\qb""#);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].message.contains("invalid escape"));
}

#[test]
fn error_unterminated_template() {
    let (_, errors) = tokenize("`abc");
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].message.contains("unterminated template"));
}

#[test]
fn error_unterminated_jsx() {
    let (_, errors) = tokenize("x = <A>");
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].message.contains("unterminated JSX"));
}

#[test]
fn error_unterminated_block_comment() {
    let (_, errors) = tokenize("/* forever");
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].message.contains("unterminated block comment"));
}

#[test]
fn error_unexpected_character() {
    let (_, errors) = tokenize("let x = @");
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].message.contains("unexpected character"));
}

#[test]
fn error_single_ampersand() {
    let (_, errors) = tokenize("a & b");
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].message.contains("expected `&&`"));
}

#[test]
fn diagnostic_renders_with_caret() {
    let src = "let s = \"abc";
    let (_, errors) = tokenize(src);
    let rendered = errors[0].render("main.lt", src);
    assert!(rendered.contains("main.lt:1:9"), "{rendered}");
    assert!(rendered.contains('^'), "{rendered}");
}
