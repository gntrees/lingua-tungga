//! Tokens and lexer diagnostics.

use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Literals & identifiers
    Int,
    Float,
    Str(String),
    Ident(String),

    // Keywords
    Package,
    Export,
    Struct,
    As,
    // Reserved: removed in v2 (`package` + `struct`, `export`, receiver params).
    // The lexer still produces these so the parser can reject them with a
    // message that names the replacement.
    Pub,
    Class,
    Test,
    Enum,
    Fn,
    Static,
    SelfKw,
    Let,
    Mut,
    Return,
    Break,
    Continue,
    If,
    Else,
    For,
    In,
    While,
    Match,
    Import,
    From,
    True,
    False,
    NoneKw,
    Underscore,

    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    Question,
    Semicolon,
    Arrow,
    FatArrow,
    Eq,
    EqEq,
    BangEq,
    Bang,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AmpAmp,
    Pipe,
    PipePipe,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    // Template literals
    TemplateStart,
    TemplateText(String),
    TemplateEnd,
    InterpStart,
    InterpEnd,

    // JSX
    JsxTagOpen,
    JsxTagCloseOpen,
    JsxTagEnd,
    JsxTagSelfClose,
    JsxText(String),

    Eof,
}

impl TokenKind {
    /// Short human-readable name used in parser diagnostics.
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Int => "an integer literal".into(),
            TokenKind::Float => "a float literal".into(),
            TokenKind::Str(_) => "a string literal".into(),
            TokenKind::Ident(name) => format!("identifier `{name}`"),
            TokenKind::TemplateStart => "` (template start)".into(),
            TokenKind::TemplateText(_) => "template text".into(),
            TokenKind::TemplateEnd => "` (template end)".into(),
            TokenKind::InterpStart => "`${`".into(),
            TokenKind::InterpEnd => "`}`".into(),
            TokenKind::JsxTagOpen => "`<`".into(),
            TokenKind::JsxTagCloseOpen => "`</`".into(),
            TokenKind::JsxTagEnd => "`>`".into(),
            TokenKind::JsxTagSelfClose => "`/>`".into(),
            TokenKind::JsxText(_) => "JSX text".into(),
            TokenKind::Eof => "end of file".into(),
            other => format!("`{}`", other.spelling()),
        }
    }

    /// Fixed spelling for punctuation/keywords; empty for value-carrying tokens.
    pub fn spelling(&self) -> &'static str {
        match self {
            TokenKind::Package => "package",
            TokenKind::Export => "export",
            TokenKind::Struct => "struct",
            TokenKind::As => "as",
            TokenKind::Pub => "pub",
            TokenKind::Class => "class",
            TokenKind::Test => "test",
            TokenKind::Enum => "enum",
            TokenKind::Fn => "fn",
            TokenKind::Static => "static",
            TokenKind::SelfKw => "self",
            TokenKind::Let => "let",
            TokenKind::Mut => "mut",
            TokenKind::Return => "return",
            TokenKind::Break => "break",
            TokenKind::Continue => "continue",
            TokenKind::If => "if",
            TokenKind::Else => "else",
            TokenKind::For => "for",
            TokenKind::In => "in",
            TokenKind::While => "while",
            TokenKind::Match => "match",
            TokenKind::Import => "import",
            TokenKind::From => "from",
            TokenKind::True => "true",
            TokenKind::False => "false",
            TokenKind::NoneKw => "None",
            TokenKind::Underscore => "_",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
            TokenKind::LBrace => "{",
            TokenKind::RBrace => "}",
            TokenKind::LBracket => "[",
            TokenKind::RBracket => "]",
            TokenKind::Comma => ",",
            TokenKind::Colon => ":",
            TokenKind::Dot => ".",
            TokenKind::Question => "?",
            TokenKind::Semicolon => ";",
            TokenKind::Arrow => "->",
            TokenKind::FatArrow => "=>",
            TokenKind::Eq => "=",
            TokenKind::EqEq => "==",
            TokenKind::BangEq => "!=",
            TokenKind::Bang => "!",
            TokenKind::Lt => "<",
            TokenKind::LtEq => "<=",
            TokenKind::Gt => ">",
            TokenKind::GtEq => ">=",
            TokenKind::AmpAmp => "&&",
            TokenKind::Pipe => "|",
            TokenKind::PipePipe => "||",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Percent => "%",
            _ => "",
        }
    }

    /// True for the reserved words. `_` is reserved too, but it is not a word,
    /// so it is deliberately excluded: it can never stand in for a name.
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            TokenKind::Package
                | TokenKind::Export
                | TokenKind::Struct
                | TokenKind::As
                | TokenKind::Pub
                | TokenKind::Class
                | TokenKind::Test
                | TokenKind::Enum
                | TokenKind::Fn
                | TokenKind::Static
                | TokenKind::SelfKw
                | TokenKind::Let
                | TokenKind::Mut
                | TokenKind::Return
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::If
                | TokenKind::Else
                | TokenKind::For
                | TokenKind::In
                | TokenKind::While
                | TokenKind::Match
                | TokenKind::Import
                | TokenKind::From
                | TokenKind::True
                | TokenKind::False
                | TokenKind::NoneKw
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub line: u32,
    pub col: u32,
}

/// Lexing errors are ordinary diagnostics.
pub type LexError = crate::diagnostics::Diagnostic;
