//! Go emitter: renders the typed IR as Go 1.22+.

use std::collections::{BTreeSet, HashSet};

use lt_hir::{ModuleId, Ty};
use lt_ir::*;

const RESERVED: &[&str] = &[
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "map",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "type",
    "var",
];

fn export_name(name: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in name.chars() {
        if ch == '_' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    if out.is_empty() {
        out.push('X');
    }
    out
}

/// How many nested `if`s a pattern needs (the body sits that much deeper).
fn pattern_depth(pattern: &IrPattern) -> usize {
    match pattern {
        IrPattern::Variant { args, .. } => args.iter().map(pattern_depth).max().unwrap_or(0) + 1,
        _ => 0,
    }
}

fn local_name(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for ch in name.chars() {
        if ch == '_' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    if RESERVED.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

fn test_name(title: &str) -> String {
    let mut out = String::from("Test");
    let mut upper = true;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            if upper {
                out.extend(ch.to_uppercase());
                upper = false;
            } else {
                out.push(ch);
            }
        } else {
            upper = true;
        }
    }
    if out == "Test" {
        out.push('X');
    }
    out
}

fn package_name(module_name: &str) -> String {
    let base = module_name.rsplit('/').next().unwrap_or(module_name);
    let mut out = String::new();
    for ch in base.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, 'p');
    }
    if RESERVED.contains(&out.as_str()) {
        out.push('_');
    }
    // Go reserves the `_test` suffix for external test packages; a real package
    // named `foo_test` collides with its own external tests.
    if out.ends_with("_test") {
        out.push_str("_pkg");
    }
    out
}

fn quote(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn indent(level: usize) -> String {
    "\t".repeat(level)
}

#[derive(Clone)]
enum Tail {
    Plain,
    Return,
    Assign(String),
}

struct Frag {
    prelude: Vec<String>,
    code: String,
}

pub fn emit_module(program: &IrProgram, module: &IrModule) -> String {
    let mut emitter = Emitter {
        program,
        module,
        temps: 0,
        used: BTreeSet::new(),
        uses_math: false,
        used_vars: HashSet::new(),
    };

    let mut body = String::new();
    for decl in &module.enums {
        body.push_str(&emitter.enum_decl(decl));
        body.push('\n');
    }
    for strukt in &module.structs {
        body.push_str(&emitter.struct_decl(strukt));
        body.push('\n');
    }
    for binding in &module.bindings {
        body.push_str(&emitter.binding_decl(binding));
        body.push('\n');
    }
    for function in &module.functions {
        body.push_str(&emitter.fn_decl(function));
        body.push('\n');
    }

    trim_end(emitter.header(&body))
}

pub fn emit_tests(program: &IrProgram, module: &IrModule) -> Option<String> {
    if module.tests.is_empty() {
        return None;
    }

    let mut emitter = Emitter {
        program,
        module,
        temps: 0,
        used: BTreeSet::new(),
        uses_math: false,
        used_vars: HashSet::new(),
    };

    let mut body = String::new();
    for test in &module.tests {
        emitter.used_vars = collect_used(&test.body);
        body.push_str(&format!(
            "func {}(t *testing.T) {{\n",
            test_name(&test.title)
        ));
        let lines = emitter.block_lines(&test.body, 1, &Tail::Plain);
        if lines.is_empty() {
            body.push_str("\t_ = t\n");
        }
        for line in lines {
            body.push_str(&line);
            body.push('\n');
        }
        body.push_str("}\n\n");
    }

    let mut out = String::new();
    out.push_str("// Generated by Lingua Tungga. Do not edit by hand.\n");
    out.push_str(&format!("package {}\n\n", package_name(&module.name)));
    // gofmt sorts the second import group by path; "testing" stays alone.
    let mut imports: Vec<(String, String)> = Vec::new();
    if body.contains("lt.") {
        imports.push((
            "linguatungga/lt".to_string(),
            "\tlt \"linguatungga/lt\"\n".to_string(),
        ));
    }
    for id in &emitter.used {
        let path = format!("linguatungga/gen/{}", program.names[*id]);
        imports.push((path.clone(), format!("\tm{} \"{path}\"\n", id)));
    }
    imports.sort_by(|a, b| a.0.cmp(&b.0));
    out.push_str("import (\n");
    out.push_str("\t\"testing\"\n\n");
    for (_, line) in &imports {
        out.push_str(line);
    }
    out.push_str(")\n\n");
    out.push_str(&body);
    Some(trim_end(out))
}

/// Emit a runnable `main` package that calls this module from Go.
pub fn emit_usage(program: &IrProgram, module: &IrModule) -> Option<String> {
    let block = lt_ir::usage_block(module)?;

    let mut emitter = Emitter {
        program,
        module,
        temps: 0,
        used: BTreeSet::new(),
        uses_math: false,
        used_vars: HashSet::new(),
    };

    let mut body = String::new();
    let mut offset = 0;
    for test in &module.tests {
        let end = (offset + test.body.stmts.len()).min(block.stmts.len());
        let test_block = IrBlock {
            stmts: block.stmts[offset..end].to_vec(),
            ty: Ty::Unit,
        };
        offset = end;
        emitter.used_vars = collect_used(&test_block);
        body.push_str("\tfunc() {\n");
        for line in emitter.block_lines(&test_block, 2, &Tail::Plain) {
            body.push_str(&line);
            body.push('\n');
        }
        body.push_str("\t}()\n\n");
    }

    let mut out = String::new();
    out.push_str("// Generated by Lingua Tungga. Do not edit by hand.\n");
    out.push_str("package main\n\n");
    let mut imports: Vec<(String, String)> = Vec::new();
    if body.contains("lt.") {
        imports.push((
            "linguatungga/lt".to_string(),
            "\tlt \"linguatungga/lt\"\n".to_string(),
        ));
    }
    if emitter.uses_math || body.contains("math.") {
        imports.push(("math".to_string(), "\t\"math\"\n".to_string()));
    }
    let self_path = format!("linguatungga/gen/{}", module.name);
    imports.push((self_path.clone(), format!("\t. \"{self_path}\"\n")));
    for id in &emitter.used {
        let path = format!("linguatungga/gen/{}", program.names[*id]);
        imports.push((path.clone(), format!("\tm{} \"{path}\"\n", id)));
    }
    imports.sort_by(|a, b| a.0.cmp(&b.0));
    out.push_str("import (\n");
    for (_, line) in &imports {
        out.push_str(line);
    }
    out.push_str(")\n\n");
    out.push_str("func main() {\n");
    out.push_str(&body);
    out.push_str("}\n");
    Some(trim_end(out))
}

struct Emitter<'a> {
    program: &'a IrProgram,
    module: &'a IrModule,
    temps: usize,
    used: BTreeSet<ModuleId>,
    uses_math: bool,
    used_vars: HashSet<String>,
}

impl<'a> Emitter<'a> {
    fn header(&self, body: &str) -> String {
        let mut out = String::new();
        out.push_str("// Generated by Lingua Tungga. Do not edit by hand.\n");
        out.push_str(&format!("package {}\n\n", package_name(&self.module.name)));
        // gofmt sorts a single import block by path.
        let mut imports: Vec<(String, String)> = Vec::new();
        if body.contains("lt.") {
            imports.push((
                "linguatungga/lt".to_string(),
                "\tlt \"linguatungga/lt\"\n".to_string(),
            ));
        }
        if self.uses_math || body.contains("math.") {
            imports.push(("math".to_string(), "\t\"math\"\n".to_string()));
        }
        for id in &self.used {
            let path = format!("linguatungga/gen/{}", self.program.names[*id]);
            imports.push((path.clone(), format!("\tm{} \"{path}\"\n", id)));
        }
        if !imports.is_empty() {
            imports.sort_by(|a, b| a.0.cmp(&b.0));
            out.push_str("import (\n");
            for (_, line) in &imports {
                out.push_str(line);
            }
            out.push_str(")\n\n");
        }
        out.push_str(body);
        out
    }

    fn temp(&mut self, prefix: &str) -> String {
        self.temps += 1;
        format!("__{prefix}{}", self.temps)
    }

    fn use_module(&mut self, module: ModuleId) {
        if module != self.module.id {
            self.used.insert(module);
        }
    }

    fn is_local(&self, module: ModuleId) -> bool {
        module == self.module.id
    }

    fn is_enum(&self, module: ModuleId, name: &str) -> bool {
        self.program.modules[module]
            .enums
            .iter()
            .any(|decl| decl.name == name)
    }

    fn type_ref(&mut self, ty: &Ty) -> String {
        match ty {
            Ty::Int => "int64".to_string(),
            Ty::Float => "float64".to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::Str => "string".to_string(),
            Ty::Unit => "".to_string(),
            Ty::List(inner) => format!("lt.List[{}]", self.type_ref(inner)),
            Ty::Set(inner) => format!("lt.Set[{}]", self.type_ref(inner)),
            Ty::Map(key, value) => {
                format!("lt.Map[{}, {}]", self.type_ref(key), self.type_ref(value))
            }
            Ty::Dict(value) => format!("lt.Dict[{}]", self.type_ref(value)),
            Ty::Option(_) | Ty::Result(_, _) => "lt.Variant".to_string(),
            Ty::Named(name, args) => {
                if self.is_enum(name.module, &name.name) {
                    return "lt.Variant".to_string();
                }
                self.use_module(name.module);
                let base = if self.is_local(name.module) {
                    name.name.clone()
                } else {
                    format!("m{}.{}", name.module, name.name)
                };
                if args.is_empty() {
                    base
                } else {
                    let rendered: Vec<String> = args.iter().map(|arg| self.type_ref(arg)).collect();
                    format!("{base}[{}]", rendered.join(", "))
                }
            }
            Ty::Generic(name) => name.clone(),
            Ty::Var(_) | Ty::Error => "any".to_string(),
        }
    }

    fn ret_type(&mut self, ty: &Ty) -> String {
        if matches!(ty, Ty::Unit) {
            String::new()
        } else {
            self.type_ref(ty)
        }
    }

    fn variant_ref(&mut self, module: ModuleId, enum_name: &str, variant: &str) -> String {
        self.use_module(module);
        let local = export_name(&format!("{}_{}", enum_name, variant));
        if self.is_local(module) {
            local
        } else {
            format!("m{}.{}", module, local)
        }
    }

    fn cast(&mut self, ty: &Ty, expr: &str) -> String {
        let target = self.type_ref(ty);
        format!("{}.({})", expr, target)
    }

    // --------------------------------------------------------------- decls

    fn enum_decl(&mut self, decl: &IrEnum) -> String {
        let mut out = String::new();
        let decl_generics = if decl.generics.is_empty() {
            String::new()
        } else {
            format!("[{} any]", decl.generics.join(", "))
        };
        for (order, variant) in decl.variants.iter().enumerate() {
            let name = export_name(&format!("{}_{}", decl.name, variant.name));
            if variant.payload.is_empty() {
                out.push_str(&format!(
                    "var {name} = lt.Variant{{Tag: {}, Order: {order}, Type: {}}}\n\n",
                    quote(&variant.name),
                    quote(&decl.name)
                ));
            } else {
                let params: Vec<String> = variant
                    .payload
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| format!("_{index} {}", self.type_ref(ty)))
                    .collect();
                let values: Vec<String> = (0..variant.payload.len())
                    .map(|index| format!("_{index}"))
                    .collect();
                out.push_str(&format!(
                    "func {name}{decl_generics}({}) lt.Variant {{\n\treturn lt.Variant{{Tag: {}, Order: {order}, Type: {}, Values: []any{{{}}}}}\n}}\n\n",
                    params.join(", "),
                    quote(&variant.name),
                    quote(&decl.name),
                    values.join(", ")
                ));
            }
        }
        out
    }

    fn struct_decl(&mut self, class: &IrStruct) -> String {
        let mut out = String::new();
        let decl_generics = if class.generics.is_empty() {
            String::new()
        } else {
            format!("[{} any]", class.generics.join(", "))
        };
        let use_generics = if class.generics.is_empty() {
            String::new()
        } else {
            format!("[{}]", class.generics.join(", "))
        };
        out.push_str(&format!("type {}{} struct {{\n", class.name, decl_generics));
        for field in &class.fields {
            out.push_str(&format!(
                "\t{} {}\n",
                export_name(&field.name),
                self.type_ref(&field.ty)
            ));
        }
        out.push_str("}\n\n");

        let params: Vec<String> = class
            .fields
            .iter()
            .map(|field| {
                let ty = self.type_ref(&field.ty);
                format!("{} {}", local_name(&field.name), ty)
            })
            .collect();
        out.push_str(&format!(
            "func New{}{}({}) {}{} {{\n",
            class.name,
            decl_generics,
            params.join(", "),
            class.name,
            use_generics
        ));
        if class.fields.is_empty() {
            // gofmt keeps a zero-field literal on one line.
            out.push_str(&format!("\treturn {}{}{{}}\n", class.name, use_generics));
        } else {
            out.push_str(&format!("\treturn {}{}{{\n", class.name, use_generics));
            for field in &class.fields {
                out.push_str(&format!(
                    "\t\t{}: {},\n",
                    export_name(&field.name),
                    local_name(&field.name)
                ));
            }
            out.push_str("\t}\n");
        }
        out.push_str("}\n\n");

        let overrides_display = self.module.functions.iter().any(|function| {
            function.name == "to_string"
                && function.is_receiver
                && matches!(function.params.first().map(|p| &p.ty), Some(Ty::Named(name, _)) if name.name == class.name)
        });

        let receiver = format!("self {}{}", class.name, use_generics);
        out.push_str(&format!("func ({receiver}) LtEq(other any) bool {{\n"));
        if class.fields.is_empty() {
            out.push_str(&format!(
                "\t_, ok := other.({}{})\n",
                class.name, use_generics
            ));
            out.push_str("\treturn ok\n");
        } else {
            out.push_str(&format!(
                "\tpeer, ok := other.({}{})\n",
                class.name, use_generics
            ));
            out.push_str("\tif !ok {\n\t\treturn false\n\t}\n");
            let checks: Vec<String> = class
                .fields
                .iter()
                .map(|field| {
                    format!(
                        "lt.Eq(self.{}, peer.{})",
                        export_name(&field.name),
                        export_name(&field.name)
                    )
                })
                .collect();
            out.push_str(&format!("\treturn {}\n", checks.join(" &&\n\t\t")));
        }
        out.push_str("}\n\n");

        out.push_str(&format!("func ({receiver}) LtCmp(other any) int {{\n"));
        if class.fields.is_empty() {
            out.push_str("\treturn 0\n");
        } else {
            out.push_str(&format!(
                "\tpeer, ok := other.({}{})\n",
                class.name, use_generics
            ));
            out.push_str("\tif !ok {\n\t\treturn 0\n\t}\n");
            for field in &class.fields {
                let name = export_name(&field.name);
                out.push_str(&format!(
                    "\tif step := lt.Cmp(self.{name}, peer.{name}); step != 0 {{\n\t\treturn step\n\t}}\n"
                ));
            }
            out.push_str("\treturn 0\n");
        }
        out.push_str("}\n\n");

        out.push_str(&format!("func ({receiver}) LtDisplay() string {{\n"));
        if overrides_display {
            out.push_str("\treturn ToString(self)\n");
        } else if class.fields.is_empty() {
            out.push_str(&format!("\treturn {}\n", quote(&class.name)));
        } else {
            let mut expression = quote(&format!("{}(", class.name));
            for (index, field) in class.fields.iter().enumerate() {
                if index > 0 {
                    expression.push_str(" + \", \"");
                }
                expression.push_str(&format!(
                    " + {} + lt.Disp(self.{})",
                    quote(&format!("{}: ", field.name)),
                    export_name(&field.name)
                ));
            }
            expression.push_str(" + \")\"");
            out.push_str(&format!("\treturn {}\n", expression));
        }
        out.push_str("}\n");
        out
    }

    fn fn_decl(&mut self, function: &IrFn) -> String {
        let params: Vec<String> = function
            .params
            .iter()
            .map(|param| {
                let ty = self.type_ref(&param.ty);
                format!("{} {}", local_name(&param.name), ty)
            })
            .collect();
        let ret = self.ret_type(&function.ret);
        self.used_vars = collect_used(&function.body);
        let name = if function.is_export {
            export_name(&function.name)
        } else {
            local_name(&function.name)
        };
        let decl_generics = if function.generics.is_empty() {
            String::new()
        } else {
            format!("[{} any]", function.generics.join(", "))
        };
        let mut out = format!(
            "func {}{}({}){} {{\n",
            name,
            decl_generics,
            params.join(", "),
            render_return(&ret)
        );
        let tail = if matches!(function.ret, Ty::Unit) {
            Tail::Plain
        } else {
            Tail::Return
        };
        let lines = self.block_lines(&function.body, 1, &tail);
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str("}\n");
        out
    }

    fn binding_decl(&mut self, binding: &IrBinding) -> String {
        let frag = self.expr(&binding.value, 0);
        let ty = self.type_ref(&binding.ty);
        let name = if binding.is_export {
            export_name(&binding.name)
        } else {
            local_name(&binding.name)
        };
        let mut out = String::new();
        for line in frag.prelude {
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str(&format!("var {name} {ty} = {}\n", frag.code));
        out
    }

    // ------------------------------------------------------------ statements

    fn block_lines(&mut self, block: &IrBlock, level: usize, tail: &Tail) -> Vec<String> {
        let mut lines = Vec::new();
        let count = block.stmts.len();
        for (index, stmt) in block.stmts.iter().enumerate() {
            let is_last = index + 1 == count;
            let stmt_tail = if is_last { tail.clone() } else { Tail::Plain };
            lines.extend(self.stmt(stmt, level, &stmt_tail));
        }
        lines
    }

    fn stmt(&mut self, stmt: &IrStmt, level: usize, tail: &Tail) -> Vec<String> {
        let pad = indent(level);
        match stmt {
            IrStmt::Break => vec![format!("{pad}break")],
            IrStmt::Continue => vec![format!("{pad}continue")],
            IrStmt::Let { name, value, .. } => {
                let frag = self.expr(value, level);
                let local = local_name(name);
                let mut lines = frag.prelude;
                lines.push(format!("{pad}{} := {}", local, frag.code));
                if !self.used_vars.contains(name) {
                    lines.push(format!("{pad}_ = {}", local));
                }
                lines
            }
            IrStmt::Assign { name, value } => {
                let frag = self.expr(value, level);
                let mut lines = frag.prelude;
                lines.push(format!("{pad}{} = {}", local_name(name), frag.code));
                lines
            }
            IrStmt::Return(value) => {
                let mut lines = Vec::new();
                match value {
                    Some(value) => {
                        let frag = self.expr(value, level);
                        lines.extend(frag.prelude);
                        lines.push(format!("{pad}return {}", frag.code));
                    }
                    None => lines.push(format!("{pad}return")),
                }
                lines
            }
            IrStmt::For {
                var, iter, body, ..
            } => {
                let frag = self.expr(iter, level);
                let mut lines = frag.prelude;
                lines.push(format!(
                    "{pad}for _, {} := range {}.Items {{",
                    local_name(var),
                    frag.code
                ));
                if !self.used_vars.contains(var) {
                    lines.push(format!("{}_ = {}", indent(level + 1), local_name(var)));
                }
                lines.extend(self.block_lines(body, level + 1, &Tail::Plain));
                lines.push(format!("{pad}}}"));
                lines
            }
            IrStmt::While { cond, body } => {
                let frag = self.expr(cond, level);
                let mut lines = frag.prelude;
                lines.push(format!("{pad}for {} {{", unwrap_condition(&frag.code)));
                lines.extend(self.block_lines(body, level + 1, &Tail::Plain));
                lines.push(format!("{pad}}}"));
                lines
            }
            IrStmt::Expr(expr) => match expr {
                IrExpr::If { .. } => self.if_stmt(expr, level, tail),
                IrExpr::Match { .. } => self.match_stmt(expr, level, tail),
                other => {
                    let is_call = matches!(
                        other,
                        IrExpr::FnCall { .. }
                            | IrExpr::Builtin { .. }
                            | IrExpr::Math { .. }
                            | IrExpr::Expect { .. }
                            | IrExpr::Print { .. }
                    );
                    let frag = self.expr(other, level);
                    let mut lines = frag.prelude;
                    match tail {
                        Tail::Plain => {
                            if is_call {
                                lines.push(format!("{pad}{}", frag.code));
                            } else {
                                lines.push(format!("{pad}_ = {}", frag.code));
                            }
                        }
                        Tail::Return => lines.push(format!("{pad}return {}", frag.code)),
                        Tail::Assign(target) => {
                            lines.push(format!("{pad}{target} = {}", frag.code))
                        }
                    }
                    lines
                }
            },
        }
    }

    fn if_stmt(&mut self, expr: &IrExpr, level: usize, tail: &Tail) -> Vec<String> {
        let IrExpr::If {
            cond,
            then_block,
            else_branch,
            ..
        } = expr
        else {
            unreachable!("if_stmt expects `if`")
        };
        let pad = indent(level);
        let cond_frag = self.expr(cond, level);
        let mut lines = cond_frag.prelude;
        lines.push(format!("{pad}if {} {{", unwrap_condition(&cond_frag.code)));
        lines.extend(self.block_lines(then_block, level + 1, tail));

        let mut current = else_branch;
        loop {
            match current {
                None => {
                    lines.push(format!("{pad}}}"));
                    break;
                }
                Some(IrElse::Block(block)) => {
                    lines.push(format!("{pad}}} else {{"));
                    lines.extend(self.block_lines(block, level + 1, tail));
                    lines.push(format!("{pad}}}"));
                    break;
                }
                Some(IrElse::If(nested)) => {
                    if let IrExpr::If {
                        cond,
                        then_block,
                        else_branch,
                        ..
                    } = nested.as_ref()
                    {
                        let cond_frag = self.expr(cond, level);
                        lines.extend(cond_frag.prelude);
                        lines.push(format!(
                            "{pad}}} else if {} {{",
                            unwrap_condition(&cond_frag.code)
                        ));
                        lines.extend(self.block_lines(then_block, level + 1, tail));
                        current = else_branch;
                    } else {
                        lines.push(format!("{pad}}}"));
                        break;
                    }
                }
            }
        }
        lines
    }

    fn match_stmt(&mut self, expr: &IrExpr, level: usize, tail: &Tail) -> Vec<String> {
        let IrExpr::Match {
            scrutinee, arms, ..
        } = expr
        else {
            unreachable!("match_stmt expects `match`")
        };
        let pad = indent(level);
        let scrutinee_frag = self.expr(scrutinee, level);
        let binding = self.temp("match");
        let done = self.temp("matched");
        let mut lines = scrutinee_frag.prelude;
        lines.push(format!("{pad}{binding} := {}", scrutinee_frag.code));
        // `switch` cannot fall through on a nested mismatch or a false guard, so
        // arms are ifs guarded by a "still looking for a match" flag.
        lines.push(format!("{pad}{done} := false"));

        let mut catch_all = false;
        for arm in arms {
            let args = match &arm.pattern {
                IrPattern::Variant { args, .. } => args.as_slice(),
                _ => &[],
            };
            let depth = args.iter().map(pattern_depth).max().unwrap_or(0);
            let guard_level = level + 1 + depth;
            let body_level = guard_level + usize::from(arm.guard.is_some());
            match &arm.pattern {
                IrPattern::Variant { variant, .. } => {
                    lines.push(format!(
                        "{pad}if !{done} && {binding}.Tag == {} {{",
                        quote(variant)
                    ));
                    let arm_vars = collect_used(&arm.body);
                    let mut core = self.block_lines(&arm.body, body_level, tail);
                    if !matches!(tail, Tail::Return) {
                        core.push(format!("{}{done} = true", indent(body_level)));
                    }
                    if let Some(guard) = &arm.guard {
                        let frag = self.expr(guard, guard_level);
                        let mut wrapped = frag.prelude;
                        wrapped.push(format!("{}if {} {{", indent(guard_level), frag.code));
                        wrapped.extend(core);
                        wrapped.push(format!("{}}}", indent(guard_level)));
                        core = wrapped;
                    }
                    let mut inner = core;
                    for index in (0..args.len()).rev() {
                        inner = self.pattern_block(
                            &args[index],
                            &format!("{binding}.Values[{index}]"),
                            level + 1,
                            &arm_vars,
                            inner,
                        );
                    }
                    lines.extend(inner);
                    lines.push(format!("{pad}}}"));
                }
                IrPattern::Binding { name, ty } => {
                    catch_all = true;
                    let cast = self.cast(ty, &binding);
                    lines.push(format!("{pad}if !{done} {{"));
                    let local = local_name(name);
                    let arm_vars = collect_used(&arm.body);
                    lines.push(format!("{}{} := {}", indent(level + 1), local, cast));
                    if !arm_vars.contains(name) {
                        lines.push(format!("{}_ = {}", indent(level + 1), local));
                    }
                    if let Some(guard) = &arm.guard {
                        let frag = self.expr(guard, level + 2);
                        lines.extend(frag.prelude);
                        lines.push(format!("{}if {} {{", indent(level + 2), frag.code));
                        lines.extend(self.block_lines(&arm.body, level + 3, tail));
                        if !matches!(tail, Tail::Return) {
                            lines.push(format!("{}{done} = true", indent(level + 3)));
                        }
                        lines.push(format!("{}}}", indent(level + 2)));
                    } else {
                        lines.extend(self.block_lines(&arm.body, level + 2, tail));
                        if !matches!(tail, Tail::Return) {
                            lines.push(format!("{}{done} = true", indent(level + 2)));
                        }
                    }
                    lines.push(format!("{pad}}}"));
                }
                IrPattern::Wildcard => {
                    catch_all = true;
                    lines.push(format!("{pad}if !{done} {{"));
                    if let Some(guard) = &arm.guard {
                        let frag = self.expr(guard, level + 2);
                        lines.extend(frag.prelude);
                        lines.push(format!("{}if {} {{", indent(level + 2), frag.code));
                        lines.extend(self.block_lines(&arm.body, level + 3, tail));
                        if !matches!(tail, Tail::Return) {
                            lines.push(format!("{}{done} = true", indent(level + 3)));
                        }
                        lines.push(format!("{}}}", indent(level + 2)));
                    } else {
                        lines.extend(self.block_lines(&arm.body, level + 2, tail));
                        if !matches!(tail, Tail::Return) {
                            lines.push(format!("{}{done} = true", indent(level + 2)));
                        }
                    }
                    lines.push(format!("{pad}}}"));
                }
            }
        }

        if !catch_all {
            lines.push(format!("{pad}if !{done} {{"));
            lines.push(format!("{}panic(\"unreachable\")", indent(level + 1)));
            lines.push(format!("{pad}}}"));
            if matches!(tail, Tail::Return) {
                lines.push(format!("{pad}panic(\"unreachable\")"));
            }
        }

        lines
    }

    /// Lines that bind and run `core` when the nested pattern matches; when it
    /// does not, the enclosing `case` falls through to the next clause.
    fn pattern_block(
        &mut self,
        pattern: &IrPattern,
        value: &str,
        level: usize,
        used: &HashSet<String>,
        core: Vec<String>,
    ) -> Vec<String> {
        let pad = indent(level);
        match pattern {
            IrPattern::Wildcard => core,
            IrPattern::Binding { name, ty } => {
                let cast = self.cast(ty, value);
                let local = local_name(name);
                let mut lines = vec![format!("{pad}{local} := {cast}")];
                if !used.contains(name) {
                    lines.push(format!("{pad}_ = {local}"));
                }
                lines.extend(core);
                lines
            }
            IrPattern::Variant { variant, args } => {
                // A payload is stored as `any`, so a nested variant needs the cast
                // before `.Tag`/`.Values` can be read.
                let scrutinee = format!("{value}.(lt.Variant)");
                let mut lines = vec![format!("{pad}if {scrutinee}.Tag == {} {{", quote(variant))];
                let mut inner = core;
                for index in (0..args.len()).rev() {
                    inner = self.pattern_block(
                        &args[index],
                        &format!("{scrutinee}.Values[{index}]"),
                        level + 1,
                        used,
                        inner,
                    );
                }
                lines.extend(inner);
                lines.push(format!("{pad}}}"));
                lines
            }
        }
    }

    // ---------------------------------------------------------- expressions

    fn expr(&mut self, expr: &IrExpr, level: usize) -> Frag {
        match expr {
            IrExpr::Int(value) => Frag {
                prelude: Vec::new(),
                // Explicit conversion: an untyped constant would infer `int`
                // inside generic constructors such as `lt.Some(1)`.
                code: format!("int64({value})"),
            },
            IrExpr::Float(value) => Frag {
                prelude: Vec::new(),
                code: format_float(*value),
            },
            IrExpr::Str(value) => Frag {
                prelude: Vec::new(),
                code: quote(value),
            },
            IrExpr::Bool(value) => Frag {
                prelude: Vec::new(),
                code: value.to_string(),
            },
            IrExpr::Var(name) => Frag {
                prelude: Vec::new(),
                code: local_name(name),
            },
            IrExpr::BindingRef { module, name, .. } => {
                let code = if *module == self.module.id {
                    let exported = self
                        .module
                        .bindings
                        .iter()
                        .any(|b| b.name == *name && b.is_export);
                    if exported {
                        export_name(name)
                    } else {
                        local_name(name)
                    }
                } else {
                    self.use_module(*module);
                    format!("m{}.{}", module, export_name(name))
                };
                Frag {
                    prelude: Vec::new(),
                    code,
                }
            }
            IrExpr::List { items, ty } => {
                let element = match ty {
                    Ty::List(inner) => self.type_ref(inner),
                    _ => "any".to_string(),
                };
                let mut prelude = Vec::new();
                let rendered: Vec<String> = items
                    .iter()
                    .map(|item| {
                        let frag = self.expr(item, level);
                        prelude.extend(frag.prelude);
                        frag.code
                    })
                    .collect();
                Frag {
                    prelude,
                    code: format!("lt.NewList([]{}{{{}}})", element, rendered.join(", ")),
                }
            }
            IrExpr::MapFrom {
                entries,
                key_ty,
                value_ty,
                prelude,
                ..
            } => {
                self.use_module(*prelude);
                let frag = self.expr(entries, level);
                let key = self.type_ref(key_ty);
                let value = self.type_ref(value_ty);
                Frag {
                    prelude: frag.prelude,
                    code: format!(
                        "lt.MapFrom[{key}, {value}, m{prelude}.Entry[{key}, {value}]]({}, func(__entry m{prelude}.Entry[{key}, {value}]) {key} {{ return __entry.Key }}, func(__entry m{prelude}.Entry[{key}, {value}]) {value} {{ return __entry.Value }})",
                        frag.code
                    ),
                }
            }
            IrExpr::Json {
                parse,
                args,
                prelude,
                ..
            } => {
                let mut prelude_lines = Vec::new();
                let rendered: Vec<String> = args
                    .iter()
                    .map(|arg| {
                        let frag = self.expr(arg, level);
                        prelude_lines.extend(frag.prelude);
                        frag.code
                    })
                    .collect();
                let code = if *parse {
                    self.use_module(*prelude);
                    format!(
                        "lt.JSONParse({}, lt.ValueFactory{{Null: m{}.ValueNull, Bool: m{}.ValueBool, Int: m{}.ValueInt, Float: m{}.ValueFloat, Str: m{}.ValueStr, Array: m{}.ValueArray, Object: m{}.ValueObject}})",
                        rendered.join(", "),
                        prelude,
                        prelude,
                        prelude,
                        prelude,
                        prelude,
                        prelude,
                        prelude
                    )
                } else {
                    format!("lt.JSONStringify({})", rendered.join(", "))
                };
                Frag {
                    prelude: prelude_lines,
                    code,
                }
            }
            IrExpr::DictLit { entries, ty } => {
                let value = match ty {
                    Ty::Dict(value) => self.type_ref(value),
                    _ => "any".to_string(),
                };
                let mut prelude = Vec::new();
                let rendered: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| {
                        let key_frag = self.expr(key, level);
                        prelude.extend(key_frag.prelude);
                        let value_frag = self.expr(value, level);
                        prelude.extend(value_frag.prelude);
                        format!("{{Key: {}, Value: {}}}", key_frag.code, value_frag.code)
                    })
                    .collect();
                Frag {
                    prelude,
                    code: format!(
                        "lt.NewDict([]lt.DictEntry[{value}]{{{}}})",
                        rendered.join(", ")
                    ),
                }
            }
            IrExpr::EmptyCollection { ty } => {
                let code = match ty {
                    Ty::Map(key, value) => format!(
                        "lt.NewMap[{}, {}]()",
                        self.type_ref(key),
                        self.type_ref(value)
                    ),
                    Ty::Set(inner) => format!("lt.NewSet[{}]()", self.type_ref(inner)),
                    _ => "lt.NewList([]any{})".to_string(),
                };
                Frag {
                    prelude: Vec::new(),
                    code,
                }
            }
            IrExpr::Field { base, name, .. } => {
                let frag = self.expr(base, level);
                Frag {
                    prelude: frag.prelude,
                    code: format!("{}.{}", frag.code, export_name(name)),
                }
            }
            IrExpr::StructNew {
                module,
                strukt,
                args,
                ..
            } => {
                let reference = if self.is_local(*module) {
                    format!("New{}", strukt)
                } else {
                    self.use_module(*module);
                    format!("m{}.New{}", module, strukt)
                };
                let mut prelude = Vec::new();
                let rendered: Vec<String> = args
                    .iter()
                    .map(|(_, value)| {
                        let frag = self.expr(value, level);
                        prelude.extend(frag.prelude);
                        frag.code
                    })
                    .collect();
                Frag {
                    prelude,
                    code: format!("{reference}({})", rendered.join(", ")),
                }
            }
            IrExpr::NewVariant {
                target,
                variant,
                args,
                ty,
            } => {
                let mut prelude = Vec::new();
                let rendered: Vec<String> = args
                    .iter()
                    .map(|arg| {
                        let frag = self.expr(arg, level);
                        prelude.extend(frag.prelude);
                        frag.code
                    })
                    .collect();
                let code = match target {
                    VariantRef::Option => {
                        if variant == "Some" {
                            format!("lt.Some({})", rendered.join(", "))
                        } else {
                            "lt.None()".to_string()
                        }
                    }
                    VariantRef::Result => {
                        if variant == "Ok" {
                            format!("lt.Ok({})", rendered.join(", "))
                        } else {
                            format!("lt.Err({})", rendered.join(", "))
                        }
                    }
                    VariantRef::Enum { module, name } => {
                        let reference = self.variant_ref(*module, name, variant);
                        if rendered.is_empty() {
                            reference
                        } else {
                            // Go cannot infer type arguments when the payload erases
                            // a type parameter (Option/Result/nested enums), so pass
                            // the enum's resolved arguments explicitly.
                            let type_args = match ty {
                                Ty::Named(_, args) if !args.is_empty() => {
                                    let rendered: Vec<String> =
                                        args.iter().map(|arg| self.type_ref(arg)).collect();
                                    format!("[{}]", rendered.join(", "))
                                }
                                _ => String::new(),
                            };
                            format!("{reference}{type_args}({})", rendered.join(", "))
                        }
                    }
                };
                Frag { prelude, code }
            }
            IrExpr::FnCall {
                module, name, args, ..
            } => {
                let mut prelude = Vec::new();
                let rendered: Vec<String> = args
                    .iter()
                    .map(|arg| {
                        let frag = self.expr(arg, level);
                        prelude.extend(frag.prelude);
                        frag.code
                    })
                    .collect();
                let local = *module == self.module.id;
                let exported = self.program.modules[*module]
                    .functions
                    .iter()
                    .any(|f| f.name == *name && f.is_export);
                let target = if local {
                    if exported {
                        export_name(name)
                    } else {
                        local_name(name)
                    }
                } else {
                    self.use_module(*module);
                    let name = if exported {
                        export_name(name)
                    } else {
                        local_name(name)
                    };
                    format!("m{}.{}", module, name)
                };
                Frag {
                    prelude,
                    code: format!("{target}({})", rendered.join(", ")),
                }
            }
            IrExpr::Builtin {
                kind,
                receiver,
                recv_ty,
                args,
                ty,
            } => {
                let recv = self.expr(receiver, level);
                let mut prelude = recv.prelude;
                let rendered: Vec<String> = args
                    .iter()
                    .map(|arg| {
                        let frag = self.expr(arg, level);
                        prelude.extend(frag.prelude);
                        frag.code
                    })
                    .collect();
                let receiver_ty = recv_ty.clone();
                let result_ty = ty.clone();
                Frag {
                    prelude,
                    code: self.builtin(*kind, &recv.code, &receiver_ty, &result_ty, &rendered),
                }
            }
            IrExpr::Math { name, args, .. } => {
                let mut prelude = Vec::new();
                let rendered: Vec<String> = args
                    .iter()
                    .map(|arg| {
                        let frag = self.expr(arg, level);
                        prelude.extend(frag.prelude);
                        frag.code
                    })
                    .collect();
                self.uses_math = true;
                let code = match name.as_str() {
                    "pi" => "math.Pi".to_string(),
                    "min" => format!("math.Min({})", rendered.join(", ")),
                    "max" => format!("math.Max({})", rendered.join(", ")),
                    other => format!("math.{}({})", export_name(other), rendered.join(", ")),
                };
                Frag { prelude, code }
            }
            IrExpr::Expect { name, args } => {
                let mut prelude = Vec::new();
                let rendered: Vec<String> = args
                    .iter()
                    .map(|arg| {
                        let frag = self.expr(arg, level);
                        prelude.extend(frag.prelude);
                        frag.code
                    })
                    .collect();
                let function = match name.as_str() {
                    "eq" | "equal" => "ExpectEqual",
                    "not_equal" => "ExpectNotEqual",
                    "true" => "ExpectTrue",
                    "false" => "ExpectFalse",
                    other => other,
                };
                Frag {
                    prelude,
                    code: format!("lt.{function}({})", rendered.join(", ")),
                }
            }
            IrExpr::Print { value } => {
                let frag = self.expr(value, level);
                Frag {
                    prelude: frag.prelude,
                    code: format!("lt.Print({})", frag.code),
                }
            }
            IrExpr::ToStr { value, .. } => {
                let frag = self.expr(value, level);
                Frag {
                    prelude: frag.prelude,
                    code: format!("lt.Disp({})", frag.code),
                }
            }
            IrExpr::Template { parts, exprs } => {
                let mut prelude = Vec::new();
                let mut code = quote(parts.first().map(String::as_str).unwrap_or(""));
                for (index, inner) in exprs.iter().enumerate() {
                    let frag = self.expr(inner, level);
                    prelude.extend(frag.prelude);
                    code.push_str(&format!(" + lt.Disp({})", frag.code));
                    if let Some(part) = parts.get(index + 1) {
                        if !part.is_empty() {
                            code.push_str(&format!(" + {}", quote(part)));
                        }
                    }
                }
                Frag { prelude, code }
            }
            IrExpr::Try {
                value,
                source_ty,
                ty,
            } => {
                let frag = self.expr(value, level);
                let mut prelude = frag.prelude;
                let temp = self.temp("try");
                let pad = indent(level);
                let error_tag = match source_ty {
                    Ty::Option(_) => "none",
                    _ => "err",
                };
                prelude.push(format!("{pad}{temp} := {}", frag.code));
                prelude.push(format!("{pad}if {temp}.Tag == {} {{", quote(error_tag)));
                prelude.push(format!("{}return {temp}", indent(level + 1)));
                prelude.push(format!("{pad}}}"));
                let inner = self.cast(ty, &format!("{temp}.Values[0]"));
                Frag {
                    prelude,
                    code: inner,
                }
            }
            IrExpr::If { ty, .. } => {
                let target = self.temp("if");
                let pad = indent(level);
                let declared = self.type_ref(ty);
                let mut prelude = vec![format!("{pad}var {target} {declared}")];
                prelude.extend(self.if_stmt(expr, level, &Tail::Assign(target.clone())));
                Frag {
                    prelude,
                    code: target,
                }
            }
            IrExpr::Match { ty, .. } => {
                let target = self.temp("match");
                let pad = indent(level);
                let declared = self.type_ref(ty);
                let mut prelude = vec![format!("{pad}var {target} {declared}")];
                prelude.extend(self.match_stmt(expr, level, &Tail::Assign(target.clone())));
                Frag {
                    prelude,
                    code: target,
                }
            }
            IrExpr::Bin { op, lhs, rhs, ty } => {
                let left = self.expr(lhs, level);
                let right = self.expr(rhs, level);
                let mut prelude = left.prelude;
                prelude.extend(right.prelude);
                let code = match op {
                    BinOp::Add => format!("({} + {})", left.code, right.code),
                    BinOp::Sub => format!("({} - {})", left.code, right.code),
                    BinOp::Mul => format!("({} * {})", left.code, right.code),
                    BinOp::Div => {
                        if matches!(ty, Ty::Float) {
                            format!("lt.FDiv({}, {})", left.code, right.code)
                        } else {
                            format!("({} / {})", left.code, right.code)
                        }
                    }
                    BinOp::Rem => {
                        if matches!(ty, Ty::Float) {
                            format!("math.Mod({}, {})", left.code, right.code)
                        } else {
                            format!("({} % {})", left.code, right.code)
                        }
                    }
                    BinOp::Eq => format!("lt.Eq({}, {})", left.code, right.code),
                    BinOp::Ne => format!("!lt.Eq({}, {})", left.code, right.code),
                    BinOp::Lt => format!("({} < {})", left.code, right.code),
                    BinOp::Le => format!("({} <= {})", left.code, right.code),
                    BinOp::Gt => format!("({} > {})", left.code, right.code),
                    BinOp::Ge => format!("({} >= {})", left.code, right.code),
                    BinOp::And => format!("({} && {})", left.code, right.code),
                    BinOp::Or => format!("({} || {})", left.code, right.code),
                };
                Frag { prelude, code }
            }
            IrExpr::Un { op, value, .. } => {
                let frag = self.expr(value, level);
                let code = match op {
                    UnOp::Not => format!("(!{})", frag.code),
                    UnOp::Neg => format!("(-{})", frag.code),
                };
                Frag {
                    prelude: frag.prelude,
                    code,
                }
            }
            IrExpr::Lambda { params, ret, body } => {
                let rendered: Vec<String> = params
                    .iter()
                    .map(|param| {
                        let ty = self.type_ref(&param.ty);
                        format!("{} {}", local_name(&param.name), ty)
                    })
                    .collect();
                let ret = self.ret_type(ret);
                let mut out = format!("func({}){} {{", rendered.join(", "), render_return(&ret));
                match body {
                    IrLambdaBody::Expr(expr) => {
                        let frag = self.expr(expr, level);
                        for line in frag.prelude {
                            out.push('\n');
                            out.push_str(&line);
                        }
                        out.push('\n');
                        out.push_str(&format!("{}return {}", indent(level + 1), frag.code));
                        out.push('\n');
                        out.push_str(&indent(level));
                        out.push('}');
                    }
                    IrLambdaBody::Block(block) => {
                        for line in self.block_lines(block, level + 1, &Tail::Return) {
                            out.push('\n');
                            out.push_str(&line);
                        }
                        out.push('\n');
                        out.push_str(&indent(level));
                        out.push('}');
                    }
                }
                Frag {
                    prelude: Vec::new(),
                    code: out,
                }
            }
        }
    }

    fn builtin(
        &mut self,
        kind: Builtin,
        recv: &str,
        recv_ty: &Ty,
        result_ty: &Ty,
        args: &[String],
    ) -> String {
        let method = |name: &str| format!("{recv}.{}({})", name, args.join(", "));
        let option_inner = || match recv_ty {
            Ty::Option(inner) => (**inner).clone(),
            _ => Ty::Error,
        };
        let option_result = || match result_ty {
            Ty::Option(inner) => (**inner).clone(),
            _ => Ty::Error,
        };
        let result_ok = || match recv_ty {
            Ty::Result(ok, _) => (**ok).clone(),
            _ => Ty::Error,
        };
        let result_err = || match recv_ty {
            Ty::Result(_, err) => (**err).clone(),
            _ => Ty::Error,
        };
        let result_mapped = || match result_ty {
            Ty::Result(ok, _) => (**ok).clone(),
            _ => Ty::Error,
        };
        let result_mapped_err = || match result_ty {
            Ty::Result(_, err) => (**err).clone(),
            _ => Ty::Error,
        };
        let list_element = || match recv_ty {
            Ty::List(inner) => (**inner).clone(),
            _ => Ty::Error,
        };
        let list_mapped = || match result_ty {
            Ty::List(inner) => (**inner).clone(),
            _ => Ty::Error,
        };

        match kind {
            Builtin::StrLen => format!("lt.StrLen({recv})"),
            Builtin::StrChar => format!("lt.StrChar({recv}, {})", args.join(", ")),
            Builtin::SetItems => format!("lt.NewList({recv}.Items)"),
            Builtin::StrTrim => format!("lt.StrTrim({recv})"),
            Builtin::StrTrimIndent => format!("lt.StrTrimIndent({recv})"),
            Builtin::StrLower => format!("lt.StrLower({recv})"),
            Builtin::StrUpper => format!("lt.StrUpper({recv})"),
            Builtin::StrSplit => format!("lt.StrSplit({recv}, {})", args.join(", ")),
            Builtin::StrStartsWith => format!("lt.StrStartsWith({recv}, {})", args.join(", ")),
            Builtin::StrEndsWith => format!("lt.StrEndsWith({recv}, {})", args.join(", ")),
            Builtin::StrContains => format!("lt.StrContains({recv}, {})", args.join(", ")),
            Builtin::StrReplace => format!("lt.StrReplace({recv}, {})", args.join(", ")),
            Builtin::StrSlice => format!("lt.StrSlice({recv}, {})", args.join(", ")),
            Builtin::StrChars => format!("lt.StrChars({recv})"),
            Builtin::StrParseInt => format!("lt.StrParseInt({recv})"),
            Builtin::StrParseFloat => format!("lt.StrParseFloat({recv})"),
            Builtin::ListLen => format!("{recv}.Len()"),
            Builtin::ListGet => method("Get"),
            Builtin::ListPush => method("Push"),
            Builtin::ListPop => format!("{recv}.Pop()"),
            Builtin::ListMap => {
                let from = self.type_ref(&list_element());
                let to = self.type_ref(&list_mapped());
                format!("lt.MapList[{from}, {to}]({recv}, {})", args.join(", "))
            }
            Builtin::ListFilter => method("Filter"),
            Builtin::ListFold => {
                let element = self.type_ref(&list_element());
                let accumulator = self.type_ref(result_ty);
                format!(
                    "lt.FoldList[{element}, {accumulator}]({recv}, {})",
                    args.join(", ")
                )
            }
            Builtin::ListAny => method("Any"),
            Builtin::ListAll => method("All"),
            Builtin::ListSort => format!("{recv}.Sort()"),
            Builtin::ListReverse => format!("{recv}.Reverse()"),
            Builtin::ListConcat => method("Concat"),
            Builtin::ListContains => method("Contains"),
            Builtin::ListIndexOf => method("IndexOf"),
            Builtin::ListJoin => format!("lt.JoinList({recv}, {})", args.join(", ")),
            Builtin::SetLen => format!("{recv}.Len()"),
            Builtin::SetInsert => method("Insert"),
            Builtin::SetRemove => method("Remove"),
            Builtin::SetContains => method("Contains"),
            Builtin::SetUnion => method("Union"),
            Builtin::SetIntersect => method("Intersect"),
            Builtin::SetDifference => method("Difference"),
            Builtin::MapLen => format!("{recv}.Len()"),
            Builtin::MapGet => method("Get"),
            Builtin::MapInsert => method("Insert"),
            Builtin::MapRemove => method("Remove"),
            Builtin::MapKeys => format!("{recv}.Keys()"),
            Builtin::MapValues => format!("{recv}.Values()"),
            Builtin::MapContainsKey => method("ContainsKey"),
            Builtin::DictLen => format!("{recv}.Len()"),
            Builtin::DictGet => method("Get"),
            Builtin::DictInsert => method("Insert"),
            Builtin::DictRemove => method("Remove"),
            Builtin::DictKeys => format!("{recv}.Keys()"),
            Builtin::DictValues => format!("{recv}.Values()"),
            Builtin::DictContainsKey => method("ContainsKey"),
            Builtin::OptionIsSome => format!("lt.OptIsSome({recv})"),
            Builtin::OptionIsNone => format!("lt.OptIsNone({recv})"),
            Builtin::OptionUnwrap => {
                let ty = self.type_ref(&option_inner());
                format!("lt.OptUnwrap[{ty}]({recv})")
            }
            Builtin::OptionUnwrapOr => {
                let ty = self.type_ref(&option_inner());
                format!("lt.OptUnwrapOr[{ty}]({recv}, {})", args.join(", "))
            }
            Builtin::OptionMap => {
                let from = self.type_ref(&option_inner());
                let to = self.type_ref(&option_result());
                format!("lt.OptMap[{from}, {to}]({recv}, {})", args.join(", "))
            }
            Builtin::OptionAndThen => {
                let from = self.type_ref(&option_inner());
                let to = self.type_ref(&option_result());
                format!("lt.OptAndThen[{from}, {to}]({recv}, {})", args.join(", "))
            }
            Builtin::ResultIsOk => format!("lt.ResIsOk({recv})"),
            Builtin::ResultIsErr => format!("lt.ResIsErr({recv})"),
            Builtin::ResultUnwrap => {
                let ok = self.type_ref(&result_ok());
                let err = self.type_ref(&result_err());
                format!("lt.ResUnwrap[{ok}, {err}]({recv})")
            }
            Builtin::ResultUnwrapOr => {
                let ok = self.type_ref(&result_ok());
                let err = self.type_ref(&result_err());
                format!("lt.ResUnwrapOr[{ok}, {err}]({recv}, {})", args.join(", "))
            }
            Builtin::ResultMap => {
                let ok = self.type_ref(&result_ok());
                let mapped = self.type_ref(&result_mapped());
                let err = self.type_ref(&result_err());
                format!(
                    "lt.ResMap[{ok}, {mapped}, {err}]({recv}, {})",
                    args.join(", ")
                )
            }
            Builtin::ResultMapErr => {
                let ok = self.type_ref(&result_ok());
                let err = self.type_ref(&result_err());
                let mapped = self.type_ref(&result_mapped_err());
                format!(
                    "lt.ResMapErr[{ok}, {err}, {mapped}]({recv}, {})",
                    args.join(", ")
                )
            }
            Builtin::ResultAndThen => {
                let ok = self.type_ref(&result_ok());
                let mapped = self.type_ref(&result_mapped());
                let err = self.type_ref(&result_err());
                format!(
                    "lt.ResAndThen[{ok}, {mapped}, {err}]({recv}, {})",
                    args.join(", ")
                )
            }
        }
    }
}

fn render_return(ret: &str) -> String {
    if ret.is_empty() {
        String::new()
    } else {
        format!(" {ret}")
    }
}

fn collect_used(block: &IrBlock) -> HashSet<String> {
    let mut used = HashSet::new();
    for stmt in &block.stmts {
        collect_stmt(stmt, &mut used);
    }
    used
}

fn collect_stmt(stmt: &IrStmt, used: &mut HashSet<String>) {
    match stmt {
        IrStmt::Let { value, .. } => collect_expr(value, used),
        IrStmt::Assign { value, .. } => collect_expr(value, used),
        IrStmt::Return(Some(value)) => collect_expr(value, used),
        IrStmt::Return(None) => {}
        IrStmt::For { iter, body, .. } => {
            collect_expr(iter, used);
            for name in collect_used(body) {
                used.insert(name);
            }
        }
        IrStmt::While { cond, body } => {
            collect_expr(cond, used);
            for name in collect_used(body) {
                used.insert(name);
            }
        }
        IrStmt::Break | IrStmt::Continue => {}
        IrStmt::Expr(expr) => collect_expr(expr, used),
    }
}

fn collect_expr(expr: &IrExpr, used: &mut HashSet<String>) {
    match expr {
        IrExpr::Var(name) => {
            used.insert(name.clone());
        }
        IrExpr::List { items, .. } => {
            for item in items {
                collect_expr(item, used);
            }
        }
        IrExpr::DictLit { entries, .. } => {
            for (key, value) in entries {
                collect_expr(key, used);
                collect_expr(value, used);
            }
        }
        IrExpr::MapFrom { entries, .. } => collect_expr(entries, used),
        IrExpr::Json { args, .. } => {
            for arg in args {
                collect_expr(arg, used);
            }
        }
        IrExpr::Field { base, .. } => collect_expr(base, used),
        IrExpr::StructNew { args, .. } => {
            for (_, value) in args {
                collect_expr(value, used);
            }
        }
        IrExpr::BindingRef { .. } => {}
        IrExpr::NewVariant { args, .. } => {
            for arg in args {
                collect_expr(arg, used);
            }
        }
        IrExpr::FnCall { args, .. } => {
            for arg in args {
                collect_expr(arg, used);
            }
        }
        IrExpr::Builtin { receiver, args, .. } => {
            collect_expr(receiver, used);
            for arg in args {
                collect_expr(arg, used);
            }
        }
        IrExpr::Math { args, .. } => {
            for arg in args {
                collect_expr(arg, used);
            }
        }
        IrExpr::Expect { args, .. } => {
            for arg in args {
                collect_expr(arg, used);
            }
        }
        IrExpr::Print { value } => collect_expr(value, used),
        IrExpr::ToStr { value, .. } => collect_expr(value, used),
        IrExpr::Template { exprs, .. } => {
            for inner in exprs {
                collect_expr(inner, used);
            }
        }
        IrExpr::Try { value, .. } => collect_expr(value, used),
        IrExpr::If {
            cond,
            then_block,
            else_branch,
            ..
        } => {
            collect_expr(cond, used);
            for name in collect_used(then_block) {
                used.insert(name);
            }
            match else_branch {
                Some(IrElse::Block(block)) => {
                    for name in collect_used(block) {
                        used.insert(name);
                    }
                }
                Some(IrElse::If(inner)) => collect_expr(inner, used),
                None => {}
            }
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            collect_expr(scrutinee, used);
            for arm in arms {
                for name in collect_used(&arm.body) {
                    used.insert(name);
                }
            }
        }
        IrExpr::Bin { lhs, rhs, .. } => {
            collect_expr(lhs, used);
            collect_expr(rhs, used);
        }
        IrExpr::Un { value, .. } => collect_expr(value, used),
        IrExpr::Lambda { body, .. } => match body {
            IrLambdaBody::Expr(expr) => collect_expr(expr, used),
            IrLambdaBody::Block(block) => {
                for name in collect_used(block) {
                    used.insert(name);
                }
            }
        },
        IrExpr::Int(_)
        | IrExpr::Float(_)
        | IrExpr::Str(_)
        | IrExpr::Bool(_)
        | IrExpr::EmptyCollection { .. } => {}
    }
}

/// gofmt drops a redundant outer parenthesis pair in `if`/`for` conditions.
/// Only strip it when the opening paren matches the final closing paren, so
/// `(a) && (b)` is left alone.
fn unwrap_condition(code: &str) -> String {
    let trimmed = code.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return trimmed.to_string();
    }
    let mut depth = 0usize;
    for (index, ch) in trimmed.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && index + 1 != trimmed.len() {
                    return trimmed.to_string();
                }
            }
            _ => {}
        }
    }
    trimmed[1..trimmed.len() - 1].trim().to_string()
}

/// gofmt ends a file with exactly one newline and no trailing blank line.
fn trim_end(mut out: String) -> String {
    while out.ends_with('\n') {
        out.pop();
    }
    out.push('\n');
    out
}

fn format_float(value: f64) -> String {
    if value.is_nan() {
        return "math.NaN()".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 {
            "math.Inf(1)".to_string()
        } else {
            "math.Inf(-1)".to_string()
        };
    }
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value:?}")
    }
}
