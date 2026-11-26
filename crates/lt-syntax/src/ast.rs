//! Abstract syntax tree for Lingua Tungga.
//!
//! The AST is deliberately close to the source: sugar such as `T?`,
//! `${...}` and JSX is preserved here and only lowered later in `lt-hir`.

use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub imports: Vec<ImportDecl>,
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub name: String,
    /// Raw path as written; `.lt` files and `https://` URLs alike.
    pub path: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Class(ClassDecl),
    Enum(EnumDecl),
    TestClass(TestClassDecl),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Item::Class(c) => c.span,
            Item::Enum(e) => e.span,
            Item::TestClass(t) => t.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub is_pub: bool,
    pub name: String,
    pub generics: Vec<String>,
    pub fields: Vec<Field>,
    pub methods: Vec<Method>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub is_pub: bool,
    pub is_static: bool,
    pub has_self: bool,
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestClassDecl {
    pub name: String,
    pub tests: Vec<TestFn>,
    pub helpers: Vec<Method>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestFn {
    pub name: String,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub is_pub: bool,
    pub name: String,
    pub generics: Vec<String>,
    pub variants: Vec<Variant>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: String,
    pub payload: Vec<Type>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Type {
    pub path: Vec<String>,
    pub args: Vec<Type>,
    /// `T?` sugar for `Option<T>`.
    pub optional: bool,
    pub span: Span,
}

impl Type {
    pub fn name(&self) -> String {
        self.path.join(".")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        mutable: bool,
        name: String,
        ty: Option<Type>,
        value: Expr,
        span: Span,
    },
    Assign {
        name: String,
        value: Expr,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    For {
        var: String,
        iter: Expr,
        body: Block,
        span: Span,
    },
    While {
        cond: Expr,
        body: Block,
        span: Span,
    },
    Expr(Expr),
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let { span, .. }
            | Stmt::Assign { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::For { span, .. }
            | Stmt::While { span, .. } => *span,
            Stmt::Expr(e) => e.span(),
        }
    }
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

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int {
        raw: String,
        span: Span,
    },
    Float {
        raw: String,
        span: Span,
    },
    Str {
        value: String,
        span: Span,
    },
    Bool {
        value: bool,
        span: Span,
    },
    NoneLit {
        span: Span,
    },
    Ident {
        name: String,
        span: Span,
    },
    List {
        items: Vec<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Args,
        span: Span,
    },
    Field {
        base: Box<Expr>,
        name: String,
        span: Span,
    },
    Try {
        value: Box<Expr>,
        span: Span,
    },
    Template {
        /// `parts.len() == exprs.len() + 1`
        parts: Vec<String>,
        exprs: Vec<Expr>,
        span: Span,
    },
    TaggedTemplate {
        tag: Box<Expr>,
        parts: Vec<String>,
        exprs: Vec<Expr>,
        span: Span,
    },
    Jsx(JsxElement),
    Lambda {
        params: Vec<String>,
        body: LambdaBody,
        span: Span,
    },
    If {
        cond: Box<Expr>,
        then_block: Block,
        else_branch: Option<ElseBranch>,
        span: Span,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Unary {
        op: UnOp,
        value: Box<Expr>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Int { span, .. }
            | Expr::Float { span, .. }
            | Expr::Str { span, .. }
            | Expr::Bool { span, .. }
            | Expr::NoneLit { span }
            | Expr::Ident { span, .. }
            | Expr::List { span, .. }
            | Expr::Call { span, .. }
            | Expr::Field { span, .. }
            | Expr::Try { span, .. }
            | Expr::Template { span, .. }
            | Expr::TaggedTemplate { span, .. }
            | Expr::Lambda { span, .. }
            | Expr::If { span, .. }
            | Expr::Match { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Unary { span, .. } => *span,
            Expr::Jsx(el) => el.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LambdaBody {
    Expr(Box<Expr>),
    Block(Block),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ElseBranch {
    If(Box<Expr>),
    Block(Block),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Args {
    Empty,
    Positional(Vec<Expr>),
    Named(Vec<NamedArg>),
}

impl Args {
    pub fn len(&self) -> usize {
        match self {
            Args::Empty => 0,
            Args::Positional(a) => a.len(),
            Args::Named(a) => a.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamedArg {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: MatchBody,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard {
        span: Span,
    },
    NoneLit {
        span: Span,
    },
    Variant {
        path: Vec<String>,
        args: Vec<Pattern>,
        span: Span,
    },
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Wildcard { span } | Pattern::NoneLit { span } => *span,
            Pattern::Variant { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchBody {
    Expr(Expr),
    Block(Block),
}

#[derive(Debug, Clone, PartialEq)]
pub struct JsxElement {
    pub name: String,
    pub attrs: Vec<JsxAttr>,
    pub children: Vec<JsxChild>,
    pub self_closing: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JsxAttr {
    pub name: String,
    pub value: JsxAttrValue,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsxAttrValue {
    Str(String),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsxChild {
    Element(JsxElement),
    Text(String),
    Expr(Expr),
}
