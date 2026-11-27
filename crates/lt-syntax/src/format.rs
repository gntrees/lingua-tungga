//! Canonical pretty-printer with comment preservation.
//!
//! `fmt(fmt(x)) == fmt(x)` is the contract. Comments are attached to the
//! construct that follows them inside the same body (file, class, enum, test
//! class, block, or match). Inline comments (sharing a line with code) are
//! rejected rather than silently moved.

use std::collections::HashMap;

use crate::ast::*;
use crate::span::Span;

pub const WIDTH: usize = 100;

const INDENT: &str = "  ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentError {
    pub message: String,
    pub line: u32,
    pub col: u32,
}

fn pad(level: usize) -> String {
    INDENT.repeat(level)
}

fn fits_at(indent: usize, text: &str) -> bool {
    !text.contains('\n') && indent * INDENT.len() + text.chars().count() <= WIDTH
}

pub fn format_file(file: &File) -> String {
    Formatter::default().file(file)
}

pub fn format_file_with_comments(
    file: &File,
    src: &str,
    comments: &[Span],
) -> Result<String, CommentError> {
    let attachment = Attachment::build(file, src, comments)?;
    Ok(Formatter { attachment }.file(file))
}

struct Container {
    start: usize,
    end: usize,
    children: Vec<usize>,
}

#[derive(Default)]
struct Attachment {
    before: HashMap<usize, Vec<String>>,
    trailing: HashMap<usize, Vec<String>>,
}

impl Attachment {
    fn build(file: &File, src: &str, comments: &[Span]) -> Result<Self, CommentError> {
        let mut containers = Vec::new();
        collect_file(file, &mut containers);

        let mut attachment = Attachment::default();
        for span in comments {
            let text = normalize_comment(src, *span)?;
            let offset = span.start;

            let container = containers
                .iter()
                .filter(|container| container.start <= offset && offset <= container.end)
                .min_by_key(|container| container.end - container.start)
                .map(|container| (container.end, &container.children));

            let Some((end, children)) = container else {
                attachment
                    .trailing
                    .entry(file.span.start)
                    .or_default()
                    .push(text);
                continue;
            };

            let next = children.iter().copied().find(|child| *child > offset);
            match next {
                Some(child) => attachment.before.entry(child).or_default().push(text),
                None => attachment.trailing.entry(end).or_default().push(text),
            }
        }
        Ok(attachment)
    }
}

fn collect_file(file: &File, out: &mut Vec<Container>) {
    let mut children: Vec<usize> = file
        .imports
        .iter()
        .map(|import| import.span.start)
        .collect();
    children.extend(file.packages.iter().map(|package| package.span.start));
    children.sort_unstable();
    out.push(Container {
        start: file.span.start,
        end: file.span.end,
        children,
    });

    for package in &file.packages {
        out.push(Container {
            start: package.span.start,
            end: package.span.end,
            children: package.items.iter().map(|item| item.span().start).collect(),
        });
        for item in &package.items {
            match item {
                PackageItem::Struct(decl) => out.push(Container {
                    start: decl.span.start,
                    end: decl.span.end,
                    children: decl.fields.iter().map(|field| field.span.start).collect(),
                }),
                PackageItem::Enum(decl) => out.push(Container {
                    start: decl.span.start,
                    end: decl.span.end,
                    children: decl.variants.iter().map(|variant| variant.span.start).collect(),
                }),
                PackageItem::Fn(decl) => collect_block(&decl.body, out),
                PackageItem::Binding(decl) => collect_expr(&decl.value, out),
                PackageItem::Test(decl) => collect_block(&decl.body, out),
            }
        }
    }
}

fn collect_block(block: &Block, out: &mut Vec<Container>) {
    out.push(Container {
        start: block.span.start,
        end: block.span.end,
        children: block.stmts.iter().map(|stmt| stmt.span().start).collect(),
    });
    for stmt in &block.stmts {
        collect_stmt(stmt, out);
    }
}

fn collect_stmt(stmt: &Stmt, out: &mut Vec<Container>) {
    match stmt {
        Stmt::Let { value, .. } => collect_expr(value, out),
        Stmt::Assign { value, .. } => collect_expr(value, out),
        Stmt::Return { value, .. } => {
            if let Some(value) = value {
                collect_expr(value, out);
            }
        }
        Stmt::For { iter, body, .. } => {
            collect_expr(iter, out);
            collect_block(body, out);
        }
        Stmt::While { cond, body, .. } => {
            collect_expr(cond, out);
            collect_block(body, out);
        }
        Stmt::Expr(expr) => collect_expr(expr, out),
    }
}

fn collect_expr(expr: &Expr, out: &mut Vec<Container>) {
    match expr {
        Expr::List { items, .. } => {
            for item in items {
                collect_expr(item, out);
            }
        }
        Expr::Call { callee, args, .. } => {
            collect_expr(callee, out);
            match args {
                Args::Positional(items) => {
                    for item in items {
                        collect_expr(item, out);
                    }
                }
                Args::Named(items) => {
                    for item in items {
                        collect_expr(&item.value, out);
                    }
                }
                Args::Empty => {}
            }
        }
        Expr::Field { base, .. } => collect_expr(base, out),
        Expr::Try { value, .. } => collect_expr(value, out),
        Expr::Template { exprs, .. } => {
            for inner in exprs {
                collect_expr(inner, out);
            }
        }
        Expr::TaggedTemplate { tag, exprs, .. } => {
            collect_expr(tag, out);
            for inner in exprs {
                collect_expr(inner, out);
            }
        }
        Expr::Jsx(element) => collect_jsx(element, out),
        Expr::Lambda { body, .. } => match body {
            LambdaBody::Expr(inner) => collect_expr(inner, out),
            LambdaBody::Block(block) => collect_block(block, out),
        },
        Expr::If {
            cond,
            then_block,
            else_branch,
            ..
        } => {
            collect_expr(cond, out);
            collect_block(then_block, out);
            match else_branch {
                Some(ElseBranch::Block(block)) => collect_block(block, out),
                Some(ElseBranch::If(nested)) => collect_expr(nested, out),
                None => {}
            }
        }
        Expr::Match {
            scrutinee,
            arms,
            span,
        } => {
            collect_expr(scrutinee, out);
            out.push(Container {
                start: span.start,
                end: span.end,
                children: arms.iter().map(|arm| arm.pattern.span().start).collect(),
            });
            for arm in arms {
                match &arm.body {
                    MatchBody::Expr(inner) => collect_expr(inner, out),
                    MatchBody::Block(block) => collect_block(block, out),
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_expr(lhs, out);
            collect_expr(rhs, out);
        }
        Expr::Unary { value, .. } => collect_expr(value, out),
        Expr::Int { .. }
        | Expr::Float { .. }
        | Expr::Str { .. }
        | Expr::Bool { .. }
        | Expr::NoneLit { .. }
        | Expr::Ident { .. } => {}
    }
}

fn collect_jsx(element: &JsxElement, out: &mut Vec<Container>) {
    for attr in &element.attrs {
        if let JsxAttrValue::Expr(expr) = &attr.value {
            collect_expr(expr, out);
        }
    }
    for child in &element.children {
        match child {
            JsxChild::Element(nested) => collect_jsx(nested, out),
            JsxChild::Expr(expr) => collect_expr(expr, out),
            JsxChild::Text(_) => {}
        }
    }
}

fn normalize_comment(src: &str, span: Span) -> Result<String, CommentError> {
    let text = &src[span.start..span.end];
    let line_start = src[..span.start]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let prefix = &src[line_start..span.start];
    let line_end = src[span.end..]
        .find('\n')
        .map(|index| span.end + index)
        .unwrap_or(src.len());
    let suffix = &src[span.end..line_end];
    let col = src[line_start..span.start].chars().count() as u32 + 1;
    let line = src[..span.start].matches('\n').count() as u32 + 1;

    if !prefix.trim().is_empty() || !suffix.trim().is_empty() {
        return Err(CommentError {
            message: "inline comments are not supported yet; put the comment on its own line"
                .to_string(),
            line,
            col,
        });
    }

    if let Some(rest) = text.strip_prefix("//") {
        // Keep `///` (and longer runs of slashes) intact.
        if rest.starts_with('/') {
            return Ok(format!("//{}", rest.trim_end()));
        }
        let content = rest.trim();
        return Ok(if content.is_empty() {
            "//".to_string()
        } else {
            format!("// {content}")
        });
    }

    let inner = text
        .strip_prefix("/*")
        .and_then(|rest| rest.strip_suffix("*/"))
        .unwrap_or(text);
    let lines: Vec<&str> = inner.lines().collect();
    if lines.len() <= 1 {
        let content = inner.trim();
        return Ok(if content.is_empty() {
            "/**/".to_string()
        } else {
            format!("/* {content} */")
        });
    }

    let mut contents: Vec<String> = lines
        .iter()
        .map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix('*')
                .map(|rest| rest.trim_start().to_string())
                .unwrap_or_else(|| trimmed.to_string())
        })
        .collect();
    while contents.first().is_some_and(|line| line.is_empty()) {
        contents.remove(0);
    }
    while contents.last().is_some_and(|line| line.is_empty()) {
        contents.pop();
    }

    let mut out = String::from("/*");
    for line in contents {
        out.push('\n');
        if line.is_empty() {
            out.push_str(" *");
        } else {
            out.push_str(" * ");
            out.push_str(&line);
        }
    }
    out.push('\n');
    out.push_str(" */");
    Ok(out)
}

#[derive(Default)]
struct Formatter {
    attachment: Attachment,
}

impl Formatter {
    fn comments(&self, out: &mut String, start: usize, level: usize) {
        if let Some(comments) = self.attachment.before.get(&start) {
            for comment in comments {
                for line in comment.lines() {
                    out.push_str(&pad(level));
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
    }

    fn trailing(&self, out: &mut String, start: usize, level: usize) {
        if let Some(comments) = self.attachment.trailing.get(&start) {
            for comment in comments {
                for line in comment.lines() {
                    out.push_str(&pad(level));
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
    }

    fn file(&self, file: &File) -> String {
        let mut out = String::new();

        for import in &file.imports {
            self.comments(&mut out, import.span.start, 0);
            out.push_str(&self.import_text(import));
            out.push('\n');
        }
        if !file.imports.is_empty() && !file.packages.is_empty() {
            out.push('\n');
        }
        for (index, package) in file.packages.iter().enumerate() {
            if index > 0 {
                let already_blank = out.ends_with("\n\n");
                if !already_blank {
                    out.push('\n');
                }
            }
            self.comments(&mut out, package.span.start, 0);
            out.push_str(&self.package_text(package));
            out.push('\n');
        }
        self.trailing(&mut out, file.span.end, 0);
        out
    }

    fn import_text(&self, import: &ImportDecl) -> String {
        let mut out = String::from("import ");
        out.push_str(&import.package);
        if let Some(alias) = &import.alias {
            out.push_str(" as ");
            out.push_str(alias);
        }
        if !import.items.is_empty() {
            let items: Vec<String> = import
                .items
                .iter()
                .map(|item| match &item.alias {
                    Some(alias) => format!("{} as {}", item.name, alias),
                    None => item.name.clone(),
                })
                .collect();
            out.push_str(" { ");
            out.push_str(&items.join(", "));
            out.push_str(" }");
        }
        out.push_str(" from \"");
        out.push_str(&escape_string(&import.path));
        out.push('"');
        out
    }

    fn package_text(&self, package: &PackageDecl) -> String {
        let head = format!("package {}", package.name);

        let mut body = String::new();
        for (index, item) in package.items.iter().enumerate() {
            if index > 0 {
                body.push('\n');
            }
            self.comments(&mut body, item.span().start, 1);
            body.push_str(&pad(1));
            body.push_str(&self.package_item_text(item, 1));
            body.push('\n');
        }
        self.trailing(&mut body, package.span.end, 1);

        if body.is_empty() {
            return format!("{head} {{}}");
        }
        format!("{head} {{\n{body}}}")
    }

    fn package_item_text(&self, item: &PackageItem, indent: usize) -> String {
        match item {
            PackageItem::Struct(decl) => self.struct_text(decl, indent),
            PackageItem::Enum(decl) => self.enum_text(decl, indent),
            PackageItem::Fn(decl) => self.fn_text(decl, indent),
            PackageItem::Binding(decl) => self.binding_text(decl, indent),
            PackageItem::Test(decl) => self.test_text(decl, indent),
        }
    }

    fn struct_text(&self, decl: &StructDecl, indent: usize) -> String {
        let mut head = String::new();
        if decl.is_export {
            head.push_str("export ");
        }
        head.push_str("struct ");
        head.push_str(&decl.name);
        head.push_str(&generics_text(&decl.generics));

        let inner = indent + 1;
        let mut body = String::new();
        for field in &decl.fields {
            self.comments(&mut body, field.span.start, inner);
            body.push_str(&pad(inner));
            body.push_str(&format!("{}: {}", field.name, type_text(&field.ty)));
            body.push('\n');
        }
        self.trailing(&mut body, decl.span.end, inner);

        if body.is_empty() {
            return format!("{head} {{}}");
        }
        format!("{head} {{\n{body}{}}}", pad(indent))
    }

    fn fn_text(&self, decl: &FnDecl, indent: usize) -> String {
        let mut out = String::new();
        if decl.is_export {
            out.push_str("export ");
        }
        out.push_str("fn ");
        out.push_str(&decl.name);
        out.push_str(&generics_text(&decl.generics));
        out.push('(');

        let params: Vec<String> = decl
            .params
            .iter()
            .map(|param| format!("{}: {}", param.name, type_text(&param.ty)))
            .collect();
        out.push_str(&params.join(", "));
        out.push(')');

        if let Some(ret) = &decl.ret {
            out.push_str(" -> ");
            out.push_str(&type_text(ret));
        }
        out.push(' ');
        out.push_str(&self.block(&decl.body, indent));
        out
    }

    fn binding_text(&self, decl: &BindingDecl, indent: usize) -> String {
        let mut out = String::new();
        if decl.is_export {
            out.push_str("export ");
        }
        out.push_str("let ");
        out.push_str(&decl.name);
        if let Some(ty) = &decl.ty {
            out.push_str(": ");
            out.push_str(&type_text(ty));
        }
        out.push_str(" = ");
        out.push_str(&self.expr(&decl.value, indent));
        out
    }

    fn test_text(&self, decl: &TestDecl, indent: usize) -> String {
        format!(
            "test \"{}\" {}",
            escape_string(&decl.title),
            self.block(&decl.body, indent)
        )
    }

    fn enum_text(&self, decl: &EnumDecl, indent: usize) -> String {
        let mut head = String::new();
        if decl.is_export {
            head.push_str("export ");
        }
        head.push_str("enum ");
        head.push_str(&decl.name);
        head.push_str(&generics_text(&decl.generics));

        let inner = indent + 1;
        let mut body = String::new();
        for variant in &decl.variants {
            self.comments(&mut body, variant.span.start, inner);
            body.push_str(&pad(inner));
            body.push_str(&variant.name);
            if !variant.payload.is_empty() {
                let payload: Vec<String> = variant.payload.iter().map(type_text).collect();
                body.push('(');
                body.push_str(&payload.join(", "));
                body.push(')');
            }
            body.push('\n');
        }
        self.trailing(&mut body, decl.span.end, inner);

        if body.is_empty() {
            return format!("{head} {{}}");
        }
        format!("{head} {{\n{body}{}}}", pad(indent))
    }

    fn block(&self, block: &Block, indent: usize) -> String {
        let mut out = String::from("{\n");
        for stmt in &block.stmts {
            self.comments(&mut out, stmt.span().start, indent + 1);
            out.push_str(&pad(indent + 1));
            out.push_str(&self.stmt_text(stmt, indent + 1));
            out.push('\n');
        }
        self.trailing(&mut out, block.span.end, indent + 1);
        out.push_str(&pad(indent));
        out.push('}');
        out
    }

    fn stmt_text(&self, stmt: &Stmt, indent: usize) -> String {
        match stmt {
            Stmt::Let {
                mutable,
                name,
                ty,
                value,
                ..
            } => {
                let mut out = String::from("let ");
                if *mutable {
                    out.push_str("mut ");
                }
                out.push_str(name);
                if let Some(ty) = ty {
                    out.push_str(": ");
                    out.push_str(&type_text(ty));
                }
                out.push_str(" = ");
                out.push_str(&self.expr(value, indent));
                out
            }
            Stmt::Assign { name, value, .. } => {
                format!("{name} = {}", self.expr(value, indent))
            }
            Stmt::Return { value: None, .. } => "return".to_string(),
            Stmt::Return {
                value: Some(value), ..
            } => format!("return {}", self.expr(value, indent)),
            Stmt::For {
                var, iter, body, ..
            } => format!(
                "for {var} in {} {}",
                self.expr(iter, indent),
                self.block(body, indent)
            ),
            Stmt::While { cond, body, .. } => {
                format!(
                    "while {} {}",
                    self.expr(cond, indent),
                    self.block(body, indent)
                )
            }
            Stmt::Expr(expr) => self.expr(expr, indent),
        }
    }

    fn expr_with_min(&self, value: &Expr, indent: usize, min: u8) -> String {
        let text = self.expr(value, indent);
        if precedence(value) < min {
            format!("({text})")
        } else {
            text
        }
    }

    fn expr(&self, value: &Expr, indent: usize) -> String {
        match value {
            Expr::Int { raw, .. } | Expr::Float { raw, .. } => raw.clone(),
            Expr::Str { value, .. } => format!("\"{}\"", escape_string(value)),
            Expr::Bool { value, .. } => value.to_string(),
            Expr::NoneLit { .. } => "None".to_string(),
            Expr::Ident { name, .. } => name.clone(),
            Expr::List { items, .. } => self.list_text(items, indent),
            Expr::Call { callee, args, .. } => self.call_text(callee, args, indent),
            Expr::Field { base, name, .. } => {
                format!("{}.{}", self.expr_with_min(base, indent, 8), name)
            }
            Expr::Try { value, .. } => format!("{}?", self.expr_with_min(value, indent, 8)),
            Expr::Template { parts, exprs, .. } => self.template_text(parts, exprs, indent),
            Expr::TaggedTemplate {
                tag, parts, exprs, ..
            } => format!(
                "{}{}",
                self.expr_with_min(tag, indent, 8),
                self.template_text(parts, exprs, indent)
            ),
            Expr::Jsx(element) => self.jsx_text(element, indent),
            Expr::Lambda { params, body, .. } => {
                let body_text = match body {
                    LambdaBody::Expr(body) => self.expr_with_min(body, indent, 1),
                    LambdaBody::Block(body) => self.block(body, indent),
                };
                format!("fn({}) => {body_text}", params.join(", "))
            }
            Expr::If {
                cond,
                then_block,
                else_branch,
                ..
            } => {
                let mut out = format!(
                    "if {} {}",
                    self.expr(cond, indent),
                    self.block(then_block, indent)
                );
                match else_branch {
                    None => {}
                    Some(ElseBranch::Block(body)) => {
                        out.push_str(" else ");
                        out.push_str(&self.block(body, indent));
                    }
                    Some(ElseBranch::If(nested)) => {
                        out.push_str(" else ");
                        out.push_str(&self.expr(nested, indent));
                    }
                }
                out
            }
            Expr::Match {
                scrutinee,
                arms,
                span,
            } => {
                let mut out = format!("match {} {{\n", self.expr(scrutinee, indent));
                for arm in arms {
                    self.comments(&mut out, arm.pattern.span().start, indent + 1);
                    out.push_str(&pad(indent + 1));
                    out.push_str(&pattern_text(&arm.pattern));
                    out.push_str(" => ");
                    match &arm.body {
                        MatchBody::Expr(body) => out.push_str(&self.expr(body, indent + 1)),
                        MatchBody::Block(body) => out.push_str(&self.block(body, indent + 1)),
                    }
                    out.push('\n');
                }
                self.trailing(&mut out, span.end, indent + 1);
                out.push_str(&pad(indent));
                out.push('}');
                out
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                let prec = match op {
                    BinOp::Or => 1,
                    BinOp::And => 2,
                    BinOp::Eq | BinOp::Ne => 3,
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 4,
                    BinOp::Add | BinOp::Sub => 5,
                    BinOp::Mul | BinOp::Div | BinOp::Rem => 6,
                };
                format!(
                    "{} {} {}",
                    self.expr_with_min(lhs, indent, prec),
                    binop_symbol(*op),
                    self.expr_with_min(rhs, indent, prec + 1)
                )
            }
            Expr::Unary { op, value, .. } => match op {
                UnOp::Not => format!("!{}", self.expr_with_min(value, indent, 7)),
                UnOp::Neg => format!("-{}", self.expr_with_min(value, indent, 7)),
            },
        }
    }

    fn list_text(&self, items: &[Expr], indent: usize) -> String {
        if items.is_empty() {
            return "[]".to_string();
        }
        let rendered: Vec<String> = items
            .iter()
            .map(|item| self.expr(item, indent + 1))
            .collect();
        let single = format!("[{}]", rendered.join(", "));
        if fits_at(indent, &single) {
            return single;
        }
        let mut out = String::from("[\n");
        for item in &rendered {
            out.push_str(&pad(indent + 1));
            out.push_str(item);
            out.push_str(",\n");
        }
        out.push_str(&pad(indent));
        out.push(']');
        out
    }

    fn call_text(&self, callee: &Expr, args: &Args, indent: usize) -> String {
        let head = self.expr_with_min(callee, indent, 8);
        let rendered: Vec<String> = match args {
            Args::Empty => Vec::new(),
            Args::Positional(items) => items
                .iter()
                .map(|item| self.expr(item, indent + 1))
                .collect(),
            Args::Named(items) => items
                .iter()
                .map(|item| format!("{}: {}", item.name, self.expr(&item.value, indent + 1)))
                .collect(),
        };

        if rendered.is_empty() {
            return format!("{head}()");
        }
        let single = format!("{head}({})", rendered.join(", "));
        if fits_at(indent, &single) {
            return single;
        }

        let mut out = format!("{head}(\n");
        for item in &rendered {
            out.push_str(&pad(indent + 1));
            out.push_str(item);
            out.push_str(",\n");
        }
        out.push_str(&pad(indent));
        out.push(')');
        out
    }

    fn template_text(&self, parts: &[String], exprs: &[Expr], indent: usize) -> String {
        let mut out = String::from("`");
        for (index, part) in parts.iter().enumerate() {
            out.push_str(&escape_template(part));
            if let Some(inner) = exprs.get(index) {
                out.push_str("${");
                out.push_str(&self.expr(inner, indent));
                out.push('}');
            }
        }
        out.push('`');
        out
    }

    fn jsx_text(&self, element: &JsxElement, indent: usize) -> String {
        let mut out = format!("<{}", element.name);
        for attr in &element.attrs {
            out.push(' ');
            out.push_str(&attr.name);
            out.push('=');
            match &attr.value {
                JsxAttrValue::Str(value) => {
                    out.push('"');
                    out.push_str(&escape_string(value));
                    out.push('"');
                }
                JsxAttrValue::Expr(value) => {
                    out.push('{');
                    out.push_str(&self.expr(value, indent));
                    out.push('}');
                }
            }
        }

        if element.self_closing {
            out.push_str(" />");
            return out;
        }

        if element.children.is_empty() {
            out.push('>');
            out.push_str(&format!("</{}>", element.name));
            return out;
        }

        let has_text = element
            .children
            .iter()
            .any(|child| matches!(child, JsxChild::Text(_)));
        let inline: String = element
            .children
            .iter()
            .map(|child| self.jsx_child_text(child, indent))
            .collect();

        let single = format!("{out}>{inline}</{}>", element.name);
        if has_text || fits_at(indent, &single) {
            return single;
        }

        out.push_str(">\n");
        for child in &element.children {
            out.push_str(&pad(indent + 1));
            out.push_str(&self.jsx_child_text(child, indent + 1));
            out.push('\n');
        }
        out.push_str(&pad(indent));
        out.push_str(&format!("</{}>", element.name));
        out
    }

    fn jsx_child_text(&self, child: &JsxChild, indent: usize) -> String {
        match child {
            JsxChild::Element(element) => self.jsx_text(element, indent),
            JsxChild::Text(text) => text.clone(),
            JsxChild::Expr(value) => format!("{{{}}}", self.expr(value, indent)),
        }
    }
}

fn generics_text(generics: &[String]) -> String {
    if generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", generics.join(", "))
    }
}

fn type_text(ty: &Type) -> String {
    let mut out = ty.path.join(".");
    if !ty.args.is_empty() {
        let args: Vec<String> = ty.args.iter().map(type_text).collect();
        out.push('<');
        out.push_str(&args.join(", "));
        out.push('>');
    }
    if ty.optional {
        out.push('?');
    }
    out
}

fn precedence(expr: &Expr) -> u8 {
    match expr {
        Expr::Binary { op, .. } => match op {
            BinOp::Or => 1,
            BinOp::And => 2,
            BinOp::Eq | BinOp::Ne => 3,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 4,
            BinOp::Add | BinOp::Sub => 5,
            BinOp::Mul | BinOp::Div | BinOp::Rem => 6,
        },
        Expr::Unary { .. } => 7,
        Expr::If { .. } | Expr::Match { .. } | Expr::Lambda { .. } => 0,
        _ => 8,
    }
}

fn binop_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

fn pattern_text(pattern: &Pattern) -> String {
    match pattern {
        Pattern::Wildcard { .. } => "_".to_string(),
        Pattern::NoneLit { .. } => "None".to_string(),
        Pattern::Variant { path, args, .. } => {
            let mut out = path.join(".");
            if !args.is_empty() {
                let rendered: Vec<String> = args.iter().map(pattern_text).collect();
                out.push('(');
                out.push_str(&rendered.join(", "));
                out.push(')');
            }
            out
        }
    }
}

fn escape_string(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

fn escape_template(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            '$' => out.push_str("\\$"),
            other => out.push(other),
        }
    }
    out
}
