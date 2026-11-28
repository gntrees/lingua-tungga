//! Target-neutral typed IR for Lingua Tungga.
//!
//! `lower` consumes a type-checked program and produces a tree in which every
//! semantic question (which class, which variant, which builtin) is already
//! answered. Emitters render that tree; they never re-analyse the source.

pub mod ir;
pub mod lower;

pub use ir::*;
pub use lower::{lower, LowerOutput};

use lt_hir::{ModuleId, Program, Ty, TypeTable};
use lt_syntax::Diagnostic;

/// Convenience wrapper: lower a checked program.
pub fn lower_program(
    program: &Program,
    types: &TypeTable,
    names: Vec<String>,
) -> (IrProgram, Vec<(ModuleId, Diagnostic)>) {
    let output = lower(program, types, names);
    (output.program, output.diagnostics)
}

/// Synthesize the body of a "call this from the host language" program.
///
/// Every `test` declaration is replayed, but `expect.*` is replaced by printing
/// its first argument. The result is a runnable program that shows the real
/// argument values and the real results, derived from the same IR the tests use
/// so it can never drift from the library API.
pub fn usage_block(module: &IrModule) -> Option<IrBlock> {
    let mut stmts = Vec::new();
    for test in &module.tests {
        for stmt in &test.body.stmts {
            stmts.push(match stmt {
                IrStmt::Expr(IrExpr::Expect { args, .. }) if !args.is_empty() => {
                    IrStmt::Expr(IrExpr::Print {
                        value: Box::new(args[0].clone()),
                    })
                }
                other => other.clone(),
            });
        }
    }
    if stmts.is_empty() {
        return None;
    }
    Some(IrBlock {
        stmts,
        ty: lt_hir::Ty::Unit,
    })
}

/// Does any module (other than the prelude itself) mention the prelude?
///
/// The `value` package is only emitted when something actually refers to
/// `Value`/`Entry`, so builds that never touch `json` or `map.from` keep their
/// output unchanged.
pub fn prelude_is_referenced(program: &IrProgram) -> bool {
    let Some(prelude) = program
        .modules
        .iter()
        .find(|module| module.name == "value")
        .map(|module| module.id)
    else {
        return false;
    };
    program.modules.iter().any(|module| {
        module.id != prelude
            && (module.structs.iter().any(|decl| {
                decl.fields
                    .iter()
                    .any(|field| type_mentions(&field.ty, prelude))
            }) || module.enums.iter().any(|decl| {
                decl.variants
                    .iter()
                    .any(|variant| variant.payload.iter().any(|ty| type_mentions(ty, prelude)))
            }) || module.functions.iter().any(|func| {
                func.params
                    .iter()
                    .any(|param| type_mentions(&param.ty, prelude))
                    || type_mentions(&func.ret, prelude)
                    || block_mentions(&func.body, prelude)
            }) || module.bindings.iter().any(|binding| {
                type_mentions(&binding.ty, prelude) || expr_mentions(&binding.value, prelude)
            }) || module
                .tests
                .iter()
                .any(|test| block_mentions(&test.body, prelude)))
    })
}

fn type_mentions(ty: &Ty, module: ModuleId) -> bool {
    match ty {
        Ty::Named(name, args) => {
            name.module == module || args.iter().any(|arg| type_mentions(arg, module))
        }
        Ty::List(inner) | Ty::Set(inner) | Ty::Option(inner) | Ty::Dict(inner) => {
            type_mentions(inner, module)
        }
        Ty::Map(key, value) | Ty::Result(key, value) => {
            type_mentions(key, module) || type_mentions(value, module)
        }
        _ => false,
    }
}

fn block_mentions(block: &IrBlock, module: ModuleId) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        IrStmt::Let { ty, value, .. } => type_mentions(ty, module) || expr_mentions(value, module),
        IrStmt::Assign { value, .. } => expr_mentions(value, module),
        IrStmt::Return(value) => value
            .as_ref()
            .is_some_and(|value| expr_mentions(value, module)),
        IrStmt::Break | IrStmt::Continue => false,
        IrStmt::For { ty, iter, body, .. } => {
            type_mentions(ty, module) || expr_mentions(iter, module) || block_mentions(body, module)
        }
        IrStmt::While { cond, body } => expr_mentions(cond, module) || block_mentions(body, module),
        IrStmt::Expr(expr) => expr_mentions(expr, module),
    })
}

fn expr_mentions(expr: &IrExpr, module: ModuleId) -> bool {
    match expr {
        IrExpr::List { items, ty } => {
            type_mentions(ty, module) || items.iter().any(|item| expr_mentions(item, module))
        }
        IrExpr::DictLit { entries, ty } => {
            type_mentions(ty, module)
                || entries
                    .iter()
                    .any(|(key, value)| expr_mentions(key, module) || expr_mentions(value, module))
        }
        IrExpr::MapFrom {
            entries,
            key_ty,
            value_ty,
            ty,
            prelude,
        } => {
            *prelude == module
                || type_mentions(key_ty, module)
                || type_mentions(value_ty, module)
                || type_mentions(ty, module)
                || expr_mentions(entries, module)
        }
        IrExpr::Json { args, prelude, .. } => {
            *prelude == module || args.iter().any(|arg| expr_mentions(arg, module))
        }
        IrExpr::BindingRef { module: target, .. } => *target == module,
        IrExpr::StructNew {
            module: target,
            args,
            ..
        } => *target == module || args.iter().any(|(_, value)| expr_mentions(value, module)),
        IrExpr::NewVariant { target, args, .. } => {
            matches!(target, VariantRef::Enum { module: target, .. } if *target == module)
                || args.iter().any(|arg| expr_mentions(arg, module))
        }
        IrExpr::FnCall {
            module: target,
            args,
            ..
        } => *target == module || args.iter().any(|arg| expr_mentions(arg, module)),
        IrExpr::Builtin { receiver, args, .. } => {
            expr_mentions(receiver, module) || args.iter().any(|arg| expr_mentions(arg, module))
        }
        IrExpr::Math { args, .. } => args.iter().any(|arg| expr_mentions(arg, module)),
        IrExpr::Expect { args, .. } => args.iter().any(|arg| expr_mentions(arg, module)),
        IrExpr::Print { value } | IrExpr::ToStr { value, .. } | IrExpr::Try { value, .. } => {
            expr_mentions(value, module)
        }
        IrExpr::Template { exprs, .. } => exprs.iter().any(|inner| expr_mentions(inner, module)),
        IrExpr::If {
            cond,
            then_block,
            else_branch,
            ..
        } => {
            expr_mentions(cond, module)
                || block_mentions(then_block, module)
                || else_branch.as_ref().is_some_and(|branch| match branch {
                    IrElse::Block(block) => block_mentions(block, module),
                    IrElse::If(expr) => expr_mentions(expr, module),
                })
        }
        IrExpr::Match {
            scrutinee, arms, ..
        } => {
            expr_mentions(scrutinee, module)
                || arms.iter().any(|arm| {
                    pattern_mentions(&arm.pattern, module) || block_mentions(&arm.body, module)
                })
        }
        IrExpr::Bin { lhs, rhs, .. } => expr_mentions(lhs, module) || expr_mentions(rhs, module),
        IrExpr::Un { value, .. } => expr_mentions(value, module),
        IrExpr::Field { base, .. } => expr_mentions(base, module),
        IrExpr::EmptyCollection { ty } => type_mentions(ty, module),
        IrExpr::Lambda { params, ret, body } => {
            params.iter().any(|param| type_mentions(&param.ty, module))
                || type_mentions(ret, module)
                || match body {
                    IrLambdaBody::Expr(expr) => expr_mentions(expr, module),
                    IrLambdaBody::Block(block) => block_mentions(block, module),
                }
        }
        _ => false,
    }
}

fn pattern_mentions(pattern: &IrPattern, module: ModuleId) -> bool {
    match pattern {
        IrPattern::Binding { ty, .. } => type_mentions(ty, module),
        IrPattern::Variant { args, .. } => args.iter().any(|arg| pattern_mentions(arg, module)),
        IrPattern::Wildcard => false,
    }
}
