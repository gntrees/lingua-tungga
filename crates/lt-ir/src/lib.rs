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
