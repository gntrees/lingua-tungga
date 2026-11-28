//! Python emitter: renders the typed IR as Python 3.11+.

use std::collections::BTreeSet;

use lt_hir::{ModuleId, Ty};
use lt_ir::*;

const RESERVED: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield", "match", "case",
];

fn snake(name: &str) -> String {
    let mut out = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    let mut out = out.trim_start_matches('_').to_string();
    if out.is_empty() {
        out.push('_');
    }
    if RESERVED.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

/// Turn a free-form test title into a valid Python function name.
/// Package-level functions and bindings that shadow a Python builtin, so type
/// annotations in the same module must be qualified (`builtins.int`).
fn module_shadowed(module: &IrModule) -> BTreeSet<String> {
    const PRIMITIVES: [&str; 8] = [
        "int", "float", "bool", "str", "list", "dict", "set", "object",
    ];
    let mut out = BTreeSet::new();
    let mut consider = |name: String| {
        if PRIMITIVES.contains(&name.as_str()) {
            out.insert(name);
        }
    };
    for function in &module.functions {
        consider(if function.is_export {
            snake(&function.name)
        } else {
            format!("_{}", snake(&function.name))
        });
    }
    for binding in &module.bindings {
        consider(if binding.is_export {
            snake(&binding.name)
        } else {
            format!("_{}", snake(&binding.name))
        });
    }
    out
}

fn test_name(title: &str) -> String {
    let mut out = String::new();
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    format!("test_{}", out.trim_matches('_'))
}

fn member(name: &str) -> String {
    let mut out = snake(name);
    if RESERVED.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

/// Names already taken on the generated enum class (`tag`/`values`/`order` are
/// dataclass fields, `to_display` is a method). A variant with one of these
/// names gets a trailing `_` so it does not shadow them.
const ENUM_MEMBERS: &[&str] = &["tag", "values", "order", "to_display"];

fn variant_member(name: &str) -> String {
    let out = member(name);
    if ENUM_MEMBERS.contains(&out.as_str()) {
        format!("{out}_")
    } else {
        out
    }
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
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn indent(level: usize) -> String {
    "    ".repeat(level)
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
        tests_mode: false,
        uses_math: false,
        aliases: Vec::new(),
        declared: std::collections::BTreeSet::new(),
        shadowed: module_shadowed(module),
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

    emitter.header(program, &body)
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
        tests_mode: true,
        uses_math: false,
        aliases: Vec::new(),
        declared: std::collections::BTreeSet::new(),
        shadowed: module_shadowed(module),
    };

    let mut body = String::new();
    for test in &module.tests {
        let test_name = test_name(&test.title);
        body.push_str(&format!("def {test_name}() -> None:\n"));
        emitter.declared.clear();
        let lines = emitter.block_lines(&test.body, 1, &Tail::Plain);
        if lines.is_empty() {
            body.push_str("    pass\n");
        }
        for line in lines {
            body.push_str(&line);
            body.push('\n');
        }
        body.push('\n');
    }

    let mut header = String::new();
    header.push_str("# Generated by Lingua Tungga. Do not edit by hand.\n");
    header.push_str("from __future__ import annotations\n\n");
    if emitter.uses_math {
        header.push_str("import math\n");
    }
    if body.contains("builtins.") {
        header.push_str("import builtins\n");
    }
    header.push_str("import lt_runtime as _lt\n");
    header.push_str("from typing import Any\n");
    for id in &emitter.used {
        header.push_str(&format!(
            "import gen.{} as m{}\n",
            program.names[*id].replace('/', "."),
            id
        ));
    }
    header.push('\n');
    Some(header + &body)
}

/// Emit a runnable program that imports this module and shows how to call it.
pub fn emit_usage(program: &IrProgram, module: &IrModule) -> Option<String> {
    let block = lt_ir::usage_block(module)?;

    let mut emitter = Emitter {
        program,
        module,
        temps: 0,
        used: BTreeSet::new(),
        tests_mode: true,
        uses_math: false,
        aliases: Vec::new(),
        declared: std::collections::BTreeSet::new(),
        shadowed: module_shadowed(module),
    };

    let mut body = String::new();
    let mut offset = 0;
    for (index, test) in module.tests.iter().enumerate() {
        let end = (offset + test.body.stmts.len()).min(block.stmts.len());
        let test_block = IrBlock {
            stmts: block.stmts[offset..end].to_vec(),
            ty: Ty::Unit,
        };
        offset = end;
        body.push_str(&format!("def _usage{index}() -> None:\n"));
        emitter.declared.clear();
        let lines = emitter.block_lines(&test_block, 1, &Tail::Plain);
        if lines.is_empty() {
            body.push_str("    pass\n");
        }
        for line in lines {
            body.push_str(&line);
            body.push('\n');
        }
        body.push('\n');
    }

    let mut header = String::new();
    header.push_str("# Generated by Lingua Tungga. Do not edit by hand.\n");
    header.push_str("from __future__ import annotations\n\n");
    if emitter.uses_math {
        header.push_str("import math\n");
    }
    if body.contains("builtins.") {
        header.push_str("import builtins\n");
    }
    header.push_str("import sys\nfrom pathlib import Path\n\n");
    header.push_str("sys.path.insert(0, str(Path(__file__).resolve().parent.parent))\n\n");
    header.push_str("import lt_runtime as _lt\n");
    header.push_str("from typing import Any\n");
    for id in &emitter.used {
        header.push_str(&format!(
            "import gen.{} as m{}\n",
            program.names[*id].replace('/', "."),
            id
        ));
    }
    header.push('\n');

    let mut main = String::from("def main() -> None:\n");
    for index in 0..module.tests.len() {
        main.push_str(&format!("    _usage{index}()\n"));
    }
    main.push('\n');
    main.push_str("if __name__ == \"__main__\":\n    main()\n");
    Some(header + &body + &main)
}

struct Emitter<'a> {
    program: &'a IrProgram,
    module: &'a IrModule,
    temps: usize,
    used: BTreeSet<ModuleId>,
    tests_mode: bool,
    uses_math: bool,
    /// Renames for `match` pattern bindings. Python has no block scope, so two
    /// arms binding the same name would collide (`mypy --strict` rejects the
    /// redefinition). Each arm gets its own map, pushed while its body is
    /// emitted.
    aliases: Vec<std::collections::HashMap<String, String>>,
    /// Every local name bound so far in the function being emitted. Python has
    /// function scope only, so a `let` in one branch and another `let` with the
    /// same name in a sibling branch would be a redefinition.
    declared: std::collections::BTreeSet<String>,
    /// Builtin type names shadowed by method names in the class being emitted.
    /// Inside that class body `int` would resolve to the method, so primitive
    /// annotations have to be written `builtins.int`.
    shadowed: BTreeSet<String>,
}

impl<'a> Emitter<'a> {
    fn header(&self, program: &IrProgram, body: &str) -> String {
        let mut out = String::new();
        out.push_str("# Generated by Lingua Tungga. Do not edit by hand.\n");
        out.push_str("from __future__ import annotations\n\n");
        if self.uses_math {
            out.push_str("import math\n");
        }
        if body.contains("builtins.") {
            out.push_str("import builtins\n");
        }
        out.push_str("import lt_runtime as _lt\n");
        out.push_str("from dataclasses import dataclass\n");
        for id in &self.used {
            out.push_str(&format!(
                "import gen.{} as m{}\n",
                program.names[*id].replace('/', "."),
                id
            ));
        }
        out.push_str("from typing import Any");
        if body.contains(": ClassVar[") {
            out.push_str(", ClassVar");
        }
        out.push_str("\n\n");
        out.push_str(body);
        out
    }

    fn temp(&mut self, prefix: &str) -> String {
        self.temps += 1;
        format!("__{prefix}{}", self.temps)
    }

    /// Resolve a source name through the active `match` arm maps, innermost
    /// first.
    /// Reserve a unique local name for `name` in the current function.
    fn declare(&mut self, name: &str) -> String {
        let base = member(name);
        if self.declared.insert(base.clone()) {
            return base;
        }
        let mut suffix = 2;
        loop {
            let candidate = format!("{base}_{suffix}");
            if self.declared.insert(candidate.clone()) {
                return candidate;
            }
            suffix += 1;
        }
    }

    /// Bind `name` in the innermost block scope to a fresh unique local.
    fn bind_local(&mut self, name: &str) -> String {
        let local = self.declare(name);
        if let Some(scope) = self.aliases.last_mut() {
            scope.insert(name.to_string(), local.clone());
        }
        local
    }

    fn alias(&self, name: &str) -> String {
        for scope in self.aliases.iter().rev() {
            if let Some(renamed) = scope.get(name) {
                return renamed.clone();
            }
        }
        member(name)
    }

    fn use_module(&mut self, module: ModuleId) {
        if module != self.module.id || self.tests_mode {
            self.used.insert(module);
        }
    }

    fn is_local(&self, module: ModuleId) -> bool {
        module == self.module.id && !self.tests_mode
    }

    fn primitive(&self, name: &str) -> String {
        if self.shadowed.contains(name) {
            format!("builtins.{name}")
        } else {
            name.to_string()
        }
    }

    fn type_ref(&mut self, ty: &Ty) -> String {
        match ty {
            Ty::Int => self.primitive("int"),
            Ty::Float => self.primitive("float"),
            Ty::Bool => self.primitive("bool"),
            Ty::Str => self.primitive("str"),
            Ty::Unit => "None".to_string(),
            Ty::List(inner) => format!("_lt.LList[{}]", self.type_ref(inner)),
            Ty::Set(inner) => format!("_lt.LSet[{}]", self.type_ref(inner)),
            Ty::Map(key, value) => {
                format!("_lt.LMap[{}, {}]", self.type_ref(key), self.type_ref(value))
            }
            Ty::Dict(value) => format!("_lt.LDict[{}]", self.type_ref(value)),
            Ty::Option(inner) => format!("_lt.Opt[{}]", self.type_ref(inner)),
            Ty::Result(ok, err) => {
                format!("_lt.Res[{}, {}]", self.type_ref(ok), self.type_ref(err))
            }
            Ty::Named(name, _) => {
                self.use_module(name.module);
                if self.is_local(name.module) {
                    name.name.clone()
                } else {
                    format!("m{}.{}", name.module, name.name)
                }
            }
            Ty::Generic(_) | Ty::Var(_) | Ty::Error => "Any".to_string(),
        }
    }

    fn struct_ref(&mut self, module: ModuleId, name: &str) -> String {
        self.use_module(module);
        if self.is_local(module) {
            name.to_string()
        } else {
            format!("m{}.{}", module, name)
        }
    }

    fn variant_ref(&mut self, module: ModuleId, enum_name: &str, variant: &str) -> String {
        self.use_module(module);
        let local = format!("{}.{}", enum_name, variant_member(variant));
        if self.is_local(module) {
            local
        } else {
            format!("m{}.{}", module, local)
        }
    }

    // --------------------------------------------------------------- decls

    fn enum_decl(&mut self, decl: &IrEnum) -> String {
        let mut out = String::new();
        out.push_str("@dataclass(frozen=True)\n");
        out.push_str(&format!("class {}:\n", decl.name));
        out.push_str("    tag: str\n    values: tuple[Any, ...]\n    order: int\n");
        // Nullary variants are singletons, not factories: a `ClassVar` keeps
        // `Shape.Point` a value while payload variants are static methods.
        for variant in decl.variants.iter().filter(|v| v.payload.is_empty()) {
            out.push_str(&format!(
                "    {}: ClassVar[{}]\n",
                variant_member(&variant.name),
                decl.name
            ));
        }
        out.push('\n');

        // Display stays structural: the type name is known statically, so the
        // method carries the whole rendering and no module-level helper exists.
        let display_ret = self.primitive("str");
        out.push_str(&format!("    def to_display(self) -> {display_ret}:\n"));
        for variant in &decl.variants {
            out.push_str(&format!(
                "        if self.tag == {}:\n",
                quote(&variant.name)
            ));
            if variant.payload.is_empty() {
                out.push_str(&format!(
                    "            return {}\n",
                    quote(&format!("{}.{}", decl.name, variant.name))
                ));
            } else {
                let mut expression = quote(&format!("{}.{}(", decl.name, variant.name));
                for index in 0..variant.payload.len() {
                    if index > 0 {
                        expression.push_str(" + \", \"");
                    }
                    expression.push_str(&format!(" + _lt.disp(self.values[{index}])"));
                }
                expression.push_str(" + \")\"");
                out.push_str(&format!("            return {}\n", expression));
            }
        }
        out.push_str("        raise AssertionError(\"unreachable\")\n");

        // One namespace per enum: `Shape.rect(...)` instead of a module-level
        // `shape_rect(...)`, so every variant lives on the enum itself.
        for (order, variant) in decl.variants.iter().enumerate() {
            if variant.payload.is_empty() {
                continue;
            }
            let name = variant_member(&variant.name);
            let params: Vec<String> = variant
                .payload
                .iter()
                .enumerate()
                .map(|(index, ty)| format!("_{index}: {}", self.type_ref(ty)))
                .collect();
            let values: Vec<String> = (0..variant.payload.len())
                .map(|index| format!("_{index}"))
                .collect();
            out.push_str("\n    @staticmethod\n");
            out.push_str(&format!(
                "    def {name}({}) -> {}:\n        return {}({}, ({},), {order})\n",
                params.join(", "),
                decl.name,
                decl.name,
                quote(&variant.name),
                values.join(", ")
            ));
        }

        for (order, variant) in decl.variants.iter().enumerate() {
            if !variant.payload.is_empty() {
                continue;
            }
            out.push_str(&format!(
                "\n{}.{} = {}({}, (), {order})\n",
                decl.name,
                variant_member(&variant.name),
                decl.name,
                quote(&variant.name)
            ));
        }
        out
    }

    fn struct_decl(&mut self, class: &IrStruct) -> String {
        let mut out = String::new();
        out.push_str("@dataclass(frozen=True)\n");
        out.push_str(&format!("class {}:\n", class.name));

        if !class.fields.is_empty() {
            for field in &class.fields {
                let ty = self.type_ref(&field.ty);
                out.push_str(&format!("    {}: {}\n", member(&field.name), ty));
            }
            out.push('\n');
        } else {
            out.push_str("    pass\n\n");
        }

        let overrides_display = self.module.functions.iter().any(|function| {
            function.name == "to_string"
                && function.is_receiver
                && matches!(function.params.first().map(|p| &p.ty), Some(Ty::Named(name, _)) if name.name == class.name)
        });

        let cmp_ret = self.primitive("int");
        out.push_str(&format!(
            "    def lt_cmp(self, other: \"{}\") -> {}:\n",
            class.name, cmp_ret
        ));
        for field in &class.fields {
            let name = member(&field.name);
            out.push_str(&format!(
                "        step = _lt.cmp(self.{name}, other.{name})\n        if step != 0:\n            return step\n"
            ));
        }
        out.push_str("        return 0\n\n");

        let display_ret = self.primitive("str");
        out.push_str(&format!("    def to_display(self) -> {display_ret}:\n"));
        if overrides_display {
            out.push_str("        return to_string(self)\n");
        } else if class.fields.is_empty() {
            out.push_str(&format!("        return {}\n", quote(&class.name)));
        } else {
            let mut expression = quote(&format!("{}(", class.name));
            for (index, field) in class.fields.iter().enumerate() {
                if index > 0 {
                    expression.push_str(" + \", \"");
                }
                expression.push_str(&format!(
                    " + {} + _lt.disp(self.{})",
                    quote(&format!("{}: ", field.name)),
                    member(&field.name)
                ));
            }
            expression.push_str(" + \")\"");
            out.push_str(&format!("        return {}\n", expression));
        }

        out
    }

    fn fn_decl(&mut self, function: &IrFn) -> String {
        let params: Vec<String> = function
            .params
            .iter()
            .map(|param| {
                let ty = self.type_ref(&param.ty);
                format!("{}: {}", member(&param.name), ty)
            })
            .collect();
        let ret = self.type_ref(&function.ret);
        let prefix = if function.is_export { "" } else { "_" };
        let mut out = format!(
            "def {}{}({}) -> {}:\n",
            prefix,
            snake(&function.name),
            params.join(", "),
            ret
        );
        let tail = if matches!(function.ret, Ty::Unit) {
            Tail::Plain
        } else {
            Tail::Return
        };
        self.declared.clear();
        for param in &function.params {
            self.declared.insert(member(&param.name));
        }
        let lines = self.block_lines(&function.body, 1, &tail);
        if lines.is_empty() {
            out.push_str("    pass\n");
        }
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    fn binding_decl(&mut self, binding: &IrBinding) -> String {
        let frag = self.expr(&binding.value, 0);
        let mut out = String::new();
        for line in frag.prelude {
            out.push_str(&line);
            out.push('\n');
        }
        let name = if binding.is_export {
            snake(&binding.name)
        } else {
            format!("_{}", snake(&binding.name))
        };
        out.push_str(&format!("{name} = {}\n", frag.code));
        out
    }

    // ------------------------------------------------------------ statements

    fn block_lines(&mut self, block: &IrBlock, level: usize, tail: &Tail) -> Vec<String> {
        self.aliases.push(std::collections::HashMap::new());
        let mut lines = Vec::new();
        let count = block.stmts.len();
        for (index, stmt) in block.stmts.iter().enumerate() {
            let is_last = index + 1 == count;
            let stmt_tail = if is_last { tail.clone() } else { Tail::Plain };
            lines.extend(self.stmt(stmt, level, &stmt_tail));
        }
        self.aliases.pop();
        lines
    }

    fn stmt(&mut self, stmt: &IrStmt, level: usize, tail: &Tail) -> Vec<String> {
        let pad = indent(level);
        match stmt {
            IrStmt::Let {
                name, ty, value, ..
            } => {
                let frag = self.expr(value, level);
                let declared = self.type_ref(ty);
                let local = self.bind_local(name);
                let mut lines = frag.prelude;
                lines.push(format!("{pad}{local}: {declared} = {}", frag.code));
                lines
            }
            IrStmt::Assign { name, value } => {
                let frag = self.expr(value, level);
                let local = self.alias(name);
                let mut lines = frag.prelude;
                lines.push(format!("{pad}{local} = {}", frag.code));
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
                let local = self.declare(var);
                self.aliases.push(std::collections::HashMap::new());
                if let Some(scope) = self.aliases.last_mut() {
                    scope.insert(var.clone(), local.clone());
                }
                lines.push(format!("{pad}for {local} in {}.items:", frag.code));
                lines.extend(self.indented_block(body, level + 1, &Tail::Plain));
                self.aliases.pop();
                lines
            }
            IrStmt::While { cond, body } => {
                let frag = self.expr(cond, level);
                let mut lines = frag.prelude;
                lines.push(format!("{pad}while {}:", frag.code));
                lines.extend(self.indented_block(body, level + 1, &Tail::Plain));
                lines
            }
            IrStmt::Expr(expr) => match expr {
                IrExpr::If { .. } => self.if_stmt(expr, level, tail),
                IrExpr::Match { .. } => self.match_stmt(expr, level, tail),
                other => {
                    let frag = self.expr(other, level);
                    let mut lines = frag.prelude;
                    match tail {
                        Tail::Plain => lines.push(format!("{pad}{}", frag.code)),
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

    /// Body lines for a compound statement, with `pass` when empty.
    fn indented_block(&mut self, block: &IrBlock, level: usize, tail: &Tail) -> Vec<String> {
        let lines = self.block_lines(block, level, tail);
        if lines.is_empty() {
            vec![format!("{}pass", indent(level))]
        } else {
            lines
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
        lines.push(format!("{pad}if {}:", cond_frag.code));
        lines.extend(self.indented_block(then_block, level + 1, tail));

        let mut current = else_branch;
        loop {
            match current {
                None => break,
                Some(IrElse::Block(block)) => {
                    lines.push(format!("{pad}else:"));
                    lines.extend(self.indented_block(block, level + 1, tail));
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
                        lines.push(format!("{pad}elif {}:", cond_frag.code));
                        lines.extend(self.indented_block(then_block, level + 1, tail));
                        current = else_branch;
                    } else {
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
        let mut lines = scrutinee_frag.prelude;
        lines.push(format!("{pad}{binding} = {}", scrutinee_frag.code));

        let mut first = true;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Variant { variant, bindings } => {
                    let keyword = if first { "if" } else { "elif" };
                    lines.push(format!(
                        "{pad}{keyword} {binding}.tag == {}:",
                        quote(variant)
                    ));
                    let body_level = level + 1;
                    let mut body = Vec::new();
                    let mut scope = std::collections::HashMap::new();
                    for (index, (name, ty)) in bindings.iter().enumerate() {
                        let declared = self.type_ref(ty);
                        let local = self.declare(name);
                        scope.insert(name.clone(), local.clone());
                        body.push(format!(
                            "{}{}: {} = {binding}.values[{index}]",
                            indent(body_level),
                            local,
                            declared
                        ));
                    }
                    self.aliases.push(scope);
                    body.extend(self.block_lines(&arm.body, body_level, tail));
                    self.aliases.pop();
                    if body.is_empty() {
                        body.push(format!("{}pass", indent(body_level)));
                    }
                    lines.extend(body);
                    first = false;
                }
                IrPattern::Binding { name, ty } => {
                    let declared = self.type_ref(ty);
                    lines.push(format!("{pad}else:"));
                    let body_level = level + 1;
                    let local = self.declare(name);
                    let mut scope = std::collections::HashMap::new();
                    scope.insert(name.clone(), local.clone());
                    let mut body = vec![format!(
                        "{}{}: {} = {binding}",
                        indent(body_level),
                        local,
                        declared
                    )];
                    self.aliases.push(scope);
                    body.extend(self.block_lines(&arm.body, body_level, tail));
                    self.aliases.pop();
                    lines.extend(body);
                    first = false;
                }
                IrPattern::Wildcard => {
                    lines.push(format!("{pad}else:"));
                    lines.extend(self.indented_block(&arm.body, level + 1, tail));
                    first = false;
                }
            }
        }

        if first {
            lines.push(format!("{pad}raise AssertionError(\"unreachable\")"));
        } else if !matches!(
            arms.last().map(|arm| &arm.pattern),
            Some(IrPattern::Binding { .. }) | Some(IrPattern::Wildcard)
        ) {
            lines.push(format!("{pad}else:"));
            lines.push(format!(
                "{}raise AssertionError(\"unreachable\")",
                indent(level + 1)
            ));
        }

        lines
    }

    // ---------------------------------------------------------- expressions

    fn expr(&mut self, expr: &IrExpr, level: usize) -> Frag {
        match expr {
            IrExpr::Int(value) => Frag {
                prelude: Vec::new(),
                code: value.to_string(),
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
                code: if *value { "True" } else { "False" }.to_string(),
            },
            IrExpr::Var(name) => Frag {
                prelude: Vec::new(),
                code: self.alias(name),
            },
            IrExpr::BindingRef { module, name, .. } => {
                let code = if *module == self.module.id && !self.tests_mode {
                    if self
                        .module
                        .bindings
                        .iter()
                        .any(|b| b.name == *name && b.is_export)
                    {
                        snake(name)
                    } else {
                        format!("_{}", snake(name))
                    }
                } else {
                    self.use_module(*module);
                    let exported = self.program.modules[*module]
                        .bindings
                        .iter()
                        .any(|b| b.name == *name && b.is_export);
                    let name = if exported {
                        snake(name)
                    } else {
                        format!("_{}", snake(name))
                    };
                    format!("m{}.{}", module, name)
                };
                Frag {
                    prelude: Vec::new(),
                    code,
                }
            }
            IrExpr::List { items, .. } => {
                let mut prelude = Vec::new();
                let rendered: Vec<String> = items
                    .iter()
                    .map(|item| {
                        let frag = self.expr(item, level);
                        prelude.extend(frag.prelude);
                        frag.code
                    })
                    .collect();
                let tuple = if rendered.len() == 1 {
                    format!("({},)", rendered[0])
                } else {
                    format!("({})", rendered.join(", "))
                };
                Frag {
                    prelude,
                    code: format!("_lt.LList({tuple})"),
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
                        "_lt.json_parse({}, m{}.Value)",
                        rendered.join(", "),
                        prelude
                    )
                } else {
                    format!("_lt.json_stringify({})", rendered.join(", "))
                };
                Frag {
                    prelude: prelude_lines,
                    code,
                }
            }
            IrExpr::DictLit { entries, .. } => {
                let mut prelude = Vec::new();
                let rendered: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| {
                        let key_frag = self.expr(key, level);
                        prelude.extend(key_frag.prelude);
                        let value_frag = self.expr(value, level);
                        prelude.extend(value_frag.prelude);
                        format!("({}, {})", key_frag.code, value_frag.code)
                    })
                    .collect();
                let tuple = if rendered.len() == 1 {
                    format!("({},)", rendered[0])
                } else {
                    format!("({})", rendered.join(", "))
                };
                Frag {
                    prelude,
                    code: format!("_lt.LDict.of({tuple})"),
                }
            }
            IrExpr::EmptyCollection { ty } => {
                let code = match ty {
                    Ty::Map(_, _) => "_lt.LMap.empty()".to_string(),
                    Ty::Set(_) => "_lt.LSet.empty()".to_string(),
                    _ => "_lt.LList(())".to_string(),
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
                    code: format!("{}.{}", frag.code, member(name)),
                }
            }
            IrExpr::StructNew {
                module,
                strukt,
                args,
                ..
            } => {
                let class_ref = self.struct_ref(*module, strukt);
                let mut prelude = Vec::new();
                let fields: Vec<String> = args
                    .iter()
                    .map(|(name, value)| {
                        let frag = self.expr(value, level);
                        prelude.extend(frag.prelude);
                        format!("{}={}", member(name), frag.code)
                    })
                    .collect();
                Frag {
                    prelude,
                    code: format!("{class_ref}({})", fields.join(", ")),
                }
            }
            IrExpr::NewVariant {
                target,
                variant,
                args,
                ..
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
                            format!("_lt.Some({})", rendered.join(", "))
                        } else {
                            "_lt.None_()".to_string()
                        }
                    }
                    VariantRef::Result => {
                        if variant == "Ok" {
                            format!("_lt.Ok({})", rendered.join(", "))
                        } else {
                            format!("_lt.Err({})", rendered.join(", "))
                        }
                    }
                    VariantRef::Enum { module, name } => {
                        let reference = self.variant_ref(*module, name, variant);
                        if rendered.is_empty() {
                            reference
                        } else {
                            format!("{reference}({})", rendered.join(", "))
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
                let local = *module == self.module.id && !self.tests_mode;
                let exported = self.program.modules[*module]
                    .functions
                    .iter()
                    .any(|f| f.name == *name && f.is_export);
                let target = if local {
                    if exported {
                        snake(name)
                    } else {
                        format!("_{}", snake(name))
                    }
                } else {
                    self.use_module(*module);
                    let name = if exported {
                        snake(name)
                    } else {
                        format!("_{}", snake(name))
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
                args,
                ..
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
                Frag {
                    prelude,
                    code: self.builtin(*kind, &recv.code, &rendered),
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
                    "pi" => "math.pi".to_string(),
                    "pow" => format!("math.pow({})", rendered.join(", ")),
                    "abs" | "min" | "max" => format!("{}({})", name, rendered.join(", ")),
                    other => format!("math.{other}({})", rendered.join(", ")),
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
                    "eq" | "equal" => "expect_equal",
                    "not_equal" => "expect_not_equal",
                    "true" => "expect_true",
                    "false" => "expect_false",
                    other => other,
                };
                Frag {
                    prelude,
                    code: format!("_lt.{function}({})", rendered.join(", ")),
                }
            }
            IrExpr::Print { value } => {
                let frag = self.expr(value, level);
                Frag {
                    prelude: frag.prelude,
                    code: format!("_lt.print_value({})", frag.code),
                }
            }
            IrExpr::ToStr { value, .. } => {
                let frag = self.expr(value, level);
                Frag {
                    prelude: frag.prelude,
                    code: format!("_lt.disp({})", frag.code),
                }
            }
            IrExpr::Template { parts, exprs } => {
                let mut prelude = Vec::new();
                let mut code = quote(parts.first().map(String::as_str).unwrap_or(""));
                for (index, inner) in exprs.iter().enumerate() {
                    let frag = self.expr(inner, level);
                    prelude.extend(frag.prelude);
                    code.push_str(&format!(" + _lt.disp({})", frag.code));
                    if let Some(part) = parts.get(index + 1) {
                        if !part.is_empty() {
                            code.push_str(&format!(" + {}", quote(part)));
                        }
                    }
                }
                Frag { prelude, code }
            }
            IrExpr::Try {
                value, source_ty, ..
            } => {
                let frag = self.expr(value, level);
                let mut prelude = frag.prelude;
                let temp = self.temp("try");
                let pad = indent(level);
                let error_tag = match source_ty {
                    Ty::Option(_) => "none",
                    _ => "err",
                };
                prelude.push(format!("{pad}{temp} = {}", frag.code));
                prelude.push(format!("{pad}if {temp}.tag == {}:", quote(error_tag)));
                prelude.push(format!("{}return {temp}", indent(level + 1)));
                Frag {
                    prelude,
                    code: format!("{temp}.values[0]"),
                }
            }
            IrExpr::If { ty, .. } => {
                let target = self.temp("if");
                let pad = indent(level);
                let declared = self.type_ref(ty);
                let mut prelude = vec![format!("{pad}{target}: {declared}")];
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
                let mut prelude = vec![format!("{pad}{target}: {declared}")];
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
                let code = self.binary(*op, &left.code, &right.code, ty);
                Frag { prelude, code }
            }
            IrExpr::Un { op, value, ty } => {
                let frag = self.expr(value, level);
                let code = match (op, ty) {
                    (UnOp::Not, _) => format!("(not ({}))", frag.code),
                    (UnOp::Neg, Ty::Int) => format!("_lt.ineg({})", frag.code),
                    (UnOp::Neg, _) => format!("(-{})", frag.code),
                };
                Frag {
                    prelude: frag.prelude,
                    code,
                }
            }
            IrExpr::Lambda { params, body, .. } => {
                let rendered: Vec<String> =
                    params.iter().map(|param| member(&param.name)).collect();
                match body {
                    IrLambdaBody::Expr(inner) => {
                        let frag = self.expr(inner, level);
                        if frag.prelude.is_empty() {
                            Frag {
                                prelude: Vec::new(),
                                code: format!("lambda {}: {}", rendered.join(", "), frag.code),
                            }
                        } else {
                            self.lambda_function(params, &[IrStmt::Expr((**inner).clone())], level)
                        }
                    }
                    IrLambdaBody::Block(block) => self.lambda_function(params, &block.stmts, level),
                }
            }
        }
    }

    fn lambda_function(&mut self, params: &[IrParam], stmts: &[IrStmt], level: usize) -> Frag {
        let name = self.temp("lambda");
        let pad = indent(level);
        let rendered: Vec<String> = params
            .iter()
            .map(|param| {
                let ty = self.type_ref(&param.ty);
                format!("{}: {}", member(&param.name), ty)
            })
            .collect();
        let mut prelude = vec![format!("{pad}def {name}({}) -> Any:", rendered.join(", "))];
        let block = IrBlock {
            stmts: stmts.to_vec(),
            ty: Ty::Unit,
        };
        let lines = self.block_lines(&block, level + 1, &Tail::Return);
        if lines.is_empty() {
            prelude.push(format!("{}pass", indent(level + 1)));
        }
        prelude.extend(lines);
        Frag {
            prelude,
            code: name,
        }
    }

    fn binary(&mut self, op: BinOp, left: &str, right: &str, ty: &Ty) -> String {
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                let symbol = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    _ => "%",
                };
                if matches!(ty, Ty::Int) {
                    let function = match op {
                        BinOp::Add => "iadd",
                        BinOp::Sub => "isub",
                        BinOp::Mul => "imul",
                        BinOp::Div => "idiv",
                        _ => "irem",
                    };
                    format!("_lt.{function}({left}, {right})")
                } else if matches!(op, BinOp::Div) {
                    format!("_lt.fdiv({left}, {right})")
                } else {
                    format!("({left} {symbol} {right})")
                }
            }
            BinOp::Eq => format!("_lt.eq({left}, {right})"),
            BinOp::Ne => format!("(not _lt.eq({left}, {right}))"),
            BinOp::Lt => format!("({left} < {right})"),
            BinOp::Le => format!("({left} <= {right})"),
            BinOp::Gt => format!("({left} > {right})"),
            BinOp::Ge => format!("({left} >= {right})"),
            BinOp::And => format!("({left} and {right})"),
            BinOp::Or => format!("({left} or {right})"),
        }
    }

    fn builtin(&mut self, kind: Builtin, recv: &str, args: &[String]) -> String {
        let call = |name: &str| format!("{recv}.{name}({})", args.join(", "));
        match kind {
            Builtin::StrLen => format!("_lt.str_len({recv})"),
            Builtin::StrTrim => format!("{recv}.strip()"),
            Builtin::StrTrimIndent => format!("_lt.str_trim_indent({recv})"),
            Builtin::StrLower => format!("{recv}.lower()"),
            Builtin::StrUpper => format!("{recv}.upper()"),
            Builtin::StrSplit => format!("_lt.str_split({recv}, {})", args.join(", ")),
            Builtin::StrStartsWith => format!("{recv}.startswith({})", args.join(", ")),
            Builtin::StrEndsWith => format!("{recv}.endswith({})", args.join(", ")),
            Builtin::StrContains => format!("({} in {recv})", args.join(", ")),
            Builtin::StrReplace => format!("_lt.str_replace({recv}, {})", args.join(", ")),
            Builtin::StrSlice => format!("_lt.str_slice({recv}, {})", args.join(", ")),
            Builtin::StrChars => format!("_lt.str_chars({recv})"),
            Builtin::StrParseInt => format!("_lt.str_parse_int({recv})"),
            Builtin::StrParseFloat => format!("_lt.str_parse_float({recv})"),
            Builtin::ListLen => format!("{recv}.len()"),
            Builtin::ListGet => call("get"),
            Builtin::ListPush => call("push"),
            Builtin::ListPop => format!("{recv}.pop()"),
            Builtin::ListMap => call("map"),
            Builtin::ListFilter => call("filter"),
            Builtin::ListFold => call("fold"),
            Builtin::ListAny => call("any"),
            Builtin::ListAll => call("all"),
            Builtin::ListSort => format!("{recv}.sort()"),
            Builtin::ListReverse => format!("{recv}.reverse()"),
            Builtin::ListConcat => call("concat"),
            Builtin::ListContains => call("contains"),
            Builtin::ListIndexOf => format!("{recv}.index_of({})", args.join(", ")),
            Builtin::ListJoin => call("join"),
            Builtin::SetLen => format!("{recv}.len()"),
            Builtin::SetInsert => call("insert"),
            Builtin::SetRemove => call("remove"),
            Builtin::SetContains => call("contains"),
            Builtin::SetUnion => call("union"),
            Builtin::SetIntersect => call("intersect"),
            Builtin::SetDifference => call("difference"),
            Builtin::MapLen => format!("{recv}.len()"),
            Builtin::MapGet => call("get"),
            Builtin::MapInsert => call("insert"),
            Builtin::MapRemove => call("remove"),
            Builtin::MapKeys => format!("{recv}.keys()"),
            Builtin::MapValues => format!("{recv}.values()"),
            Builtin::MapContainsKey => format!("{recv}.contains_key({})", args.join(", ")),
            Builtin::DictLen => format!("{recv}.len()"),
            Builtin::DictGet => call("get"),
            Builtin::DictInsert => call("insert"),
            Builtin::DictRemove => call("remove"),
            Builtin::DictKeys => format!("{recv}.keys()"),
            Builtin::DictValues => format!("{recv}.values()"),
            Builtin::DictContainsKey => format!("{recv}.contains_key({})", args.join(", ")),
            Builtin::OptionIsSome => format!("_lt.opt_is_some({recv})"),
            Builtin::OptionIsNone => format!("_lt.opt_is_none({recv})"),
            Builtin::OptionUnwrap => format!("_lt.opt_unwrap({recv})"),
            Builtin::OptionUnwrapOr => format!("_lt.opt_unwrap_or({recv}, {})", args.join(", ")),
            Builtin::OptionMap => format!("_lt.opt_map({recv}, {})", args.join(", ")),
            Builtin::OptionAndThen => format!("_lt.opt_and_then({recv}, {})", args.join(", ")),
            Builtin::ResultIsOk => format!("_lt.res_is_ok({recv})"),
            Builtin::ResultIsErr => format!("_lt.res_is_err({recv})"),
            Builtin::ResultUnwrap => format!("_lt.res_unwrap({recv})"),
            Builtin::ResultUnwrapOr => format!("_lt.res_unwrap_or({recv}, {})", args.join(", ")),
            Builtin::ResultMap => format!("_lt.res_map({recv}, {})", args.join(", ")),
            Builtin::ResultMapErr => format!("_lt.res_map_err({recv}, {})", args.join(", ")),
            Builtin::ResultAndThen => format!("_lt.res_and_then({recv}, {})", args.join(", ")),
        }
    }
}

fn format_float(value: f64) -> String {
    if value.is_nan() {
        return "float(\"nan\")".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 {
            "float(\"inf\")".to_string()
        } else {
            "float(\"-inf\")".to_string()
        };
    }
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value:?}")
    }
}
