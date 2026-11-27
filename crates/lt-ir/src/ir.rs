//! Target-neutral typed IR.
//!
//! Produced from a type-checked [`lt_hir::Program`]. Every expression carries
//! its final type; resolution decisions (which struct, which function, which
//! enum variant) are already made, so an emitter only has to render.

use lt_hir::{ModuleId, Ty, TypeName};

#[derive(Debug, Clone)]
pub struct IrProgram {
    pub modules: Vec<IrModule>,
    /// Module id -> output-relative path without extension (e.g. `models/user`).
    pub names: Vec<String>,
}

/// One package.
#[derive(Debug, Clone)]
pub struct IrModule {
    pub id: ModuleId,
    /// Package name; also the output module name.
    pub name: String,
    pub structs: Vec<IrStruct>,
    pub enums: Vec<IrEnum>,
    pub functions: Vec<IrFn>,
    pub bindings: Vec<IrBinding>,
    pub tests: Vec<IrTest>,
}

#[derive(Debug, Clone)]
pub struct IrStruct {
    pub name: String,
    pub generics: Vec<String>,
    pub is_export: bool,
    pub fields: Vec<IrField>,
}

#[derive(Debug, Clone)]
pub struct IrField {
    pub name: String,
    pub ty: Ty,
}

/// A package-level function. Receiver functions are ordinary functions whose
/// first parameter is a struct/enum declared in the same package.
#[derive(Debug, Clone)]
pub struct IrFn {
    pub name: String,
    pub is_export: bool,
    pub generics: Vec<String>,
    pub params: Vec<IrParam>,
    pub ret: Ty,
    pub body: IrBlock,
    /// True when the first parameter is the receiver type.
    pub is_receiver: bool,
}

#[derive(Debug, Clone)]
pub struct IrParam {
    pub name: String,
    pub ty: Ty,
}

#[derive(Debug, Clone)]
pub struct IrEnum {
    pub name: String,
    pub generics: Vec<String>,
    pub is_export: bool,
    pub variants: Vec<IrVariant>,
}

#[derive(Debug, Clone)]
pub struct IrVariant {
    pub name: String,
    pub payload: Vec<Ty>,
}

/// Immutable package-level binding.
#[derive(Debug, Clone)]
pub struct IrBinding {
    pub name: String,
    pub is_export: bool,
    pub ty: Ty,
    pub value: IrExpr,
}

#[derive(Debug, Clone)]
pub struct IrTest {
    pub title: String,
    pub body: IrBlock,
}

#[derive(Debug, Clone)]
pub struct IrBlock {
    pub stmts: Vec<IrStmt>,
    pub ty: Ty,
}

#[derive(Debug, Clone)]
pub enum IrStmt {
    Let {
        name: String,
        ty: Ty,
        mutable: bool,
        value: IrExpr,
    },
    Assign {
        name: String,
        value: IrExpr,
    },
    Return(Option<IrExpr>),
    For {
        var: String,
        ty: Ty,
        iter: IrExpr,
        body: IrBlock,
    },
    While {
        cond: IrExpr,
        body: IrBlock,
    },
    Expr(IrExpr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
}

#[derive(Debug, Clone)]
pub enum IrExpr {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    List {
        items: Vec<IrExpr>,
        ty: Ty,
    },
    /// `map.empty()` / `set.empty()`.
    EmptyCollection {
        ty: Ty,
    },
    Var(String),
    /// A package-level binding, possibly in another package.
    BindingRef {
        module: ModuleId,
        name: String,
        ty: Ty,
    },
    Field {
        base: Box<IrExpr>,
        name: String,
        ty: Ty,
    },
    StructNew {
        module: ModuleId,
        strukt: String,
        /// Values in field declaration order.
        args: Vec<(String, IrExpr)>,
        ty: Ty,
    },
    NewVariant {
        target: VariantRef,
        variant: String,
        args: Vec<IrExpr>,
        ty: Ty,
    },
    /// A package-level function call, including receiver calls: the receiver
    /// is the first argument.
    FnCall {
        module: ModuleId,
        name: String,
        args: Vec<IrExpr>,
        ty: Ty,
    },
    Builtin {
        kind: Builtin,
        receiver: Box<IrExpr>,
        /// Type of the receiver, needed for explicit type arguments in Go.
        recv_ty: Ty,
        args: Vec<IrExpr>,
        ty: Ty,
    },
    Math {
        name: String,
        args: Vec<IrExpr>,
        ty: Ty,
    },
    Expect {
        name: String,
        args: Vec<IrExpr>,
    },
    /// Write a value to standard output using its `Display` implementation.
    ///
    /// `lower` never produces this; it exists so the playground can synthesize
    /// a runnable "how to call this from the target language" program.
    Print {
        value: Box<IrExpr>,
    },
    /// Render a value with its `Display` implementation (`to_string()`).
    ToStr {
        value: Box<IrExpr>,
        ty: Ty,
    },
    Template {
        parts: Vec<String>,
        exprs: Vec<IrExpr>,
    },
    Try {
        value: Box<IrExpr>,
        /// Type of `value` (`Option<_>` or `Result<_, _>`).
        source_ty: Ty,
        ty: Ty,
    },
    If {
        cond: Box<IrExpr>,
        then_block: IrBlock,
        else_branch: Option<IrElse>,
        ty: Ty,
    },
    Match {
        scrutinee: Box<IrExpr>,
        arms: Vec<IrArm>,
        ty: Ty,
    },
    Bin {
        op: BinOp,
        lhs: Box<IrExpr>,
        rhs: Box<IrExpr>,
        ty: Ty,
    },
    Un {
        op: UnOp,
        value: Box<IrExpr>,
        ty: Ty,
    },
    Lambda {
        params: Vec<IrParam>,
        ret: Ty,
        body: IrLambdaBody,
    },
}

#[derive(Debug, Clone)]
pub enum IrElse {
    Block(IrBlock),
    If(Box<IrExpr>),
}

#[derive(Debug, Clone)]
pub enum IrLambdaBody {
    Expr(Box<IrExpr>),
    Block(IrBlock),
}

#[derive(Debug, Clone)]
pub enum VariantRef {
    Enum { module: ModuleId, name: String },
    Option,
    Result,
}

impl VariantRef {
    pub fn type_name(&self) -> String {
        match self {
            VariantRef::Enum { name, .. } => format!("{name}.{name}"),
            VariantRef::Option => "Option".to_string(),
            VariantRef::Result => "Result".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IrArm {
    pub pattern: IrPattern,
    pub body: IrBlock,
}

#[derive(Debug, Clone)]
pub enum IrPattern {
    Wildcard,
    /// Binds the whole scrutinee (a catch-all).
    Binding {
        name: String,
        ty: Ty,
    },
    Variant {
        variant: String,
        bindings: Vec<(String, Ty)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    StrLen,
    StrTrim,
    StrTrimIndent,
    StrLower,
    StrUpper,
    StrSplit,
    StrStartsWith,
    StrEndsWith,
    StrContains,
    StrReplace,
    StrSlice,
    StrChars,
    StrParseInt,
    StrParseFloat,
    ListLen,
    ListGet,
    ListPush,
    ListPop,
    ListMap,
    ListFilter,
    ListFold,
    ListAny,
    ListAll,
    ListSort,
    ListReverse,
    ListConcat,
    ListContains,
    ListIndexOf,
    ListJoin,
    SetLen,
    SetInsert,
    SetRemove,
    SetContains,
    SetUnion,
    SetIntersect,
    SetDifference,
    MapLen,
    MapGet,
    MapInsert,
    MapRemove,
    MapKeys,
    MapValues,
    MapContainsKey,
    OptionIsSome,
    OptionIsNone,
    OptionUnwrap,
    OptionUnwrapOr,
    OptionMap,
    OptionAndThen,
    ResultIsOk,
    ResultIsErr,
    ResultUnwrap,
    ResultUnwrapOr,
    ResultMap,
    ResultMapErr,
    ResultAndThen,
}

impl Builtin {
    pub fn from_method(receiver: &Ty, method: &str) -> Option<Builtin> {
        use Builtin::*;
        Some(match (receiver, method) {
            (Ty::Str, "len") => StrLen,
            (Ty::Str, "trim") => StrTrim,
            (Ty::Str, "trim_indent") => StrTrimIndent,
            (Ty::Str, "lower") => StrLower,
            (Ty::Str, "upper") => StrUpper,
            (Ty::Str, "split") => StrSplit,
            (Ty::Str, "starts_with") => StrStartsWith,
            (Ty::Str, "ends_with") => StrEndsWith,
            (Ty::Str, "contains") => StrContains,
            (Ty::Str, "replace") => StrReplace,
            (Ty::Str, "slice") => StrSlice,
            (Ty::Str, "chars") => StrChars,
            (Ty::Str, "parse_int") => StrParseInt,
            (Ty::Str, "parse_float") => StrParseFloat,
            (Ty::List(_), "len") => ListLen,
            (Ty::List(_), "get") => ListGet,
            (Ty::List(_), "push") => ListPush,
            (Ty::List(_), "pop") => ListPop,
            (Ty::List(_), "map") => ListMap,
            (Ty::List(_), "filter") => ListFilter,
            (Ty::List(_), "fold") => ListFold,
            (Ty::List(_), "any") => ListAny,
            (Ty::List(_), "all") => ListAll,
            (Ty::List(_), "sort") => ListSort,
            (Ty::List(_), "reverse") => ListReverse,
            (Ty::List(_), "concat") => ListConcat,
            (Ty::List(_), "contains") => ListContains,
            (Ty::List(_), "index_of") => ListIndexOf,
            (Ty::List(_), "join") => ListJoin,
            (Ty::Set(_), "len") => SetLen,
            (Ty::Set(_), "insert") => SetInsert,
            (Ty::Set(_), "remove") => SetRemove,
            (Ty::Set(_), "contains") => SetContains,
            (Ty::Set(_), "union") => SetUnion,
            (Ty::Set(_), "intersect") => SetIntersect,
            (Ty::Set(_), "difference") => SetDifference,
            (Ty::Map(_, _), "len") => MapLen,
            (Ty::Map(_, _), "get") => MapGet,
            (Ty::Map(_, _), "insert") => MapInsert,
            (Ty::Map(_, _), "remove") => MapRemove,
            (Ty::Map(_, _), "keys") => MapKeys,
            (Ty::Map(_, _), "values") => MapValues,
            (Ty::Map(_, _), "contains_key") => MapContainsKey,
            (Ty::Option(_), "is_some") => OptionIsSome,
            (Ty::Option(_), "is_none") => OptionIsNone,
            (Ty::Option(_), "unwrap") => OptionUnwrap,
            (Ty::Option(_), "unwrap_or") => OptionUnwrapOr,
            (Ty::Option(_), "map") => OptionMap,
            (Ty::Option(_), "and_then") => OptionAndThen,
            (Ty::Result(_, _), "is_ok") => ResultIsOk,
            (Ty::Result(_, _), "is_err") => ResultIsErr,
            (Ty::Result(_, _), "unwrap") => ResultUnwrap,
            (Ty::Result(_, _), "unwrap_or") => ResultUnwrapOr,
            (Ty::Result(_, _), "map") => ResultMap,
            (Ty::Result(_, _), "map_err") => ResultMapErr,
            (Ty::Result(_, _), "and_then") => ResultAndThen,
            _ => return None,
        })
    }

    /// How many arguments are lambdas, and which parameter types they take.
    pub fn lambda_params(&self, receiver: &Ty, first_arg_ty: Option<&Ty>) -> Option<Vec<Ty>> {
        use Builtin::*;
        let (ok, err, inner) = match receiver {
            Ty::List(inner) | Ty::Set(inner) => ((**inner).clone(), Ty::Error, (**inner).clone()),
            Ty::Option(inner) => ((**inner).clone(), Ty::Error, (**inner).clone()),
            Ty::Result(ok, err) => ((**ok).clone(), (**err).clone(), (**ok).clone()),
            _ => return None,
        };
        Some(match self {
            ListMap | ListFilter | ListAny | ListAll => vec![inner],
            ListFold => vec![first_arg_ty.cloned().unwrap_or(Ty::Error), inner],
            OptionMap | OptionAndThen => vec![ok],
            ResultMap | ResultAndThen => vec![ok],
            ResultMapErr => vec![err],
            _ => return None,
        })
    }
}

pub fn type_name_of(ty: &Ty) -> Option<&TypeName> {
    match ty {
        Ty::Named(name, _) => Some(name),
        _ => None,
    }
}
