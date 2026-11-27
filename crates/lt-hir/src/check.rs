//! Declaration collection, type resolution, and body type checking.

use std::collections::HashMap;

use lt_syntax::ast::{self, BinOp, LambdaBody, MatchBody, Pattern, UnOp};
use lt_syntax::span::line_col_at;
use lt_syntax::{Diagnostic, Span};

use crate::program::{
    builtin_enum, ClassInfo, DeclKind, EnumInfo, MethodInfo, ModuleId, Program, VariantInfo,
};
use crate::types::{mentions_any_generic, substitute, Subst, Ty, TypeName, TypeTable};

pub fn check_program(mut program: Program) -> (Program, Vec<(ModuleId, Diagnostic)>, TypeTable) {
    let mut diagnostics = Vec::new();
    let mut types = TypeTable::default();
    collect_declarations(&mut program, &mut diagnostics);
    resolve_declarations(&mut program, &mut diagnostics);
    check_bodies(&program, &mut diagnostics, &mut types);
    (program, diagnostics, types)
}

fn diag(diagnostics: &mut Vec<Diagnostic>, src: &str, span: Span, message: String) {
    let loc = line_col_at(src, span.start);
    diagnostics.push(Diagnostic::error(message, span, loc.line, loc.col));
}

// ---------------------------------------------------------------- declarations

fn collect_declarations(program: &mut Program, diagnostics: &mut Vec<(ModuleId, Diagnostic)>) {
    for module_id in 0..program.modules.len() {
        let mut classes = Vec::new();
        let mut enums = Vec::new();
        let mut local = Vec::new();
        {
            let module = &program.modules[module_id];
            for (index, item) in module.ast.items.iter().enumerate() {
                match item {
                    ast::Item::Class(class) => {
                        if classes.iter().any(|c: &ClassInfo| c.name == class.name)
                            || enums.iter().any(|e: &EnumInfo| e.name == class.name)
                        {
                            diag(
                                &mut local,
                                &module.source,
                                class.span,
                                format!("`{}` is declared more than once in this file", class.name),
                            );
                        }
                        classes.push(ClassInfo {
                            name: class.name.clone(),
                            is_pub: class.is_pub,
                            generics: class.generics.clone(),
                            fields: Vec::new(),
                            methods: Vec::new(),
                            span: class.span,
                            ast_index: index,
                        });
                    }
                    ast::Item::Enum(decl) => {
                        if classes.iter().any(|c: &ClassInfo| c.name == decl.name)
                            || enums.iter().any(|e: &EnumInfo| e.name == decl.name)
                        {
                            diag(
                                &mut local,
                                &module.source,
                                decl.span,
                                format!("`{}` is declared more than once in this file", decl.name),
                            );
                        }
                        enums.push(EnumInfo {
                            name: decl.name.clone(),
                            is_pub: decl.is_pub,
                            generics: decl.generics.clone(),
                            variants: Vec::new(),
                            span: decl.span,
                            ast_index: index,
                        });
                    }
                    ast::Item::TestClass(_) => {}
                }
            }
        }
        diagnostics.extend(local.into_iter().map(|d| (module_id, d)));
        program.modules[module_id].classes = classes;
        program.modules[module_id].enums = enums;
    }
}

fn resolve_declarations(program: &mut Program, diagnostics: &mut Vec<(ModuleId, Diagnostic)>) {
    for module_id in 0..program.modules.len() {
        let mut classes = Vec::new();
        let mut enums = Vec::new();
        let mut local = Vec::new();
        {
            let module = &program.modules[module_id];
            for (index, item) in module.ast.items.iter().enumerate() {
                match item {
                    ast::Item::Class(class) => {
                        classes.push(resolve_class(program, module_id, index, class, &mut local));
                    }
                    ast::Item::Enum(decl) => {
                        enums.push(resolve_enum(program, module_id, index, decl, &mut local));
                    }
                    ast::Item::TestClass(_) => {}
                }
            }
        }
        diagnostics.extend(local.into_iter().map(|d| (module_id, d)));
        program.modules[module_id].classes = classes;
        program.modules[module_id].enums = enums;
    }
}

fn resolve_class(
    program: &Program,
    module_id: ModuleId,
    ast_index: usize,
    class: &ast::ClassDecl,
    diagnostics: &mut Vec<Diagnostic>,
) -> ClassInfo {
    let source = &program.modules[module_id].source;

    let mut fields = Vec::new();
    let mut methods = Vec::new();

    for field in &class.fields {
        if fields
            .iter()
            .any(|f: &crate::program::FieldInfo| f.name == field.name)
        {
            diag(
                diagnostics,
                source,
                field.span,
                format!("duplicate field `{}` in `{}`", field.name, class.name),
            );
        }
        let ty = resolve_type(program, module_id, &class.generics, &field.ty, diagnostics);
        fields.push(crate::program::FieldInfo {
            name: field.name.clone(),
            ty,
            span: field.span,
        });
    }

    for method in &class.methods {
        if methods.iter().any(|m: &MethodInfo| m.name == method.name) {
            diag(
                diagnostics,
                source,
                method.span,
                format!(
                    "duplicate method `{}` in `{}`; Lingua Tungga has no overloading",
                    method.name, class.name
                ),
            );
        }
        let is_static = method.is_static || !method.has_self;
        if method.is_static && method.has_self {
            diag(
                diagnostics,
                source,
                method.span,
                "a `static fn` cannot take `self`".to_string(),
            );
        }
        let mut seen = std::collections::HashSet::new();
        for param in &method.params {
            if !seen.insert(&param.name) {
                diag(
                    diagnostics,
                    source,
                    param.span,
                    format!("duplicate parameter `{}`", param.name),
                );
            }
        }

        let params = method
            .params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    resolve_type(program, module_id, &class.generics, &param.ty, diagnostics),
                    param.span,
                )
            })
            .collect::<Vec<_>>();

        let ret = match &method.ret {
            Some(ty) => resolve_type(program, module_id, &class.generics, ty, diagnostics),
            None => Ty::Unit,
        };

        if is_static {
            let uses_class_generic = params
                .iter()
                .any(|(_, ty, _)| mentions_any_generic(ty, &class.generics))
                || mentions_any_generic(&ret, &class.generics);
            if uses_class_generic {
                diag(
                    diagnostics,
                    source,
                    method.span,
                    format!(
                        "static method `{}` cannot use the class type parameters of `{}`",
                        method.name, class.name
                    ),
                );
            }
        }

        methods.push(MethodInfo {
            name: method.name.clone(),
            is_pub: method.is_pub,
            is_static,
            has_self: method.has_self,
            params,
            ret,
            span: method.span,
        });
    }

    ClassInfo {
        name: class.name.clone(),
        is_pub: class.is_pub,
        generics: class.generics.clone(),
        fields,
        methods,
        span: class.span,
        ast_index,
    }
}

fn resolve_enum(
    program: &Program,
    module_id: ModuleId,
    ast_index: usize,
    decl: &ast::EnumDecl,
    diagnostics: &mut Vec<Diagnostic>,
) -> EnumInfo {
    let source = &program.modules[module_id].source;

    let mut variants = Vec::new();
    for variant in &decl.variants {
        if variants
            .iter()
            .any(|v: &VariantInfo| v.name == variant.name)
        {
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
        name: decl.name.clone(),
        is_pub: decl.is_pub,
        generics: decl.generics.clone(),
        variants,
        span: decl.span,
        ast_index,
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
    if ty.path.len() != 1 {
        diag(
            diagnostics,
            source,
            ty.span,
            "nested type paths are not supported; import the type instead".to_string(),
        );
        return Ty::Error;
    }
    let name = &ty.path[0];

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

    let base = if generics.iter().any(|generic| generic == name) {
        Ty::Generic(name.clone())
    } else if let Some(primitive) = primitive_type(name) {
        primitive
    } else if let Some(expected) = builtin_arity(name) {
        if args.len() != expected {
            arity_error(expected, diagnostics);
            Ty::Error
        } else {
            builtin_type(name, &args)
        }
    } else {
        match lookup_decl(program, module_id, name) {
            Some((type_name, _, arity)) => {
                if args.len() != arity {
                    arity_error(arity, diagnostics);
                    Ty::Error
                } else {
                    Ty::Named(type_name, args.clone())
                }
            }
            None => {
                diag(
                    diagnostics,
                    source,
                    ty.span,
                    format!("unknown type `{name}`"),
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

/// Look up a class/enum name visible from `module_id`.
fn lookup_decl(
    program: &Program,
    module_id: ModuleId,
    name: &str,
) -> Option<(TypeName, DeclKind, usize)> {
    let module = &program.modules[module_id];
    if let Some(class) = module.classes.iter().find(|c| c.name == name) {
        return Some((
            TypeName {
                module: module_id,
                name: name.to_string(),
            },
            DeclKind::Class,
            class.generics.len(),
        ));
    }
    if let Some(decl) = module.enums.iter().find(|e| e.name == name) {
        return Some((
            TypeName {
                module: module_id,
                name: name.to_string(),
            },
            DeclKind::Enum,
            decl.generics.len(),
        ));
    }
    for import in &module.imports {
        if import.name != name {
            continue;
        }
        let target = &program.modules[import.module];
        if let Some(class) = target.classes.iter().find(|c| c.name == name) {
            if class.is_pub {
                return Some((
                    TypeName {
                        module: import.module,
                        name: name.to_string(),
                    },
                    DeclKind::Class,
                    class.generics.len(),
                ));
            }
        }
        if let Some(decl) = target.enums.iter().find(|e| e.name == name) {
            if decl.is_pub {
                return Some((
                    TypeName {
                        module: import.module,
                        name: name.to_string(),
                    },
                    DeclKind::Enum,
                    decl.generics.len(),
                ));
            }
        }
    }
    if builtin_enum(name).is_some() {
        return Some((TypeName::builtin(name), DeclKind::Enum, 0));
    }
    None
}

// ---------------------------------------------------------------- body checking

fn check_bodies(
    program: &Program,
    diagnostics: &mut Vec<(ModuleId, Diagnostic)>,
    types: &mut TypeTable,
) {
    for module in &program.modules {
        for item in &module.ast.items {
            match item {
                ast::Item::Class(class) => {
                    let Some(info) = module.classes.iter().find(|c| c.name == class.name) else {
                        continue;
                    };
                    for method in &class.methods {
                        let Some(method_info) = info.methods.iter().find(|m| m.name == method.name)
                        else {
                            continue;
                        };
                        let mut checker = Checker::new(program, module.id);
                        checker.class_generics = class.generics.clone();
                        checker.self_ty = Some(Ty::Named(
                            TypeName {
                                module: module.id,
                                name: class.name.clone(),
                            },
                            class
                                .generics
                                .iter()
                                .map(|g| Ty::Generic(g.clone()))
                                .collect(),
                        ));
                        checker.ret = method_info.ret.clone();

                        checker.scopes.push(HashMap::new());
                        if method.has_self {
                            let self_ty = checker.self_ty.clone().unwrap();
                            checker.declare("self", self_ty, false, method.span);
                        }
                        for (param, (_, ty, _)) in
                            method.params.iter().zip(method_info.params.iter())
                        {
                            checker.declare(&param.name, ty.clone(), false, param.span);
                        }

                        let body_ty = checker.check_block(&method.body, Some(&method_info.ret));
                        checker.expect(
                            method.body.span,
                            &method_info.ret,
                            &body_ty,
                            " as the method result",
                        );
                        checker.scopes.pop();
                        for ((start, end), ty) in checker.types {
                            types.insert(module.id, (start, end), ty);
                        }
                        diagnostics.extend(checker.diags.into_iter().map(|d| (module.id, d)));
                    }
                }
                ast::Item::TestClass(decl) => {
                    let mut local = Vec::new();
                    let mut helpers = Vec::new();
                    for helper in &decl.helpers {
                        let params: Vec<Ty> = helper
                            .params
                            .iter()
                            .map(|param| {
                                resolve_type(program, module.id, &[], &param.ty, &mut local)
                            })
                            .collect();
                        let ret = match &helper.ret {
                            Some(ty) => resolve_type(program, module.id, &[], ty, &mut local),
                            None => Ty::Unit,
                        };
                        helpers.push((helper.name.clone(), params, ret));
                    }
                    for (helper_index, helper) in decl.helpers.iter().enumerate() {
                        let helper_ret = helpers
                            .get(helper_index)
                            .map(|(_, _, ret)| ret.clone())
                            .unwrap_or(Ty::Unit);
                        let mut checker = Checker::new(program, module.id);
                        checker.in_test = true;
                        checker.test_helpers = helpers.clone();
                        checker.ret = helper_ret.clone();
                        checker.scopes.push(HashMap::new());
                        for param in &helper.params {
                            let ty = resolve_type(program, module.id, &[], &param.ty, &mut local);
                            checker.declare(&param.name, ty, false, param.span);
                        }
                        let body_ty = checker.check_block(&helper.body, Some(&helper_ret));
                        checker.expect(
                            helper.body.span,
                            &helper_ret,
                            &body_ty,
                            " as the helper result",
                        );
                        checker.scopes.pop();
                        for ((start, end), ty) in checker.types {
                            types.insert(module.id, (start, end), ty);
                        }
                        diagnostics.extend(checker.diags.into_iter().map(|d| (module.id, d)));
                    }
                    diagnostics.extend(local.into_iter().map(|d| (module.id, d)));
                    for test in &decl.tests {
                        let mut checker = Checker::new(program, module.id);
                        checker.in_test = true;
                        checker.test_helpers = helpers.clone();
                        checker.ret = Ty::Unit;
                        checker.scopes.push(HashMap::new());
                        let body_ty = checker.check_block(&test.body, Some(&Ty::Unit));
                        checker.expect(test.body.span, &Ty::Unit, &body_ty, " as the test result");
                        checker.scopes.pop();
                        for ((start, end), ty) in checker.types {
                            types.insert(module.id, (start, end), ty);
                        }
                        diagnostics.extend(checker.diags.into_iter().map(|d| (module.id, d)));
                    }
                }
                ast::Item::Enum(_) => {}
            }
        }
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
    class_generics: Vec<String>,
    self_ty: Option<Ty>,
    ret: Ty,
    in_test: bool,
    in_lambda: bool,
    test_helpers: Vec<(String, Vec<Ty>, Ty)>,
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
            class_generics: Vec::new(),
            self_ty: None,
            ret: Ty::Unit,
            in_test: false,
            in_lambda: false,
            test_helpers: Vec::new(),
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

    /// Look up a class visible from this module.
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
        if let Some(import) = program.modules[self.module]
            .imports
            .iter()
            .find(|import| import.name == name)
        {
            if let Some(class) = program.find_class(import.module, name) {
                if class.is_pub {
                    return Some((
                        class,
                        TypeName {
                            module: import.module,
                            name: name.to_string(),
                        },
                    ));
                }
                return None;
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
        if let Some(import) = program.modules[self.module]
            .imports
            .iter()
            .find(|import| import.name == name)
        {
            if let Some(decl) = program.find_enum(import.module, name) {
                if decl.is_pub {
                    return Some((
                        decl,
                        TypeName {
                            module: import.module,
                            name: name.to_string(),
                        },
                    ));
                }
                return None;
            }
        }
        None
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
                                &self.class_generics,
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
                        None => self.diag(*span, format!("unknown variable `{name}`")),
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
                    self.diag(*span, format!("unknown variable `{name}`"));
                    Ty::Error
                }
            },
            ast::Expr::List { items, span } => self.check_list(items, *span, expected),
            ast::Expr::Call { callee, args, span } => self.check_call(callee, args, *span),
            ast::Expr::Field { base, name, span } => self.check_field(base, name, *span),
            ast::Expr::Try { value, span } => self.check_try(value, *span),
            ast::Expr::Template { exprs, .. } => {
                for inner in exprs {
                    let _ = self.check_expr(inner, None);
                }
                Ty::Str
            }
            ast::Expr::TaggedTemplate { tag, span, .. } => self.check_tagged(tag, *span),
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
            ..
        } = base
        {
            if let Some((decl, type_name)) = self.enum_ref(owner) {
                return self.enum_value(decl, type_name, name, span);
            }
            if self.class_ref(owner).is_some() {
                self.diag(
                    *owner_span,
                    format!("`{owner}.{name}` is a static member; call it with parentheses"),
                );
                return Ty::Error;
            }
        }

        let base_ty = self.check_expr(base, None);
        let base_ty = self.resolve_deep(&base_ty);
        match &base_ty {
            Ty::Named(type_name, args) => {
                let program = self.program;
                if let Some(class) = program.class_of(type_name) {
                    let subst: Subst = class
                        .generics
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect();
                    if let Some(field) = class.fields.iter().find(|field| field.name == name) {
                        return self.resolve_deep(&substitute(&field.ty, &subst));
                    }
                    self.diag(span, format!("`{}` has no field `{name}`", class.name));
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
                    format!("`{shown}` has no fields; only class values have fields"),
                );
                Ty::Error
            }
        }
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
                if let Some((class, type_name)) = self.class_ref(name) {
                    let class = class.clone();
                    return self.construct_class(&class, type_name, args, span);
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
                if let Some((_, params, ret)) = self
                    .test_helpers
                    .iter()
                    .find(|(helper, _, _)| helper == name)
                    .cloned()
                {
                    let Some(items) = self.positional(args, span, name) else {
                        return Ty::Error;
                    };
                    if items.len() != params.len() {
                        self.diag(
                            span,
                            format!(
                                "`{name}` expects {} argument(s), found {}",
                                params.len(),
                                items.len()
                            ),
                        );
                        return Ty::Error;
                    }
                    for (item, expected) in items.iter().zip(params.iter()) {
                        let actual = self.check_expr(item, Some(expected));
                        self.expect(item.span(), expected, &actual, " in this argument");
                    }
                    return ret;
                }
                if self.lookup_var(name).is_some() {
                    self.diag(span, format!("`{name}` is a value, not a function"));
                } else {
                    self.diag(span, format!("unknown function or class `{name}`"));
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
                    ..
                } = &**base
                {
                    if let Some((class, type_name)) = self.class_ref(owner) {
                        let class = class.clone();
                        return self.call_static(&class, type_name, name, args, span);
                    }
                    if let Some((decl, type_name)) = self.enum_ref(owner) {
                        let decl = decl.clone();
                        return self.construct_variant(&decl, type_name, name, args, span);
                    }
                    match owner.as_str() {
                        "Map" | "Set" if name == "empty" => {
                            let Some(items) = self.positional(args, span, name) else {
                                return Ty::Error;
                            };
                            if !items.is_empty() {
                                self.diag(span, format!("`{owner}.empty` takes no arguments"));
                                return Ty::Error;
                            }
                            return if owner == "Map" {
                                Ty::map(self.fresh(), self.fresh())
                            } else {
                                Ty::set(self.fresh())
                            };
                        }
                        "Math" => return self.call_math(name, args, span),
                        "Expect" => return self.call_expect(name, args, span),
                        "Json" => {
                            self.diag(*owner_span, "`Json` is not available in v1 yet");
                            return Ty::Error;
                        }
                        _ => {}
                    }
                    if self.lookup_var(owner).is_none() {
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

    fn construct_class(
        &mut self,
        class: &ClassInfo,
        type_name: TypeName,
        args: &ast::Args,
        span: Span,
    ) -> Ty {
        let ast::Args::Named(named) = args else {
            self.diag(
                span,
                format!(
                    "constructing `{}` requires named arguments, e.g. `{}(field: value)`",
                    class.name, class.name
                ),
            );
            return Ty::Error;
        };

        let subst: Subst = class
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

        for field in &class.fields {
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
                        field.name, class.name
                    ),
                ),
            }
        }

        for arg in named {
            if !class.fields.iter().any(|field| field.name == arg.name) {
                self.diag(
                    arg.span,
                    format!("`{}` has no field `{}`", class.name, arg.name),
                );
            }
        }

        self.instantiate_named(&type_name, &class.generics, &subst, span)
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

    fn call_static(
        &mut self,
        class: &ClassInfo,
        _type_name: TypeName,
        name: &str,
        args: &ast::Args,
        span: Span,
    ) -> Ty {
        let Some(method) = class
            .methods
            .iter()
            .find(|method| method.name == name && !method.has_self)
        else {
            if class.methods.iter().any(|method| method.name == name) {
                self.diag(
                    span,
                    format!(
                        "`{}.{name}` is an instance method; call it on a value",
                        class.name
                    ),
                );
            } else {
                self.diag(
                    span,
                    format!("`{}` has no static method `{name}`", class.name),
                );
            }
            return Ty::Error;
        };
        let method = method.clone();

        let Some(items) = self.positional(args, span, name) else {
            return Ty::Error;
        };
        if items.len() != method.params.len() {
            self.diag(
                span,
                format!(
                    "`{}.{name}` expects {} argument(s), found {}",
                    class.name,
                    method.params.len(),
                    items.len()
                ),
            );
            return Ty::Error;
        }
        for (item, (param_name, param_ty, _)) in items.iter().zip(method.params.iter()) {
            let actual = self.check_expr(item, Some(param_ty));
            self.expect(
                item.span(),
                param_ty,
                &actual,
                &format!(" for `{param_name}`"),
            );
        }
        self.resolve_deep(&method.ret)
    }

    fn call_method(&mut self, receiver: &Ty, name: &str, args: &ast::Args, span: Span) -> Ty {
        let receiver_ty = self.resolve_deep(receiver);

        if let Ty::Named(type_name, type_args) = &receiver_ty {
            let program = self.program;
            if let Some(class) = program.class_of(type_name) {
                let class = class.clone();
                let subst: Subst = class
                    .generics
                    .iter()
                    .cloned()
                    .zip(type_args.iter().cloned())
                    .collect();

                let Some(method) = class
                    .methods
                    .iter()
                    .find(|method| method.name == name && method.has_self)
                else {
                    if class.methods.iter().any(|method| method.name == name) {
                        self.diag(
                            span,
                            format!(
                                "`{}.{name}` is a static method; call it on the class",
                                class.name
                            ),
                        );
                    } else {
                        self.diag(span, format!("`{}` has no method `{name}`", class.name));
                    }
                    return Ty::Error;
                };
                let method = method.clone();

                let Some(items) = self.positional(args, span, name) else {
                    return Ty::Error;
                };
                if items.len() != method.params.len() {
                    self.diag(
                        span,
                        format!(
                            "`{}` expects {} argument(s), found {}",
                            method.name,
                            method.params.len(),
                            items.len()
                        ),
                    );
                    return Ty::Error;
                }
                for (item, (param_name, param_ty, _)) in items.iter().zip(method.params.iter()) {
                    let expected = substitute(param_ty, &subst);
                    let actual = self.check_expr(item, Some(&expected));
                    self.expect(
                        item.span(),
                        &expected,
                        &actual,
                        &format!(" for `{param_name}`"),
                    );
                }
                return self.resolve_deep(&substitute(&method.ret, &subst));
            }
            if type_name.is_builtin() {
                let shown = self.display_ty(&receiver_ty);
                self.diag(span, format!("`{shown}` has no methods"));
                return Ty::Error;
            }
        }

        self.builtin_method(&receiver_ty, name, args, span)
    }

    fn check_tagged(&mut self, tag: &ast::Expr, span: Span) -> Ty {
        let ast::Expr::Field { base, name, .. } = tag else {
            self.diag(
                span,
                "a tagged template tag must be a static method on a class, e.g. `Sql.query`",
            );
            return Ty::Error;
        };
        let ast::Expr::Ident { name: owner, .. } = base.as_ref() else {
            self.diag(
                span,
                "a tagged template tag must be a static method on a class, e.g. `Sql.query`",
            );
            return Ty::Error;
        };
        let Some((class, _)) = self.class_ref(owner) else {
            self.diag(span, format!("unknown type `{owner}` for this tag"));
            return Ty::Error;
        };
        let class = class.clone();
        let Some(method) = class
            .methods
            .iter()
            .find(|method| method.name == *name && method.is_static)
        else {
            self.diag(span, format!("`{owner}.{name}` is not a static method"));
            return Ty::Error;
        };
        let expected = [Ty::list(Ty::Str), Ty::list(Ty::Str)];
        if method.params.len() != 2 {
            self.diag(
                span,
                format!(
                    "a tagged template needs `(List<Str>, List<Str>)`, but `{owner}.{name}` takes {} parameter(s)",
                    method.params.len()
                ),
            );
            return Ty::Error;
        }
        for (index, param) in method.params.iter().enumerate() {
            self.expect(
                span,
                &expected[index],
                &param.1.clone(),
                " in the tag signature",
            );
        }
        self.resolve_deep(&method.ret)
    }

    fn check_jsx(&mut self, element: &ast::JsxElement) -> Ty {
        let Some((class, type_name)) = self.class_ref(&element.name) else {
            self.diag(
                element.span,
                format!("unknown class `{}` in JSX", element.name),
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
            self.diag(span, "`Expect` is only available inside a `test class`");
            return Ty::Error;
        }
        let Some(items) = self.positional(args, span, name) else {
            return Ty::Error;
        };
        match name {
            "equal" | "not_equal" => {
                if items.len() != 2 {
                    self.diag(span, format!("`Expect.{name}` expects 2 arguments"));
                    return Ty::Error;
                }
                let left = self.check_expr(items[0], None);
                let right = self.check_expr(items[1], Some(&left));
                self.expect(items[1].span(), &left, &right, " in this assertion");
                Ty::Unit
            }
            "true" | "false" => {
                if items.len() != 1 {
                    self.diag(span, format!("`Expect.{name}` expects 1 argument"));
                    return Ty::Error;
                }
                let ty = self.check_expr(items[0], Some(&Ty::Bool));
                self.expect(items[0].span(), &Ty::Bool, &ty, " in this assertion");
                Ty::Unit
            }
            _ => {
                self.diag(span, format!("`Expect` has no method `{name}`"));
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
