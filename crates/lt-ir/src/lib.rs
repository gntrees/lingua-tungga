//! Target-neutral typed IR for Lingua Tungga.
//!
//! `lower` consumes a type-checked program and produces a tree in which every
//! semantic question (which class, which variant, which builtin) is already
//! answered. Emitters render that tree; they never re-analyse the source.

pub mod ir;
pub mod lower;

pub use ir::*;
pub use lower::{lower, LowerOutput};

use lt_hir::{ModuleId, Program, TypeTable};
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
