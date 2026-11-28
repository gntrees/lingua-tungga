//! Lowering: type-checked AST -> target-neutral IR.

use std::collections::HashMap;

use lt_hir::{
    builtin_enum, types::substitute, BindingInfo, EnumInfo, FnInfo, ModuleId, Program, StructInfo,
    Ty, TypeName, TypeTable,
};
use lt_syntax::ast::{self, LambdaBody, MatchBody, Pattern};
use lt_syntax::span::line_col_at;
use lt_syntax::{BinOp as AstBinOp, Diagnostic, Span, UnOp as AstUnOp};

use crate::ir::*;

pub struct LowerOutput {
    pub program: IrProgram,
    pub diagnostics: Vec<(ModuleId, Diagnostic)>,
}

pub fn lower(program: &Program, types: &TypeTable, names: Vec<String>) -> LowerOutput {
    let mut modules = Vec::new();
    let mut diagnostics = Vec::new();

    for (module_id, module) in program.modules.iter().enumerate() {
        let mut lowerer = Lowerer {
            program,
            types,
            module: module_id,
            diags: Vec::new(),
            scopes: Vec::new(),
            fn_generics: Vec::new(),
        };

        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut functions = Vec::new();
        let mut bindings = Vec::new();
        let mut tests = Vec::new();

        for item in &module.ast.items {
            match item {
                ast::PackageItem::Struct(decl) => {
                    if let Some(info) = module.structs.iter().find(|s| s.name == decl.name) {
                        structs.push(lowerer.lower_struct(decl, info));
                    }
                }
                ast::PackageItem::Enum(decl) => {
                    if let Some(info) = module.enums.iter().find(|e| e.name == decl.name) {
                        enums.push(lowerer.lower_enum(decl, info));
                    }
                }
                ast::PackageItem::Fn(decl) => {
                    if let Some(info) = module.functions.iter().find(|f| f.name == decl.name) {
                        functions.push(lowerer.lower_fn(decl, info));
                    }
                }
                ast::PackageItem::Binding(decl) => {
                    if let Some(info) = module.bindings.iter().find(|b| b.name == decl.name) {
                        bindings.push(lowerer.lower_binding(decl, info));
                    }
                }
                ast::PackageItem::Test(decl) => {
                    lowerer.scopes.clear();
                    lowerer.scopes.push(HashMap::new());
                    let body = lowerer.block(&decl.body);
                    lowerer.scopes.clear();
                    tests.push(IrTest {
                        title: decl.title.clone(),
                        body,
                    });
                }
            }
        }

        diagnostics.extend(lowerer.diags.into_iter().map(|d| (module_id, d)));

        modules.push(IrModule {
            id: module_id,
            name: names[module_id].clone(),
            structs,
            enums,
            functions,
            bindings,
            tests,
        });
    }

    LowerOutput {
        program: IrProgram { modules, names },
        diagnostics,
    }
}

/// Option/Result use lower-case runtime tags; user enums keep their variant name.
fn runtime_tag(scrutinee: &Ty, variant: &str) -> String {
    match (scrutinee, variant) {
        (Ty::Option(_), "Some") => "some".to_string(),
        (Ty::Option(_), "None") => "none".to_string(),
        (Ty::Result(_, _), "Ok") => "ok".to_string(),
        (Ty::Result(_, _), "Err") => "err".to_string(),
        _ => variant.to_string(),
    }
}

fn lower_binop(op: AstBinOp) -> BinOp {
    match op {
        AstBinOp::Add => BinOp::Add,
        AstBinOp::Sub => BinOp::Sub,
        AstBinOp::Mul => BinOp::Mul,
        AstBinOp::Div => BinOp::Div,
        AstBinOp::Rem => BinOp::Rem,
        AstBinOp::Eq => BinOp::Eq,
        AstBinOp::Ne => BinOp::Ne,
        AstBinOp::Lt => BinOp::Lt,
        AstBinOp::Le => BinOp::Le,
        AstBinOp::Gt => BinOp::Gt,
        AstBinOp::Ge => BinOp::Ge,
        AstBinOp::And => BinOp::And,
        AstBinOp::Or => BinOp::Or,
    }
}

struct Lowerer<'a> {
    program: &'a Program,
    types: &'a TypeTable,
    module: ModuleId,
    diags: Vec<Diagnostic>,
    scopes: Vec<HashMap<String, Ty>>,
    fn_generics: Vec<String>,
}

fn destructured_module(program: &Program, module: ModuleId, name: &str) -> Option<ModuleId> {
    for import in &program.modules[module].imports {
        for item in &import.items {
            if item.binding == name {
                return Some(import.module);
            }
        }
    }
    None
}

impl<'a> Lowerer<'a> {
    // ------------------------------------------------------------- helpers

    fn diag(&mut self, span: Span, message: impl Into<String>) {
        let loc = line_col_at(&self.program.modules[self.module].source, span.start);
        self.diags
            .push(Diagnostic::error(message, span, loc.line, loc.col));
    }

    fn unsupported(&mut self, span: Span, message: impl Into<String>) -> IrExpr {
        self.diag(span, message);
        IrExpr::Var("undefined".to_string())
    }

    fn ty_of(&self, expr: &ast::Expr) -> Ty {
        let span = expr.span();
        self.types
            .get(self.module, (span.start, span.end))
            .cloned()
            .unwrap_or(Ty::Error)
    }

    fn declare(&mut self, name: &str, ty: Ty) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    fn qualified_enum(&self, module: ModuleId, name: &str) -> Option<&'a EnumInfo> {
        self.program.modules[module]
            .enums
            .iter()
            .find(|decl| decl.name == name && decl.is_export)
    }

    fn struct_ref(&self, name: &str) -> Option<(&'a StructInfo, TypeName)> {
        if let Some(module) = self.program.prelude() {
            if let Some(decl) = self.program.find_struct(module, name) {
                if decl.is_export {
                    return Some((
                        decl,
                        TypeName {
                            module,
                            name: name.to_string(),
                        },
                    ));
                }
            }
        }
        let program = self.program;
        if let Some(decl) = program.find_struct(self.module, name) {
            return Some((
                decl,
                TypeName {
                    module: self.module,
                    name: name.to_string(),
                },
            ));
        }
        let module = destructured_module(program, self.module, name)?;
        let decl = program.find_struct(module, name)?;
        if !decl.is_export {
            return None;
        }
        Some((
            decl,
            TypeName {
                module,
                name: name.to_string(),
            },
        ))
    }

    fn fn_ref(&self, name: &str) -> Option<(ModuleId, &'a FnInfo)> {
        let program = self.program;
        if let Some(decl) = program.find_fn(self.module, name) {
            return Some((self.module, decl));
        }
        let module = destructured_module(program, self.module, name)?;
        let decl = program.find_fn(module, name)?;
        if !decl.is_export {
            return None;
        }
        Some((module, decl))
    }

    fn binding_ref(&self, name: &str) -> Option<(ModuleId, &'a BindingInfo)> {
        let program = self.program;
        if let Some(decl) = program.find_binding(self.module, name) {
            return Some((self.module, decl));
        }
        let module = destructured_module(program, self.module, name)?;
        let decl = program.find_binding(module, name)?;
        if !decl.is_export {
            return None;
        }
        Some((module, decl))
    }

    fn package_ref(&self, name: &str) -> Option<ModuleId> {
        self.program.modules[self.module]
            .imports
            .iter()
            .find(|import| import.binding == name)
            .map(|import| import.module)
    }

    fn enum_ref(&self, name: &str) -> Option<(&'a EnumInfo, TypeName)> {
        let program = self.program;
        if let Some(module) = program.prelude() {
            if let Some(decl) = program.find_enum(module, name) {
                if decl.is_export {
                    return Some((
                        decl,
                        TypeName {
                            module,
                            name: name.to_string(),
                        },
                    ));
                }
            }
        }
        if let Some(decl) = program.find_enum(self.module, name) {
            return Some((
                decl,
                TypeName {
                    module: self.module,
                    name: name.to_string(),
                },
            ));
        }
        let module = destructured_module(program, self.module, name)?;
        let decl = program.find_enum(module, name)?;
        if !decl.is_export {
            return None;
        }
        Some((
            decl,
            TypeName {
                module,
                name: name.to_string(),
            },
        ))
    }

    fn variants_of(&self, ty: &Ty) -> Option<Vec<(String, Vec<Ty>)>> {
        match ty {
            Ty::Option(inner) => Some(vec![
                ("Some".to_string(), vec![(**inner).clone()]),
                ("None".to_string(), vec![]),
            ]),
            Ty::Result(ok, err) => Some(vec![
                ("Ok".to_string(), vec![(**ok).clone()]),
                ("Err".to_string(), vec![(**err).clone()]),
            ]),
            Ty::Named(name, args) => {
                if let Some(variants) = builtin_enum(&name.name) {
                    return Some(
                        variants
                            .iter()
                            .map(|(variant, arity)| (variant.to_string(), vec![Ty::Error; *arity]))
                            .collect(),
                    );
                }
                let decl = self.program.enum_of(name)?;
                let subst: HashMap<String, Ty> = decl
                    .generics
                    .iter()
                    .cloned()
                    .zip(args.iter().cloned())
                    .collect();
                Some(
                    decl.variants
                        .iter()
                        .map(|variant| {
                            (
                                variant.name.clone(),
                                variant
                                    .payload
                                    .iter()
                                    .map(|ty| substitute(ty, &subst))
                                    .collect(),
                            )
                        })
                        .collect(),
                )
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------ top items

    fn lower_struct(&mut self, decl: &ast::StructDecl, info: &StructInfo) -> IrStruct {
        IrStruct {
            name: decl.name.clone(),
            generics: decl.generics.clone(),
            is_export: info.is_export,
            fields: info
                .fields
                .iter()
                .map(|field| IrField {
                    name: field.name.clone(),
                    ty: field.ty.clone(),
                })
                .collect(),
        }
    }

    fn lower_fn(&mut self, decl: &ast::FnDecl, info: &FnInfo) -> IrFn {
        self.fn_generics = decl.generics.clone();
        self.scopes.clear();
        self.scopes.push(HashMap::new());
        let params: Vec<IrParam> = info
            .params
            .iter()
            .map(|(name, ty, _)| IrParam {
                name: name.clone(),
                ty: ty.clone(),
            })
            .collect();
        for param in &params {
            self.declare(&param.name, param.ty.clone());
        }
        let body = self.block(&decl.body);
        self.scopes.clear();

        let is_receiver = info.params.first().is_some_and(
            |(_, ty, _)| matches!(ty, Ty::Named(name, _) if name.module == self.module),
        );

        IrFn {
            name: decl.name.clone(),
            is_export: info.is_export,
            generics: decl.generics.clone(),
            params,
            ret: info.ret.clone(),
            body,
            is_receiver,
        }
    }

    fn lower_binding(&mut self, decl: &ast::BindingDecl, info: &BindingInfo) -> IrBinding {
        self.scopes.clear();
        let value = self.expr(&decl.value);
        self.scopes.clear();
        IrBinding {
            name: decl.name.clone(),
            is_export: info.is_export,
            ty: info.ty.clone(),
            value,
        }
    }

    fn lower_enum(&mut self, decl: &ast::EnumDecl, info: &EnumInfo) -> IrEnum {
        IrEnum {
            name: decl.name.clone(),
            generics: decl.generics.clone(),
            is_export: info.is_export,
            variants: info
                .variants
                .iter()
                .map(|variant| IrVariant {
                    name: variant.name.clone(),
                    payload: variant.payload.clone(),
                })
                .collect(),
        }
    }

    // -------------------------------------------------------------- blocks

    fn block(&mut self, block: &ast::Block) -> IrBlock {
        self.scopes.push(HashMap::new());
        let mut stmts = Vec::new();
        for stmt in &block.stmts {
            stmts.push(self.stmt(stmt));
        }
        self.scopes.pop();

        let ty = match block.stmts.last() {
            Some(ast::Stmt::Expr(expr)) => self.ty_of(expr),
            _ => Ty::Unit,
        };

        IrBlock { stmts, ty }
    }

    fn stmt(&mut self, stmt: &ast::Stmt) -> IrStmt {
        match stmt {
            ast::Stmt::Let {
                mutable,
                name,
                value,
                ..
            } => {
                let value_ir = self.expr(value);
                let ty = self.ty_of(value);
                self.declare(name, ty.clone());
                IrStmt::Let {
                    name: name.clone(),
                    ty,
                    mutable: *mutable,
                    value: value_ir,
                }
            }
            ast::Stmt::Assign { name, value, .. } => IrStmt::Assign {
                name: name.clone(),
                value: self.expr(value),
            },
            ast::Stmt::Return { value, .. } => {
                IrStmt::Return(value.as_ref().map(|expr| self.expr(expr)))
            }
            ast::Stmt::For {
                var, iter, body, ..
            } => {
                let iter_ty = self.ty_of(iter);
                let (element, iter_ir) = match iter_ty.clone() {
                    Ty::List(inner) => (*inner, self.expr(iter)),
                    Ty::Dict(_) => (
                        Ty::Str,
                        IrExpr::Builtin {
                            kind: Builtin::DictKeys,
                            receiver: Box::new(self.expr(iter)),
                            recv_ty: iter_ty.clone(),
                            args: Vec::new(),
                            ty: Ty::list(Ty::Str),
                        },
                    ),
                    Ty::Map(key, _) => {
                        let keys = Ty::list((*key).clone());
                        (
                            (*key).clone(),
                            IrExpr::Builtin {
                                kind: Builtin::MapKeys,
                                receiver: Box::new(self.expr(iter)),
                                recv_ty: iter_ty.clone(),
                                args: Vec::new(),
                                ty: keys,
                            },
                        )
                    }
                    _ => (Ty::Error, self.expr(iter)),
                };
                self.scopes.push(HashMap::new());
                self.declare(var, element.clone());
                let body_ir = self.block(body);
                self.scopes.pop();
                IrStmt::For {
                    var: var.clone(),
                    ty: element,
                    iter: iter_ir,
                    body: body_ir,
                }
            }
            ast::Stmt::Break { .. } => IrStmt::Break,
            ast::Stmt::Continue { .. } => IrStmt::Continue,
            ast::Stmt::While { cond, body, .. } => IrStmt::While {
                cond: self.expr(cond),
                body: self.block(body),
            },
            ast::Stmt::Expr(expr) => IrStmt::Expr(self.expr(expr)),
        }
    }

    // --------------------------------------------------------- expressions

    fn expr(&mut self, expr: &ast::Expr) -> IrExpr {
        match expr {
            ast::Expr::Int { raw, span } => match raw.replace('_', "").parse::<i64>() {
                Ok(value) => IrExpr::Int(value),
                Err(_) => {
                    self.diag(*span, "integer literal does not fit in `Int` (i64)");
                    IrExpr::Int(0)
                }
            },
            ast::Expr::Float { raw, .. } => {
                IrExpr::Float(raw.replace('_', "").parse::<f64>().unwrap_or(0.0))
            }
            ast::Expr::Str { value, .. } => IrExpr::Str(value.clone()),
            ast::Expr::Bool { value, .. } => IrExpr::Bool(*value),
            ast::Expr::NoneLit { .. } => IrExpr::NewVariant {
                target: VariantRef::Option,
                variant: "None".to_string(),
                args: Vec::new(),
                ty: self.ty_of(expr),
            },
            ast::Expr::Ident { name, .. } => {
                if self.scopes.iter().any(|scope| scope.contains_key(name)) {
                    IrExpr::Var(name.clone())
                } else if let Some((module, binding)) = self.binding_ref(name) {
                    IrExpr::BindingRef {
                        module,
                        name: binding.name.clone(),
                        ty: binding.ty.clone(),
                    }
                } else {
                    IrExpr::Var(name.clone())
                }
            }
            ast::Expr::List { items, .. } => IrExpr::List {
                items: items.iter().map(|item| self.expr(item)).collect(),
                ty: self.ty_of(expr),
            },
            ast::Expr::Index { base, index, .. } => {
                let receiver_ty = self.ty_of(base);
                let kind = match receiver_ty {
                    Ty::List(_) => Builtin::ListGet,
                    Ty::Map(_, _) => Builtin::MapGet,
                    Ty::Dict(_) => Builtin::DictGet,
                    _ => Builtin::ListGet,
                };
                IrExpr::Builtin {
                    kind,
                    receiver: Box::new(self.expr(base)),
                    recv_ty: receiver_ty,
                    args: vec![self.expr(index)],
                    ty: self.ty_of(expr),
                }
            }
            ast::Expr::Dict { entries, .. } => IrExpr::DictLit {
                entries: entries
                    .iter()
                    .map(|(key, value)| (self.expr(key), self.expr(value)))
                    .collect(),
                ty: self.ty_of(expr),
            },
            ast::Expr::Call { callee, args, span } => self.call(callee, args, *span, expr),
            ast::Expr::Field { base, name, .. } => {
                // `pkg.Enum.Variant`
                if let ast::Expr::Field {
                    base: inner,
                    name: enum_name,
                    ..
                } = base.as_ref()
                {
                    if let ast::Expr::Ident { name: owner, .. } = inner.as_ref() {
                        let is_local = self.scopes.iter().any(|scope| scope.contains_key(owner));
                        if !is_local {
                            if let Some(module) = self.package_ref(owner) {
                                if let Some(decl) = self.qualified_enum(module, enum_name) {
                                    return IrExpr::NewVariant {
                                        target: VariantRef::Enum {
                                            module,
                                            name: decl.name.clone(),
                                        },
                                        variant: name.clone(),
                                        args: Vec::new(),
                                        ty: self.ty_of(expr),
                                    };
                                }
                            }
                        }
                    }
                }
                if let ast::Expr::Ident { name: owner, .. } = base.as_ref() {
                    let is_local = self.scopes.iter().any(|scope| scope.contains_key(owner));
                    if !is_local {
                        if let Some(module) = self.package_ref(owner) {
                            if let Some(binding) = self.program.find_binding(module, name) {
                                return IrExpr::BindingRef {
                                    module,
                                    name: binding.name.clone(),
                                    ty: binding.ty.clone(),
                                };
                            }
                        }
                    }
                    if let Some((decl, type_name)) = self.enum_ref(owner) {
                        return IrExpr::NewVariant {
                            target: VariantRef::Enum {
                                module: type_name.module,
                                name: decl.name.clone(),
                            },
                            variant: name.clone(),
                            args: Vec::new(),
                            ty: self.ty_of(expr),
                        };
                    }
                }
                IrExpr::Field {
                    base: Box::new(self.expr(base)),
                    name: name.clone(),
                    ty: self.ty_of(expr),
                }
            }
            ast::Expr::Try { value, .. } => {
                let source_ty = self.ty_of(value);
                let value_ir = self.expr(value);
                let ty = self.ty_of(expr);
                IrExpr::Try {
                    value: Box::new(value_ir),
                    source_ty,
                    ty,
                }
            }
            ast::Expr::Template { parts, exprs, .. } => IrExpr::Template {
                parts: parts.clone(),
                exprs: exprs.iter().map(|inner| self.expr(inner)).collect(),
            },
            ast::Expr::TaggedTemplate {
                tag, parts, exprs, ..
            } => self.tagged(tag, parts, exprs, expr),
            ast::Expr::Jsx(element) => {
                let ty = self.ty_of(expr);
                self.jsx(element, ty)
            }
            ast::Expr::Lambda { span, .. } => self.unsupported(
                *span,
                "this lambda is not in a position the v1 backend supports",
            ),
            ast::Expr::If {
                cond,
                then_block,
                else_branch,
                ..
            } => IrExpr::If {
                cond: Box::new(self.expr(cond)),
                then_block: self.block(then_block),
                else_branch: else_branch.as_ref().map(|branch| match branch {
                    ast::ElseBranch::Block(block) => IrElse::Block(self.block(block)),
                    ast::ElseBranch::If(nested) => IrElse::If(Box::new(self.expr(nested))),
                }),
                ty: self.ty_of(expr),
            },
            ast::Expr::Match {
                scrutinee, arms, ..
            } => self.match_expr(scrutinee, arms, expr),
            ast::Expr::Binary { op, lhs, rhs, .. } => IrExpr::Bin {
                op: lower_binop(*op),
                lhs: Box::new(self.expr(lhs)),
                rhs: Box::new(self.expr(rhs)),
                ty: self.ty_of(expr),
            },
            ast::Expr::Unary { op, value, .. } => IrExpr::Un {
                op: match op {
                    AstUnOp::Not => UnOp::Not,
                    AstUnOp::Neg => UnOp::Neg,
                },
                value: Box::new(self.expr(value)),
                ty: self.ty_of(expr),
            },
        }
    }

    fn positional<'e>(&mut self, args: &'e ast::Args, span: Span) -> Option<Vec<&'e ast::Expr>> {
        match args {
            ast::Args::Empty => Some(Vec::new()),
            ast::Args::Positional(items) => Some(items.iter().collect()),
            ast::Args::Named(_) => {
                self.diag(span, "this call takes positional arguments only");
                None
            }
        }
    }

    fn struct_new(
        &mut self,
        decl: &StructInfo,
        type_name: TypeName,
        args: &ast::Args,
        span: Span,
        ty: Ty,
    ) -> IrExpr {
        let ast::Args::Named(named) = args else {
            return self.unsupported(span, "construction needs named arguments");
        };
        let lowered: Vec<(String, IrExpr)> = decl
            .fields
            .iter()
            .map(|field| {
                let value = named
                    .iter()
                    .find(|arg| arg.name == field.name)
                    .map(|arg| self.expr(&arg.value))
                    .unwrap_or_else(|| IrExpr::Var("undefined".to_string()));
                (field.name.clone(), value)
            })
            .collect();
        IrExpr::StructNew {
            module: type_name.module,
            strukt: decl.name.clone(),
            args: lowered,
            ty,
        }
    }

    fn positional_call(
        &mut self,
        args: &ast::Args,
        span: Span,
        module: ModuleId,
        name: &str,
        ty: Ty,
    ) -> IrExpr {
        let Some(items) = self.positional(args, span) else {
            return IrExpr::Var("undefined".to_string());
        };
        IrExpr::FnCall {
            module,
            name: name.to_string(),
            args: items.iter().map(|item| self.expr(item)).collect(),
            ty,
        }
    }

    fn package_call(
        &mut self,
        module: ModuleId,
        name: &str,
        args: &ast::Args,
        span: Span,
        ty: Ty,
    ) -> IrExpr {
        let program = self.program;
        if let Some(decl) = program.find_fn(module, name) {
            let fn_name = decl.name.clone();
            return self.positional_call(args, span, module, &fn_name, ty);
        }
        if let Some(decl) = program.find_struct(module, name) {
            let decl = decl.clone();
            let type_name = TypeName {
                module,
                name: name.to_string(),
            };
            return self.struct_new(&decl, type_name, args, span, ty);
        }
        self.unsupported(span, format!("unsupported package member `{name}`"))
    }

    fn call(
        &mut self,
        callee: &ast::Expr,
        args: &ast::Args,
        span: Span,
        whole: &ast::Expr,
    ) -> IrExpr {
        let ty = self.ty_of(whole);

        match callee {
            ast::Expr::Ident { name, .. } => {
                if let Some((decl, type_name)) = self.struct_ref(name) {
                    let decl = decl.clone();
                    return self.struct_new(&decl, type_name, args, span, ty);
                }

                if matches!(name.as_str(), "Some" | "Ok" | "Err") {
                    let Some(items) = self.positional(args, span) else {
                        return IrExpr::Var("undefined".to_string());
                    };
                    let target = if name == "Some" {
                        VariantRef::Option
                    } else {
                        VariantRef::Result
                    };
                    return IrExpr::NewVariant {
                        target,
                        variant: name.clone(),
                        args: items.iter().map(|item| self.expr(item)).collect(),
                        ty,
                    };
                }

                if let Some((module, decl)) = self.fn_ref(name) {
                    let fn_name = decl.name.clone();
                    return self.positional_call(args, span, module, &fn_name, ty);
                }

                self.unsupported(span, format!("unsupported call of `{name}`"))
            }
            ast::Expr::Field { base, name, .. } => {
                if let ast::Expr::Field {
                    base: inner,
                    name: enum_name,
                    ..
                } = base.as_ref()
                {
                    if let ast::Expr::Ident { name: owner, .. } = inner.as_ref() {
                        let is_local = self.scopes.iter().any(|scope| scope.contains_key(owner));
                        if !is_local {
                            if let Some(module) = self.package_ref(owner) {
                                if let Some(decl) = self.qualified_enum(module, enum_name) {
                                    let Some(items) = self.positional(args, span) else {
                                        return IrExpr::Var("undefined".to_string());
                                    };
                                    return IrExpr::NewVariant {
                                        target: VariantRef::Enum {
                                            module,
                                            name: decl.name.clone(),
                                        },
                                        variant: name.clone(),
                                        args: items.iter().map(|item| self.expr(item)).collect(),
                                        ty,
                                    };
                                }
                            }
                        }
                    }
                }
                if let ast::Expr::Ident { name: owner, .. } = base.as_ref() {
                    let is_local = self.scopes.iter().any(|scope| scope.contains_key(owner));
                    if !is_local {
                        if let Some(module) = self.package_ref(owner) {
                            return self.package_call(module, name, args, span, ty);
                        }
                    }
                    if let Some((decl, type_name)) = self.enum_ref(owner) {
                        let decl = decl.clone();
                        let Some(items) = self.positional(args, span) else {
                            return IrExpr::Var("undefined".to_string());
                        };
                        return IrExpr::NewVariant {
                            target: VariantRef::Enum {
                                module: type_name.module,
                                name: decl.name.clone(),
                            },
                            variant: name.clone(),
                            args: items.iter().map(|item| self.expr(item)).collect(),
                            ty,
                        };
                    }
                    if owner == "map" && name == "from" {
                        let Some(items) = self.positional(args, span) else {
                            return IrExpr::Var("undefined".to_string());
                        };
                        let Some(prelude) = self.program.prelude() else {
                            self.diag(span, "the `value` prelude is missing");
                            return IrExpr::Var("undefined".to_string());
                        };
                        let (key_ty, value_ty) = match ty {
                            Ty::Map(ref key, ref value) => ((**key).clone(), (**value).clone()),
                            _ => (Ty::Error, Ty::Error),
                        };
                        return IrExpr::MapFrom {
                            entries: Box::new(self.expr(items[0])),
                            key_ty,
                            value_ty,
                            ty,
                            prelude,
                        };
                    }
                    if (owner == "map" || owner == "set") && name == "empty" {
                        let Some(items) = self.positional(args, span) else {
                            return IrExpr::Var("undefined".to_string());
                        };
                        if !items.is_empty() {
                            self.diag(span, format!("`{owner}.empty` takes no arguments"));
                        }
                        return IrExpr::EmptyCollection { ty };
                    }
                    if owner == "math" {
                        let Some(items) = self.positional(args, span) else {
                            return IrExpr::Var("undefined".to_string());
                        };
                        return IrExpr::Math {
                            name: name.clone(),
                            args: items.iter().map(|item| self.expr(item)).collect(),
                            ty,
                        };
                    }
                    if owner == "json" {
                        let Some(items) = self.positional(args, span) else {
                            return IrExpr::Var("undefined".to_string());
                        };
                        let Some(prelude) = self.program.prelude() else {
                            self.diag(span, "the `value` prelude is missing");
                            return IrExpr::Var("undefined".to_string());
                        };
                        return IrExpr::Json {
                            parse: name == "parse",
                            args: items.iter().map(|item| self.expr(item)).collect(),
                            prelude,
                            ty,
                        };
                    }
                    if owner == "expect" {
                        let Some(items) = self.positional(args, span) else {
                            return IrExpr::Var("undefined".to_string());
                        };
                        return IrExpr::Expect {
                            name: name.clone(),
                            args: items.iter().map(|item| self.expr(item)).collect(),
                        };
                    }
                }

                let receiver_ty = self.ty_of(base);
                let receiver = self.expr(base);
                let Some(items) = self.positional(args, span) else {
                    return IrExpr::Var("undefined".to_string());
                };

                if let Ty::Named(type_name, _) = &receiver_ty {
                    let program = self.program;
                    if let Some(decl) = program.find_fn(type_name.module, name) {
                        let is_receiver = (decl.is_export || type_name.module == self.module)
                            && decl.params.first().is_some_and(|(_, ty, _)| {
                                matches!(ty, Ty::Named(first, _) if first.module == type_name.module && first.name == type_name.name)
                            });
                        if is_receiver {
                            let module = type_name.module;
                            let fn_name = decl.name.clone();
                            let mut lowered = vec![receiver];
                            lowered.extend(items.iter().map(|item| self.expr(item)));
                            return IrExpr::FnCall {
                                module,
                                name: fn_name,
                                args: lowered,
                                ty,
                            };
                        }
                    }
                }

                if let Some(kind) = Builtin::from_method(&receiver_ty, name) {
                    let first_ty = items.first().map(|item| self.ty_of(item));
                    let lambda_params = kind.lambda_params(&receiver_ty, first_ty.as_ref());
                    let lowered: Vec<IrExpr> = items
                        .iter()
                        .map(|item| match (&lambda_params, &item) {
                            (Some(params), ast::Expr::Lambda { .. }) => {
                                self.lambda(item, params.clone())
                            }
                            _ => self.expr(item),
                        })
                        .collect();
                    return IrExpr::Builtin {
                        kind,
                        receiver: Box::new(receiver),
                        recv_ty: receiver_ty,
                        args: lowered,
                        ty,
                    };
                }

                if name == "to_string" && items.is_empty() {
                    return IrExpr::ToStr {
                        value: Box::new(receiver),
                        ty: receiver_ty,
                    };
                }

                self.unsupported(span, format!("unsupported method call `{name}`"))
            }
            _ => self.unsupported(span, "this expression is not callable"),
        }
    }

    fn lambda(&mut self, expr: &ast::Expr, param_types: Vec<Ty>) -> IrExpr {
        let ast::Expr::Lambda {
            params: names,
            body,
            span,
        } = expr
        else {
            return self.unsupported(expr.span(), "expected a lambda");
        };

        if names.len() != param_types.len() {
            return self.unsupported(*span, "lambda parameter count mismatch");
        }

        self.scopes.push(HashMap::new());
        let mut params = Vec::new();
        for (name, ty) in names.iter().zip(param_types) {
            self.declare(name, ty.clone());
            params.push(IrParam {
                name: name.clone(),
                ty,
            });
        }

        let body = match body {
            LambdaBody::Expr(inner) => IrLambdaBody::Expr(Box::new(self.expr(inner))),
            LambdaBody::Block(block) => IrLambdaBody::Block(self.block(block)),
        };

        self.scopes.pop();
        let ret = self.ty_of(expr);
        IrExpr::Lambda { params, ret, body }
    }

    fn match_expr(
        &mut self,
        scrutinee: &ast::Expr,
        arms: &[ast::MatchArm],
        whole: &ast::Expr,
    ) -> IrExpr {
        let scrutinee_ty = self.ty_of(scrutinee);
        let variants = self.variants_of(&scrutinee_ty).unwrap_or_default();
        let scrutinee_ir = self.expr(scrutinee);

        let mut lowered = Vec::new();
        for arm in arms {
            let pattern = self.pattern(&arm.pattern, &scrutinee_ty, &variants);
            let body = match &arm.body {
                MatchBody::Expr(expr) => {
                    let value = self.expr(expr);
                    let ty = self.ty_of(expr);
                    IrBlock {
                        stmts: vec![IrStmt::Expr(value)],
                        ty,
                    }
                }
                MatchBody::Block(block) => self.block(block),
            };
            lowered.push(IrArm { pattern, body });
        }

        IrExpr::Match {
            scrutinee: Box::new(scrutinee_ir),
            arms: lowered,
            ty: self.ty_of(whole),
        }
    }

    fn pattern(
        &mut self,
        pattern: &Pattern,
        scrutinee_ty: &Ty,
        variants: &[(String, Vec<Ty>)],
    ) -> IrPattern {
        match pattern {
            Pattern::Wildcard { .. } => IrPattern::Wildcard,
            Pattern::NoneLit { .. } => IrPattern::Variant {
                variant: match scrutinee_ty {
                    Ty::Option(_) => "none".to_string(),
                    _ => "None".to_string(),
                },
                args: Vec::new(),
            },
            Pattern::Variant { path, args, span } => {
                let name = path.last().cloned().unwrap_or_default();
                if let Some((_, payload)) = variants.iter().find(|(variant, _)| *variant == name) {
                    let args: Vec<IrPattern> = args
                        .iter()
                        .zip(payload.iter())
                        .map(|(arg, payload_ty)| match arg {
                            Pattern::Wildcard { .. } => IrPattern::Wildcard,
                            Pattern::NoneLit { .. } => IrPattern::Variant {
                                variant: match payload_ty {
                                    Ty::Option(_) => "none".to_string(),
                                    _ => "None".to_string(),
                                },
                                args: Vec::new(),
                            },
                            Pattern::Variant {
                                path: inner,
                                args: inner_args,
                                span: inner_span,
                            } => {
                                let inner_name = inner.last().cloned().unwrap_or_default();
                                if inner.len() == 1 && inner_args.is_empty() {
                                    IrPattern::Binding {
                                        name: inner_name,
                                        ty: payload_ty.clone(),
                                    }
                                } else {
                                    let inner_variants =
                                        self.variants_of(payload_ty).unwrap_or_default();
                                    let lowered = self.pattern(arg, payload_ty, &inner_variants);
                                    if matches!(lowered, IrPattern::Wildcard)
                                        && inner_variants.is_empty()
                                    {
                                        self.diag(
                                            *inner_span,
                                            format!(
                                                "`{inner_name}` is not a variant of this value"
                                            ),
                                        );
                                    }
                                    lowered
                                }
                            }
                        })
                        .collect();
                    IrPattern::Variant {
                        variant: runtime_tag(scrutinee_ty, &name),
                        args,
                    }
                } else if path.len() == 1 && args.is_empty() {
                    IrPattern::Binding {
                        name,
                        ty: scrutinee_ty.clone(),
                    }
                } else {
                    self.diag(*span, format!("unknown variant `{name}` in pattern"));
                    IrPattern::Wildcard
                }
            }
        }
    }

    fn tagged(
        &mut self,
        tag: &ast::Expr,
        parts: &[String],
        exprs: &[ast::Expr],
        whole: &ast::Expr,
    ) -> IrExpr {
        let ty = self.ty_of(whole);

        let (module, decl) = match tag {
            ast::Expr::Ident { name, .. } => match self.fn_ref(name) {
                Some((module, decl)) => (module, decl.clone()),
                None => return self.unsupported(tag.span(), format!("unknown tag `{name}`")),
            },
            ast::Expr::Field { base, name, .. } => {
                let ast::Expr::Ident { name: owner, .. } = base.as_ref() else {
                    return self.unsupported(tag.span(), "unsupported tagged template");
                };
                let Some(module) = self.package_ref(owner) else {
                    return self.unsupported(tag.span(), format!("unknown package `{owner}`"));
                };
                match self.program.find_fn(module, name) {
                    Some(decl) => (module, decl.clone()),
                    None => {
                        return self
                            .unsupported(tag.span(), format!("`{owner}.{name}` is not a function"))
                    }
                }
            }
            _ => return self.unsupported(tag.span(), "unsupported tagged template"),
        };

        let value_ty = decl
            .params
            .get(1)
            .map(|(_, ty, _)| ty.clone())
            .unwrap_or_else(|| Ty::list(Ty::Str));

        IrExpr::FnCall {
            module,
            name: decl.name.clone(),
            args: vec![
                IrExpr::List {
                    items: parts.iter().map(|part| IrExpr::Str(part.clone())).collect(),
                    ty: Ty::list(Ty::Str),
                },
                IrExpr::List {
                    items: exprs.iter().map(|inner| self.expr(inner)).collect(),
                    ty: value_ty,
                },
            ],
            ty,
        }
    }

    fn jsx_struct(&self, path: &[String]) -> Option<(StructInfo, TypeName)> {
        match path {
            [name] => self
                .struct_ref(name)
                .map(|(decl, type_name)| (decl.clone(), type_name)),
            [package, name] => {
                let module = self.package_ref(package)?;
                self.program.modules[module]
                    .structs
                    .iter()
                    .find(|decl| decl.name == *name && decl.is_export)
                    .cloned()
                    .map(|decl| {
                        (
                            decl,
                            TypeName {
                                module,
                                name: name.clone(),
                            },
                        )
                    })
            }
            _ => None,
        }
    }

    fn jsx(&mut self, element: &ast::JsxElement, ty: Ty) -> IrExpr {
        let Some((class, type_name)) = self.jsx_struct(&element.path) else {
            return self.unsupported(
                element.span,
                format!("unknown struct `{}`", element.path.join(".")),
            );
        };
        let class = class.clone();

        let mut provided: HashMap<String, IrExpr> = HashMap::new();
        for attr in &element.attrs {
            let value = match &attr.value {
                ast::JsxAttrValue::Str(value) => IrExpr::Str(value.clone()),
                ast::JsxAttrValue::Expr(expr) => self.expr(expr),
            };
            provided.insert(attr.name.clone(), value);
        }

        if let Some(children_field) = class.fields.iter().find(|field| field.name == "children") {
            let element_ty = match &children_field.ty {
                Ty::List(inner) => (**inner).clone(),
                _ => Ty::Error,
            };
            let items: Vec<IrExpr> = element
                .children
                .iter()
                .map(|child| match child {
                    ast::JsxChild::Element(nested) => self.jsx(nested, element_ty.clone()),
                    ast::JsxChild::Text(text) => IrExpr::Str(text.clone()),
                    ast::JsxChild::Expr(expr) => self.expr(expr),
                })
                .collect();
            provided.insert(
                "children".to_string(),
                IrExpr::List {
                    items,
                    ty: Ty::list(element_ty),
                },
            );
        }

        let args = class
            .fields
            .iter()
            .map(|field| {
                (
                    field.name.clone(),
                    provided
                        .remove(&field.name)
                        .unwrap_or_else(|| IrExpr::Var("undefined".to_string())),
                )
            })
            .collect();

        IrExpr::StructNew {
            module: type_name.module,
            strukt: class.name.clone(),
            args,
            ty,
        }
    }
}
