//! Recursive-descent parser.
//!
//! Statement boundaries are newlines. A binary operator only continues an
//! expression when it sits on the same line as the previous token, or when we
//! are inside `(` / `[` / `${`. That single rule keeps `a` newline `-b` as two
//! statements while still allowing multi-line calls and templates.

use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::token::{Token, TokenKind};

pub struct ParseOutput {
    pub file: File,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse a whole `.lt` source file. Never panics; errors are collected.
pub fn parse(src: &str) -> ParseOutput {
    let (tokens, lex_errors) = crate::tokenize(src);
    let mut parser = Parser::new(src, tokens);
    let file = parser.parse_file();
    let mut diagnostics = lex_errors;
    diagnostics.append(&mut parser.errors);
    ParseOutput { file, diagnostics }
}

fn same_kind(a: &TokenKind, b: &TokenKind) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

fn is_taggable(expr: &Expr) -> bool {
    matches!(expr, Expr::Ident { .. } | Expr::Field { .. })
}

fn binop_of(kind: &TokenKind) -> Option<(BinOp, u8)> {
    Some(match kind {
        TokenKind::PipePipe => (BinOp::Or, 1),
        TokenKind::AmpAmp => (BinOp::And, 2),
        TokenKind::EqEq => (BinOp::Eq, 3),
        TokenKind::BangEq => (BinOp::Ne, 3),
        TokenKind::Lt => (BinOp::Lt, 4),
        TokenKind::LtEq => (BinOp::Le, 4),
        TokenKind::Gt => (BinOp::Gt, 4),
        TokenKind::GtEq => (BinOp::Ge, 4),
        TokenKind::Plus => (BinOp::Add, 5),
        TokenKind::Minus => (BinOp::Sub, 5),
        TokenKind::Star => (BinOp::Mul, 6),
        TokenKind::Slash => (BinOp::Div, 6),
        TokenKind::Percent => (BinOp::Rem, 6),
        _ => return None,
    })
}

struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<Diagnostic>,
    bracket_depth: u32,
    last_end: usize,
    last_end_line: u32,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            src,
            tokens,
            pos: 0,
            errors: Vec::new(),
            bracket_depth: 0,
            last_end: 0,
            last_end_line: 1,
        }
    }

    // ------------------------------------------------------------ tokens

    fn peek(&self) -> &Token {
        let last = self.tokens.len().saturating_sub(1);
        &self.tokens[self.pos.min(last)]
    }

    fn peek_at(&self, n: usize) -> &Token {
        let last = self.tokens.len().saturating_sub(1);
        &self.tokens[(self.pos + n).min(last)]
    }

    fn at(&self, kind: &TokenKind) -> bool {
        same_kind(&self.peek().kind, kind)
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn bump(&mut self) -> Token {
        let token = self.peek().clone();
        if !matches!(token.kind, TokenKind::Eof) {
            self.pos += 1;
            self.last_end = token.span.end;
            self.last_end_line =
                token.line + token.span.text(self.src).matches('\n').count() as u32;
            match token.kind {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::InterpStart => {
                    self.bracket_depth += 1;
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::InterpEnd => {
                    self.bracket_depth = self.bracket_depth.saturating_sub(1);
                }
                _ => {}
            }
        }
        token
    }

    fn eat(&mut self, kind: &TokenKind) -> Option<Token> {
        if self.at(kind) {
            Some(self.bump())
        } else {
            None
        }
    }

    fn expect(&mut self, kind: TokenKind, ctx: &str) -> Option<Token> {
        if self.at(&kind) {
            return Some(self.bump());
        }
        let token = self.peek().clone();
        let suffix = if ctx.is_empty() {
            String::new()
        } else {
            format!(" {ctx}")
        };
        self.error(
            format!(
                "expected {}{suffix}, found {}",
                kind.describe(),
                token.kind.describe()
            ),
            token.span,
            token.line,
            token.col,
        );
        None
    }

    fn expect_ident(&mut self, ctx: &str) -> Option<(String, Span)> {
        let token = self.peek().clone();
        if let TokenKind::Ident(name) = token.kind {
            self.bump();
            Some((name, token.span))
        } else {
            let suffix = if ctx.is_empty() {
                String::new()
            } else {
                format!(" {ctx}")
            };
            self.error(
                format!(
                    "expected an identifier{suffix}, found {}",
                    self.peek().kind.describe()
                ),
                token.span,
                token.line,
                token.col,
            );
            None
        }
    }

    /// Member names after `.` may collide with `true`/`false` (e.g. `Expect.true`).
    fn expect_member_name(&mut self, ctx: &str) -> Option<(String, Span)> {
        let token = self.peek().clone();
        let name = match &token.kind {
            TokenKind::Ident(name) => name.clone(),
            TokenKind::True => "true".to_string(),
            TokenKind::False => "false".to_string(),
            _ => {
                self.error(
                    format!(
                        "expected a member name {ctx}, found {}",
                        token.kind.describe()
                    ),
                    token.span,
                    token.line,
                    token.col,
                );
                return None;
            }
        };
        self.bump();
        Some((name, token.span))
    }

    fn error(&mut self, message: impl Into<String>, span: Span, line: u32, col: u32) {
        self.errors
            .push(Diagnostic::error(message, span, line, col));
    }

    fn error_here(&mut self, message: impl Into<String>) {
        let token = self.peek().clone();
        self.error(message, token.span, token.line, token.col);
    }

    /// May this token continue the previous line's expression?
    fn continuation_ok(&self, line: u32) -> bool {
        self.bracket_depth > 0 || line == self.last_end_line
    }

    // ------------------------------------------------------------- file

    fn parse_file(&mut self) -> File {
        let mut imports = Vec::new();
        while self.at(&TokenKind::Import) {
            let before = self.pos;
            match self.parse_import() {
                Some(import) => imports.push(import),
                None => self.recover_item(),
            }
            if self.at_eof() {
                break;
            }
            if self.pos == before {
                self.bump();
            }
        }

        let mut items = Vec::new();
        while !self.at_eof() {
            let before = self.pos;
            match self.parse_item() {
                Some(item) => items.push(item),
                None => self.recover_item(),
            }
            if self.at_eof() {
                break;
            }
            if self.pos == before {
                self.bump();
            }
        }

        File {
            imports,
            items,
            span: Span::new(0, self.src.len()),
        }
    }

    fn recover_item(&mut self) {
        while !self.at_eof() {
            if self.at(&TokenKind::Pub)
                || self.at(&TokenKind::Class)
                || self.at(&TokenKind::Enum)
                || self.at(&TokenKind::Test)
                || self.at(&TokenKind::Import)
            {
                return;
            }
            self.bump();
        }
    }

    fn parse_import(&mut self) -> Option<ImportDecl> {
        let start = self.expect(TokenKind::Import, "")?;
        let (name, _) = self.expect_ident("after `import`")?;
        self.expect(TokenKind::From, "in `import`")?;
        let token = self.peek().clone();
        let path = match token.kind {
            TokenKind::Str(value) => {
                self.bump();
                value
            }
            _ => {
                self.error(
                    format!(
                        "expected a string literal path, found {}",
                        token.kind.describe()
                    ),
                    token.span,
                    token.line,
                    token.col,
                );
                return None;
            }
        };
        Some(ImportDecl {
            name,
            path,
            span: Span::new(start.span.start, self.last_end),
        })
    }

    // ------------------------------------------------------------ items

    fn parse_item(&mut self) -> Option<Item> {
        let start = self.peek().span.start;
        let is_pub = self.eat(&TokenKind::Pub).is_some();

        if self.at(&TokenKind::Class) {
            return self.parse_class(start, is_pub).map(Item::Class);
        }
        if self.at(&TokenKind::Enum) {
            return self.parse_enum(start, is_pub).map(Item::Enum);
        }
        if !is_pub && self.at(&TokenKind::Test) {
            return self.parse_test_class(start).map(Item::TestClass);
        }

        self.error_here("expected `class`, `enum`, or `test class`");
        None
    }

    fn parse_class(&mut self, start: usize, is_pub: bool) -> Option<ClassDecl> {
        self.expect(TokenKind::Class, "")?;
        let (name, _) = self.expect_ident("as class name")?;
        let generics = self.parse_generic_params()?;
        self.expect(TokenKind::LBrace, "to start the class body")?;

        let mut fields = Vec::new();
        let mut methods = Vec::new();
        loop {
            if self.at(&TokenKind::RBrace) {
                self.bump();
                break;
            }
            if self.at_eof() {
                self.error_here("unexpected end of file, expected `}` to close the class");
                break;
            }

            let before = self.pos;
            if self.at(&TokenKind::Fn) || self.at(&TokenKind::Pub) || self.at(&TokenKind::Static) {
                if let Some(method) = self.parse_method() {
                    methods.push(method);
                }
            } else if matches!(self.peek().kind, TokenKind::Ident(_)) {
                if let Some(field) = self.parse_field() {
                    fields.push(field);
                }
            } else {
                self.error_here("expected a field declaration or a method");
            }
            if self.pos == before {
                self.bump();
            }
        }

        Some(ClassDecl {
            is_pub,
            name,
            generics,
            fields,
            methods,
            span: Span::new(start, self.last_end),
        })
    }

    fn parse_field(&mut self) -> Option<Field> {
        let (name, nspan) = self.expect_ident("as field name")?;
        self.expect(TokenKind::Colon, "after field name")?;
        let ty = self.parse_type()?;
        Some(Field {
            name,
            ty,
            span: Span::new(nspan.start, self.last_end),
        })
    }

    fn parse_method(&mut self) -> Option<Method> {
        let start = self.peek().span.start;
        let is_pub = self.eat(&TokenKind::Pub).is_some();
        let is_static = self.eat(&TokenKind::Static).is_some();
        self.expect(TokenKind::Fn, "to start a method")?;
        let (name, _) = self.expect_ident("as method name")?;
        self.expect(TokenKind::LParen, "to start parameters")?;

        let mut has_self = false;
        let mut params = Vec::new();
        if self.at(&TokenKind::SelfKw) {
            self.bump();
            has_self = true;
            if self.eat(&TokenKind::Comma).is_some() {
                params = self.parse_params()?;
            }
        } else if !self.at(&TokenKind::RParen) {
            params = self.parse_params()?;
        }
        self.expect(TokenKind::RParen, "to close parameters")?;

        let ret = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;

        Some(Method {
            is_pub,
            is_static,
            has_self,
            name,
            params,
            ret,
            body,
            span: Span::new(start, self.last_end),
        })
    }

    fn parse_params(&mut self) -> Option<Vec<Param>> {
        let mut params = Vec::new();
        loop {
            let (name, nspan) = self.expect_ident("as parameter name")?;
            self.expect(TokenKind::Colon, "after parameter name")?;
            let ty = self.parse_type()?;
            params.push(Param {
                name,
                ty,
                span: Span::new(nspan.start, self.last_end),
            });
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
            if self.at(&TokenKind::RParen) {
                break;
            }
        }
        Some(params)
    }

    fn parse_test_class(&mut self, start: usize) -> Option<TestClassDecl> {
        self.expect(TokenKind::Test, "")?;
        self.expect(TokenKind::Class, "after `test`")?;
        let (name, _) = self.expect_ident("as test class name")?;
        self.expect(TokenKind::LBrace, "to start the test class body")?;

        let mut tests = Vec::new();
        let mut helpers = Vec::new();
        loop {
            if self.at(&TokenKind::RBrace) {
                self.bump();
                break;
            }
            if self.at_eof() {
                self.error_here("unexpected end of file, expected `}` to close the test class");
                break;
            }

            let before = self.pos;
            if self.at(&TokenKind::Test) {
                let test_start = self.bump().span.start;
                self.expect(TokenKind::Fn, "after `test`")?;
                let (test_name, _) = self.expect_ident("as test name")?;
                self.expect(TokenKind::LParen, "to start test parameters")?;
                self.expect(TokenKind::RParen, "after `(`; tests take no parameters")?;
                if let Some(body) = self.parse_block() {
                    tests.push(TestFn {
                        name: test_name,
                        body,
                        span: Span::new(test_start, self.last_end),
                    });
                }
            } else if self.at(&TokenKind::Fn)
                || self.at(&TokenKind::Pub)
                || self.at(&TokenKind::Static)
            {
                if let Some(method) = self.parse_method() {
                    helpers.push(method);
                }
            } else {
                self.error_here("expected `test fn` or a helper method");
            }
            if self.pos == before {
                self.bump();
            }
        }

        Some(TestClassDecl {
            name,
            tests,
            helpers,
            span: Span::new(start, self.last_end),
        })
    }

    fn parse_enum(&mut self, start: usize, is_pub: bool) -> Option<EnumDecl> {
        self.expect(TokenKind::Enum, "")?;
        let (name, _) = self.expect_ident("as enum name")?;
        let generics = self.parse_generic_params()?;
        self.expect(TokenKind::LBrace, "to start the enum body")?;

        let mut variants = Vec::new();
        loop {
            if self.at(&TokenKind::RBrace) {
                self.bump();
                break;
            }
            if self.at_eof() {
                self.error_here("unexpected end of file, expected `}` to close the enum");
                break;
            }

            let before = self.pos;
            let (variant_name, vspan) = match self.expect_ident("as enum variant") {
                Some(v) => v,
                None => {
                    self.bump();
                    continue;
                }
            };
            let mut payload = Vec::new();
            if self.eat(&TokenKind::LParen).is_some() {
                loop {
                    if self.at(&TokenKind::RParen) {
                        break;
                    }
                    match self.parse_type() {
                        Some(ty) => payload.push(ty),
                        None => break,
                    }
                    if self.eat(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                self.expect(TokenKind::RParen, "to close enum variant payload")?;
            }
            variants.push(Variant {
                name: variant_name,
                payload,
                span: Span::new(vspan.start, self.last_end),
            });
            if self.pos == before {
                self.bump();
            }
        }

        Some(EnumDecl {
            is_pub,
            name,
            generics,
            variants,
            span: Span::new(start, self.last_end),
        })
    }

    fn parse_generic_params(&mut self) -> Option<Vec<String>> {
        if self.eat(&TokenKind::Lt).is_none() {
            return Some(Vec::new());
        }
        let mut names = Vec::new();
        loop {
            let (name, _) = self.expect_ident("as generic parameter")?;
            names.push(name);
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
            if self.at(&TokenKind::Gt) {
                break;
            }
        }
        self.expect(TokenKind::Gt, "to close generic parameters")?;
        Some(names)
    }

    // ------------------------------------------------------------- types

    fn parse_type(&mut self) -> Option<Type> {
        let (first, fspan) = self.expect_ident("as a type name")?;
        let mut path = vec![first];
        while self.eat(&TokenKind::Dot).is_some() {
            let (segment, _) = self.expect_ident("after `.` in a type path")?;
            path.push(segment);
        }

        let mut args = Vec::new();
        if self.eat(&TokenKind::Lt).is_some() {
            loop {
                args.push(self.parse_type()?);
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
                if self.at(&TokenKind::Gt) {
                    break;
                }
            }
            self.expect(TokenKind::Gt, "to close type arguments")?;
        }

        let optional = self.eat(&TokenKind::Question).is_some();

        Some(Type {
            path,
            args,
            optional,
            span: Span::new(fspan.start, self.last_end),
        })
    }

    // -------------------------------------------------------- statements

    fn parse_block(&mut self) -> Option<Block> {
        let open = self.expect(TokenKind::LBrace, "to start a block")?;
        let mut stmts = Vec::new();
        loop {
            if self.at(&TokenKind::RBrace) {
                let close = self.bump();
                return Some(Block {
                    stmts,
                    span: Span::new(open.span.start, close.span.end),
                });
            }
            if self.at_eof() {
                self.error_here("unexpected end of file, expected `}`");
                return Some(Block {
                    stmts,
                    span: Span::new(open.span.start, self.last_end),
                });
            }

            let before = self.pos;
            match self.parse_stmt() {
                Some(stmt) => stmts.push(stmt),
                None => self.sync_stmt(),
            }
            if self.pos == before {
                self.bump();
            }
        }
    }

    fn sync_stmt(&mut self) {
        while !self.at_eof() {
            if self.at(&TokenKind::RBrace)
                || self.at(&TokenKind::Let)
                || self.at(&TokenKind::Return)
                || self.at(&TokenKind::For)
                || self.at(&TokenKind::While)
            {
                return;
            }
            self.bump();
        }
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        match self.peek().kind {
            TokenKind::Let => self.parse_let(),
            TokenKind::Return => self.parse_return(),
            TokenKind::For => self.parse_for(),
            TokenKind::While => self.parse_while(),
            _ => {
                if self.is_assign_start() {
                    self.parse_assign()
                } else {
                    let expr = self.parse_expr()?;
                    Some(Stmt::Expr(expr))
                }
            }
        }
    }

    fn is_assign_start(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Ident(_))
            && same_kind(&self.peek_at(1).kind, &TokenKind::Eq)
    }

    fn parse_let(&mut self) -> Option<Stmt> {
        let start = self.bump().span.start;
        let mutable = self.eat(&TokenKind::Mut).is_some();
        let (name, _) = self.expect_ident("after `let`")?;
        let ty = if self.eat(&TokenKind::Colon).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(TokenKind::Eq, "in a `let` binding")?;
        let value = self.parse_expr()?;
        Some(Stmt::Let {
            mutable,
            name,
            ty,
            value,
            span: Span::new(start, self.last_end),
        })
    }

    fn parse_assign(&mut self) -> Option<Stmt> {
        let (name, nspan) = self.expect_ident("in an assignment")?;
        self.expect(TokenKind::Eq, "in an assignment")?;
        let value = self.parse_expr()?;
        Some(Stmt::Assign {
            name,
            value,
            span: Span::new(nspan.start, self.last_end),
        })
    }

    fn parse_return(&mut self) -> Option<Stmt> {
        let start = self.bump().span.start;
        let has_value = !self.at(&TokenKind::RBrace)
            && !self.at_eof()
            && self.peek().line == self.last_end_line;
        let value = if has_value {
            Some(self.parse_expr()?)
        } else {
            None
        };
        Some(Stmt::Return {
            value,
            span: Span::new(start, self.last_end),
        })
    }

    fn parse_for(&mut self) -> Option<Stmt> {
        let start = self.bump().span.start;
        let (var, _) = self.expect_ident("after `for`")?;
        self.expect(TokenKind::In, "in a `for` loop")?;
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        Some(Stmt::For {
            var,
            iter,
            body,
            span: Span::new(start, self.last_end),
        })
    }

    fn parse_while(&mut self) -> Option<Stmt> {
        let start = self.bump().span.start;
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Some(Stmt::While {
            cond,
            body,
            span: Span::new(start, self.last_end),
        })
    }

    // ------------------------------------------------------- expressions

    fn parse_expr(&mut self) -> Option<Expr> {
        if self.at(&TokenKind::Fn) {
            return self.parse_lambda();
        }
        self.parse_binary(1)
    }

    fn parse_lambda(&mut self) -> Option<Expr> {
        let start = self.expect(TokenKind::Fn, "to start a lambda")?;
        self.expect(TokenKind::LParen, "to start lambda parameters")?;

        // Lambda parameters are bare names: their types are inferred.
        let mut params = Vec::new();
        while !self.at(&TokenKind::RParen) {
            let (name, _) = self.expect_ident("as lambda parameter")?;
            params.push(name);
            if self.eat(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(TokenKind::RParen, "to close lambda parameters")?;
        self.expect(TokenKind::FatArrow, "after lambda parameters")?;

        let body = if self.at(&TokenKind::LBrace) {
            LambdaBody::Block(self.parse_block()?)
        } else {
            LambdaBody::Expr(Box::new(self.parse_expr()?))
        };

        Some(Expr::Lambda {
            params,
            body,
            span: Span::new(start.span.start, self.last_end),
        })
    }

    fn parse_binary(&mut self, min_prec: u8) -> Option<Expr> {
        let mut lhs = self.parse_unary()?;
        while let Some((op, prec)) = binop_of(&self.peek().kind) {
            if prec < min_prec {
                break;
            }
            if !self.continuation_ok(self.peek().line) {
                break;
            }
            self.bump();
            let rhs = self.parse_binary(prec + 1)?;
            let span = Span::new(lhs.span().start, rhs.span().end);
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Some(lhs)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        if self.at(&TokenKind::Bang) || self.at(&TokenKind::Minus) {
            let op = if self.at(&TokenKind::Bang) {
                UnOp::Not
            } else {
                UnOp::Neg
            };
            let start = self.bump().span.start;
            let value = self.parse_unary()?;
            return Some(Expr::Unary {
                op,
                span: Span::new(start, value.span().end),
                value: Box::new(value),
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            let token = self.peek().clone();
            match token.kind {
                TokenKind::Dot => {
                    self.bump();
                    let (name, _) = self.expect_member_name("after `.`")?;
                    let span = Span::new(expr.span().start, self.last_end);
                    expr = Expr::Field {
                        base: Box::new(expr),
                        name,
                        span,
                    };
                }
                TokenKind::LParen => {
                    self.bump();
                    let args = self.parse_args()?;
                    let span = Span::new(expr.span().start, self.last_end);
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                        span,
                    };
                }
                TokenKind::Question => {
                    self.bump();
                    let span = Span::new(expr.span().start, self.last_end);
                    expr = Expr::Try {
                        value: Box::new(expr),
                        span,
                    };
                }
                TokenKind::TemplateStart
                    if self.continuation_ok(token.line) && is_taggable(&expr) =>
                {
                    let (parts, exprs, tspan) = self.parse_template()?;
                    let span = Span::new(expr.span().start, tspan.end);
                    expr = Expr::TaggedTemplate {
                        tag: Box::new(expr),
                        parts,
                        exprs,
                        span,
                    };
                }
                _ => break,
            }
        }
        Some(expr)
    }

    fn parse_args(&mut self) -> Option<Args> {
        if self.at(&TokenKind::RParen) {
            self.bump();
            return Some(Args::Empty);
        }

        let named = matches!(self.peek().kind, TokenKind::Ident(_))
            && same_kind(&self.peek_at(1).kind, &TokenKind::Colon);

        if named {
            let mut items = Vec::new();
            loop {
                let (name, nspan) = self.expect_ident("as named argument")?;
                self.expect(TokenKind::Colon, "after named argument")?;
                let value = self.parse_expr()?;
                items.push(NamedArg {
                    name,
                    span: Span::new(nspan.start, value.span().end),
                    value,
                });
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
                if self.at(&TokenKind::RParen) {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "to close arguments")?;
            Some(Args::Named(items))
        } else {
            let mut items = Vec::new();
            loop {
                items.push(self.parse_expr()?);
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
                if self.at(&TokenKind::RParen) {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "to close arguments")?;
            Some(Args::Positional(items))
        }
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Int => {
                self.bump();
                Some(Expr::Int {
                    raw: token.span.text(self.src).to_string(),
                    span: token.span,
                })
            }
            TokenKind::Float => {
                self.bump();
                Some(Expr::Float {
                    raw: token.span.text(self.src).to_string(),
                    span: token.span,
                })
            }
            TokenKind::Str(value) => {
                self.bump();
                Some(Expr::Str {
                    value,
                    span: token.span,
                })
            }
            TokenKind::True => {
                self.bump();
                Some(Expr::Bool {
                    value: true,
                    span: token.span,
                })
            }
            TokenKind::False => {
                self.bump();
                Some(Expr::Bool {
                    value: false,
                    span: token.span,
                })
            }
            TokenKind::NoneKw => {
                self.bump();
                Some(Expr::NoneLit { span: token.span })
            }
            TokenKind::Ident(name) => {
                self.bump();
                Some(Expr::Ident {
                    name,
                    span: token.span,
                })
            }
            TokenKind::SelfKw => {
                self.bump();
                Some(Expr::Ident {
                    name: "self".to_string(),
                    span: token.span,
                })
            }
            TokenKind::LBracket => {
                let start = self.bump().span.start;
                let mut items = Vec::new();
                while !self.at(&TokenKind::RBracket) {
                    if self.at_eof() {
                        self.error_here("unexpected end of file in a list literal");
                        break;
                    }
                    items.push(self.parse_expr()?);
                    if self.eat(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                self.expect(TokenKind::RBracket, "to close the list literal")?;
                Some(Expr::List {
                    items,
                    span: Span::new(start, self.last_end),
                })
            }
            TokenKind::TemplateStart => {
                let (parts, exprs, span) = self.parse_template()?;
                Some(Expr::Template { parts, exprs, span })
            }
            TokenKind::JsxTagOpen => {
                let element = self.parse_jsx()?;
                Some(Expr::Jsx(element))
            }
            TokenKind::LParen => {
                self.bump();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen, "to close the group")?;
                Some(expr)
            }
            TokenKind::If => self.parse_if(),
            TokenKind::Match => self.parse_match(),
            _ => {
                self.error(
                    format!("expected an expression, found {}", token.kind.describe()),
                    token.span,
                    token.line,
                    token.col,
                );
                None
            }
        }
    }

    fn parse_template(&mut self) -> Option<(Vec<String>, Vec<Expr>, Span)> {
        let start = self.expect(TokenKind::TemplateStart, "to start a template literal")?;
        let mut parts = vec![String::new()];
        let mut exprs = Vec::new();

        let end = loop {
            let token = self.peek().clone();
            match token.kind {
                TokenKind::TemplateText(text) => {
                    self.bump();
                    if let Some(last) = parts.last_mut() {
                        last.push_str(&text);
                    }
                }
                TokenKind::InterpStart => {
                    self.bump();
                    let expr = self.parse_expr()?;
                    exprs.push(expr);
                    self.expect(TokenKind::InterpEnd, "to close `${ ... }`")?;
                    parts.push(String::new());
                }
                TokenKind::TemplateEnd => break self.bump().span,
                other => {
                    self.error(
                        format!(
                            "expected template text or `` ` ``, found {}",
                            other.describe()
                        ),
                        token.span,
                        token.line,
                        token.col,
                    );
                    return None;
                }
            }
        };

        Some((parts, exprs, Span::new(start.span.start, end.end)))
    }

    fn parse_if(&mut self) -> Option<Expr> {
        let start = self.expect(TokenKind::If, "to start `if`")?;
        let cond = self.parse_expr()?;
        let then_block = self.parse_block()?;
        let else_branch = if self.eat(&TokenKind::Else).is_some() {
            if self.at(&TokenKind::If) {
                Some(ElseBranch::If(Box::new(self.parse_if()?)))
            } else {
                Some(ElseBranch::Block(self.parse_block()?))
            }
        } else {
            None
        };
        Some(Expr::If {
            cond: Box::new(cond),
            then_block,
            else_branch,
            span: Span::new(start.span.start, self.last_end),
        })
    }

    fn parse_match(&mut self) -> Option<Expr> {
        let start = self.expect(TokenKind::Match, "to start `match`")?;
        let scrutinee = self.parse_expr()?;
        self.expect(TokenKind::LBrace, "to start match arms")?;

        let mut arms = Vec::new();
        loop {
            if self.at(&TokenKind::RBrace) {
                self.bump();
                break;
            }
            if self.at_eof() {
                self.error_here("unexpected end of file, expected `}` to close `match`");
                break;
            }

            let before = self.pos;
            let pattern = match self.parse_pattern() {
                Some(p) => p,
                None => {
                    self.bump();
                    continue;
                }
            };
            self.expect(TokenKind::FatArrow, "after a match pattern")?;
            let body = if self.at(&TokenKind::LBrace) {
                MatchBody::Block(self.parse_block()?)
            } else {
                MatchBody::Expr(self.parse_expr()?)
            };
            arms.push(MatchArm {
                span: Span::new(pattern.span().start, self.last_end),
                pattern,
                body,
            });
            if self.pos == before {
                self.bump();
            }
        }

        Some(Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span: Span::new(start.span.start, self.last_end),
        })
    }

    fn parse_pattern(&mut self) -> Option<Pattern> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Underscore => {
                self.bump();
                Some(Pattern::Wildcard { span: token.span })
            }
            TokenKind::NoneKw => {
                self.bump();
                Some(Pattern::NoneLit { span: token.span })
            }
            TokenKind::Ident(name) => {
                self.bump();
                let mut path = vec![name];
                while self.eat(&TokenKind::Dot).is_some() {
                    let (segment, _) = self.expect_ident("after `.` in a pattern")?;
                    path.push(segment);
                }
                let mut args = Vec::new();
                if self.eat(&TokenKind::LParen).is_some() {
                    while !self.at(&TokenKind::RParen) {
                        if self.at_eof() {
                            break;
                        }
                        args.push(self.parse_pattern()?);
                        if self.eat(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                    self.expect(TokenKind::RParen, "to close the pattern")?;
                }
                Some(Pattern::Variant {
                    path,
                    args,
                    span: Span::new(token.span.start, self.last_end),
                })
            }
            _ => {
                self.error(
                    format!("expected a pattern, found {}", token.kind.describe()),
                    token.span,
                    token.line,
                    token.col,
                );
                None
            }
        }
    }

    fn parse_jsx(&mut self) -> Option<JsxElement> {
        let open = self.expect(TokenKind::JsxTagOpen, "to start a JSX element")?;
        let (name, _) = self.expect_ident("as JSX tag name")?;

        let mut attrs = Vec::new();
        loop {
            if self.at(&TokenKind::JsxTagSelfClose) {
                self.bump();
                return Some(JsxElement {
                    name,
                    attrs,
                    children: Vec::new(),
                    self_closing: true,
                    span: Span::new(open.span.start, self.last_end),
                });
            }
            if self.at(&TokenKind::JsxTagEnd) {
                self.bump();
                break;
            }
            if self.at_eof() {
                self.error_here("unexpected end of file in a JSX tag");
                return None;
            }

            let (attr_name, aspan) = self.expect_ident("as JSX attribute")?;
            self.expect(TokenKind::Eq, "after a JSX attribute name")?;
            let value_token = self.peek().clone();
            let value = match value_token.kind {
                TokenKind::Str(value) => {
                    self.bump();
                    JsxAttrValue::Str(value)
                }
                TokenKind::LBrace => {
                    self.bump();
                    let expr = self.parse_expr()?;
                    self.expect(TokenKind::RBrace, "to close a JSX attribute expression")?;
                    JsxAttrValue::Expr(expr)
                }
                _ => {
                    self.error(
                        format!(
                            "expected a string or `{{ expression }}` as JSX attribute value, found {}",
                            value_token.kind.describe()
                        ),
                        value_token.span,
                        value_token.line,
                        value_token.col,
                    );
                    return None;
                }
            };
            attrs.push(JsxAttr {
                name: attr_name,
                value,
                span: Span::new(aspan.start, self.last_end),
            });
        }

        let mut children = Vec::new();
        loop {
            let token = self.peek().clone();
            match token.kind {
                TokenKind::JsxText(text) => {
                    self.bump();
                    if !text.trim().is_empty() {
                        children.push(JsxChild::Text(text));
                    }
                }
                TokenKind::LBrace => {
                    self.bump();
                    let expr = self.parse_expr()?;
                    self.expect(TokenKind::RBrace, "to close a JSX child expression")?;
                    children.push(JsxChild::Expr(expr));
                }
                TokenKind::JsxTagOpen => {
                    children.push(JsxChild::Element(self.parse_jsx()?));
                }
                TokenKind::JsxTagCloseOpen => {
                    self.bump();
                    let (close_name, cspan) = self.expect_ident("in a JSX closing tag")?;
                    if close_name != name {
                        self.error(
                            format!(
                                "JSX closing tag `{close_name}` does not match opening tag `{name}`"
                            ),
                            cspan,
                            token.line,
                            token.col,
                        );
                    }
                    self.expect(TokenKind::JsxTagEnd, "to close a JSX tag")?;
                    break;
                }
                _ => {
                    self.error(
                        format!(
                            "expected JSX text, `{{ expression }}`, or a closing tag, found {}",
                            token.kind.describe()
                        ),
                        token.span,
                        token.line,
                        token.col,
                    );
                    return None;
                }
            }
        }

        Some(JsxElement {
            name,
            attrs,
            children,
            self_closing: false,
            span: Span::new(open.span.start, self.last_end),
        })
    }
}
