use crate::source::{Span};

pub type ExprNodeId = usize;
pub type DeclNodeId = usize;


#[derive(Debug, Clone)]
pub struct TypeNameString {
    pub name: String,
    pub generics: Vec<TypeNameString>,
    pub span: Span
}


#[derive(Debug, Clone)]
pub enum AtomExprNode {
    Decimal {
        dec: String,
        span: Span,
    },
    Int {
        int: String,
        span: Span,
    },
    Str {
        string: String,
        span: Span,
    },
    Name {
        name: String,
        span: Span,
    },
    Tuple {
        exprs: Vec<ExprNodeId>,
        span: Span,
    },
    Ellipsis {
        span: Span,
    }
}

#[derive(Debug, Clone)]
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
}


#[derive(Debug, Clone)]
pub struct ExprNode {
    pub span: Span,
    pub kind: ExprNodeKind,
}

#[derive(Debug, Clone)]
pub struct ElseIf {
    pub cond: ExprNodeId,
    pub body: ExprNodeId
}

#[derive(Debug, Clone)]
pub enum ExprNodeKind {
    Atom {
        expr: AtomExprNode,
    },
    Binary {
        left: ExprNodeId,
        right: ExprNodeId,
        op: Operator,
    },
    Unary {
        op: Operator,
        right: ExprNodeId,
    },
    Move {
        target: ExprNodeId,
    },
    Copy {
        target: ExprNodeId,
    },
    Ref {
        target: ExprNodeId,
    },
    MutRef {
        target: ExprNodeId,
    },
    Share {
        target: ExprNodeId,
    },
    Call {
        callee: ExprNodeId,
        args: Vec<ExprNodeId>,
    },
    UnsafeExternalCall {
        callee: ExprNodeId,
        args: Vec<ExprNodeId>,
    },
    Member {
        left: ExprNodeId,
        right: String,
    },
    TypeCast {
        expr: ExprNodeId,
        into_type_str: TypeNameString,
    },
    Do {
        exprs: Vec<ExprNodeId>,
    },
    Let {
        name: String,
        expr: ExprNodeId,
        type_str: TypeNameString,
        mutable: bool,
    },
    If {
        cond: ExprNodeId,
        then_expr: ExprNodeId,
        elifs: Vec<ElseIf>,
        else_expr: Option<ExprNodeId>,
    },
    Return {
        expr: Option<ExprNodeId>,
    }
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub type_str: TypeNameString,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub type_str: TypeNameString,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct GenericVar {
    pub name: String,
    pub constraint: Vec<TypeNameString>,
}

#[derive(Debug, Clone)]
pub struct Ctor {
    pub name: String,
    pub generic_vars: Vec<GenericVar>,
    pub from_type_str: TypeNameString,
    pub return_type_str: TypeNameString,
    pub visibility: Visibility,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type_str: TypeNameString,
    pub visibility: Visibility,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct AnnotationDecl {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DeclNode {
    pub name: String,
    pub visibility: Visibility,
    pub span: Span,
    pub kind: DeclNodeKind,
    pub annotations: Vec<AnnotationDecl>,
}

#[derive(Debug, Clone)]
pub enum DeclNodeKind {
    Fun {
        params: Vec<Param>,
        return_type_str: TypeNameString,
        block: Vec<ExprNodeId>,
    },
    FileUnit {  // file module
        top_decls: Vec<DeclNodeId>,
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


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Public,
    PublicExternal
}

#[derive(Debug, Clone)]
pub struct Require {
    pub path: Vec<String>,
    pub only: Vec<String>,
    pub is_open: bool, // 将被导入模块的顶层声明塞入当前模块的中(不递归展开)
    pub span: Span,
}


#[derive(Debug, Clone)]
pub struct CrateAst {
    pub external_requires: Vec<Require>,
    pub expr_pool: Vec<ExprNode>,
    pub decl_pool: Vec<DeclNode>,
}