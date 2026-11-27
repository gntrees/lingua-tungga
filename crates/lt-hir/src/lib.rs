//! Resolved program graph and type checker for Lingua Tungga.
//!
//! The typed lowering (IR) lands in M3; today this crate owns module
//! resolution, import validation, symbol tables, and body type checking.

pub mod check;
pub mod loader;
pub mod program;
pub mod types;

use std::path::PathBuf;

use lt_syntax::Diagnostic;

pub use check::check_program;
pub use loader::{
    join_url, load, load_with_remote, normalize, FsLoader, MapLoader, NoRemote, RemoteResolver,
    ResolvedRemote, SourceLoader,
};
pub use program::{
    builtin_enum, BindingInfo, DeclKind, EnumInfo, FieldInfo, FnInfo, ImportInfo, ImportItemInfo,
    Module, ModuleId, Program, StructInfo, TestInfo, VariantInfo,
};
pub use types::{Ty, TypeName, TypeTable};

/// A diagnostic together with the file it belongs to.
#[derive(Debug, Clone)]
pub struct FileDiagnostic {
    pub path: PathBuf,
    pub diagnostic: Diagnostic,
}

#[derive(Debug)]
pub struct CheckOutput {
    pub program: Program,
    pub diagnostics: Vec<FileDiagnostic>,
    pub types: TypeTable,
}

/// Load `entries` plus their imports, then type check the whole graph.
pub fn check_entries(entries: &[PathBuf], loader: &mut dyn SourceLoader) -> CheckOutput {
    check_entries_with_remote(entries, loader, &mut NoRemote)
}

pub fn check_entries_with_remote(
    entries: &[PathBuf],
    loader: &mut dyn SourceLoader,
    remote: &mut dyn RemoteResolver,
) -> CheckOutput {
    let loaded = load_with_remote(entries, loader, remote);

    // If a file could not be read, parsed, or fetched, type checking would only
    // add cascading "unknown type" noise. Report the load errors instead.
    if !loaded.diagnostics.is_empty() {
        return CheckOutput {
            program: loaded.program,
            diagnostics: loaded
                .diagnostics
                .into_iter()
                .map(|(path, diagnostic)| FileDiagnostic { path, diagnostic })
                .collect(),
            types: TypeTable::default(),
        };
    }

    let (program, module_diagnostics, types) = check_program(loaded.program);

    let mut diagnostics: Vec<FileDiagnostic> = loaded
        .diagnostics
        .into_iter()
        .map(|(path, diagnostic)| FileDiagnostic { path, diagnostic })
        .collect();

    diagnostics.extend(
        module_diagnostics
            .into_iter()
            .map(|(module, diagnostic)| FileDiagnostic {
                path: program.modules[module].path.clone(),
                diagnostic,
            }),
    );

    CheckOutput {
        program,
        diagnostics,
        types,
    }
}

impl CheckOutput {
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }
}
