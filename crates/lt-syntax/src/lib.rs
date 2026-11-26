//! Lingua Tungga syntax frontend.
//!
//! M1 scope: lexer first. Parser and AST land on top of the token stream.

pub mod lexer;
pub mod span;
pub mod token;

pub use lexer::Lexer;
pub use span::Span;
pub use token::{LexError, Token, TokenKind};

/// Tokenize an entire source file.
///
/// Always returns at least one token ([`TokenKind::Eof`]). Lexing is
/// error-tolerant: every error is collected and the lexer keeps going.
pub fn tokenize(src: &str) -> (Vec<Token>, Vec<LexError>) {
    Lexer::new(src).run()
}
