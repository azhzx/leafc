use crate::source::{SourceId, Span};

pub type AtomExprNodeId = usize;
pub type ExprNodeId = usize;
pub type DeclNodeId = usize;


#[derive(Debug, Clone)]
pub struct TypeNameString {
    pub name: String,
    pub generics: Vec<TypeNameString>,
    pub span: Span
}


#[derive(Debug, Clone)]
pub enum Binder {
    Ignore,
    Ellipsis,
    BindTo {
        name: String,
    },
    Unpack {
        unpack: Unpack
    }
}

#[derive(Debug, Clone)]
pub enum Unpack {
    Tuple {
        tuple: Vec<Binder>
    },
    ADT {
        type_name: String,
        binder: Vec<Binder>,
    },
    Struct {
        type_name: String,
        binder: Vec<Binder>,
    }
}

#[derive(Debug, Clone)]
pub enum CaseMode {
    Literal {
        lit: AtomExprNodeId,
        span: Span,
    },
    Guard {
        binding_name: String,
        expr: ExprNodeId,
        span: Span,
    },
    Unpack {
        unpack: Unpack,
    }

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
    Complex {
        int: AtomExprNodeId,
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
pub enum ExprNode {
    Atom {
        expr: AtomExprNodeId,
        span: Span,
    },
    Binary {
        left: ExprNodeId,
        right: ExprNodeId,
        op: Operator,
        span: Span,
    },
    Unary {
        op: Operator,
        right: ExprNodeId,
        span: Span,
    },
    Move {
        target: ExprNodeId,
        span: Span,
    },
    Copy {
        target: ExprNodeId,
        span: Span,
    },
    Ref {
        target: ExprNodeId,
        span: Span,
    },
    MutRef {
        target: ExprNodeId,
        span: Span,
    },
    Share {
        target: ExprNodeId,
        span: Span,
    },
    Call {
        callee: ExprNodeId,
        args: Vec<ExprNodeId>,
        span: Span,
    },
    UnsafeExternalCall {
        callee: ExprNodeId,
        args: Vec<ExprNodeId>,
        span: Span,
    },
    Member {
        left: ExprNodeId,
        right: String,
        span: Span,
    },
    TypeCast {
        expr: ExprNodeId,
        into_type_str: TypeNameString,
        span: Span,
    },
    Do {
        exprs: Vec<ExprNodeId>,
        span: Span,
    },
    Let {
        name: String,
        expr: ExprNodeId,
        span: Span,
        type_str: TypeNameString,
        mutable: bool,
    },
    If {
        cond: ExprNodeId,
        then_expr: ExprNodeId,
        elifs: Vec<(ExprNodeId, ExprNodeId)>,
        else_expr: Option<ExprNodeId>,
        span: Span,
    },

    /// parse for match is delay
    Match {
        expr: ExprNodeId,
        cases: Vec<(CaseMode, ExprNodeId)>,
        default_case: ExprNodeId,
        span: Span,
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
pub enum DeclNode {
    Fun {
        name: String,
        params: Vec<Param>,
        return_type_str: TypeNameString,
        block: Vec<ExprNodeId>,
        visibility: Visibility,
        span: Span,
    },
    FunDecl {
        name: String,
        params: Vec<Param>,
        return_type_str: TypeNameString,
        visibility: Visibility,
        span: Span,
    },
    Abstract {
        name: String,
        has_abst: Vec<String>,
        generic_vars: Vec<GenericVar>,
        visibility: Visibility,
        methods: Vec<MethodDecl>,
        span: Span,
    },
    TypeStruct {
        name: String,
        fields: Vec<Field>,
        has_abst: Vec<String>,
        generic_vars: Vec<GenericVar>,
        visibility: Visibility,
        span: Span,
    },
    TypeAlias {
        name: String,
        ref_to: TypeNameString,
        has_abst: Vec<String>,
        generic_vars: Vec<GenericVar>,
        visibility: Visibility,
        span: Span,
    },
    ADT {
        name: String,
        has_abst: Vec<String>,
        generic_vars: Vec<GenericVar>,
        ctors: Vec<Ctor>,
        visibility: Visibility,
        span: Span,
    },
    CType {
        name: String,
        visibility: Visibility,
        span: Span,
    },
    External {
        name: String,
        sym_name: String,
        params: Vec<Param>,
        return_type_str: TypeNameString,
        visibility: Visibility,
        span: Span,
    }
}


#[derive(Debug, Clone)]
pub enum Visibility {
    Private,
    Public,
    PublicExternal
}


#[derive(Debug, Clone)]
pub struct FileAst {
    pub file: SourceId,
    pub atom_expr_pool: Vec<AtomExprNode>,
    pub expr_pool: Vec<ExprNode>,
    pub decl_pool: Vec<DeclNode>,
    pub type_name_pool: Vec<TypeNameString>,
}