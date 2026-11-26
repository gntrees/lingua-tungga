//! Hand-written, error-tolerant lexer.
//!
//! The lexer is **mode driven**: template literals, `${}` interpolations and
//! JSX run in nested modes tracked on a stack. That is what lets `}` mean
//! three different things (block end, interpolation end, JSX expression end)
//! without the parser having to re-lex anything.

use crate::span::Span;
use crate::token::{LexError, Token, TokenKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Closer {
    /// `}` closes a `${ ... }` interpolation.
    InterpEnd,
    /// `}` closes a `{ ... }` JSX child/attribute expression.
    RBrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    /// Inside template text, between `` ` `` and `${`/`` ` ``.
    Template,
    /// Inside an expression. `depth` counts nested `{` so `}` is unambiguous.
    Interp { depth: u32, closer: Closer },
    /// Inside a JSX tag: `<name attr="v">`, `</name>`, `<name/>`.
    JsxTag { closing: bool },
    /// Between a JSX opening tag and its matching close tag.
    JsxChildren,
}

#[derive(Debug, Clone, Copy)]
struct Mark {
    pos: usize,
    line: u32,
    col: u32,
}

pub struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    line: u32,
    col: u32,
    modes: Vec<Mode>,
    prev_sig: Option<TokenKind>,
    errors: Vec<LexError>,
    comments: Vec<Span>,
}

/// Full lexer output, including the spans of skipped comments.
pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub errors: Vec<LexError>,
    pub comments: Vec<Span>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            pos: 0,
            line: 1,
            col: 1,
            modes: Vec::new(),
            prev_sig: None,
            errors: Vec::new(),
            comments: Vec::new(),
        }
    }

    pub fn run(self) -> (Vec<Token>, Vec<LexError>) {
        let output = self.run_full();
        (output.tokens, output.errors)
    }

    pub fn run_full(mut self) -> LexOutput {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = token.kind == TokenKind::Eof;
            if !is_eof {
                self.prev_sig = Some(token.kind.clone());
            }
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        LexOutput {
            tokens,
            errors: self.errors,
            comments: self.comments,
        }
    }

    // ---------------------------------------------------------------- modes

    fn next_token(&mut self) -> Token {
        match self.modes.last().cloned() {
            Some(Mode::Template) => self.lex_template_part(),
            Some(Mode::JsxChildren) => self.lex_jsx_children(),
            Some(Mode::JsxTag { closing }) => self.lex_jsx_tag(closing),
            _ => self.lex_normal(),
        }
    }

    // -------------------------------------------------------------- char io

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.src[self.pos..].chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn bump_n(&mut self, n: usize) {
        for _ in 0..n {
            self.bump();
        }
    }

    fn mark(&self) -> Mark {
        Mark {
            pos: self.pos,
            line: self.line,
            col: self.col,
        }
    }

    fn emit(&self, kind: TokenKind, mark: Mark) -> Token {
        Token {
            kind,
            span: Span::new(mark.pos, self.pos),
            line: mark.line,
            col: mark.col,
        }
    }

    fn emit_eof(&self) -> Token {
        let mark = self.mark();
        self.emit(TokenKind::Eof, mark)
    }

    fn error_at(&mut self, mark: Mark, message: impl Into<String>) {
        self.errors.push(LexError::error(
            message,
            Span::new(mark.pos, self.pos.max(mark.pos)),
            mark.line,
            mark.col,
        ));
    }

    // ----------------------------------------------------------- trivia

    /// Skip whitespace and comments. Returns false when at EOF.
    fn skip_trivia(&mut self) -> bool {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek_at(1) == Some('/') => {
                    let start = self.mark();
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                    self.comments.push(Span::new(start.pos, self.pos));
                }
                Some('/') if self.peek_at(1) == Some('*') => {
                    let start = self.mark();
                    self.bump_n(2);
                    loop {
                        match self.peek() {
                            None => {
                                self.error_at(start, "unterminated block comment");
                                return false;
                            }
                            Some('*') if self.peek_at(1) == Some('/') => {
                                self.bump_n(2);
                                break;
                            }
                            _ => {
                                self.bump();
                            }
                        }
                    }
                    self.comments.push(Span::new(start.pos, self.pos));
                }
                _ => return self.peek().is_some(),
            }
        }
    }

    // ------------------------------------------------------------- normal

    fn lex_normal(&mut self) -> Token {
        if !self.skip_trivia() {
            return self.emit_eof();
        }

        let mark = self.mark();
        let c = self.peek().expect("checked above");

        match c {
            '"' => self.lex_string(mark),
            '`' => {
                self.bump();
                self.modes.push(Mode::Template);
                self.emit(TokenKind::TemplateStart, mark)
            }
            c if c.is_ascii_digit() => self.lex_number(mark),
            c if is_ident_start(c) => self.lex_ident(mark),
            '<' if self.jsx_allowed_here() && self.peek_at(1).is_some_and(is_ident_start) => {
                self.bump();
                self.modes.push(Mode::JsxTag { closing: false });
                self.emit(TokenKind::JsxTagOpen, mark)
            }
            _ => self.lex_punct(mark),
        }
    }

    /// JSX may only start where an expression may start. Without this, `a < b`
    /// and `List<Int>` would be misread as the beginning of an element.
    fn jsx_allowed_here(&self) -> bool {
        match &self.prev_sig {
            None => true,
            Some(kind) => matches!(
                kind,
                TokenKind::Eq
                    | TokenKind::FatArrow
                    | TokenKind::LParen
                    | TokenKind::LBracket
                    | TokenKind::LBrace
                    | TokenKind::Comma
                    | TokenKind::Return
                    | TokenKind::InterpStart
            ),
        }
    }

    fn lex_punct(&mut self, mark: Mark) -> Token {
        let c = self.peek().expect("checked by caller");
        match c {
            '{' => {
                self.bump();
                if let Some(Mode::Interp { depth, .. }) = self.modes.last_mut() {
                    *depth += 1;
                }
                self.emit(TokenKind::LBrace, mark)
            }
            '}' => {
                let top = self.modes.last().cloned();
                if let Some(Mode::Interp { depth, closer }) = top {
                    if depth == 0 {
                        self.modes.pop();
                        self.bump();
                        let kind = match closer {
                            Closer::InterpEnd => TokenKind::InterpEnd,
                            Closer::RBrace => TokenKind::RBrace,
                        };
                        return self.emit(kind, mark);
                    }
                    if let Some(Mode::Interp { depth, .. }) = self.modes.last_mut() {
                        *depth -= 1;
                    }
                    self.bump();
                    return self.emit(TokenKind::RBrace, mark);
                }
                self.bump();
                self.emit(TokenKind::RBrace, mark)
            }
            '-' => {
                if self.peek_at(1) == Some('>') {
                    self.bump_n(2);
                    self.emit(TokenKind::Arrow, mark)
                } else {
                    self.bump();
                    self.emit(TokenKind::Minus, mark)
                }
            }
            '=' => {
                if self.peek_at(1) == Some('>') {
                    self.bump_n(2);
                    self.emit(TokenKind::FatArrow, mark)
                } else if self.peek_at(1) == Some('=') {
                    self.bump_n(2);
                    self.emit(TokenKind::EqEq, mark)
                } else {
                    self.bump();
                    self.emit(TokenKind::Eq, mark)
                }
            }
            '!' => {
                if self.peek_at(1) == Some('=') {
                    self.bump_n(2);
                    self.emit(TokenKind::BangEq, mark)
                } else {
                    self.bump();
                    self.emit(TokenKind::Bang, mark)
                }
            }
            '<' => {
                if self.peek_at(1) == Some('=') {
                    self.bump_n(2);
                    self.emit(TokenKind::LtEq, mark)
                } else {
                    self.bump();
                    self.emit(TokenKind::Lt, mark)
                }
            }
            '>' => {
                if self.peek_at(1) == Some('=') {
                    self.bump_n(2);
                    self.emit(TokenKind::GtEq, mark)
                } else {
                    self.bump();
                    self.emit(TokenKind::Gt, mark)
                }
            }
            '&' => {
                if self.peek_at(1) == Some('&') {
                    self.bump_n(2);
                    self.emit(TokenKind::AmpAmp, mark)
                } else {
                    self.bump();
                    self.error_at(mark, "expected `&&`, found `&`");
                    self.lex_normal()
                }
            }
            '|' => {
                if self.peek_at(1) == Some('|') {
                    self.bump_n(2);
                    self.emit(TokenKind::PipePipe, mark)
                } else {
                    self.bump();
                    self.error_at(mark, "expected `||`, found `|`");
                    self.lex_normal()
                }
            }
            '(' => {
                self.bump();
                self.emit(TokenKind::LParen, mark)
            }
            ')' => {
                self.bump();
                self.emit(TokenKind::RParen, mark)
            }
            '[' => {
                self.bump();
                self.emit(TokenKind::LBracket, mark)
            }
            ']' => {
                self.bump();
                self.emit(TokenKind::RBracket, mark)
            }
            ',' => {
                self.bump();
                self.emit(TokenKind::Comma, mark)
            }
            ':' => {
                self.bump();
                self.emit(TokenKind::Colon, mark)
            }
            '.' => {
                self.bump();
                self.emit(TokenKind::Dot, mark)
            }
            '?' => {
                self.bump();
                self.emit(TokenKind::Question, mark)
            }
            ';' => {
                self.bump();
                self.emit(TokenKind::Semicolon, mark)
            }
            '+' => {
                self.bump();
                self.emit(TokenKind::Plus, mark)
            }
            '*' => {
                self.bump();
                self.emit(TokenKind::Star, mark)
            }
            '/' => {
                self.bump();
                self.emit(TokenKind::Slash, mark)
            }
            '%' => {
                self.bump();
                self.emit(TokenKind::Percent, mark)
            }
            other => {
                self.bump();
                self.error_at(mark, format!("unexpected character `{other}`"));
                self.lex_normal()
            }
        }
    }

    fn lex_number(&mut self, mark: Mark) -> Token {
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                self.bump();
            } else {
                break;
            }
        }

        let mut is_float = false;
        if self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            is_float = true;
            self.bump();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || c == '_' {
                    self.bump();
                } else {
                    break;
                }
            }
            if matches!(self.peek(), Some('e') | Some('E')) {
                let save = self.mark();
                self.bump();
                if matches!(self.peek(), Some('+') | Some('-')) {
                    self.bump();
                }
                if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() || c == '_' {
                            self.bump();
                        } else {
                            break;
                        }
                    }
                } else {
                    self.error_at(save, "expected digits after exponent marker");
                }
            }
        }

        if self.src[mark.pos..self.pos].ends_with('_') {
            self.error_at(mark, "numeric literal may not end with `_`");
        }

        self.emit(
            if is_float {
                TokenKind::Float
            } else {
                TokenKind::Int
            },
            mark,
        )
    }

    fn lex_ident(&mut self, mark: Mark) -> Token {
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                self.bump();
            } else {
                break;
            }
        }

        let text = &self.src[mark.pos..self.pos];
        let kind = match text {
            "pub" => TokenKind::Pub,
            "class" => TokenKind::Class,
            "test" => TokenKind::Test,
            "enum" => TokenKind::Enum,
            "fn" => TokenKind::Fn,
            "static" => TokenKind::Static,
            "self" => TokenKind::SelfKw,
            "let" => TokenKind::Let,
            "mut" => TokenKind::Mut,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "while" => TokenKind::While,
            "match" => TokenKind::Match,
            "import" => TokenKind::Import,
            "from" => TokenKind::From,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "None" => TokenKind::NoneKw,
            "_" => TokenKind::Underscore,
            _ => TokenKind::Ident(text.to_string()),
        };
        self.emit(kind, mark)
    }

    fn lex_string(&mut self, mark: Mark) -> Token {
        self.bump(); // opening quote
        let mut out = String::new();
        loop {
            match self.peek() {
                None => {
                    self.error_at(mark, "unterminated string literal");
                    return self.emit_eof();
                }
                Some('"') => {
                    self.bump();
                    return self.emit(TokenKind::Str(out), mark);
                }
                Some('\n') => {
                    self.error_at(mark, "string literal may not contain a raw newline");
                    return self.emit(TokenKind::Str(out), mark);
                }
                Some('\\') => {
                    self.bump();
                    if !self.lex_escape(&mut out, mark) {
                        return self.emit_eof();
                    }
                }
                Some(c) => {
                    out.push(c);
                    self.bump();
                }
            }
        }
    }

    /// Decode one escape sequence (the backslash is already consumed).
    /// Returns false when the input ended mid-escape.
    fn lex_escape(&mut self, out: &mut String, mark: Mark) -> bool {
        let Some(c) = self.peek() else {
            self.error_at(mark, "unterminated escape sequence");
            return false;
        };
        match c {
            'n' => {
                out.push('\n');
                self.bump();
            }
            't' => {
                out.push('\t');
                self.bump();
            }
            'r' => {
                out.push('\r');
                self.bump();
            }
            '\\' => {
                out.push('\\');
                self.bump();
            }
            '"' => {
                out.push('"');
                self.bump();
            }
            '`' => {
                out.push('`');
                self.bump();
            }
            '$' => {
                out.push('$');
                self.bump();
            }
            'u' => {
                let esc = self.mark();
                self.bump();
                if self.peek() != Some('{') {
                    self.error_at(esc, "expected `{` after `\\u`");
                    return true;
                }
                self.bump();
                let mut hex = String::new();
                while let Some(c) = self.peek() {
                    if c == '}' {
                        break;
                    }
                    hex.push(c);
                    self.bump();
                }
                if self.peek() != Some('}') {
                    self.error_at(esc, "unterminated unicode escape");
                    return true;
                }
                self.bump();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(ch) => out.push(ch),
                    None => self.error_at(esc, "invalid unicode escape"),
                }
            }
            other => {
                self.error_at(mark, format!("invalid escape `\\{other}`"));
                out.push(other);
                self.bump();
            }
        }
        true
    }

    // ------------------------------------------------------------ template

    fn lex_template_part(&mut self) -> Token {
        let mark = self.mark();
        let mut text = String::new();
        loop {
            match self.peek() {
                None => {
                    self.error_at(mark, "unterminated template literal");
                    self.modes.clear();
                    return self.emit_eof();
                }
                Some('`') => {
                    if !text.is_empty() {
                        return self.emit(TokenKind::TemplateText(text), mark);
                    }
                    self.bump();
                    self.modes.pop();
                    return self.emit(TokenKind::TemplateEnd, mark);
                }
                Some('$') if self.peek_at(1) == Some('{') => {
                    if !text.is_empty() {
                        return self.emit(TokenKind::TemplateText(text), mark);
                    }
                    self.bump_n(2);
                    self.modes.push(Mode::Interp {
                        depth: 0,
                        closer: Closer::InterpEnd,
                    });
                    return self.emit(TokenKind::InterpStart, mark);
                }
                Some('\\') => {
                    self.bump();
                    if !self.lex_escape(&mut text, mark) {
                        self.modes.clear();
                        return self.emit_eof();
                    }
                }
                Some(c) => {
                    text.push(c);
                    self.bump();
                }
            }
        }
    }

    // ----------------------------------------------------------------- jsx

    fn lex_jsx_tag(&mut self, closing: bool) -> Token {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                _ => break,
            }
        }

        let mark = self.mark();
        match self.peek() {
            None => {
                self.error_at(mark, "unterminated JSX tag");
                self.modes.clear();
                self.emit_eof()
            }
            Some('/') if self.peek_at(1) == Some('>') => {
                self.bump_n(2);
                self.modes.pop();
                self.emit(TokenKind::JsxTagSelfClose, mark)
            }
            Some('>') => {
                self.bump();
                self.modes.pop();
                if !closing {
                    self.modes.push(Mode::JsxChildren);
                } else {
                    // The element's children mode sits directly below.
                    if matches!(self.modes.last(), Some(Mode::JsxChildren)) {
                        self.modes.pop();
                    }
                }
                self.emit(TokenKind::JsxTagEnd, mark)
            }
            Some('{') => {
                self.bump();
                self.modes.push(Mode::Interp {
                    depth: 0,
                    closer: Closer::RBrace,
                });
                self.emit(TokenKind::LBrace, mark)
            }
            Some('"') => self.lex_string(mark),
            Some(c) if is_ident_start(c) => self.lex_ident(mark),
            Some('=') => {
                self.bump();
                self.emit(TokenKind::Eq, mark)
            }
            Some(c) => {
                self.bump();
                self.error_at(mark, format!("unexpected character `{c}` in JSX tag"));
                self.lex_jsx_tag(closing)
            }
        }
    }

    fn lex_jsx_children(&mut self) -> Token {
        let mark = self.mark();
        match self.peek() {
            None => {
                self.error_at(mark, "unterminated JSX element");
                self.modes.clear();
                self.emit_eof()
            }
            Some('{') => {
                self.bump();
                self.modes.push(Mode::Interp {
                    depth: 0,
                    closer: Closer::RBrace,
                });
                self.emit(TokenKind::LBrace, mark)
            }
            Some('<') => {
                if self.peek_at(1) == Some('/') {
                    self.bump_n(2);
                    self.modes.push(Mode::JsxTag { closing: true });
                    self.emit(TokenKind::JsxTagCloseOpen, mark)
                } else {
                    self.bump();
                    self.modes.push(Mode::JsxTag { closing: false });
                    self.emit(TokenKind::JsxTagOpen, mark)
                }
            }
            Some(_) => {
                let mut text = String::new();
                while let Some(c) = self.peek() {
                    if c == '<' || c == '{' {
                        break;
                    }
                    text.push(c);
                    self.bump();
                }
                self.emit(TokenKind::JsxText(text), mark)
            }
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}
