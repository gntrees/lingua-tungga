//! Declaration collection, type resolution, and body type checking.

use std::collections::{HashMap, HashSet};

use lt_syntax::ast::{self, BinOp, LambdaBody, MatchBody, Pattern, UnOp};
use lt_syntax::span::line_col_at;
use lt_syntax::{Diagnostic, Span};

use crate::program::{
    builtin_enum, BindingInfo, EnumInfo, FieldInfo, FnInfo, ModuleId, Program, StructInfo,
    TestInfo, VariantInfo,
};
use crate::types::{mentions_any_generic, substitute, Subst, Ty, TypeName, TypeTable};

pub fn check_program(mut program: Program) -> (Program, Vec<(ModuleId, Diagnostic)>, TypeTable) {
    let mut diagnostics = Vec::new();
    let mut types = TypeTable::default();
    collect_declarations(&mut program, &mut diagnostics);
    resolve_declarations(&mut program, &mut diagnostics);
    check_bindings(&mut program, &mut diagnostics, &mut types);
    check_bodies(&mut program, &mut diagnostics, &mut types);
    (program, diagnostics, types)
}

fn diag(diagnostics: &mut Vec<Diagnostic>, src: &str, span: Span, message: String) {
    let loc = line_col_at(src, span.start);
    diagnostics.push(Diagnostic::error(message, span, loc.line, loc.col));
}

// ------------------------------------------------------------------- naming

fn is_pascal_case(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() => {}
        _ => return false,
    }
    name.chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_snake_case(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

fn check_type_name(
    diagnostics: &mut Vec<Diagnostic>,
    source: &str,
    span: Span,
    name: &str,
    kind: &str,
) {
    if !is_pascal_case(name) {
        diag(
            diagnostics,
            source,
            span,
            format!("{kind} `{name}` must be `PascalCase`"),
        );
    }
}

fn check_value_name(
    diagnostics: &mut Vec<Diagnostic>,
    source: &str,
    span: Span,
    name: &str,
    kind: &str,
) {
    if !is_snake_case(name) {
        diag(
            diagnostics,
            source,
            span,
            format!("{kind} `{name}` must be `snake_case`"),
        );
    }
}

// -------------------------------------------------------------- declarations

fn collect_declarations(program: &mut Program, diagnostics: &mut Vec<(ModuleId, Diagnostic)>) {
    for module_id in 0..program.modules.len() {
        let mut structs: Vec<StructInfo> = Vec::new();
        let mut enums: Vec<EnumInfo> = Vec::new();
        let mut functions: Vec<FnInfo> = Vec::new();
        let mut bindings: Vec<BindingInfo> = Vec::new();
        let mut tests: Vec<TestInfo> = Vec::new();
        let mut local = Vec::new();
        // One namespace per package; compared case-insensitively because Go
        // exports by capitalisation and two such names would collide there.
        let mut taken: HashSet<String> = HashSet::new();

        {
            let module = &program.modules[module_id];
            let source = &module.source;

            if !is_snake_case(&module.name) {
                diag(
                    &mut local,
                    source,
                    module.ast.span,
                    format!(
                        "package name `{}` must be `snake_case` (lowercase, digits, `_`)",
                        module.name
                    ),
                );
            }

            for (index, item) in module.ast.items.iter().enumerate() {
                let (name, span): (&str, Span) = match item {
                    ast::PackageItem::Struct(decl) => (&decl.name, decl.span),
                    ast::PackageItem::Enum(decl) => (&decl.name, decl.span),
                    ast::PackageItem::Fn(decl) => (&decl.name, decl.span),
                    ast::PackageItem::Binding(decl) => (&decl.name, decl.span),
                    ast::PackageItem::Test(decl) => {
                        tests.push(TestInfo {
                            title: decl.title.clone(),
                            span: decl.span,
                            ast_index: index,
                        });
                        continue;
                    }
                };
                if !taken.insert(name.to_lowercase()) {
                    diag(
                        &mut local,
                        source,
                        span,
                        format!("`{name}` is declared more than once in this package"),
                    );
                    continue;
                }
                match item {
                    ast::PackageItem::Struct(decl) => {
                        check_type_name(&mut local, source, decl.span, &decl.name, "struct");
                        structs.push(StructInfo {
                            name: decl.name.clone(),
                            is_export: decl.is_export,
                            generics: decl.generics.clone(),
                            fields: Vec::new(),
                            span: decl.span,
                            ast_index: index,
                        });
                    }
                    ast::PackageItem::Enum(decl) => {
                        check_type_name(&mut local, source, decl.span, &decl.name, "enum");
                        enums.push(EnumInfo {
                            name: decl.name.clone(),
                            is_export: decl.is_export,
                            generics: decl.generics.clone(),
                            variants: Vec::new(),
                            span: decl.span,
                            ast_index: index,
                        });
                    }
                    ast::PackageItem::Fn(decl) => {
                        check_value_name(&mut local, source, decl.span, &decl.name, "function");
                        functions.push(FnInfo {
                            name: decl.name.clone(),
                            is_export: decl.is_export,
                            generics: decl.generics.clone(),
                            params: Vec::new(),
                            ret: Ty::Error,
                            span: decl.span,
                            ast_index: index,
                        });
                    }
                    ast::PackageItem::Binding(decl) => {
                        check_value_name(&mut local, source, decl.span, &decl.name, "binding");
                        bindings.push(BindingInfo {
                            name: decl.name.clone(),
                            is_export: decl.is_export,
                            ty: Ty::Error,
                            span: decl.span,
                            ast_index: index,
                        });
                    }
                    ast::PackageItem::Test(_) => {}
                }
            }
        }

        diagnostics.extend(local.into_iter().map(|d| (module_id, d)));
        program.modules[module_id].structs = structs;
        program.modules[module_id].enums = enums;
        program.modules[module_id].functions = functions;
        program.modules[module_id].bindings = bindings;
        program.modules[module_id].tests = tests;
    }
}

fn resolve_declarations(program: &mut Program, diagnostics: &mut Vec<(ModuleId, Diagnostic)>) {
    for module_id in 0..program.modules.len() {
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut functions = Vec::new();
        let mut bindings = Vec::new();
        let mut local = Vec::new();
        {
            let module = &program.modules[module_id];
            for struct_info in &module.structs {
                let ast::PackageItem::Struct(decl) = &module.ast.items[struct_info.ast_index]
                else {
                    continue;
                };
                structs.push(resolve_struct(
                    program,
                    module_id,
                    struct_info,
                    decl,
                    &mut local,
                ));
            }
            for enum_info in &module.enums {
                let ast::PackageItem::Enum(decl) = &module.ast.items[enum_info.ast_index] else {
                    continue;
                };
                enums.push(resolve_enum(
                    program, module_id, enum_info, decl, &mut local,
                ));
            }
            for fn_info in &module.functions {
                let ast::PackageItem::Fn(decl) = &module.ast.items[fn_info.ast_index] else {
                    continue;
                };
                functions.push(resolve_fn(program, module_id, fn_info, decl, &mut local));
            }
            for binding_info in &module.bindings {
                let ast::PackageItem::Binding(decl) = &module.ast.items[binding_info.ast_index]
                else {
                    continue;
                };
                let mut info = binding_info.clone();
                if let Some(ty) = &decl.ty {
                    info.ty = resolve_type(program, module_id, &[], ty, &mut local);
                }
                bindings.push(info);
            }
        }
        diagnostics.extend(local.into_iter().map(|d| (module_id, d)));
        program.modules[module_id].structs = structs;
        program.modules[module_id].enums = enums;
        program.modules[module_id].functions = functions;
        program.modules[module_id].bindings = bindings;
    }
}

fn resolve_struct(
    program: &Program,
    module_id: ModuleId,
    info: &StructInfo,
    decl: &ast::StructDecl,
    diagnostics: &mut Vec<Diagnostic>,
) -> StructInfo {
    let source = &program.modules[module_id].source;
    let mut fields: Vec<FieldInfo> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for field in &decl.fields {
        check_value_name(diagnostics, source, field.span, &field.name, "field");
        if !seen.insert(field.name.to_lowercase()) {
            diag(
                diagnostics,
                source,
                field.span,
                format!("duplicate field `{}` in `{}`", field.name, decl.name),
            );
        }
        let ty = resolve_type(program, module_id, &decl.generics, &field.ty, diagnostics);
        fields.push(FieldInfo {
            name: field.name.clone(),
            ty,
            span: field.span,
        });
    }

    StructInfo {
        name: info.name.clone(),
        is_export: info.is_export,
        generics: info.generics.clone(),
        fields,
        span: info.span,
        ast_index: info.ast_index,
    }
}

fn resolve_enum(
    program: &Program,
    module_id: ModuleId,
    info: &EnumInfo,
    decl: &ast::EnumDecl,
    diagnostics: &mut Vec<Diagnostic>,
) -> EnumInfo {
    let source = &program.modules[module_id].source;
    let mut variants: Vec<VariantInfo> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for variant in &decl.variants {
        check_type_name(
            diagnostics,
            source,
            variant.span,
            &variant.name,
            "enum variant",
        );
        if !seen.insert(variant.name.to_lowercase()) {
            diag(
                diagnostics,
                source,
                variant.span,
                format!(
                    "duplicate variant `{}` in enum `{}`",
                    variant.name, decl.name
                ),
            );
        }
        let payload = variant
            .payload
            .iter()
            .map(|ty| resolve_type(program, module_id, &decl.generics, ty, diagnostics))
            .collect();
        variants.push(VariantInfo {
            name: variant.name.clone(),
            payload,
            span: variant.span,
        });
    }

    EnumInfo {
        name: info.name.clone(),
        is_export: info.is_export,
        generics: info.generics.clone(),
        variants,
        span: info.span,
        ast_index: info.ast_index,
    }
}

fn resolve_fn(
    program: &Program,
    module_id: ModuleId,
    info: &FnInfo,
    decl: &ast::FnDecl,
    diagnostics: &mut Vec<Diagnostic>,
) -> FnInfo {
    let source = &program.modules[module_id].source;
    let mut seen: HashSet<String> = HashSet::new();
    for param in &decl.params {
        check_value_name(diagnostics, source, param.span, &param.name, "parameter");
        if !seen.insert(param.name.clone()) {
            diag(
                diagnostics,
                source,
                param.span,
                format!("duplicate parameter `{}`", param.name),
            );
        }
    }

    let params = decl
        .params
        .iter()
        .map(|param| {
            (
                param.name.clone(),
                resolve_type(program, module_id, &decl.generics, &param.ty, diagnostics),
                param.span,
            )
        })
        .collect::<Vec<_>>();

    let ret = match &decl.ret {
        Some(ty) => resolve_type(program, module_id, &decl.generics, ty, diagnostics),
        None => Ty::Unit,
    };

    if params
        .iter()
        .any(|(_, ty, _)| mentions_any_generic(ty, &decl.generics))
        != mentions_any_generic(&ret, &decl.generics)
    {
        // Generics need not be used, but a generic that appears only in the
        // return type cannot be inferred, so report it at the declaration.
        for generic in &decl.generics {
            if !params
                .iter()
                .any(|(_, ty, _)| mentions_any_generic(ty, std::slice::from_ref(generic)))
                && mentions_any_generic(&ret, std::slice::from_ref(generic))
            {
                diag(
                    diagnostics,
                    source,
                    decl.span,
                    format!(
                        "type parameter `{generic}` of `{}` appears only in the return type and cannot be inferred",
                        decl.name
                    ),
                );
            }
        }
    }

    FnInfo {
        name: info.name.clone(),
        is_export: info.is_export,
        generics: info.generics.clone(),
        params,
        ret,
        span: info.span,
        ast_index: info.ast_index,
    }
}

/// Resolve a syntactic type annotation against the module's scope.
fn resolve_type(
    program: &Program,
    module_id: ModuleId,
    generics: &[String],
    ty: &ast::Type,
    diagnostics: &mut Vec<Diagnostic>,
) -> Ty {
    let source = &program.modules[module_id].source;

    if ty.path.len() > 2 {
        diag(
            diagnostics,
            source,
            ty.span,
            "type paths have at most two segments: `package.Type`".to_string(),
        );
        return Ty::Error;
    }
    let name = ty.path.last().cloned().unwrap_or_default();
    let qualifier = if ty.path.len() == 2 {
        Some(ty.path[0].as_str())
    } else {
        None
    };

    let args: Vec<Ty> = ty
        .args
        .iter()
        .map(|arg| resolve_type(program, module_id, generics, arg, diagnostics))
        .collect();

    let arity_error = |expected: usize, diagnostics: &mut Vec<Diagnostic>| {
        diag(
            diagnostics,
            source,
            ty.span,
            format!(
                "`{name}` expects {expected} type argument(s), found {}",
                args.len()
            ),
        );
    };

    let base = if qualifier.is_none() && generics.iter().any(|generic| generic == &name) {
        Ty::Generic(name)
    } else if let Some(primitive) = primitive_type(&name) {
        primitive
    } else if qualifier.is_none() && builtin_arity(&name).is_some() {
        let expected = builtin_arity(&name).unwrap();
        if args.len() != expected {
            arity_error(expected, diagnostics);
            Ty::Error
        } else {
            builtin_type(&name, &args)
        }
    } else {
        match lookup_type(program, module_id, qualifier, &name) {
            Some((type_name, arity)) => {
                if args.len() != arity {
                    arity_error(arity, diagnostics);
                    Ty::Error
                } else {
                    Ty::Named(type_name, args.clone())
                }
            }
            None => {
                let described = match qualifier {
                    Some(package) => format!("{package}.{name}"),
                    None => name,
                };
                diag(
                    diagnostics,
                    source,
                    ty.span,
                    format!("unknown type `{described}`"),
                );
                Ty::Error
            }
        }
    };

    if ty.optional {
        Ty::option(base)
    } else {
        base
    }
}

fn primitive_type(name: &str) -> Option<Ty> {
    match name {
        "Int" => Some(Ty::Int),
        "Float" => Some(Ty::Float),
        "Bool" => Some(Ty::Bool),
        "Str" => Some(Ty::Str),
        _ => None,
    }
}

fn builtin_arity(name: &str) -> Option<usize> {
    match name {
        "List" | "Set" | "Option" => Some(1),
        "Map" | "Result" => Some(2),
        _ => None,
    }
}

fn builtin_type(name: &str, args: &[Ty]) -> Ty {
    match name {
        "List" => Ty::list(args[0].clone()),
        "Set" => Ty::set(args[0].clone()),
        "Option" => Ty::option(args[0].clone()),
        "Map" => Ty::map(args[0].clone(), args[1].clone()),
        "Result" => Ty::result(args[0].clone(), args[1].clone()),
        _ => Ty::Error,
    }
}

/// Module a package qualifier is bound to in `module_id`.
fn qualifier_module(program: &Program, module_id: ModuleId, qualifier: &str) -> Option<ModuleId> {
    program.modules[module_id]
        .imports
        .iter()
        .find(|import| import.binding == qualifier)
        .map(|import| import.module)
}

/// Module that exported `name` through a destructuring import.
fn destructured_module(program: &Program, module_id: ModuleId, name: &str) -> Option<ModuleId> {
    for import in &program.modules[module_id].imports {
        for item in &import.items {
            if item.binding == name {
                return Some(import.module);
            }
        }
    }
    None
}

/// The local module, or the module a name was imported from.
fn visible_modules(program: &Program, module_id: ModuleId, name: &str) -> Vec<(ModuleId, bool)> {
    let mut out = vec![(module_id, false)];
    if let Some(module) = destructured_module(program, module_id, name) {
        out.push((module, true));
    }
    out
}

/// Look up a struct/enum by bare name (no qualifier).
fn lookup_type(
    program: &Program,
    module_id: ModuleId,
    qualifier: Option<&str>,
    name: &str,
) -> Option<(TypeName, usize)> {
    let candidates: Vec<(ModuleId, bool)> = match qualifier {
        Some(package) => qualifier_module(program, module_id, package)
            .map(|module| vec![(module, true)])
            .unwrap_or_default(),
        None => visible_modules(program, module_id, name),
    };

    for (module, must_export) in candidates {
        let target = &program.modules[module];
        if let Some(decl) = target.structs.iter().find(|decl| decl.name == name) {
            if !must_export || decl.is_export {
                return Some((
                    TypeName {
                        module,
                        name: name.to_string(),
                    },
                    decl.generics.len(),
                ));
            }
        }
        if let Some(decl) = target.enums.iter().find(|decl| decl.name == name) {
            if !must_export || decl.is_export {
                return Some((
                    TypeName {
                        module,
                        name: name.to_string(),
                    },
                    decl.generics.len(),
                ));
            }
        }
    }

    if qualifier.is_none() && builtin_enum(name).is_some() {
        return Some((TypeName::builtin(name), 0));
    }
    None
}

// ------------------------------------------------------------ body checking

/// Resolve package-level bindings. Annotated bindings are checked against their
/// annotation; unannotated ones are inferred in declaration order.
fn check_bindings(
    program: &mut Program,
    diagnostics: &mut Vec<(ModuleId, Diagnostic)>,
    types: &mut TypeTable,
) {
    for module_id in 0..program.modules.len() {
        let count = program.modules[module_id].bindings.len();
        for index in 0..count {
            let ast_index = program.modules[module_id].bindings[index].ast_index;
            let decl = match &program.modules[module_id].ast.items[ast_index] {
                ast::PackageItem::Binding(decl) => decl.clone(),
                _ => continue,
            };
            let annotated = program.modules[module_id].bindings[index].ty.clone();

            let (ty, local_diags, local_types) = {
                let mut checker = Checker::new(program, module_id);
                let value_ty = checker.check_expr(&decl.value, None);
                if !matches!(annotated, Ty::Error) {
                    checker.expect(decl.value.span(), &annotated, &value_ty, " in this binding");
                }
                let ty = if matches!(annotated, Ty::Error) {
                    checker.resolve_deep(&value_ty)
                } else {
                    annotated
                };
                (ty, checker.diags, checker.types)
            };

            program.modules[module_id].bindings[index].ty = ty;
            for ((start, end), ty) in local_types {
                types.insert(module_id, (start, end), ty);
            }
            diagnostics.extend(local_diags.into_iter().map(|d| (module_id, d)));
        }
    }
}

fn check_bodies(
    program: &mut Program,
    diagnostics: &mut Vec<(ModuleId, Diagnostic)>,
    types: &mut TypeTable,
) {
    for module_id in 0..program.modules.len() {
        let source = program.modules[module_id].source.clone();

        // Functions.
        for fn_info in program.modules[module_id].functions.clone() {
            let ast::PackageItem::Fn(decl) =
                &program.modules[module_id].ast.items[fn_info.ast_index]
            else {
                continue;
            };
            let decl = decl.clone();
            let (local_diags, local_types) = {
                let mut checker = Checker::new(program, module_id);
                checker.fn_generics = fn_info.generics.clone();
                checker.ret = fn_info.ret.clone();
                checker.scopes.push(HashMap::new());
                for (param, (_, ty, _)) in decl.params.iter().zip(fn_info.params.iter()) {
                    checker.declare(&param.name, ty.clone(), false, param.span);
                }
                let body_ty = checker.check_block(&decl.body, Some(&fn_info.ret));
                checker.expect(
                    decl.body.span,
                    &fn_info.ret,
                    &body_ty,
                    " as the function result",
                );
                checker.scopes.pop();
                (checker.diags, checker.types)
            };
            for ((start, end), ty) in local_types {
                types.insert(module_id, (start, end), ty);
            }
            diagnostics.extend(local_diags.into_iter().map(|d| (module_id, d)));
        }

        // Tests.
        for test in program.modules[module_id].tests.clone() {
            let ast::PackageItem::Test(decl) =
                &program.modules[module_id].ast.items[test.ast_index]
            else {
                continue;
            };
            let decl = decl.clone();
            let (local_diags, local_types) = {
                let mut checker = Checker::new(program, module_id);
                checker.in_test = true;
                checker.ret = Ty::Unit;
                checker.scopes.push(HashMap::new());
                let body_ty = checker.check_block(&decl.body, Some(&Ty::Unit));
                checker.expect(decl.body.span, &Ty::Unit, &body_ty, " as the test result");
                checker.scopes.pop();
                (checker.diags, checker.types)
            };
            for ((start, end), ty) in local_types {
                types.insert(module_id, (start, end), ty);
            }
            diagnostics.extend(local_diags.into_iter().map(|d| (module_id, d)));
        }

        let _ = &source;
    }
}
#[derive(Clone)]
struct VarInfo {
    ty: Ty,
    mutable: bool,
}

struct Checker<'a> {
    program: &'a Program,
    source: &'a str,
    module: ModuleId,
    diags: Vec<Diagnostic>,
    vars: HashMap<u32, Ty>,
    next_var: u32,
    scopes: Vec<HashMap<String, VarInfo>>,
    fn_generics: Vec<String>,
    ret: Ty,
    in_test: bool,
    in_lambda: bool,
    types: HashMap<(usize, usize), Ty>,
}

impl<'a> Checker<'a> {
    fn new(program: &'a Program, module: ModuleId) -> Self {
        Self {
            program,
            source: &program.modules[module].source,
            module,
            diags: Vec::new(),
            vars: HashMap::new(),
            next_var: 0,
            scopes: Vec::new(),
            fn_generics: Vec::new(),
            ret: Ty::Unit,
            in_test: false,
            in_lambda: false,
            types: HashMap::new(),
        }
    }

    // ------------------------------------------------------------- utilities

    fn diag(&mut self, span: Span, message: impl Into<String>) {
        let loc = line_col_at(self.source, span.start);
        self.diags
            .push(Diagnostic::error(message, span, loc.line, loc.col));
    }

    fn fresh(&mut self) -> Ty {
        let id = self.next_var;
        self.next_var += 1;
        Ty::Var(id)
    }

    fn resolve_deep(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Var(id) => match self.vars.get(id) {
                Some(bound) => self.resolve_deep(&bound.clone()),
                None => ty.clone(),
            },
            Ty::List(inner) => Ty::list(self.resolve_deep(inner)),
            Ty::Set(inner) => Ty::set(self.resolve_deep(inner)),
            Ty::Option(inner) => Ty::option(self.resolve_deep(inner)),
            Ty::Map(key, value) => Ty::map(self.resolve_deep(key), self.resolve_deep(value)),
            Ty::Result(ok, err) => Ty::result(self.resolve_deep(ok), self.resolve_deep(err)),
            Ty::Named(name, args) => Ty::Named(
                name.clone(),
                args.iter().map(|arg| self.resolve_deep(arg)).collect(),
            ),
            other => other.clone(),
        }
    }

    fn display_ty(&self, ty: &Ty) -> String {
        self.resolve_deep(ty).to_string()
    }

    fn unify(&mut self, left: &Ty, right: &Ty) -> bool {
        let left = self.resolve_deep(left);
        let right = self.resolve_deep(right);
        match (&left, &right) {
            (Ty::Error, _) | (_, Ty::Error) => true,
            (Ty::Var(a), Ty::Var(b)) if a == b => true,
            (Ty::Var(id), _) => {
                self.vars.insert(*id, right.clone());
                true
            }
            (_, Ty::Var(id)) => {
                self.vars.insert(*id, left.clone());
                true
            }
            (Ty::Int, Ty::Int)
            | (Ty::Float, Ty::Float)
            | (Ty::Bool, Ty::Bool)
            | (Ty::Str, Ty::Str)
            | (Ty::Unit, Ty::Unit) => true,
            (Ty::Generic(a), Ty::Generic(b)) => a == b,
            (Ty::List(a), Ty::List(b))
            | (Ty::Set(a), Ty::Set(b))
            | (Ty::Option(a), Ty::Option(b)) => self.unify(a, b),
            (Ty::Map(a1, a2), Ty::Map(b1, b2)) | (Ty::Result(a1, a2), Ty::Result(b1, b2)) => {
                self.unify(a1, b1) && self.unify(a2, b2)
            }
            (Ty::Named(a1, a2), Ty::Named(b1, b2)) => {
                a1 == b1 && a2.len() == b2.len() && a2.iter().zip(b2).all(|(x, y)| self.unify(x, y))
            }
            _ => false,
        }
    }

    fn expect(&mut self, span: Span, expected: &Ty, actual: &Ty, context: &str) {
        if !self.unify(expected, actual) {
            let expected = self.display_ty(expected);
            let actual = self.display_ty(actual);
            self.diag(
                span,
                format!("expected `{expected}`{context}, found `{actual}`"),
            );
        }
    }

    fn lookup_var(&self, name: &str) -> Option<&VarInfo> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn declare(&mut self, name: &str, ty: Ty, mutable: bool, span: Span) {
        let scope = self.scopes.last_mut().expect("a scope is always open");
        if scope.contains_key(name) {
            self.diag(span, format!("`{name}` is already declared in this scope"));
            return;
        }
        scope.insert(name.to_string(), VarInfo { ty, mutable });
    }

    /// Look up a struct visible from this module: local first, then a
    /// destructured import, then a `package.Type` qualifier is handled by the
    /// caller.
    fn struct_ref(&self, name: &str) -> Option<(&'a StructInfo, TypeName)> {
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
        if let Some(module) = destructured_module(program, self.module, name) {
            if let Some(decl) = program.find_struct(module, name) {
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
        None
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
        if let Some(module) = destructured_module(program, self.module, name) {
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
        None
    }

    /// Package-level function visible from this module, with the module that
    /// declares it (needed for generic substitution and lowering).
    fn fn_ref(&self, name: &str) -> Option<(ModuleId, &'a FnInfo)> {
        let program = self.program;
        if let Some(decl) = program.find_fn(self.module, name) {
            return Some((self.module, decl));
        }
        if let Some(module) = destructured_module(program, self.module, name) {
            if let Some(decl) = program.find_fn(module, name) {
                if decl.is_export {
                    return Some((module, decl));
                }
            }
        }
        None
    }

    fn binding_ref(&self, name: &str) -> Option<(ModuleId, &'a BindingInfo)> {
        let program = self.program;
        if let Some(decl) = program.find_binding(self.module, name) {
            return Some((self.module, decl));
        }
        if let Some(module) = destructured_module(program, self.module, name) {
            if let Some(decl) = program.find_binding(module, name) {
                if decl.is_export {
                    return Some((module, decl));
                }
            }
        }
        None
    }

    /// Module bound to a package qualifier (`money` from `import money …`).
    fn package_ref(&self, name: &str) -> Option<ModuleId> {
        qualifier_module(self.program, self.module, name)
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
                let subst: Subst = decl
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

    // ------------------------------------------------------------ statements

    fn check_block(&mut self, block: &ast::Block, expected_last: Option<&Ty>) -> Ty {
        self.scopes.push(HashMap::new());
        let count = block.stmts.len();
        let mut last = None;

        for (index, stmt) in block.stmts.iter().enumerate() {
            let is_last = index + 1 == count;
            match stmt {
                ast::Stmt::Let {
                    mutable,
                    name,
                    ty,
                    value,
                    span,
                } => {
                    let declared = match ty {
                        Some(ty) => {
                            let declared = crate::check::resolve_type(
                                self.program,
                                self.module,
                                &self.fn_generics,
                                ty,
                                &mut self.diags,
                            );
                            let value_ty = self.check_expr(value, Some(&declared));
                            self.expect(value.span(), &declared, &value_ty, " in this binding");
                            declared
                        }
                        None => self.check_expr(value, None),
                    };
                    self.declare(name, declared, *mutable, *span);
                    last = None;
                }
                ast::Stmt::Assign { name, value, span } => {
                    match self.lookup_var(name) {
                        Some(info) => {
                            if !info.mutable {
                                self.diag(
                                    *span,
                                    format!("`{name}` is immutable; declare it with `let mut`"),
                                );
                            } else {
                                let ty = info.ty.clone();
                                let value_ty = self.check_expr(value, Some(&ty));
                                self.expect(value.span(), &ty, &value_ty, " in this assignment");
                            }
                        }
                        None => {
                            if self.binding_ref(name).is_some() {
                                self.diag(
                                    *span,
                                    format!("`{name}` is immutable; package-level bindings cannot be assigned"),
                                );
                            } else {
                                self.diag(*span, format!("unknown variable `{name}`"));
                            }
                        }
                    }
                    last = None;
                }
                ast::Stmt::Return { value, span } => {
                    match value {
                        Some(value) => {
                            let ret = self.ret.clone();
                            let value_ty = self.check_expr(value, Some(&ret));
                            self.expect(value.span(), &ret, &value_ty, " in this `return`");
                        }
                        None => {
                            if !self.unify(&self.ret.clone(), &Ty::Unit) {
                                let ret = self.display_ty(&self.ret.clone());
                                self.diag(*span, format!("`return` needs a value of type `{ret}`"));
                            }
                        }
                    }
                    last = None;
                }
                ast::Stmt::For {
                    var,
                    iter,
                    body,
                    span,
                } => {
                    let iter_ty = self.check_expr(iter, None);
                    match self.resolve_deep(&iter_ty) {
                        Ty::List(element) => {
                            self.scopes.push(HashMap::new());
                            self.declare(var, *element, false, *span);
                            let body_ty = self.check_block(body, None);
                            if !self.unify(&body_ty, &Ty::Unit) {
                                let body_ty = self.display_ty(&body_ty);
                                self.diag(
                                    body.span,
                                    format!(
                                        "a loop body must not produce a value, found `{body_ty}`"
                                    ),
                                );
                            }
                            self.scopes.pop();
                        }
                        Ty::Error => {}
                        other => {
                            let other = self.display_ty(&other);
                            self.diag(
                                iter.span(),
                                format!("`for` needs a `List`, found `{other}`"),
                            );
                        }
                    }
                    last = None;
                }
                ast::Stmt::While { cond, body, .. } => {
                    let cond_ty = self.check_expr(cond, Some(&Ty::Bool));
                    self.expect(cond.span(), &Ty::Bool, &cond_ty, " in a `while` condition");
                    let body_ty = self.check_block(body, None);
                    if !self.unify(&body_ty, &Ty::Unit) {
                        let body_ty = self.display_ty(&body_ty);
                        self.diag(
                            body.span,
                            format!("a loop body must not produce a value, found `{body_ty}`"),
                        );
                    }
                    last = None;
                }
                ast::Stmt::Expr(expr) => {
                    let expected = if is_last { expected_last } else { None };
                    last = Some(self.check_expr(expr, expected));
                }
            }
        }

        self.scopes.pop();
        last.unwrap_or(Ty::Unit)
    }

    // ----------------------------------------------------------- expressions

    fn check_expr(&mut self, expr: &ast::Expr, expected: Option<&Ty>) -> Ty {
        let ty = self.check_expr_shallow(expr, expected);
        // Bind any inference variables to the expected type before recording,
        // so the IR (and therefore the emitters) never sees `?` placeholders
        // that were actually resolved by the surrounding context.
        if let Some(expected) = expected {
            let expected = expected.clone();
            let _ = self.unify(&expected, &ty);
        }
        self.record_type(expr, &ty);
        ty
    }

    fn record_type(&mut self, expr: &ast::Expr, ty: &Ty) {
        let span = expr.span();
        let resolved = self.resolve_deep(ty);
        self.types.insert((span.start, span.end), resolved);
    }

    fn check_expr_shallow(&mut self, expr: &ast::Expr, expected: Option<&Ty>) -> Ty {
        match expr {
            ast::Expr::Int { .. } => Ty::Int,
            ast::Expr::Float { .. } => Ty::Float,
            ast::Expr::Str { .. } => Ty::Str,
            ast::Expr::Bool { .. } => Ty::Bool,
            ast::Expr::NoneLit { .. } => match expected.map(|ty| self.resolve_deep(ty)) {
                Some(Ty::Option(inner)) => Ty::option(*inner),
                Some(other) => {
                    let other = other.to_string();
                    self.diag(expr.span(), format!("expected `{other}`, found `None`"));
                    Ty::Error
                }
                None => Ty::option(self.fresh()),
            },
            ast::Expr::Ident { name, span } => match self.lookup_var(name) {
                Some(info) => info.ty.clone(),
                None => {
                    if let Some((_, binding)) = self.binding_ref(name) {
                        binding.ty.clone()
                    } else {
                        self.diag(*span, format!("unknown variable `{name}`"));
                        Ty::Error
                    }
                }
            },
            ast::Expr::List { items, span } => self.check_list(items, *span, expected),
            ast::Expr::Call { callee, args, span } => self.check_call(callee, args, *span),
            ast::Expr::Field { base, name, span } => self.check_field(base, name, *span),
            ast::Expr::Try { value, span } => self.check_try(value, *span),
            ast::Expr::Template { exprs, .. } => {
                for inner in exprs {
                    let inner_ty = self.check_expr(inner, None);
                    self.require_value(inner.span(), &inner_ty);
                }
                Ty::Str
            }
            ast::Expr::TaggedTemplate {
                tag, exprs, span, ..
            } => self.check_tagged(tag, exprs, *span),
            ast::Expr::Jsx(element) => self.check_jsx(element),
            ast::Expr::Lambda { span, .. } => {
                self.diag(
                    *span,
                    "lambdas are only supported as arguments to builtin higher-order methods in v1",
                );
                Ty::Error
            }
            ast::Expr::If {
                cond,
                then_block,
                else_branch,
                span,
            } => self.check_if(cond, then_block, else_branch.as_ref(), *span),
            ast::Expr::Match {
                scrutinee,
                arms,
                span,
            } => self.check_match(scrutinee, arms, *span),
            ast::Expr::Binary { op, lhs, rhs, span } => self.check_binary(*op, lhs, rhs, *span),
            ast::Expr::Unary { op, value, span } => self.check_unary(*op, value, *span),
        }
    }

    fn check_list(&mut self, items: &[ast::Expr], span: Span, expected: Option<&Ty>) -> Ty {
        if let Some(Ty::List(element)) = expected.map(|ty| self.resolve_deep(ty)) {
            for item in items {
                let item_ty = self.check_expr(item, Some(element.as_ref()));
                self.expect(
                    item.span(),
                    element.as_ref(),
                    &item_ty,
                    " in this list element",
                );
            }
            return Ty::list(*element);
        }
        if items.is_empty() {
            self.diag(
                span,
                "cannot infer the element type of an empty list; annotate the binding".to_string(),
            );
            return Ty::Error;
        }
        let element = self.check_expr(&items[0], None);
        for item in &items[1..] {
            let item_ty = self.check_expr(item, Some(&element));
            self.expect(item.span(), &element, &item_ty, " in this list element");
        }
        Ty::list(element)
    }

    fn check_binary(&mut self, op: BinOp, lhs: &ast::Expr, rhs: &ast::Expr, span: Span) -> Ty {
        let left = self.check_expr(lhs, None);
        let right = self.check_expr(rhs, None);
        let left = self.resolve_deep(&left);
        let right = self.resolve_deep(&right);
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                match (&left, &right) {
                    (Ty::Int, Ty::Int) => Ty::Int,
                    (Ty::Float, Ty::Float) => Ty::Float,
                    (Ty::Error, _) | (_, Ty::Error) => Ty::Error,
                    _ => {
                        self.diag(
                            span,
                            format!(
                                "arithmetic needs `Int` or `Float` on both sides, found `{left}` and `{right}`"
                            ),
                        );
                        Ty::Error
                    }
                }
            }
            BinOp::Eq | BinOp::Ne => {
                if !self.unify(&left, &right) {
                    self.diag(
                        span,
                        format!(
                            "cannot compare `{}` with `{}`",
                            self.display_ty(&left),
                            self.display_ty(&right)
                        ),
                    );
                }
                Ty::Bool
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let comparable = matches!(left, Ty::Int | Ty::Float | Ty::Str);
                if !comparable || !self.unify(&left, &right) {
                    self.diag(
                        span,
                        format!(
                            "comparison needs both sides to be `Int`, `Float`, or `Str`, found `{}` and `{}`",
                            self.display_ty(&left),
                            self.display_ty(&right)
                        ),
                    );
                }
                Ty::Bool
            }
            BinOp::And | BinOp::Or => {
                self.expect(
                    lhs.span(),
                    &Ty::Bool,
                    &left,
                    " on the left of this operator",
                );
                self.expect(
                    rhs.span(),
                    &Ty::Bool,
                    &right,
                    " on the right of this operator",
                );
                Ty::Bool
            }
        }
    }

    fn check_unary(&mut self, op: UnOp, value: &ast::Expr, span: Span) -> Ty {
        let ty = self.check_expr(value, None);
        let resolved = self.resolve_deep(&ty);
        match op {
            UnOp::Not => {
                self.expect(span, &Ty::Bool, &resolved, " after `!`");
                Ty::Bool
            }
            UnOp::Neg => match resolved {
                Ty::Int => Ty::Int,
                Ty::Float => Ty::Float,
                Ty::Error => Ty::Error,
                other => {
                    let other = other.to_string();
                    self.diag(span, format!("`-` needs `Int` or `Float`, found `{other}`"));
                    Ty::Error
                }
            },
        }
    }

    fn check_if(
        &mut self,
        cond: &ast::Expr,
        then_block: &ast::Block,
        else_branch: Option<&ast::ElseBranch>,
        span: Span,
    ) -> Ty {
        let cond_ty = self.check_expr(cond, Some(&Ty::Bool));
        self.expect(cond.span(), &Ty::Bool, &cond_ty, " in an `if` condition");
        let then_ty = self.check_block(then_block, None);

        match else_branch {
            None => {
                if !self.unify(&then_ty, &Ty::Unit) {
                    let then_ty = self.display_ty(&then_ty);
                    self.diag(
                        span,
                        format!("this `if` produces `{then_ty}` but has no `else` branch"),
                    );
                }
                Ty::Unit
            }
            Some(ast::ElseBranch::Block(block)) => {
                let else_ty = self.check_block(block, None);
                self.expect(block.span, &then_ty, &else_ty, " in this `else` branch");
                then_ty
            }
            Some(ast::ElseBranch::If(nested)) => {
                let else_ty = self.check_expr(nested, None);
                self.expect(nested.span(), &then_ty, &else_ty, " in this `else` branch");
                then_ty
            }
        }
    }

    fn check_try(&mut self, value: &ast::Expr, span: Span) -> Ty {
        if self.in_lambda {
            self.diag(
                span,
                "`?` cannot be used inside a lambda: it would return from the lambda, not the method"
                    .to_string(),
            );
            return Ty::Error;
        }
        let inner = self.check_expr(value, None);
        match self.resolve_deep(&inner) {
            Ty::Option(inner_ty) => {
                let ret = self.resolve_deep(&self.ret.clone());
                match ret {
                    Ty::Option(_) => *inner_ty,
                    _ => {
                        self.diag(
                            span,
                            "`?` on an `Option` needs the enclosing method to return `Option`",
                        );
                        Ty::Error
                    }
                }
            }
            Ty::Result(ok, err) => {
                let ret = self.resolve_deep(&self.ret.clone());
                match ret {
                    Ty::Result(_, ret_err) => {
                        self.expect(span, &ret_err, &err, " as the error type of this method");
                        *ok
                    }
                    _ => {
                        self.diag(
                            span,
                            "`?` on a `Result` needs the enclosing method to return `Result`",
                        );
                        Ty::Error
                    }
                }
            }
            Ty::Error => Ty::Error,
            other => {
                let other = other.to_string();
                self.diag(
                    span,
                    format!("`?` needs an `Option` or `Result`, found `{other}`"),
                );
                Ty::Error
            }
        }
    }

    fn check_match(&mut self, scrutinee: &ast::Expr, arms: &[ast::MatchArm], span: Span) -> Ty {
        let scrutinee_ty = self.check_expr(scrutinee, None);
        let scrutinee_ty = self.resolve_deep(&scrutinee_ty);

        let Some(variants) = self.variants_of(&scrutinee_ty) else {
            let shown = self.display_ty(&scrutinee_ty);
            self.diag(
                span,
                format!("`match` needs an enum value, found `{shown}`"),
            );
            return Ty::Error;
        };

        let mut covered: Vec<String> = Vec::new();
        let mut exhaustive = false;
        let mut result: Option<Ty> = None;

        for arm in arms {
            let mut bindings = Vec::new();
            if self.bind_pattern(&arm.pattern, &scrutinee_ty, &mut bindings, &mut covered) {
                exhaustive = true;
            }

            self.scopes.push(HashMap::new());
            for (name, ty) in bindings {
                self.declare(&name, ty, false, arm.pattern.span());
            }
            let body_ty = match &arm.body {
                MatchBody::Expr(expr) => self.check_expr(expr, None),
                MatchBody::Block(block) => self.check_block(block, None),
            };
            self.scopes.pop();

            match &result {
                None => result = Some(body_ty),
                Some(existing) => {
                    let existing = existing.clone();
                    self.expect(
                        arm.pattern.span(),
                        &existing,
                        &body_ty,
                        " in this match arm",
                    );
                }
            }
        }

        if !exhaustive {
            let missing: Vec<String> = variants
                .iter()
                .filter(|(name, _)| !covered.iter().any(|c| c == name))
                .map(|(name, _)| name.clone())
                .collect();
            if !missing.is_empty() {
                self.diag(
                    span,
                    format!("`match` is not exhaustive; missing {}", missing.join(", ")),
                );
            }
        }

        result.unwrap_or(Ty::Unit)
    }

    /// Bind a pattern, returning true when it is a catch-all.
    fn bind_pattern(
        &mut self,
        pattern: &Pattern,
        ty: &Ty,
        bindings: &mut Vec<(String, Ty)>,
        covered: &mut Vec<String>,
    ) -> bool {
        match pattern {
            Pattern::Wildcard { .. } => true,
            Pattern::NoneLit { .. } => {
                covered.push("None".to_string());
                false
            }
            Pattern::Variant { path, args, span } => {
                let name = path.last().cloned().unwrap_or_default();
                let variants = self.variants_of(ty);
                let found = variants
                    .as_ref()
                    .and_then(|variants| variants.iter().find(|(variant, _)| *variant == name));

                if let Some((_, payload)) = found {
                    covered.push(name.clone());
                    if payload.len() != args.len() {
                        self.diag(
                            *span,
                            format!(
                                "variant `{name}` expects {} value(s), found {}",
                                payload.len(),
                                args.len()
                            ),
                        );
                    }
                    let mut wildcard = false;
                    for (pattern, payload_ty) in args.iter().zip(payload.iter()) {
                        if let Pattern::Variant {
                            args: inner_args,
                            span: inner_span,
                            ..
                        } = pattern
                        {
                            if !inner_args.is_empty() {
                                self.diag(
                                    *inner_span,
                                    "nested patterns are not supported by the v1 backend yet",
                                );
                            }
                        }
                        if self.bind_pattern(pattern, payload_ty, bindings, covered) {
                            wildcard = true;
                        }
                    }
                    wildcard
                } else if path.len() == 1 && args.is_empty() {
                    // A bare name inside a pattern binds the matched value.
                    bindings.push((name, ty.clone()));
                    true
                } else {
                    self.diag(*span, format!("unknown variant `{name}`"));
                    false
                }
            }
        }
    }

    fn check_field(&mut self, base: &ast::Expr, name: &str, span: Span) -> Ty {
        if let ast::Expr::Ident {
            name: owner,
            span: owner_span,
        } = base
        {
            if let Some((decl, type_name)) = self.enum_ref(owner) {
                let decl = decl.clone();
                return self.enum_value(&decl, type_name, name, span);
            }
            // A local binding wins over a package qualifier of the same name.
            if self.lookup_var(owner).is_none() {
                if let Some(module) = self.package_ref(owner) {
                    return self.package_member(module, name, span);
                }
            }
            if self.struct_ref(owner).is_some() {
                self.diag(
                    *owner_span,
                    format!(
                        "`{owner}.{name}` is not a value; construct `{owner}` with named arguments"
                    ),
                );
                return Ty::Error;
            }
        }

        let base_ty = self.check_expr(base, None);
        let base_ty = self.resolve_deep(&base_ty);
        match &base_ty {
            Ty::Named(type_name, args) => {
                if let Some(decl) = self.program.struct_of(type_name) {
                    let decl = decl.clone();
                    let subst: Subst = decl
                        .generics
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect();
                    if let Some(field) = decl.fields.iter().find(|field| field.name == name) {
                        return self.resolve_deep(&substitute(&field.ty, &subst));
                    }
                    self.diag(span, format!("`{}` has no field `{name}`", decl.name));
                    return Ty::Error;
                }
                if let Some(decl) = self.program.enum_of(type_name) {
                    let decl = decl.clone();
                    self.diag(
                        span,
                        format!(
                            "`{}` is an enum; use `{}.<Variant>` to build a value",
                            decl.name, decl.name
                        ),
                    );
                    return Ty::Error;
                }
                let shown = base_ty.to_string();
                self.diag(span, format!("`{shown}` has no fields"));
                Ty::Error
            }
            Ty::Error => Ty::Error,
            other => {
                let shown = other.to_string();
                self.diag(
                    span,
                    format!("`{shown}` has no fields; only struct values have fields"),
                );
                Ty::Error
            }
        }
    }

    /// `package.name` in value position: only exported bindings are values.
    fn package_member(&mut self, module: ModuleId, name: &str, span: Span) -> Ty {
        let package = self.program.modules[module].name.clone();

        if let Some(binding) = self.program.find_binding(module, name) {
            let (exported, ty) = (binding.is_export, binding.ty.clone());
            if !exported {
                self.diag(
                    span,
                    format!("`{name}` is private to package `{package}`; add `export`"),
                );
                return Ty::Error;
            }
            return self.resolve_deep(&ty);
        }
        if self.program.find_fn(module, name).is_some() {
            self.diag(
                span,
                format!("`{name}` is a function in `{package}`; call it with parentheses"),
            );
            return Ty::Error;
        }
        if self.program.find_struct(module, name).is_some()
            || self.program.find_enum(module, name).is_some()
        {
            self.diag(
                span,
                format!("`{name}` is a type in `{package}`, not a value"),
            );
            return Ty::Error;
        }
        self.diag(span, format!("package `{package}` has no `{name}`"));
        Ty::Error
    }

    fn enum_value(
        &mut self,
        decl: &EnumInfo,
        type_name: TypeName,
        variant_name: &str,
        span: Span,
    ) -> Ty {
        let Some(variant) = decl.variants.iter().find(|v| v.name == variant_name) else {
            self.diag(
                span,
                format!("enum `{}` has no variant `{variant_name}`", decl.name),
            );
            return Ty::Error;
        };
        if !variant.payload.is_empty() {
            self.diag(
                span,
                format!(
                    "variant `{}.{}` needs {} argument(s); call it with parentheses",
                    decl.name,
                    variant.name,
                    variant.payload.len()
                ),
            );
            return Ty::Error;
        }
        let args = decl.generics.iter().map(|_| self.fresh()).collect();
        Ty::Named(type_name, args)
    }

    fn check_call(&mut self, callee: &ast::Expr, args: &ast::Args, span: Span) -> Ty {
        match callee {
            ast::Expr::Ident { name, .. } => {
                if let Some((decl, type_name)) = self.struct_ref(name) {
                    let decl = decl.clone();
                    return self.construct_struct(&decl, type_name, args, span);
                }
                match name.as_str() {
                    "Some" => {
                        let items = match self.positional(args, span, "Some") {
                            Some(items) => items,
                            None => return Ty::Error,
                        };
                        if items.len() != 1 {
                            self.diag(span, "`Some` takes exactly one value".to_string());
                            return Ty::Error;
                        }
                        let inner = self.check_expr(items[0], None);
                        return Ty::option(inner);
                    }
                    "Ok" | "Err" => {
                        let items = match self.positional(args, span, name) {
                            Some(items) => items,
                            None => return Ty::Error,
                        };
                        if items.len() != 1 {
                            self.diag(span, format!("`{name}` takes exactly one value"));
                            return Ty::Error;
                        }
                        let inner = self.check_expr(items[0], None);
                        return if name == "Ok" {
                            Ty::result(inner, self.fresh())
                        } else {
                            Ty::result(self.fresh(), inner)
                        };
                    }
                    _ => {}
                }
                if let Some((module, decl)) = self.fn_ref(name) {
                    let decl = decl.clone();
                    return self.call_fn(module, &decl, args, span, name);
                }
                if self.lookup_var(name).is_some() || self.binding_ref(name).is_some() {
                    self.diag(span, format!("`{name}` is a value, not a function"));
                } else {
                    self.diag(span, format!("unknown function or type `{name}`"));
                }
                Ty::Error
            }
            ast::Expr::Field {
                base,
                name,
                span: field_span,
            } => {
                if let ast::Expr::Ident {
                    name: owner,
                    span: owner_span,
                } = &**base
                {
                    if self.lookup_var(owner).is_none() {
                        if let Some(module) = self.package_ref(owner) {
                            return self.call_package_member(module, name, args, span);
                        }
                    }
                    if let Some((decl, type_name)) = self.enum_ref(owner) {
                        let decl = decl.clone();
                        return self.construct_variant(&decl, type_name, name, args, span);
                    }
                    match owner.as_str() {
                        "map" | "set" if name == "empty" => {
                            let Some(items) = self.positional(args, span, name) else {
                                return Ty::Error;
                            };
                            if !items.is_empty() {
                                self.diag(span, format!("`{owner}.empty` takes no arguments"));
                                return Ty::Error;
                            }
                            return if owner == "map" {
                                Ty::map(self.fresh(), self.fresh())
                            } else {
                                Ty::set(self.fresh())
                            };
                        }
                        "math" => return self.call_math(name, args, span),
                        "expect" => return self.call_expect(name, args, span),
                        "json" => {
                            self.diag(*owner_span, "`json` is not available yet");
                            return Ty::Error;
                        }
                        _ => {}
                    }
                    if self.struct_ref(owner).is_some() {
                        self.diag(
                            *owner_span,
                            format!("`{owner}.{name}` is not a function; `{owner}` is a struct"),
                        );
                        return Ty::Error;
                    }
                    if self.lookup_var(owner).is_none() && self.binding_ref(owner).is_none() {
                        self.diag(*owner_span, format!("unknown type or value `{owner}`"));
                        return Ty::Error;
                    }
                }
                let _ = field_span;
                let receiver_ty = self.check_expr(base, None);
                self.call_method(&receiver_ty, name, args, span)
            }
            _ => {
                self.diag(span, "this expression is not callable".to_string());
                Ty::Error
            }
        }
    }

    fn positional<'e>(
        &mut self,
        args: &'e ast::Args,
        span: Span,
        context: &str,
    ) -> Option<Vec<&'e ast::Expr>> {
        match args {
            ast::Args::Empty => Some(Vec::new()),
            ast::Args::Positional(items) => Some(items.iter().collect()),
            ast::Args::Named(_) => {
                self.diag(span, format!("`{context}` takes positional arguments only"));
                None
            }
        }
    }

    fn construct_struct(
        &mut self,
        decl: &StructInfo,
        type_name: TypeName,
        args: &ast::Args,
        span: Span,
    ) -> Ty {
        let ast::Args::Named(named) = args else {
            self.diag(
                span,
                format!(
                    "constructing `{}` requires named arguments, e.g. `{}(field: value)`",
                    decl.name, decl.name
                ),
            );
            return Ty::Error;
        };

        let subst: Subst = decl
            .generics
            .iter()
            .cloned()
            .map(|generic| (generic, self.fresh()))
            .collect();

        for (index, arg) in named.iter().enumerate() {
            if named[..index].iter().any(|other| other.name == arg.name) {
                self.diag(
                    arg.span,
                    format!("field `{}` is provided more than once", arg.name),
                );
            }
        }

        for field in &decl.fields {
            match named.iter().find(|arg| arg.name == field.name) {
                Some(arg) => {
                    let field_ty = substitute(&field.ty, &subst);
                    let value_ty = self.check_expr(&arg.value, Some(&field_ty));
                    self.expect(
                        arg.value.span(),
                        &field_ty,
                        &value_ty,
                        &format!(" for field `{}`", field.name),
                    );
                }
                None => self.diag(
                    span,
                    format!(
                        "missing field `{}` when constructing `{}`",
                        field.name, decl.name
                    ),
                ),
            }
        }

        for arg in named {
            if !decl.fields.iter().any(|field| field.name == arg.name) {
                self.diag(
                    arg.span,
                    format!("`{}` has no field `{}`", decl.name, arg.name),
                );
            }
        }

        self.instantiate_named(&type_name, &decl.generics, &subst, span)
    }

    fn instantiate_named(
        &mut self,
        type_name: &TypeName,
        generics: &[String],
        subst: &Subst,
        span: Span,
    ) -> Ty {
        let mut args = Vec::new();
        for generic in generics {
            let ty = subst
                .get(generic)
                .map(|ty| self.resolve_deep(ty))
                .unwrap_or(Ty::Error);
            if ty.is_var() {
                self.diag(
                    span,
                    format!(
                        "cannot infer type parameter `{generic}` of `{}`",
                        type_name.name
                    ),
                );
                args.push(Ty::Error);
            } else {
                args.push(ty);
            }
        }
        Ty::Named(type_name.clone(), args)
    }

    fn construct_variant(
        &mut self,
        decl: &EnumInfo,
        type_name: TypeName,
        variant_name: &str,
        args: &ast::Args,
        span: Span,
    ) -> Ty {
        let Some(variant) = decl.variants.iter().find(|v| v.name == variant_name) else {
            self.diag(
                span,
                format!("enum `{}` has no variant `{variant_name}`", decl.name),
            );
            return Ty::Error;
        };

        let subst: Subst = decl
            .generics
            .iter()
            .cloned()
            .map(|generic| (generic, self.fresh()))
            .collect();

        let Some(items) = self.positional(args, span, variant_name) else {
            return Ty::Error;
        };
        if items.len() != variant.payload.len() {
            self.diag(
                span,
                format!(
                    "variant `{}.{}` expects {} value(s), found {}",
                    decl.name,
                    variant.name,
                    variant.payload.len(),
                    items.len()
                ),
            );
            return Ty::Error;
        }
        for (item, payload_ty) in items.iter().zip(variant.payload.iter()) {
            let expected = substitute(payload_ty, &subst);
            let actual = self.check_expr(item, Some(&expected));
            self.expect(item.span(), &expected, &actual, " in this enum variant");
        }

        self.instantiate_named(&type_name, &decl.generics, &subst, span)
    }

    /// Call a package-level function with positional arguments.
    fn call_fn(
        &mut self,
        _module: ModuleId,
        decl: &FnInfo,
        args: &ast::Args,
        span: Span,
        display: &str,
    ) -> Ty {
        let Some(items) = self.positional(args, span, display) else {
            return Ty::Error;
        };
        if items.len() != decl.params.len() {
            self.diag(
                span,
                format!(
                    "`{display}` expects {} argument(s), found {}",
                    decl.params.len(),
                    items.len()
                ),
            );
            return Ty::Error;
        }

        let subst: Subst = decl
            .generics
            .iter()
            .cloned()
            .map(|generic| (generic, self.fresh()))
            .collect();

        for (item, (param_name, param_ty, _)) in items.iter().zip(decl.params.iter()) {
            let expected = substitute(param_ty, &subst);
            let actual = self.check_expr(item, Some(&expected));
            self.expect(
                item.span(),
                &expected,
                &actual,
                &format!(" for `{param_name}`"),
            );
        }

        self.resolve_deep(&substitute(&decl.ret, &subst))
    }

    /// `package.name(...)`: a function call, or struct construction when the
    /// member is a type.
    fn call_package_member(
        &mut self,
        module: ModuleId,
        name: &str,
        args: &ast::Args,
        span: Span,
    ) -> Ty {
        let package = self.program.modules[module].name.clone();

        if let Some(decl) = self.program.find_fn(module, name) {
            let (exported, decl) = (decl.is_export, decl.clone());
            if !exported {
                self.diag(
                    span,
                    format!("`{name}` is private to package `{package}`; add `export`"),
                );
                return Ty::Error;
            }
            return self.call_fn(module, &decl, args, span, &format!("{package}.{name}"));
        }

        if let Some(decl) = self.program.find_struct(module, name) {
            let (exported, decl) = (decl.is_export, decl.clone());
            if !exported {
                self.diag(
                    span,
                    format!("`{name}` is private to package `{package}`; add `export`"),
                );
                return Ty::Error;
            }
            let type_name = TypeName {
                module,
                name: name.to_string(),
            };
            return self.construct_struct(&decl, type_name, args, span);
        }

        if self.program.find_enum(module, name).is_some() {
            self.diag(
                span,
                format!("`{name}` is an enum; construct a variant, e.g. `{name}.Some(...)`"),
            );
            return Ty::Error;
        }

        self.diag(span, format!("package `{package}` has no `{name}`"));
        Ty::Error
    }

    /// Receiver sugar: `p.f(args)` for `fn f(p: Point, …)` declared in the
    /// package that owns `Point`.
    fn call_receiver_fn(
        &mut self,
        decl: &FnInfo,
        receiver: &Ty,
        type_args: &[Ty],
        args: &ast::Args,
        span: Span,
    ) -> Ty {
        let Some(items) = self.positional(args, span, &decl.name) else {
            return Ty::Error;
        };
        if items.len() + 1 != decl.params.len() {
            self.diag(
                span,
                format!(
                    "`{}` expects {} argument(s) plus the receiver, found {}",
                    decl.name,
                    decl.params.len() - 1,
                    items.len()
                ),
            );
            return Ty::Error;
        }

        let subst: Subst = decl
            .generics
            .iter()
            .cloned()
            .zip(type_args.iter().cloned())
            .collect();

        for (item, (param_name, param_ty, _)) in items.iter().zip(decl.params.iter().skip(1)) {
            let expected = substitute(param_ty, &subst);
            let actual = self.check_expr(item, Some(&expected));
            self.expect(
                item.span(),
                &expected,
                &actual,
                &format!(" for `{param_name}`"),
            );
        }

        let _ = receiver;
        self.resolve_deep(&substitute(&decl.ret, &subst))
    }

    fn call_method(&mut self, receiver: &Ty, name: &str, args: &ast::Args, span: Span) -> Ty {
        let receiver_ty = self.resolve_deep(receiver);

        // Receiver function declared in the package that owns the type.
        let receiver_fn = match &receiver_ty {
            Ty::Named(type_name, _) => self
                .program
                .find_fn(type_name.module, name)
                .filter(|decl| {
                    decl.params.first().is_some_and(|(_, ty, _)| match ty {
                        Ty::Named(first, _) => {
                            first.module == type_name.module && first.name == type_name.name
                        }
                        _ => false,
                    })
                })
                .cloned(),
            _ => None,
        };

        if name == "to_string" && receiver_fn.is_none() {
            let Some(items) = self.positional(args, span, name) else {
                return Ty::Error;
            };
            if !items.is_empty() {
                self.diag(
                    span,
                    format!("`to_string` expects 0 argument(s), found {}", items.len()),
                );
                return Ty::Error;
            }
            if matches!(&receiver_ty, Ty::Unit) {
                self.diag(span, "`Unit` has no value to convert to `Str`");
                return Ty::Error;
            }
            return Ty::Str;
        }

        if let Some(decl) = receiver_fn {
            let type_args = match &receiver_ty {
                Ty::Named(_, args) => args.clone(),
                _ => Vec::new(),
            };
            return self.call_receiver_fn(&decl, &receiver_ty, &type_args, args, span);
        }

        if let Ty::Named(type_name, _) = &receiver_ty {
            if type_name.is_builtin() {
                let shown = self.display_ty(&receiver_ty);
                self.diag(span, format!("`{shown}` has no methods"));
                return Ty::Error;
            }
            let shown = self.display_ty(&receiver_ty);
            self.diag(span, format!("`{shown}` has no function `{name}`"));
            return Ty::Error;
        }

        self.builtin_method(&receiver_ty, name, args, span)
    }

    fn require_value(&mut self, span: Span, ty: &Ty) {
        if matches!(self.resolve_deep(ty), Ty::Unit) {
            self.diag(
                span,
                "this expression has type `Unit`, which cannot be interpolated",
            );
        }
    }

    fn check_tagged(&mut self, tag: &ast::Expr, exprs: &[ast::Expr], span: Span) -> Ty {
        let (display, func) = match tag {
            ast::Expr::Ident { name, .. } => match self.fn_ref(name) {
                Some((_, decl)) => (name.clone(), decl.clone()),
                None => {
                    self.diag(span, format!("unknown function `{name}` for this tag"));
                    return Ty::Error;
                }
            },
            ast::Expr::Field { base, name, .. } => {
                let ast::Expr::Ident { name: owner, .. } = base.as_ref() else {
                    self.diag(
                        span,
                        "a tagged template tag must name a package-level function",
                    );
                    return Ty::Error;
                };
                let Some(module) = self.package_ref(owner) else {
                    self.diag(span, format!("unknown package `{owner}` for this tag"));
                    return Ty::Error;
                };
                match self.program.find_fn(module, name) {
                    Some(decl) if decl.is_export => (format!("{owner}.{name}"), decl.clone()),
                    Some(_) => {
                        self.diag(
                            span,
                            format!(
                                "`{owner}.{name}` is private to package `{owner}`; add `export`"
                            ),
                        );
                        return Ty::Error;
                    }
                    None => {
                        self.diag(span, format!("`{owner}.{name}` is not a function"));
                        return Ty::Error;
                    }
                }
            }
            _ => {
                self.diag(
                    span,
                    "a tagged template tag must name a package-level function",
                );
                return Ty::Error;
            }
        };

        if func.params.len() != 2 {
            self.diag(
                span,
                format!(
                    "a tagged template needs `(List<Str>, List<T>)`, but `{display}` takes {} parameter(s)",
                    func.params.len()
                ),
            );
            return Ty::Error;
        }
        self.expect(
            span,
            &Ty::list(Ty::Str),
            &func.params[0].1.clone(),
            " in the tag signature",
        );
        let value_list_ty = self.resolve_deep(&func.params[1].1);
        let Ty::List(element) = value_list_ty else {
            let shown = self.display_ty(&func.params[1].1);
            self.diag(
                span,
                format!("the second parameter of `{display}` must be `List<T>`, found `{shown}`"),
            );
            return Ty::Error;
        };
        self.require_value(span, &element);
        for inner in exprs {
            let actual = self.check_expr(inner, Some(&element));
            self.expect(
                inner.span(),
                &element,
                &actual,
                " in this tagged template value",
            );
        }
        self.resolve_deep(&func.ret)
    }

    fn check_jsx(&mut self, element: &ast::JsxElement) -> Ty {
        let Some((class, type_name)) = self.struct_ref(&element.name) else {
            self.diag(
                element.span,
                format!("unknown struct `{}` in JSX", element.name),
            );
            return Ty::Error;
        };
        let class = class.clone();

        let subst: Subst = class
            .generics
            .iter()
            .cloned()
            .map(|generic| (generic, self.fresh()))
            .collect();

        let mut provided: Vec<String> = Vec::new();

        for attr in &element.attrs {
            let Some(field) = class.fields.iter().find(|field| field.name == attr.name) else {
                self.diag(
                    attr.span,
                    format!("`{}` has no field `{}`", class.name, attr.name),
                );
                continue;
            };
            if attr.name == "children" {
                self.diag(
                    attr.span,
                    "`children` is provided by JSX child elements, not as an attribute".to_string(),
                );
                continue;
            }
            if provided.iter().any(|name| name == &attr.name) {
                self.diag(
                    attr.span,
                    format!("field `{}` is provided more than once", attr.name),
                );
            }
            provided.push(attr.name.clone());
            let field_ty = substitute(&field.ty, &subst);
            let (value_ty, value_span) = match &attr.value {
                ast::JsxAttrValue::Str(_) => (Ty::Str, attr.span),
                ast::JsxAttrValue::Expr(expr) => {
                    (self.check_expr(expr, Some(&field_ty)), expr.span())
                }
            };
            self.expect(
                value_span,
                &field_ty,
                &value_ty,
                &format!(" for attribute `{}`", attr.name),
            );
        }

        let children_field = class.fields.iter().find(|field| field.name == "children");

        if element.children.is_empty() {
            // `children` is part of the constructor, so it must be supplied even
            // when empty; the empty list satisfies it without further checks.
            if children_field.is_some() {
                provided.push("children".to_string());
            }
        } else {
            let Some(field) = children_field else {
                self.diag(
                    element.span,
                    format!(
                        "`{}` has children but declares no `children` field",
                        class.name
                    ),
                );
                return Ty::Error;
            };
            let field_ty = substitute(&field.ty, &subst);
            let element_ty = match self.resolve_deep(&field_ty) {
                Ty::List(inner) => *inner,
                Ty::Error => Ty::Error,
                other => {
                    let shown = other.to_string();
                    self.diag(
                        field.span,
                        format!(
                            "`children` of `{}` must be a `List<...>`, found `{shown}`",
                            class.name
                        ),
                    );
                    Ty::Error
                }
            };
            for child in &element.children {
                match child {
                    ast::JsxChild::Element(nested) => {
                        let ty = self.check_jsx(nested);
                        self.expect(nested.span, &element_ty, &ty, " as a JSX child");
                    }
                    ast::JsxChild::Text(_) => {
                        self.expect(
                            element.span,
                            &element_ty,
                            &Ty::Str,
                            " for JSX text children",
                        );
                    }
                    ast::JsxChild::Expr(expr) => {
                        let ty = self.check_expr(expr, Some(&element_ty));
                        self.expect(expr.span(), &element_ty, &ty, " as a JSX child");
                    }
                }
            }
            provided.push("children".to_string());
        }

        for field in &class.fields {
            if !provided.iter().any(|name| name == &field.name) {
                self.diag(
                    element.span,
                    format!(
                        "missing field `{}` when constructing `{}`",
                        field.name, class.name
                    ),
                );
            }
        }

        self.instantiate_named(&type_name, &class.generics, &subst, element.span)
    }

    // -------------------------------------------------------------- builtins

    fn call_math(&mut self, name: &str, args: &ast::Args, span: Span) -> Ty {
        let Some(items) = self.positional(args, span, name) else {
            return Ty::Error;
        };
        let arity = match name {
            "abs" | "floor" | "ceil" | "round" | "sqrt" => 1,
            "min" | "max" | "pow" => 2,
            "pi" => 0,
            _ => {
                self.diag(span, format!("`Math` has no method `{name}`"));
                return Ty::Error;
            }
        };
        if items.len() != arity {
            self.diag(
                span,
                format!(
                    "`Math.{name}` expects {arity} argument(s), found {}",
                    items.len()
                ),
            );
            return Ty::Error;
        }
        for item in &items {
            let ty = self.check_expr(item, Some(&Ty::Float));
            self.expect(item.span(), &Ty::Float, &ty, &format!(" in `Math.{name}`"));
        }
        Ty::Float
    }

    fn call_expect(&mut self, name: &str, args: &ast::Args, span: Span) -> Ty {
        if !self.in_test {
            self.diag(span, "`expect` is only available inside a `test` block");
            return Ty::Error;
        }
        let Some(items) = self.positional(args, span, name) else {
            return Ty::Error;
        };
        match name {
            "eq" | "not_equal" => {
                if items.len() != 2 {
                    self.diag(span, format!("`expect.{name}` expects 2 arguments"));
                    return Ty::Error;
                }
                let left = self.check_expr(items[0], None);
                let right = self.check_expr(items[1], Some(&left));
                self.expect(items[1].span(), &left, &right, " in this assertion");
                Ty::Unit
            }
            "true" | "false" => {
                if items.len() != 1 {
                    self.diag(span, format!("`expect.{name}` expects 1 argument"));
                    return Ty::Error;
                }
                let ty = self.check_expr(items[0], Some(&Ty::Bool));
                self.expect(items[0].span(), &Ty::Bool, &ty, " in this assertion");
                Ty::Unit
            }
            _ => {
                self.diag(span, format!("`expect` has no function `{name}`"));
                Ty::Error
            }
        }
    }

    fn builtin_method(&mut self, receiver: &Ty, name: &str, args: &ast::Args, span: Span) -> Ty {
        let receiver = self.resolve_deep(receiver);

        match &receiver {
            Ty::Str => {
                let arity = match name {
                    "len" => 0,
                    "trim" | "trim_indent" | "lower" | "upper" | "chars" => 0,
                    "parse_int" | "parse_float" => 0,
                    "split" | "starts_with" | "ends_with" | "contains" => 1,
                    "replace" | "slice" => 2,
                    _ => {
                        self.diag(span, format!("`Str` has no method `{name}`"));
                        return Ty::Error;
                    }
                };
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != arity {
                    self.diag(
                        span,
                        format!(
                            "`Str.{name}` expects {arity} argument(s), found {}",
                            items.len()
                        ),
                    );
                    return Ty::Error;
                }
                match name {
                    "split" | "starts_with" | "ends_with" | "contains" => {
                        for item in &items {
                            let ty = self.check_expr(item, Some(&Ty::Str));
                            self.expect(item.span(), &Ty::Str, &ty, &format!(" in `Str.{name}`"));
                        }
                        if name == "split" {
                            Ty::list(Ty::Str)
                        } else {
                            Ty::Bool
                        }
                    }
                    "replace" => {
                        for item in &items {
                            let ty = self.check_expr(item, Some(&Ty::Str));
                            self.expect(item.span(), &Ty::Str, &ty, " in `Str.replace`");
                        }
                        Ty::Str
                    }
                    "slice" => {
                        for item in &items {
                            let ty = self.check_expr(item, Some(&Ty::Int));
                            self.expect(item.span(), &Ty::Int, &ty, " in `Str.slice`");
                        }
                        Ty::Str
                    }
                    "len" => Ty::Int,
                    "chars" => Ty::list(Ty::Str),
                    "parse_int" => Ty::option(Ty::Int),
                    "parse_float" => Ty::option(Ty::Float),
                    _ => Ty::Str,
                }
            }
            Ty::List(element) => self.list_method(element, name, args, span),
            Ty::Set(element) => {
                let arity = match name {
                    "len" => 0,
                    "insert" | "remove" | "contains" => 1,
                    "union" | "intersect" | "difference" => 1,
                    _ => {
                        self.diag(span, format!("`Set` has no method `{name}`"));
                        return Ty::Error;
                    }
                };
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != arity {
                    self.diag(
                        span,
                        format!(
                            "`Set.{name}` expects {arity} argument(s), found {}",
                            items.len()
                        ),
                    );
                    return Ty::Error;
                }
                if name == "len" {
                    return Ty::Int;
                }
                let set_ty = Ty::set(*element.clone());
                let expected = if matches!(name, "union" | "intersect" | "difference") {
                    set_ty.clone()
                } else {
                    *element.clone()
                };
                for item in &items {
                    let ty = self.check_expr(item, Some(&expected));
                    self.expect(item.span(), &expected, &ty, &format!(" in `Set.{name}`"));
                }
                if name == "contains" {
                    Ty::Bool
                } else {
                    set_ty
                }
            }
            Ty::Map(key, value) => {
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                let expected: Vec<Ty> = match name {
                    "len" | "keys" | "values" => vec![],
                    "get" | "remove" | "contains_key" => vec![*key.clone()],
                    "insert" => vec![*key.clone(), *value.clone()],
                    _ => {
                        self.diag(span, format!("`Map` has no method `{name}`"));
                        return Ty::Error;
                    }
                };
                if items.len() != expected.len() {
                    self.diag(
                        span,
                        format!(
                            "`Map.{name}` expects {} argument(s), found {}",
                            expected.len(),
                            items.len()
                        ),
                    );
                    return Ty::Error;
                }
                for (item, expected) in items.iter().zip(expected.iter()) {
                    let ty = self.check_expr(item, Some(expected));
                    self.expect(item.span(), expected, &ty, &format!(" in `Map.{name}`"));
                }
                match name {
                    "len" => Ty::Int,
                    "get" => Ty::option(*value.clone()),
                    "insert" | "remove" => Ty::map(*key.clone(), *value.clone()),
                    "keys" => Ty::list(*key.clone()),
                    "values" => Ty::list(*value.clone()),
                    "contains_key" => Ty::Bool,
                    _ => Ty::Error,
                }
            }
            Ty::Option(inner) => {
                let inner = *inner.clone();
                match name {
                    "is_some" | "is_none" => {
                        self.require_arity(args, span, 0, name);
                        Ty::Bool
                    }
                    "unwrap" => {
                        self.require_arity(args, span, 0, name);
                        inner
                    }
                    "unwrap_or" => {
                        let Some(items) = self.positional(args, span, name) else {
                            return Ty::Error;
                        };
                        if items.len() != 1 {
                            self.diag(span, "`Option.unwrap_or` expects 1 argument".to_string());
                            return Ty::Error;
                        }
                        let ty = self.check_expr(items[0], Some(&inner));
                        self.expect(items[0].span(), &inner, &ty, " in `unwrap_or`");
                        inner
                    }
                    "map" => {
                        let Some(items) = self.positional(args, span, name) else {
                            return Ty::Error;
                        };
                        if items.len() != 1 {
                            self.diag(span, "`Option.map` expects 1 argument".to_string());
                            return Ty::Error;
                        }
                        let Some(result) = self.check_lambda(items[0], &[inner], "map") else {
                            return Ty::Error;
                        };
                        Ty::option(result)
                    }
                    "and_then" => {
                        let Some(items) = self.positional(args, span, name) else {
                            return Ty::Error;
                        };
                        if items.len() != 1 {
                            self.diag(span, "`Option.and_then` expects 1 argument".to_string());
                            return Ty::Error;
                        }
                        let Some(result) = self.check_lambda(items[0], &[inner], "and_then") else {
                            return Ty::Error;
                        };
                        match self.resolve_deep(&result) {
                            Ty::Option(_) => result,
                            other => {
                                let shown = other.to_string();
                                self.diag(
                                    items[0].span(),
                                    format!("`and_then` needs a lambda returning `Option`, found `{shown}`"),
                                );
                                Ty::Error
                            }
                        }
                    }
                    _ => {
                        self.diag(span, format!("`Option` has no method `{name}`"));
                        Ty::Error
                    }
                }
            }
            Ty::Result(ok, err) => {
                let ok = *ok.clone();
                let err = *err.clone();
                match name {
                    "is_ok" | "is_err" => {
                        self.require_arity(args, span, 0, name);
                        Ty::Bool
                    }
                    "unwrap" => {
                        self.require_arity(args, span, 0, name);
                        ok
                    }
                    "unwrap_or" => {
                        let Some(items) = self.positional(args, span, name) else {
                            return Ty::Error;
                        };
                        if items.len() != 1 {
                            self.diag(span, "`Result.unwrap_or` expects 1 argument".to_string());
                            return Ty::Error;
                        }
                        let ty = self.check_expr(items[0], Some(&ok));
                        self.expect(items[0].span(), &ok, &ty, " in `unwrap_or`");
                        ok
                    }
                    "map" => {
                        let Some(items) = self.positional(args, span, name) else {
                            return Ty::Error;
                        };
                        if items.len() != 1 {
                            self.diag(span, "`Result.map` expects 1 argument".to_string());
                            return Ty::Error;
                        }
                        let Some(result) = self.check_lambda(items[0], &[ok], "map") else {
                            return Ty::Error;
                        };
                        Ty::result(result, err)
                    }
                    "map_err" => {
                        let Some(items) = self.positional(args, span, name) else {
                            return Ty::Error;
                        };
                        if items.len() != 1 {
                            self.diag(span, "`Result.map_err` expects 1 argument".to_string());
                            return Ty::Error;
                        }
                        let Some(result) = self.check_lambda(items[0], &[err], "map_err") else {
                            return Ty::Error;
                        };
                        Ty::result(ok, result)
                    }
                    "and_then" => {
                        let Some(items) = self.positional(args, span, name) else {
                            return Ty::Error;
                        };
                        if items.len() != 1 {
                            self.diag(span, "`Result.and_then` expects 1 argument".to_string());
                            return Ty::Error;
                        }
                        let Some(result) = self.check_lambda(items[0], &[ok], "and_then") else {
                            return Ty::Error;
                        };
                        match self.resolve_deep(&result) {
                            Ty::Result(_, _) => result,
                            other => {
                                let shown = other.to_string();
                                self.diag(
                                    items[0].span(),
                                    format!("`and_then` needs a lambda returning `Result`, found `{shown}`"),
                                );
                                Ty::Error
                            }
                        }
                    }
                    _ => {
                        self.diag(span, format!("`Result` has no method `{name}`"));
                        Ty::Error
                    }
                }
            }
            Ty::Error => Ty::Error,
            other => {
                let shown = other.to_string();
                self.diag(span, format!("`{shown}` has no methods"));
                Ty::Error
            }
        }
    }

    fn require_arity(&mut self, args: &ast::Args, span: Span, arity: usize, name: &str) {
        if let Some(items) = self.positional(args, span, name) {
            if items.len() != arity {
                self.diag(
                    span,
                    format!(
                        "`{name}` expects {arity} argument(s), found {}",
                        items.len()
                    ),
                );
            }
        }
    }

    fn list_method(&mut self, element: &Ty, name: &str, args: &ast::Args, span: Span) -> Ty {
        let element = element.clone();
        let list_ty = Ty::list(element.clone());

        match name {
            "len" => {
                self.require_arity(args, span, 0, name);
                Ty::Int
            }
            "get" => {
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != 1 {
                    self.diag(span, "`List.get` expects 1 argument".to_string());
                    return Ty::Error;
                }
                let ty = self.check_expr(items[0], Some(&Ty::Int));
                self.expect(items[0].span(), &Ty::Int, &ty, " in `List.get`");
                Ty::option(element)
            }
            "push" => {
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != 1 {
                    self.diag(span, "`List.push` expects 1 argument".to_string());
                    return Ty::Error;
                }
                let ty = self.check_expr(items[0], Some(&element));
                self.expect(items[0].span(), &element, &ty, " in `List.push`");
                list_ty
            }
            "pop" => {
                self.require_arity(args, span, 0, name);
                Ty::option(element)
            }
            "map" => {
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != 1 {
                    self.diag(span, "`List.map` expects 1 argument".to_string());
                    return Ty::Error;
                }
                let Some(result) = self.check_lambda(items[0], &[element], "map") else {
                    return Ty::Error;
                };
                Ty::list(result)
            }
            "filter" => {
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != 1 {
                    self.diag(span, "`List.filter` expects 1 argument".to_string());
                    return Ty::Error;
                }
                let Some(result) =
                    self.check_lambda(items[0], std::slice::from_ref(&element), "filter")
                else {
                    return Ty::Error;
                };
                self.expect(
                    items[0].span(),
                    &Ty::Bool,
                    &result,
                    " as the `filter` predicate",
                );
                list_ty
            }
            "fold" => {
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != 2 {
                    self.diag(span, "`List.fold` expects 2 arguments".to_string());
                    return Ty::Error;
                }
                let accumulator = self.check_expr(items[0], None);
                let Some(result) =
                    self.check_lambda(items[1], &[accumulator.clone(), element], "fold")
                else {
                    return Ty::Error;
                };
                self.expect(
                    items[1].span(),
                    &accumulator,
                    &result,
                    " as the `fold` result",
                );
                accumulator
            }
            "any" | "all" => {
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != 1 {
                    self.diag(span, format!("`List.{name}` expects 1 argument"));
                    return Ty::Error;
                }
                let Some(result) = self.check_lambda(items[0], &[element], name) else {
                    return Ty::Error;
                };
                self.expect(
                    items[0].span(),
                    &Ty::Bool,
                    &result,
                    &format!(" as the `{name}` predicate"),
                );
                Ty::Bool
            }
            "sort" | "reverse" => {
                self.require_arity(args, span, 0, name);
                list_ty
            }
            "concat" => {
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != 1 {
                    self.diag(span, "`List.concat` expects 1 argument".to_string());
                    return Ty::Error;
                }
                let ty = self.check_expr(items[0], Some(&list_ty));
                self.expect(items[0].span(), &list_ty, &ty, " in `List.concat`");
                list_ty
            }
            "contains" => {
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != 1 {
                    self.diag(span, "`List.contains` expects 1 argument".to_string());
                    return Ty::Error;
                }
                let ty = self.check_expr(items[0], Some(&element));
                self.expect(items[0].span(), &element, &ty, " in `List.contains`");
                Ty::Bool
            }
            "index_of" => {
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != 1 {
                    self.diag(span, "`List.index_of` expects 1 argument".to_string());
                    return Ty::Error;
                }
                let ty = self.check_expr(items[0], Some(&element));
                self.expect(items[0].span(), &element, &ty, " in `List.index_of`");
                Ty::option(Ty::Int)
            }
            "join" => {
                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != 1 {
                    self.diag(span, "`List.join` expects 1 argument".to_string());
                    return Ty::Error;
                }
                self.expect(
                    span,
                    &Ty::Str,
                    &element,
                    " for `List.join`; it needs `List<Str>`",
                );
                let ty = self.check_expr(items[0], Some(&Ty::Str));
                self.expect(items[0].span(), &Ty::Str, &ty, " in `List.join`");
                Ty::Str
            }
            _ => {
                self.diag(span, format!("`List` has no method `{name}`"));
                Ty::Error
            }
        }
    }

    fn check_lambda(&mut self, expr: &ast::Expr, params: &[Ty], context: &str) -> Option<Ty> {
        let ast::Expr::Lambda {
            params: names,
            body,
            span,
        } = expr
        else {
            self.diag(
                expr.span(),
                format!("`{context}` needs a lambda argument, e.g. `fn(x) => ...`"),
            );
            return None;
        };
        if names.len() != params.len() {
            self.diag(
                *span,
                format!(
                    "`{context}` lambda expects {} parameter(s), found {}",
                    params.len(),
                    names.len()
                ),
            );
            return None;
        }
        self.scopes.push(HashMap::new());
        for (name, ty) in names.iter().zip(params.iter()) {
            self.declare(name, ty.clone(), false, *span);
        }
        let previous = self.in_lambda;
        self.in_lambda = true;
        let result = match body {
            LambdaBody::Expr(expr) => self.check_expr(expr, None),
            LambdaBody::Block(block) => self.check_block(block, None),
        };
        self.in_lambda = previous;
        self.record_type(expr, &result);
        self.scopes.pop();
        Some(result)
    }
}
