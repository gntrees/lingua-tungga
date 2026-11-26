//! Semantic types used by the checker.

use std::collections::HashMap;
use std::fmt;

/// Module id reserved for types the compiler knows without a source file.
pub const BUILTIN: usize = usize::MAX;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeName {
    pub module: usize,
    pub name: String,
}

impl TypeName {
    pub fn builtin(name: &str) -> Self {
        Self {
            module: BUILTIN,
            name: name.to_string(),
        }
    }

    pub fn is_builtin(&self) -> bool {
        self.module == BUILTIN
    }
}

impl fmt::Display for TypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    Int,
    Float,
    Bool,
    Str,
    Unit,
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Set(Box<Ty>),
    Option(Box<Ty>),
    Result(Box<Ty>, Box<Ty>),
    /// Nominal class or enum type, with type arguments.
    Named(TypeName, Vec<Ty>),
    /// A type parameter, only valid inside a declaration.
    Generic(String),
    /// An inference variable, only valid while checking a body.
    Var(u32),
    /// Poison value: an error was already reported, do not cascade.
    Error,
}

impl Ty {
    pub fn list(inner: Ty) -> Ty {
        Ty::List(Box::new(inner))
    }

    pub fn set(inner: Ty) -> Ty {
        Ty::Set(Box::new(inner))
    }

    pub fn option(inner: Ty) -> Ty {
        Ty::Option(Box::new(inner))
    }

    pub fn result(ok: Ty, err: Ty) -> Ty {
        Ty::Result(Box::new(ok), Box::new(err))
    }

    pub fn map(key: Ty, value: Ty) -> Ty {
        Ty::Map(Box::new(key), Box::new(value))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    pub fn is_var(&self) -> bool {
        matches!(self, Ty::Var(_))
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => f.write_str("Int"),
            Ty::Float => f.write_str("Float"),
            Ty::Bool => f.write_str("Bool"),
            Ty::Str => f.write_str("Str"),
            Ty::Unit => f.write_str("Unit"),
            Ty::List(inner) => write!(f, "List<{inner}>"),
            Ty::Map(key, value) => write!(f, "Map<{key}, {value}>"),
            Ty::Set(inner) => write!(f, "Set<{inner}>"),
            Ty::Option(inner) => write!(f, "Option<{inner}>"),
            Ty::Result(ok, err) => write!(f, "Result<{ok}, {err}>"),
            Ty::Named(name, args) => {
                if args.is_empty() {
                    write!(f, "{name}")
                } else {
                    let rendered: Vec<String> = args.iter().map(Ty::to_string).collect();
                    write!(f, "{}<{}>", name, rendered.join(", "))
                }
            }
            Ty::Generic(name) => f.write_str(name),
            Ty::Var(_) => f.write_str("?"),
            Ty::Error => f.write_str("<error>"),
        }
    }
}

pub type Subst = HashMap<String, Ty>;

pub fn substitute(ty: &Ty, subst: &Subst) -> Ty {
    match ty {
        Ty::Generic(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Ty::List(inner) => Ty::list(substitute(inner, subst)),
        Ty::Set(inner) => Ty::set(substitute(inner, subst)),
        Ty::Option(inner) => Ty::option(substitute(inner, subst)),
        Ty::Map(key, value) => Ty::map(substitute(key, subst), substitute(value, subst)),
        Ty::Result(ok, err) => Ty::result(substitute(ok, subst), substitute(err, subst)),
        Ty::Named(name, args) => Ty::Named(
            name.clone(),
            args.iter().map(|arg| substitute(arg, subst)).collect(),
        ),
        _ => ty.clone(),
    }
}

/// Does this type mention any of the given generic parameter names?
pub fn mentions_any_generic(ty: &Ty, generics: &[String]) -> bool {
    match ty {
        Ty::Generic(name) => generics.iter().any(|g| g == name),
        Ty::List(inner) | Ty::Set(inner) | Ty::Option(inner) => {
            mentions_any_generic(inner, generics)
        }
        Ty::Map(key, value) | Ty::Result(key, value) => {
            mentions_any_generic(key, generics) || mentions_any_generic(value, generics)
        }
        Ty::Named(_, args) => args.iter().any(|arg| mentions_any_generic(arg, generics)),
        _ => false,
    }
}
