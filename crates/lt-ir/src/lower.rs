//! Lowering: type-checked AST -> target-neutral IR.

use std::collections::HashMap;

use lt_hir::{
    builtin_enum, types::substitute, ClassInfo, EnumInfo, ModuleId, Program, Ty, TypeName,
    TypeTable,
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
            self_ty: None,
            class_generics: Vec::new(),
            test_helpers: Vec::new(),
        };

        let mut classes = Vec::new();
        let mut enums = Vec::new();
        let mut tests = Vec::new();

        for item in &module.ast.items {
            match item {
                ast::Item::Class(class) => {
                    if let Some(info) = module.classes.iter().find(|c| c.name == class.name) {
                        classes.push(lowerer.lower_class(class, info));
                    }
                }
                ast::Item::Enum(decl) => {
                    if let Some(info) = module.enums.iter().find(|e| e.name == decl.name) {
                        enums.push(lowerer.lower_enum(decl, info));
                    }
                }
                ast::Item::TestClass(decl) => {
                    tests.push(lowerer.lower_test_class(decl));
                }
            }
        }

        diagnostics.extend(lowerer.diags.into_iter().map(|d| (module_id, d)));

        modules.push(IrModule {
            id: module_id,
            name: names[module_id].clone(),
            classes,
            enums,
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
    self_ty: Option<Ty>,
    class_generics: Vec<String>,
    test_helpers: Vec<(String, Vec<Ty>, Ty)>,
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

    fn class_ref(&self, name: &str) -> Option<(&'a ClassInfo, TypeName)> {
        let program = self.program;
        if let Some(class) = program.find_class(self.module, name) {
            return Some((
                class,
                TypeName {
                    module: self.module,
                    name: name.to_string(),
                },
            ));
        }
        let import = program.modules[self.module]
            .imports
            .iter()
            .find(|import| import.name == name)?;
        let class = program.find_class(import.module, name)?;
        Some((
            class,
            TypeName {
                module: import.module,
                name: name.to_string(),
            },
        ))
    }

    fn enum_ref(&self, name: &str) -> Option<(&'a EnumInfo, TypeName)> {
        let program = self.program;
        if let Some(decl) = program.find_enum(self.module, name) {
            return Some((
                decl,
                TypeName {
                    module: self.module,
                    name: name.to_string(),
                },
            ));
        }
        let import = program.modules[self.module]
            .imports
            .iter()
            .find(|import| import.name == name)?;
        let decl = program.find_enum(import.module, name)?;
        Some((
            decl,
            TypeName {
                module: import.module,
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

    fn lower_class(&mut self, class: &ast::ClassDecl, info: &ClassInfo) -> IrClass {
        let type_name = TypeName {
            module: self.module,
            name: class.name.clone(),
        };
        self.class_generics = class.generics.clone();
        self.self_ty = Some(Ty::Named(
            type_name,
            class
                .generics
                .iter()
                .map(|g| Ty::Generic(g.clone()))
                .collect(),
        ));

        let mut methods = Vec::new();
        for method in &class.methods {
            let Some(method_info) = info.methods.iter().find(|m| m.name == method.name) else {
                continue;
            };
            self.scopes.clear();
            self.scopes.push(HashMap::new());
            if method.has_self {
                if let Some(self_ty) = self.self_ty.clone() {
                    self.declare("self", self_ty);
                }
            }
            for (param, (_, ty, _)) in method.params.iter().zip(method_info.params.iter()) {
                self.declare(&param.name, ty.clone());
            }
            let body = self.block(&method.body);
            self.scopes.clear();

            methods.push(IrMethod {
                name: method.name.clone(),
                is_pub: method_info.is_pub,
                is_static: method_info.is_static,
                params: method_info
                    .params
                    .iter()
                    .map(|(name, ty, _)| IrParam {
                        name: name.clone(),
                        ty: ty.clone(),
                    })
                    .collect(),
                ret: method_info.ret.clone(),
                body,
            });
        }

        IrClass {
            name: class.name.clone(),
            generics: class.generics.clone(),
            is_pub: info.is_pub,
            fields: info
                .fields
                .iter()
                .map(|field| IrField {
                    name: field.name.clone(),
                    ty: field.ty.clone(),
                })
                .collect(),
            methods,
        }
    }

    fn lower_enum(&mut self, decl: &ast::EnumDecl, info: &EnumInfo) -> IrEnum {
        IrEnum {
            name: decl.name.clone(),
            generics: decl.generics.clone(),
            is_pub: info.is_pub,
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

    fn lower_test_class(&mut self, decl: &ast::TestClassDecl) -> IrTestClass {
        let mut signatures: Vec<(String, Vec<Ty>, Ty)> = Vec::new();
        for helper in &decl.helpers {
            let params: Vec<Ty> = helper
                .params
                .iter()
                .map(|param| self.type_annotation(&param.ty))
                .collect();
            let ret = match &helper.ret {
                Some(ty) => self.type_annotation(ty),
                None => Ty::Unit,
            };
            signatures.push((helper.name.clone(), params, ret));
        }
        self.test_helpers = signatures.clone();
        let mut helpers = Vec::new();
        for (index, helper) in decl.helpers.iter().enumerate() {
            self.scopes.clear();
            self.scopes.push(HashMap::new());
            let params: Vec<IrParam> = helper
                .params
                .iter()
                .map(|param| IrParam {
                    name: param.name.clone(),
                    ty: self.type_annotation(&param.ty),
                })
                .collect();
            for param in &params {
                self.declare(&param.name, param.ty.clone());
            }
            let body = self.block(&helper.body);
            self.scopes.clear();
            helpers.push(IrMethod {
                name: helper.name.clone(),
                is_pub: false,
                is_static: true,
                params,
                ret: signatures
                    .get(index)
                    .map(|(_, _, ret)| ret.clone())
                    .unwrap_or(Ty::Unit),
                body,
            });
        }

        let mut tests = Vec::new();
        for test in &decl.tests {
            self.scopes.clear();
            self.scopes.push(HashMap::new());
            let body = self.block(&test.body);
            self.scopes.clear();
            tests.push(IrTestFn {
                name: test.name.clone(),
                body,
            });
        }

        self.test_helpers.clear();

        IrTestClass {
            name: decl.name.clone(),
            tests,
            helpers,
        }
    }

    fn type_annotation(&self, ty: &ast::Type) -> Ty {
        let name = ty.path.last().cloned().unwrap_or_default();
        let args: Vec<Ty> = ty
            .args
            .iter()
            .map(|arg| self.type_annotation(arg))
            .collect();
        let base = match name.as_str() {
            "Int" => Ty::Int,
            "Float" => Ty::Float,
            "Bool" => Ty::Bool,
            "Str" => Ty::Str,
            "List" if args.len() == 1 => Ty::list(args[0].clone()),
            "Set" if args.len() == 1 => Ty::set(args[0].clone()),
            "Option" if args.len() == 1 => Ty::option(args[0].clone()),
            "Map" if args.len() == 2 => Ty::map(args[0].clone(), args[1].clone()),
            "Result" if args.len() == 2 => Ty::result(args[0].clone(), args[1].clone()),
            _ => match self.class_ref(&name) {
                Some((_, type_name)) => Ty::Named(type_name, args),
                None => match self.enum_ref(&name) {
                    Some((_, type_name)) => Ty::Named(type_name, args),
                    None => Ty::Error,
                },
            },
        };
        if ty.optional {
            Ty::option(base)
        } else {
            base
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
                let element = match &iter_ty {
                    Ty::List(inner) => (**inner).clone(),
                    _ => Ty::Error,
                };
                let iter_ir = self.expr(iter);
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
                if name == "self" {
                    IrExpr::SelfRef
                } else {
                    IrExpr::Var(name.clone())
                }
            }
            ast::Expr::List { items, .. } => IrExpr::List {
                items: items.iter().map(|item| self.expr(item)).collect(),
                ty: self.ty_of(expr),
            },
            ast::Expr::Call { callee, args, span } => self.call(callee, args, *span, expr),
            ast::Expr::Field { base, name, .. } => {
                if let ast::Expr::Ident { name: owner, .. } = base.as_ref() {
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
                if let Some((class, type_name)) = self.class_ref(name) {
                    let class = class.clone();
                    let ast::Args::Named(named) = args else {
                        return self.unsupported(span, "construction needs named arguments");
                    };
                    let lowered: Vec<(String, IrExpr)> = class
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
                    return IrExpr::ClassNew {
                        module: type_name.module,
                        class: class.name.clone(),
                        args: lowered,
                        ty,
                    };
                }
                if let Some((helper_name, params, ret)) = self
                    .test_helpers
                    .iter()
                    .find(|(helper, _, _)| helper == name)
                    .cloned()
                {
                    let Some(items) = self.positional(args, span) else {
                        return IrExpr::Var("undefined".to_string());
                    };
                    let lowered: Vec<IrExpr> = items
                        .iter()
                        .zip(params.iter())
                        .map(|(item, _)| self.expr(item))
                        .collect();
                    return IrExpr::LocalCall {
                        name: helper_name,
                        args: lowered,
                        ty: ret,
                    };
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
                self.unsupported(span, format!("unsupported call of `{name}`"))
            }
            ast::Expr::Field { base, name, .. } => {
                if let ast::Expr::Ident { name: owner, .. } = base.as_ref() {
                    if let Some((class, type_name)) = self.class_ref(owner) {
                        let class = class.clone();
                        let Some(items) = self.positional(args, span) else {
                            return IrExpr::Var("undefined".to_string());
                        };
                        return IrExpr::StaticCall {
                            module: type_name.module,
                            class: class.name.clone(),
                            method: name.clone(),
                            args: items.iter().map(|item| self.expr(item)).collect(),
                            ty,
                        };
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
                    if owner == "Map" || owner == "Set" {
                        let Some(items) = self.positional(args, span) else {
                            return IrExpr::Var("undefined".to_string());
                        };
                        if !items.is_empty() {
                            self.diag(span, format!("`{owner}.empty` takes no arguments"));
                        }
                        return IrExpr::EmptyCollection { ty };
                    }
                    if owner == "Math" {
                        let Some(items) = self.positional(args, span) else {
                            return IrExpr::Var("undefined".to_string());
                        };
                        return IrExpr::Math {
                            name: name.clone(),
                            args: items.iter().map(|item| self.expr(item)).collect(),
                            ty,
                        };
                    }
                    if owner == "Expect" {
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

                IrExpr::InstanceCall {
                    receiver: Box::new(receiver),
                    method: name.clone(),
                    args: items.iter().map(|item| self.expr(item)).collect(),
                    ty,
                }
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
                bindings: Vec::new(),
            },
            Pattern::Variant { path, args, span } => {
                let name = path.last().cloned().unwrap_or_default();
                if let Some((_, payload)) = variants.iter().find(|(variant, _)| *variant == name) {
                    let mut bindings = Vec::new();
                    for (arg, payload_ty) in args.iter().zip(payload.iter()) {
                        match arg {
                            Pattern::Wildcard { .. } => {}
                            Pattern::Variant {
                                path: inner,
                                args: inner_args,
                                ..
                            } if inner.len() == 1 && inner_args.is_empty() => {
                                bindings.push((inner[0].clone(), payload_ty.clone()));
                            }
                            _ => self.diag(
                                *span,
                                "nested patterns are not supported by the v1 backend yet",
                            ),
                        }
                    }
                    IrPattern::Variant {
                        variant: runtime_tag(scrutinee_ty, &name),
                        bindings,
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
        let (owner, method) = match tag {
            ast::Expr::Field { base, name, .. } => match base.as_ref() {
                ast::Expr::Ident { name: owner, .. } => (owner.clone(), name.clone()),
                _ => return self.unsupported(tag.span(), "unsupported tagged template"),
            },
            _ => return self.unsupported(tag.span(), "unsupported tagged template"),
        };
        let Some((class, type_name)) = self.class_ref(&owner) else {
            return self.unsupported(tag.span(), format!("unknown tag owner `{owner}`"));
        };

        let value_ty = class
            .methods
            .iter()
            .find(|candidate| candidate.name == method && candidate.is_static)
            .and_then(|candidate| candidate.params.get(1))
            .map(|(_, ty, _)| ty.clone())
            .unwrap_or_else(|| Ty::list(Ty::Str));

        let part_items: Vec<IrExpr> = parts.iter().map(|part| IrExpr::Str(part.clone())).collect();
        let value_items: Vec<IrExpr> = exprs.iter().map(|inner| self.expr(inner)).collect();

        IrExpr::StaticCall {
            module: type_name.module,
            class: class.name.clone(),
            method,
            args: vec![
                IrExpr::List {
                    items: part_items,
                    ty: Ty::list(Ty::Str),
                },
                IrExpr::List {
                    items: value_items,
                    ty: value_ty,
                },
            ],
            ty,
        }
    }

    fn jsx(&mut self, element: &ast::JsxElement, ty: Ty) -> IrExpr {
        let Some((class, type_name)) = self.class_ref(&element.name) else {
            return self.unsupported(element.span, format!("unknown class `{}`", element.name));
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

        IrExpr::ClassNew {
            module: type_name.module,
            class: class.name.clone(),
            args,
            ty,
        }
    }
}
