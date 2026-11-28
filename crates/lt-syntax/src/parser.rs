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

/// Keywords that may stand in for a declaration name: none of them can begin a
/// statement, an expression, a type, or an item, so the position is unambiguous
/// (`from` in particular is no longer part of the import grammar).
const SOFT_KEYWORDS: &[&str] = &["from", "as", "in"];

/// Where a name is being read, which decides how much a keyword may step in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NameScope {
    /// Only an identifier: type names, struct/enum/variant names, generic params.
    Strict,
    /// Identifiers plus `SOFT_KEYWORDS`: bindings, function names, packages.
    Soft,
    /// Any keyword, because the position already proves it is a name: after `.`,
    /// before `:`, and anywhere else the caller used lookahead.
    Any,
}

/// Is this token usable as a name at all (ignoring the keyword tier)?
fn is_name_token(kind: &TokenKind) -> bool {
    matches!(kind, TokenKind::Ident(_)) || kind.is_keyword()
}

/// Is this token a name that needs no lookahead to disambiguate?
fn is_soft_name_token(kind: &TokenKind) -> bool {
    match kind {
        TokenKind::Ident(_) => true,
        kind if kind.is_keyword() => SOFT_KEYWORDS.contains(&kind.spelling()),
        _ => false,
    }
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
    /// Id handed to the next `import ( … )` block.
    next_group: u32,
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
            next_group: 0,
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
        self.expect_name(ctx, NameScope::Strict)
    }

    /// A declaration name: an identifier, or a keyword that cannot begin a
    /// statement, expression, type, or item.
    fn expect_soft_name(&mut self, ctx: &str) -> Option<(String, Span)> {
        self.expect_name(ctx, NameScope::Soft)
    }

    /// A name in a position that already proves it is a name (after `.`, before
    /// `:`), so every keyword is acceptable.
    fn expect_any_name(&mut self, ctx: &str) -> Option<(String, Span)> {
        self.expect_name(ctx, NameScope::Any)
    }

    fn expect_name(&mut self, ctx: &str, scope: NameScope) -> Option<(String, Span)> {
        let token = self.peek().clone();
        if let TokenKind::Ident(name) = token.kind.clone() {
            self.bump();
            return Some((name, token.span));
        }
        if token.kind.is_keyword() {
            let word = token.kind.spelling();
            let allowed = match scope {
                NameScope::Strict => false,
                NameScope::Soft => SOFT_KEYWORDS.contains(&word),
                NameScope::Any => true,
            };
            if allowed {
                self.bump();
                return Some((word.to_string(), token.span));
            }
            self.error(
                format!("`{word}` is a keyword; rename it to `{word}_`"),
                token.span,
                token.line,
                token.col,
            );
            return None;
        }
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

    /// Member names after `.` may collide with any keyword (`Expect.true`,
    /// `range.from`), because the position has already settled that it is a name.
    fn expect_member_name(&mut self, ctx: &str) -> Option<(String, Span)> {
        self.expect_any_name(ctx)
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
            let mut parsed = self.parse_import();
            imports.append(&mut parsed);
            if self.at_eof() {
                break;
            }
            if self.pos == before {
                self.bump();
            }
        }

        let mut packages = Vec::new();
        while !self.at_eof() {
            let before = self.pos;
            match self.parse_package() {
                Some(package) => packages.push(package),
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
            packages,
            span: Span::new(0, self.src.len()),
        }
    }

    fn recover_item(&mut self) {
        while !self.at_eof() {
            if self.at(&TokenKind::Package)
                || self.at(&TokenKind::Import)
                || self.at(&TokenKind::Class)
                || self.at(&TokenKind::Pub)
                || self.at(&TokenKind::Static)
                || self.at(&TokenKind::SelfKw)
            {
                return;
            }
            self.bump();
        }
    }

    /// v1 keywords that v2 removed. Returning a hint lets the parser produce an
    /// error that names the replacement instead of a generic "expected X".
    fn removed_keyword(&self) -> Option<(&'static str, &'static str)> {
        match self.peek().kind {
            TokenKind::Class => Some(("class", "`package` + `struct`")),
            TokenKind::Pub => Some(("pub", "`export`")),
            TokenKind::Static => Some(("static", "an explicit receiver parameter")),
            TokenKind::SelfKw => Some(("self", "an explicit receiver parameter")),
            _ => None,
        }
    }

    fn parse_import(&mut self) -> Vec<ImportDecl> {
        if self.expect(TokenKind::Import, "").is_none() {
            return Vec::new();
        }

        if self.at(&TokenKind::LParen) {
            let open = self.bump();
            let group = self.next_group;
            self.next_group += 1;
            let mut out = Vec::new();
            let mut last_line: Option<u32> = None;
            while !self.at(&TokenKind::RParen) && !self.at_eof() {
                let before = self.pos;
                match self.parse_import_entry(Some(group), last_line) {
                    Some(import) => {
                        last_line = Some(self.last_end_line);
                        out.push(import);
                    }
                    None => self.recover_import_entry(),
                }
                if self.at_eof() {
                    break;
                }
                if self.pos == before {
                    self.bump();
                }
            }
            let close = self.expect(TokenKind::RParen, "to close the import group");
            if out.is_empty() {
                let end = close.map_or(open.span.end, |token| token.span.end);
                self.error(
                    "an empty import group does nothing; remove it".to_string(),
                    Span::new(open.span.start, end),
                    open.line,
                    open.col,
                );
            }
            return out;
        }

        self.parse_import_entry(None, None).into_iter().collect()
    }

    /// One `name [as alias] [{ items }] "path"` entry, inside a group or on its
    /// own. `previous_line` is the last source line of the entry before it, used
    /// to remember blank-line separated sub-groups.
    fn parse_import_entry(
        &mut self,
        group: Option<u32>,
        previous_line: Option<u32>,
    ) -> Option<ImportDecl> {
        // Go's dot and blank imports collapse the qualifier rules this language
        // is built on, so reject them by name instead of "expected identifier".
        match self.peek().kind {
            TokenKind::Dot => {
                self.error_here(
                    "a dot import is not supported: every name is referenced through its package qualifier",
                );
                self.skip_line();
                return None;
            }
            TokenKind::Underscore => {
                self.error_here(
                    "a blank import is not supported: packages are only loaded when a name from them is used",
                );
                self.skip_line();
                return None;
            }
            _ => {}
        }

        let start = self.peek().span.start;
        let mut blank_before = false;
        if let Some(previous) = previous_line {
            blank_before = self.peek().line > previous + 1;
        }

        let (package, _) = self.expect_soft_name("after `import`")?;
        let alias = if self.eat(&TokenKind::As).is_some() {
            Some(self.expect_soft_name("after `as`")?.0)
        } else {
            None
        };

        let mut items = Vec::new();
        if let Some(open) = self.eat(&TokenKind::LBrace) {
            while !self.at(&TokenKind::RBrace) && !self.at_eof() {
                let (name, nspan) = self.expect_soft_name("as imported name")?;
                let item_alias = if self.eat(&TokenKind::As).is_some() {
                    Some(self.expect_soft_name("after `as`")?.0)
                } else {
                    None
                };
                items.push(ImportItem {
                    name,
                    alias: item_alias,
                    span: Span::new(nspan.start, self.last_end),
                });
                if self.eat(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            let close = self.expect(TokenKind::RBrace, "to close the import list")?;
            if items.is_empty() {
                self.error(
                    "an empty import list does nothing; remove the braces or name at least one item",
                    Span::new(open.span.start, close.span.end),
                    open.line,
                    open.col,
                );
            }
        }

        // `from` is gone, but recognising it turns the old spelling into a
        // message that says how to write it now.
        if self.at(&TokenKind::From) {
            self.error_here("`from` is no longer used in imports; write `import name \"path\"`");
            self.bump();
        }

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
            package,
            alias,
            items,
            path,
            group,
            blank_before,
            span: Span::new(start, self.last_end),
        })
    }

    /// Skip to the next entry boundary after a malformed import.
    fn recover_import_entry(&mut self) {
        while !self.at_eof() {
            if self.at(&TokenKind::Str(String::new()))
                || self.at(&TokenKind::RParen)
                || self.at(&TokenKind::Import)
            {
                return;
            }
            self.bump();
        }
    }

    /// Consume the rest of the current line, so one rejected import does not
    /// cascade into "expected `package`" for the leftover path string.
    fn skip_line(&mut self) {
        let line = self.peek().line;
        while !self.at_eof() && self.peek().line == line && !self.at(&TokenKind::RParen) {
            self.bump();
        }
    }

    // --------------------------------------------------------- packages

    fn parse_package(&mut self) -> Option<PackageDecl> {
        if let Some((removed, replacement)) = self.removed_keyword() {
            self.error_here(format!(
                "`{removed}` was removed in v2; use {replacement} instead"
            ));
            return None;
        }
        let start = self.expect(TokenKind::Package, "")?.span.start;
        let (name, _) = self.expect_soft_name("as package name")?;
        self.expect(TokenKind::LBrace, "to start the package body")?;

        let mut items = Vec::new();
        loop {
            if self.at(&TokenKind::RBrace) {
                self.bump();
                break;
            }
            if self.at_eof() {
                self.error_here("unexpected end of file, expected `}` to close the package");
                break;
            }

            let before = self.pos;
            if let Some(item) = self.parse_package_item() {
                items.push(item);
            }
            if self.pos == before {
                self.bump();
            }
        }

        Some(PackageDecl {
            name,
            items,
            span: Span::new(start, self.last_end),
        })
    }

    fn parse_package_item(&mut self) -> Option<PackageItem> {
        let start = self.peek().span.start;
        let is_export = self.eat(&TokenKind::Export).is_some();

        if let Some((removed, replacement)) = self.removed_keyword() {
            self.error_here(format!(
                "`{removed}` was removed in v2; use {replacement} instead"
            ));
            return None;
        }

        if self.at(&TokenKind::Struct) {
            return self.parse_struct(start, is_export).map(PackageItem::Struct);
        }
        if self.at(&TokenKind::Enum) {
            return self.parse_enum(start, is_export).map(PackageItem::Enum);
        }
        if self.at(&TokenKind::Fn) {
            return self.parse_fn(start, is_export).map(PackageItem::Fn);
        }
        if self.at(&TokenKind::Let) {
            return self
                .parse_binding(start, is_export)
                .map(PackageItem::Binding);
        }
        if !is_export && self.at(&TokenKind::Test) {
            return self.parse_test(start).map(PackageItem::Test);
        }

        self.error_here("expected `struct`, `enum`, `fn`, `let`, or `test`");
        None
    }

    fn parse_struct(&mut self, start: usize, is_export: bool) -> Option<StructDecl> {
        self.expect(TokenKind::Struct, "")?;
        let (name, _) = self.expect_ident("as struct name")?;
        let generics = self.parse_generic_params()?;
        self.expect(TokenKind::LBrace, "to start the struct body")?;

        let mut fields = Vec::new();
        loop {
            if self.at(&TokenKind::RBrace) {
                self.bump();
                break;
            }
            if self.at_eof() {
                self.error_here("unexpected end of file, expected `}` to close the struct");
                break;
            }

            let before = self.pos;
            if is_name_token(&self.peek().kind)
                && same_kind(&self.peek_at(1).kind, &TokenKind::Colon)
            {
                if let Some(field) = self.parse_field() {
                    fields.push(field);
                }
            } else if self.at(&TokenKind::Fn) || self.at(&TokenKind::Export) {
                self.error_here(
                    "structs hold data only; declare functions at package level and take the struct as the first parameter",
                );
            } else {
                self.error_here("expected a field declaration");
            }
            if self.pos == before {
                self.bump();
            }
        }

        Some(StructDecl {
            is_export,
            name,
            generics,
            fields,
            span: Span::new(start, self.last_end),
        })
    }

    fn parse_field(&mut self) -> Option<Field> {
        let (name, nspan) = self.expect_any_name("as field name")?;
        self.expect(TokenKind::Colon, "after field name")?;
        let ty = self.parse_type()?;
        Some(Field {
            name,
            ty,
            span: Span::new(nspan.start, self.last_end),
        })
    }

    fn parse_fn(&mut self, start: usize, is_export: bool) -> Option<FnDecl> {
        self.expect(TokenKind::Fn, "to start a function")?;
        let (name, _) = self.expect_soft_name("as function name")?;
        let generics = self.parse_generic_params()?;
        self.expect(TokenKind::LParen, "to start parameters")?;

        let params = if self.at(&TokenKind::RParen) {
            Vec::new()
        } else {
            self.parse_params()?
        };
        self.expect(TokenKind::RParen, "to close parameters")?;

        let ret = if self.eat(&TokenKind::Arrow).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;

        Some(FnDecl {
            is_export,
            name,
            generics,
            params,
            ret,
            body,
            span: Span::new(start, self.last_end),
        })
    }

    fn parse_binding(&mut self, start: usize, is_export: bool) -> Option<BindingDecl> {
        self.expect(TokenKind::Let, "")?;
        if self.at(&TokenKind::Mut) {
            self.error_here("package-level bindings are immutable; `mut` is local-only");
            return None;
        }
        let (name, _) = self.expect_soft_name("as binding name")?;
        let ty = if self.eat(&TokenKind::Colon).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(TokenKind::Eq, "in a package-level binding")?;
        let value = self.parse_expr()?;

        Some(BindingDecl {
            is_export,
            name,
            ty,
            value,
            span: Span::new(start, self.last_end),
        })
    }

    fn parse_test(&mut self, start: usize) -> Option<TestDecl> {
        self.expect(TokenKind::Test, "")?;
        let token = self.peek().clone();
        let title = match token.kind {
            TokenKind::Str(value) => {
                self.bump();
                value
            }
            _ => {
                self.error(
                    format!(
                        "expected a string literal test title, found {}",
                        token.kind.describe()
                    ),
                    token.span,
                    token.line,
                    token.col,
                );
                return None;
            }
        };
        let body = self.parse_block()?;
        Some(TestDecl {
            title,
            body,
            span: Span::new(start, self.last_end),
        })
    }

    fn parse_params(&mut self) -> Option<Vec<Param>> {
        let mut params = Vec::new();
        loop {
            let (name, nspan) = self.expect_any_name("as parameter name")?;
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

    fn parse_enum(&mut self, start: usize, is_export: bool) -> Option<EnumDecl> {
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
            is_export,
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
                Some(stmt) => {
                    stmts.push(stmt);
                    // Statements are newline separated: `xs[0]` used to parse as
                    // two statements on one line and quietly drop the first.
                    if !self.at(&TokenKind::RBrace)
                        && !self.at_eof()
                        && self.peek().line == self.last_end_line
                    {
                        self.error_here("statements must start on a new line");
                    }
                }
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
            TokenKind::Break => {
                let span = self.bump().span;
                Some(Stmt::Break { span })
            }
            TokenKind::Continue => {
                let span = self.bump().span;
                Some(Stmt::Continue { span })
            }
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
        is_soft_name_token(&self.peek().kind) && same_kind(&self.peek_at(1).kind, &TokenKind::Eq)
    }

    fn parse_let(&mut self) -> Option<Stmt> {
        let start = self.bump().span.start;
        let mutable = self.eat(&TokenKind::Mut).is_some();
        let (name, _) = self.expect_soft_name("after `let`")?;
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
        let (name, nspan) = self.expect_soft_name("in an assignment")?;
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
        let (var, _) = self.expect_soft_name("after `for`")?;
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
            let (name, _) = self.expect_any_name("as lambda parameter")?;
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
                // `xs[0]` only continues the line it starts on; a `[` on the
                // next line begins a fresh statement.
                TokenKind::LBracket if token.line == self.last_end_line => {
                    self.bump();
                    let index = self.parse_expr()?;
                    self.expect(TokenKind::RBracket, "to close the index")?;
                    let span = Span::new(expr.span().start, self.last_end);
                    expr = Expr::Index {
                        base: Box::new(expr),
                        index: Box::new(index),
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

        let named =
            is_name_token(&self.peek().kind) && same_kind(&self.peek_at(1).kind, &TokenKind::Colon);

        if named {
            let mut items = Vec::new();
            loop {
                let (name, nspan) = self.expect_any_name("as named argument")?;
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
            // `from(1)`, `x.from()` and friends: the soft set can name a
            // function, so it must also be readable as an expression.
            ref kind if is_soft_name_token(kind) => {
                let name = token.kind.spelling().to_string();
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
                // `[:]` is the empty dict; `[k: v, ...]` is a dict; everything
                // else is a list. One expression of lookahead decides.
                if self.eat(&TokenKind::Colon).is_some() {
                    self.expect(TokenKind::RBracket, "to close the empty dict literal")?;
                    return Some(Expr::Dict {
                        entries: Vec::new(),
                        span: Span::new(start, self.last_end),
                    });
                }
                if self.at(&TokenKind::RBracket) {
                    self.bump();
                    return Some(Expr::List {
                        items: Vec::new(),
                        span: Span::new(start, self.last_end),
                    });
                }
                let first = self.parse_expr()?;
                if self.eat(&TokenKind::Colon).is_some() {
                    let mut entries = Vec::new();
                    let value = self.parse_expr()?;
                    self.check_dict_key(&first);
                    entries.push((first, value));
                    while self.eat(&TokenKind::Comma).is_some() {
                        if self.at(&TokenKind::RBracket) {
                            break;
                        }
                        let key = self.parse_expr()?;
                        self.expect(TokenKind::Colon, "after a dict key")?;
                        let value = self.parse_expr()?;
                        self.check_dict_key(&key);
                        entries.push((key, value));
                    }
                    self.expect(TokenKind::RBracket, "to close the dict literal")?;
                    return Some(Expr::Dict {
                        entries,
                        span: Span::new(start, self.last_end),
                    });
                }
                let mut items = vec![first];
                while self.eat(&TokenKind::Comma).is_some() {
                    if self.at(&TokenKind::RBracket) {
                        break;
                    }
                    items.push(self.parse_expr()?);
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

    /// Dict keys are string literals; `["a" + b: 1]` is rejected at the point
    /// where the reader can see the offending expression.
    fn check_dict_key(&mut self, key: &Expr) {
        if !matches!(key, Expr::Str { .. }) {
            let span = key.span();
            let start = span.start;
            let mut line = 1;
            let mut col = 1;
            for (index, ch) in self.src.char_indices() {
                if index >= start {
                    break;
                }
                if ch == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
            }
            self.error(
                "dict keys must be string literals, e.g. [\"name\": 1]".to_string(),
                span,
                line,
                col,
            );
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
            ref kind if is_soft_name_token(kind) => {
                let (name, _) = self.expect_soft_name("in a pattern")?;
                let mut path = vec![name];
                while self.eat(&TokenKind::Dot).is_some() {
                    let (segment, _) = self.expect_member_name("after `.` in a pattern")?;
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
        let mut path = vec![self.expect_ident("as JSX tag name")?.0];
        while self.eat(&TokenKind::Dot).is_some() {
            path.push(self.expect_ident("after `.` in a JSX tag")?.0);
        }

        let mut attrs = Vec::new();
        loop {
            if self.at(&TokenKind::JsxTagSelfClose) {
                self.bump();
                return Some(JsxElement {
                    path,
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

            let (attr_name, aspan) = self.expect_any_name("as JSX attribute")?;
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
                    let mut close_path = vec![close_name];
                    while self.eat(&TokenKind::Dot).is_some() {
                        close_path.push(self.expect_ident("after `.` in a JSX closing tag")?.0);
                    }
                    if close_path != path {
                        self.error(
                            format!(
                                "JSX closing tag `{}` does not match opening tag `{}`",
                                close_path.join("."),
                                path.join(".")
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
            path,
            attrs,
            children,
            self_closing: false,
            span: Span::new(open.span.start, self.last_end),
        })
    }
}
