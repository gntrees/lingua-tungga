//! The resolved program: packages, imports, and declaration tables.
//!
//! In v2 the semantic unit is a **package**, not a file. A file with three
//! `package` blocks yields three [`Module`]s sharing one path and one source.
//! `ModuleId` is therefore the package's identity, which keeps
//! `TypeName { module, name }` meaningful.

use std::path::PathBuf;

use lt_syntax::ast;
use lt_syntax::Span;

use crate::types::{Ty, TypeName};

pub type ModuleId = usize;

#[derive(Debug)]
pub struct Program {
    pub modules: Vec<Module>,
}

#[derive(Debug)]
pub struct Module {
    pub id: ModuleId,
    /// Local path, or a synthetic path for remote modules (stable output name).
    pub path: PathBuf,
    /// Canonical URL for packages that came from an `https://` import.
    pub origin: Option<String>,
    pub source: String,
    /// Package name as written in `package <name> { … }`.
    pub name: String,
    pub ast: ast::PackageDecl,
    pub imports: Vec<ImportInfo>,
    pub structs: Vec<StructInfo>,
    pub enums: Vec<EnumInfo>,
    pub functions: Vec<FnInfo>,
    pub bindings: Vec<BindingInfo>,
    pub tests: Vec<TestInfo>,
}

/// `import <package> [as <binding>] [{ items }] from "<path>"`.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// Package name as written; verified to exist in the target file.
    pub package: String,
    /// Name the package is bound to in this file (alias or package name).
    pub binding: String,
    pub module: ModuleId,
    pub items: Vec<ImportItemInfo>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImportItemInfo {
    pub name: String,
    /// Local name (alias or `name`).
    pub binding: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub ty: Ty,
    pub span: Span,
}

/// A package-level function. Receiver functions are ordinary functions whose
/// first parameter type is a struct/enum declared in the same package.
#[derive(Debug, Clone)]
pub struct FnInfo {
    pub name: String,
    pub is_export: bool,
    pub generics: Vec<String>,
    pub params: Vec<(String, Ty, Span)>,
    pub ret: Ty,
    pub span: Span,
    /// Index into `Module::ast.items`.
    pub ast_index: usize,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub name: String,
    pub is_export: bool,
    pub generics: Vec<String>,
    pub fields: Vec<FieldInfo>,
    pub span: Span,
    /// Index into `Module::ast.items`.
    pub ast_index: usize,
}

#[derive(Debug, Clone)]
pub struct VariantInfo {
    pub name: String,
    pub payload: Vec<Ty>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub name: String,
    pub is_export: bool,
    pub generics: Vec<String>,
    pub variants: Vec<VariantInfo>,
    pub span: Span,
    pub ast_index: usize,
}

/// Immutable package-level binding. `ty` is filled in by the checker: from the
/// annotation when present, otherwise inferred from the initializer.
#[derive(Debug, Clone)]
pub struct BindingInfo {
    pub name: String,
    pub is_export: bool,
    pub ty: Ty,
    pub span: Span,
    pub ast_index: usize,
}

#[derive(Debug, Clone)]
pub struct TestInfo {
    pub title: String,
    pub span: Span,
    pub ast_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    Struct,
    Enum,
}

/// Built-in enums that exist without a source module.
pub fn builtin_enum(name: &str) -> Option<&'static [(&'static str, usize)]> {
    match name {
        "Ordering" => Some(&[("Less", 0), ("Equal", 0), ("Greater", 0)]),
        _ => None,
    }
}

impl TypeName {
    pub fn is_builtin_enum(name: &TypeName) -> bool {
        name.is_builtin() && builtin_enum(&name.name).is_some()
    }
}

impl Program {
    pub fn module(&self, id: ModuleId) -> &Module {
        &self.modules[id]
    }

    pub fn struct_of(&self, name: &TypeName) -> Option<&StructInfo> {
        self.modules
            .get(name.module)?
            .structs
            .iter()
            .find(|decl| decl.name == name.name)
    }

    pub fn enum_of(&self, name: &TypeName) -> Option<&EnumInfo> {
        self.modules
            .get(name.module)?
            .enums
            .iter()
            .find(|decl| decl.name == name.name)
    }

    /// The built-in `value` package, which every program carries.
    pub fn prelude(&self) -> Option<ModuleId> {
        self.modules
            .iter()
            .find(|module| module.name == "value")
            .map(|module| module.id)
    }

    pub fn find_struct(&self, module: ModuleId, name: &str) -> Option<&StructInfo> {
        self.modules[module]
            .structs
            .iter()
            .find(|decl| decl.name == name)
    }

    pub fn find_enum(&self, module: ModuleId, name: &str) -> Option<&EnumInfo> {
        self.modules[module]
            .enums
            .iter()
            .find(|decl| decl.name == name)
    }

    pub fn find_fn(&self, module: ModuleId, name: &str) -> Option<&FnInfo> {
        self.modules[module]
            .functions
            .iter()
            .find(|decl| decl.name == name)
    }

    pub fn find_binding(&self, module: ModuleId, name: &str) -> Option<&BindingInfo> {
        self.modules[module]
            .bindings
            .iter()
            .find(|decl| decl.name == name)
    }

    /// Package with this name anywhere in the build, if any.
    pub fn package_named(&self, name: &str) -> Option<ModuleId> {
        self.modules.iter().position(|module| module.name == name)
    }
}
