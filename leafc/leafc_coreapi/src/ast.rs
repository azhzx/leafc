use std::sync::Arc;
use crate::source::{Span};


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TypeNameString {
    pub name: String,
    pub generics: Vec<TypeNameString>,
    pub span: Span
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum AtomExprNode {
    Decimal {
        dec: String,
    },
    Int {
        int: String,
    },
    Str {
        string: String,
    },
    Name {
        name: String,
    },
    Tuple {
        exprs: Vec<ExprRedNode>,
    },
    Ellipsis
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Operator {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Not,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    UserOp(String)
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ExprRedNode {
    pub span: Span,
    pub inner: Arc<ExprNode>,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ExprNode {
    pub kind: ExprNodeKind,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ElseIf {
    pub cond: ExprRedNode,
    pub body: ExprRedNode
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ExprNodeKind {
    Atom {
        expr: AtomExprNode,
    },
    Binary {
        left: ExprRedNode,
        right: ExprRedNode,
        op: Operator,
    },
    Unary {
        op: Operator,
        right: ExprRedNode,
    },
    Move {
        target: ExprRedNode,
    },
    Copy {
        target: ExprRedNode,
    },
    Ref {
        target: ExprRedNode,
    },
    MutRef {
        target: ExprRedNode,
    },
    Share {
        target: ExprRedNode,
    },
    Call {
        callee: ExprRedNode,
        args: Vec<ExprRedNode>,
    },
    UnsafeExternalCall {
        callee: ExprRedNode,
        args: Vec<ExprRedNode>,
    },
    Member {
        left: ExprRedNode,
        right: String,
    },
    TypeCast {
        expr: ExprRedNode,
        into_type: ExprRedNode,
    },
    Do {
        exprs: Vec<ExprRedNode>,
    },
    Let {
        name: String,
        expr: ExprRedNode,
        type_str: TypeNameString,
        mutable: bool,
    },
    If {
        cond: ExprRedNode,
        then_expr: ExprRedNode,
        elifs: Vec<ElseIf>,
        else_expr: Option<ExprRedNode>,
    },
    Return {
        expr: Option<ExprRedNode>,
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub type_str: TypeNameString,
    pub span: Span,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub type_str: TypeNameString,
    pub span: Span,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GenericVar {
    pub name: String,
    pub constraint: Vec<TypeNameString>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Ctor {
    pub name: String,
    pub generic_vars: Vec<GenericVar>,
    pub from_type_str: TypeNameString,
    pub return_type_str: TypeNameString,
    pub visibility: Visibility,
    pub span: Span,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct MethodDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type_str: TypeNameString,
    pub visibility: Visibility,
    pub span: Span,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct AnnotationDecl {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct DeclRedNode {
    pub span: Span,
    pub inner: Arc<DeclNode>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct DeclNode {
    pub name: String,
    pub visibility: Visibility,
    pub kind: DeclNodeKind,
    pub annotations: Vec<AnnotationDecl>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum DeclNodeKind {
    Fun {
        params: Vec<Param>,
        return_type_str: TypeNameString,
        block: Vec<ExprRedNode>,
    },
    FunDecl {
        params: Vec<Param>,
        return_type_str: TypeNameString,
    },
    Abstract {
        has_abst: Vec<String>,
        generic_vars: Vec<GenericVar>,
        methods: Vec<MethodDecl>,
    },
    TypeStruct {
        fields: Vec<Field>,
        has_abst: Vec<String>,
        generic_vars: Vec<GenericVar>,
    },
    TypeAlias {
        ref_to: TypeNameString,
        has_abst: Vec<String>,
        generic_vars: Vec<GenericVar>,
    },
    TypeDecl,
    ADT {
        has_abst: Vec<String>,
        generic_vars: Vec<GenericVar>,
        ctors: Vec<Ctor>,
    },
    CType,
    External {
        sym_name: String,
        params: Vec<Param>,
        return_type_str: TypeNameString,
    }
}


#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Visibility {
    Private,
    Public,
    PublicExternal
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Require {
    pub path: Vec<String>,
    pub only: Vec<String>,
    pub is_open: bool, // 将被导入模块的顶层声明塞入当前模块的中(不递归展开)
    pub span: Span,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FileRedUnit {
    pub span: Span,
    pub name: String,
    pub top_decls: Vec<DeclRedNode>,
    pub file_unit_requires: Vec<Require>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CrateAst {
    pub external_requires: Vec<Require>,
    pub file_units: Vec<FileRedUnit>,
}