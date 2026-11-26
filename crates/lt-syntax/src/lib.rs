//! Lingua Tungga syntax frontend.
//!
//! M1 scope: lexer first. Parser and AST land on top of the token stream.

pub mod ast;
pub mod diagnostics;
pub mod format;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod token;

pub use ast::*;
pub use diagnostics::{Diagnostic, Severity};
pub use format::format_file;
pub use lexer::{LexOutput, Lexer};
pub use parser::{parse, ParseOutput};
pub use span::Span;
pub use token::{LexError, Token, TokenKind};

/// Tokenize an entire source file.
///
/// Always returns at least one token ([`TokenKind::Eof`]). Lexing is
/// error-tolerant: every error is collected and the lexer keeps going.
pub fn tokenize(src: &str) -> (Vec<Token>, Vec<LexError>) {
    Lexer::new(src).run()
}

/// Tokenize and also report the spans of comments, which the token stream
/// drops. Formatters use this to avoid silently deleting comments.
pub fn tokenize_full(src: &str) -> LexOutput {
    Lexer::new(src).run_full()
}
