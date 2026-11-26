//! The resolved program: modules, imports, and declaration tables.

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
    /// Canonical URL for modules that came from an `https://` import.
    pub origin: Option<String>,
    pub source: String,
    pub ast: ast::File,
    pub imports: Vec<ImportInfo>,
    pub classes: Vec<ClassInfo>,
    pub enums: Vec<EnumInfo>,
}

#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub name: String,
    pub module: ModuleId,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    pub is_pub: bool,
    pub is_static: bool,
    pub has_self: bool,
    pub params: Vec<(String, Ty, Span)>,
    pub ret: Ty,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub is_pub: bool,
    pub generics: Vec<String>,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
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
    pub is_pub: bool,
    pub generics: Vec<String>,
    pub variants: Vec<VariantInfo>,
    pub span: Span,
    pub ast_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    Class,
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

    pub fn class_of(&self, name: &TypeName) -> Option<&ClassInfo> {
        self.modules
            .get(name.module)?
            .classes
            .iter()
            .find(|class| class.name == name.name)
    }

    pub fn enum_of(&self, name: &TypeName) -> Option<&EnumInfo> {
        self.modules
            .get(name.module)?
            .enums
            .iter()
            .find(|decl| decl.name == name.name)
    }

    pub fn find_class(&self, module: ModuleId, name: &str) -> Option<&ClassInfo> {
        self.modules[module].classes.iter().find(|c| c.name == name)
    }

    pub fn find_enum(&self, module: ModuleId, name: &str) -> Option<&EnumInfo> {
        self.modules[module].enums.iter().find(|e| e.name == name)
    }
}
