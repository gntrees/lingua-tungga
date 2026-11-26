//! Canonical pretty-printer.
//!
//! `fmt(fmt(x)) == fmt(x)` is the contract. Layout decisions depend only on the
//! rendered text and the indentation, never on the original source, so the
//! output is stable across runs and machines.

use crate::ast::*;

pub const WIDTH: usize = 100;

const INDENT: &str = "  ";

fn pad(level: usize) -> String {
    INDENT.repeat(level)
}

fn fits_at(indent: usize, text: &str) -> bool {
    !text.contains('\n') && indent * INDENT.len() + text.chars().count() <= WIDTH
}

pub fn format_file(file: &File) -> String {
    let mut out = String::new();

    for import in &file.imports {
        out.push_str(&import_text(import));
        out.push('\n');
    }
    if !file.imports.is_empty() && !file.items.is_empty() {
        out.push('\n');
    }
    for (index, item) in file.items.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&item_text(item));
        out.push('\n');
    }

    out
}

fn import_text(import: &ImportDecl) -> String {
    format!(
        "import {} from \"{}\"",
        import.name,
        escape_string(&import.path)
    )
}

fn item_text(item: &Item) -> String {
    match item {
        Item::Class(class) => class_text(class),
        Item::Enum(decl) => enum_text(decl),
        Item::TestClass(decl) => test_class_text(decl),
    }
}

fn class_text(class: &ClassDecl) -> String {
    let mut head = String::new();
    if class.is_pub {
        head.push_str("pub ");
    }
    head.push_str("class ");
    head.push_str(&class.name);
    head.push_str(&generics_text(&class.generics));

    if class.fields.is_empty() && class.methods.is_empty() {
        return format!("{head} {{}}");
    }

    let mut body = String::new();
    for field in &class.fields {
        body.push_str(&pad(1));
        body.push_str(&format!("{}: {}", field.name, type_text(&field.ty)));
        body.push('\n');
    }
    if !class.fields.is_empty() && !class.methods.is_empty() {
        body.push('\n');
    }
    for (index, method) in class.methods.iter().enumerate() {
        if index > 0 {
            body.push('\n');
        }
        body.push_str(&pad(1));
        body.push_str(&method_text(method, 1));
        body.push('\n');
    }

    format!("{head} {{\n{body}}}")
}

fn test_class_text(decl: &TestClassDecl) -> String {
    let head = format!("test class {}", decl.name);
    if decl.tests.is_empty() && decl.helpers.is_empty() {
        return format!("{head} {{}}");
    }

    let mut body = String::new();
    for (index, helper) in decl.helpers.iter().enumerate() {
        if index > 0 {
            body.push('\n');
        }
        body.push_str(&pad(1));
        body.push_str(&method_text(helper, 1));
        body.push('\n');
    }
    for (index, test) in decl.tests.iter().enumerate() {
        if index > 0 || !decl.helpers.is_empty() {
            body.push('\n');
        }
        body.push_str(&pad(1));
        body.push_str(&format!(
            "test fn {}() {}",
            test.name,
            block(&test.body, 1)
        ));
        body.push('\n');
    }

    format!("{head} {{\n{body}}}")
}

fn enum_text(decl: &EnumDecl) -> String {
    let mut head = String::new();
    if decl.is_pub {
        head.push_str("pub ");
    }
    head.push_str("enum ");
    head.push_str(&decl.name);
    head.push_str(&generics_text(&decl.generics));

    if decl.variants.is_empty() {
        return format!("{head} {{}}");
    }

    let mut body = String::new();
    for variant in &decl.variants {
        body.push_str(&pad(1));
        body.push_str(&variant.name);
        if !variant.payload.is_empty() {
            let payload: Vec<String> = variant.payload.iter().map(type_text).collect();
            body.push('(');
            body.push_str(&payload.join(", "));
            body.push(')');
        }
        body.push('\n');
    }

    format!("{head} {{\n{body}}}")
}

fn generics_text(generics: &[String]) -> String {
    if generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", generics.join(", "))
    }
}

fn method_text(method: &Method, indent: usize) -> String {
    let mut out = String::new();
    if method.is_pub {
        out.push_str("pub ");
    }
    if method.is_static {
        out.push_str("static ");
    }
    out.push_str("fn ");
    out.push_str(&method.name);
    out.push('(');

    let mut params: Vec<String> = Vec::new();
    if method.has_self {
        params.push("self".to_string());
    }
    for param in &method.params {
        params.push(format!("{}: {}", param.name, type_text(&param.ty)));
    }
    out.push_str(&params.join(", "));
    out.push(')');

    if let Some(ret) = &method.ret {
        out.push_str(" -> ");
        out.push_str(&type_text(ret));
    }
    out.push(' ');
    out.push_str(&block(&method.body, indent));
    out
}

fn block(block: &Block, indent: usize) -> String {
    if block.stmts.is_empty() {
        return "{}".to_string();
    }
    let mut out = String::from("{\n");
    for stmt in &block.stmts {
        out.push_str(&pad(indent + 1));
        out.push_str(&stmt_text(stmt, indent + 1));
        out.push('\n');
    }
    out.push_str(&pad(indent));
    out.push('}');
    out
}

fn stmt_text(stmt: &Stmt, indent: usize) -> String {
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
            out.push_str(&expr(value, indent));
            out
        }
        Stmt::Assign { name, value, .. } => format!("{name} = {}", expr(value, indent)),
        Stmt::Return { value: None, .. } => "return".to_string(),
        Stmt::Return {
            value: Some(value), ..
        } => format!("return {}", expr(value, indent)),
        Stmt::For {
            var, iter, body, ..
        } => format!("for {var} in {} {}", expr(iter, indent), block(body, indent)),
        Stmt::While { cond, body, .. } => {
            format!("while {} {}", expr(cond, indent), block(body, indent))
        }
        Stmt::Expr(expr_value) => expr(expr_value, indent),
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

fn expr_with_min(value: &Expr, indent: usize, min: u8) -> String {
    let text = expr(value, indent);
    if precedence(value) < min {
        format!("({text})")
    } else {
        text
    }
}

fn expr(value: &Expr, indent: usize) -> String {
    match value {
        Expr::Int { raw, .. } | Expr::Float { raw, .. } => raw.clone(),
        Expr::Str { value, .. } => format!("\"{}\"", escape_string(value)),
        Expr::Bool { value, .. } => value.to_string(),
        Expr::NoneLit { .. } => "None".to_string(),
        Expr::Ident { name, .. } => name.clone(),
        Expr::List { items, .. } => list_text(items, indent),
        Expr::Call { callee, args, .. } => call_text(callee, args, indent),
        Expr::Field { base, name, .. } => format!("{}.{}", expr_with_min(base, indent, 8), name),
        Expr::Try { value, .. } => format!("{}?", expr_with_min(value, indent, 8)),
        Expr::Template { parts, exprs, .. } => template_text(parts, exprs, indent),
        Expr::TaggedTemplate {
            tag, parts, exprs, ..
        } => format!(
            "{}{}",
            expr_with_min(tag, indent, 8),
            template_text(parts, exprs, indent)
        ),
        Expr::Jsx(element) => jsx_text(element, indent),
        Expr::Lambda { params, body, .. } => {
            let body_text = match body {
                LambdaBody::Expr(body) => expr_with_min(body, indent, 1),
                LambdaBody::Block(body) => block(body, indent),
            };
            format!("fn({}) => {body_text}", params.join(", "))
        }
        Expr::If {
            cond,
            then_block,
            else_branch,
            ..
        } => {
            let mut out = format!("if {} {}", expr(cond, indent), block(then_block, indent));
            match else_branch {
                None => {}
                Some(ElseBranch::Block(body)) => {
                    out.push_str(" else ");
                    out.push_str(&block(body, indent));
                }
                Some(ElseBranch::If(nested)) => {
                    out.push_str(" else ");
                    out.push_str(&expr(nested, indent));
                }
            }
            out
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            let mut out = format!("match {} {{\n", expr(scrutinee, indent));
            for arm in arms {
                out.push_str(&pad(indent + 1));
                out.push_str(&pattern_text(&arm.pattern));
                out.push_str(" => ");
                match &arm.body {
                    MatchBody::Expr(body) => out.push_str(&expr(body, indent + 1)),
                    MatchBody::Block(body) => out.push_str(&block(body, indent + 1)),
                }
                out.push('\n');
            }
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
                expr_with_min(lhs, indent, prec),
                binop_symbol(*op),
                expr_with_min(rhs, indent, prec + 1)
            )
        }
        Expr::Unary { op, value, .. } => match op {
            UnOp::Not => format!("!{}", expr_with_min(value, indent, 7)),
            UnOp::Neg => format!("-{}", expr_with_min(value, indent, 7)),
        },
    }
}

fn list_text(items: &[Expr], indent: usize) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }
    let rendered: Vec<String> = items.iter().map(|item| expr(item, indent + 1)).collect();
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

fn call_text(callee: &Expr, args: &Args, indent: usize) -> String {
    let head = expr_with_min(callee, indent, 8);
    let rendered: Vec<String> = match args {
        Args::Empty => Vec::new(),
        Args::Positional(items) => items.iter().map(|item| expr(item, indent + 1)).collect(),
        Args::Named(items) => items
            .iter()
            .map(|item| format!("{}: {}", item.name, expr(&item.value, indent + 1)))
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

fn template_text(parts: &[String], exprs: &[Expr], indent: usize) -> String {
    let mut out = String::from("`");
    for (index, part) in parts.iter().enumerate() {
        out.push_str(&escape_template(part));
        if let Some(inner) = exprs.get(index) {
            out.push_str("${");
            out.push_str(&expr(inner, indent));
            out.push('}');
        }
    }
    out.push('`');
    out
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

fn jsx_text(element: &JsxElement, indent: usize) -> String {
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
                out.push_str(&expr(value, indent));
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
        .map(|child| jsx_child_text(child, indent))
        .collect();

    let single = format!("{out}>{inline}</{}>", element.name);
    if has_text || fits_at(indent, &single) {
        return single;
    }

    out.push_str(">\n");
    for child in &element.children {
        out.push_str(&pad(indent + 1));
        out.push_str(&jsx_child_text(child, indent + 1));
        out.push('\n');
    }
    out.push_str(&pad(indent));
    out.push_str(&format!("</{}>", element.name));
    out
}

fn jsx_child_text(child: &JsxChild, indent: usize) -> String {
    match child {
        JsxChild::Element(element) => jsx_text(element, indent),
        JsxChild::Text(text) => text.clone(),
        JsxChild::Expr(value) => format!("{{{}}}", expr(value, indent)),
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
