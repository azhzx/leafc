use crate::source::{SourceId, Span};
use crate::symbol::TyId;

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
        concrete_type: TyId,
    },
    Int {
        int: String,
        span: Span,
        concrete_type: TyId,
    },
    Str {
        string: String,
        span: Span,
        concrete_type: TyId,
    },
    Name {
        name: String,
        span: Span,
        concrete_type: TyId,
    },
    Complex {
        int: AtomExprNodeId,
        span: Span,
        concrete_type: TyId,
    },
    Tuple {
        exprs: Vec<ExprNodeId>,
        span: Span,
        concrete_type: TyId,
    },
    Ellipsis {
        span: Span,
        concrete_type: TyId,
    }
}


#[derive(Debug, Clone)]
pub enum ExprNode {
    Atom {
        expr: AtomExprNode,
        span: Span,
        concrete_type: TyId,
    },
    Binary {
        left: ExprNodeId,
        right: ExprNodeId,
        op: String,
        span: Span,
        concrete_type: TyId,
    },
    Unary {
        op: String,
        right: ExprNodeId,
        span: Span,
        concrete_type: TyId,
    },
    Move {
        target: ExprNodeId,
        span: Span,
        concrete_type: TyId,
    },
    Copy {
        target: ExprNodeId,
        span: Span,
        concrete_type: TyId,
    },
    Ref {
        target: ExprNodeId,
        span: Span,
        concrete_type: TyId,
    },
    MutRef {
        target: ExprNodeId,
        span: Span,
        concrete_type: TyId,
    },
    Share {
        target: ExprNodeId,
        span: Span,
        concrete_type: TyId,
    },
    Call {
        callee: ExprNodeId,
        args: Vec<ExprNodeId>,
        span: Span,
        concrete_type: TyId,
    },
    UnsafeExternalCall {
        callee: ExprNodeId,
        args: Vec<ExprNodeId>,
        span: Span,
        concrete_type: TyId,
    },
    Member {
        left: ExprNodeId,
        right: ExprNodeId,
        span: Span,
        concrete_type: TyId,
    },
    TypeCast {
        expr: ExprNodeId,
        into_type_str: TypeNameString,
        span: Span,
        concrete_type: TyId,
    },
    Do {
        muted: Vec<String>,
        expr: Vec<ExprNodeId>,
        span: Span,
        concrete_type: TyId,
    },
    Let {
        name: String,
        expr: ExprNodeId,
        span: Span,
        concrete_type: TyId,
    },
    LetMut {
        name: String,
        expr: ExprNodeId,
        span: Span,
        concrete_type: TyId,
    },
    If {
        cond: ExprNodeId,
        then_expr: ExprNodeId,
        elifs: Vec<(ExprNodeId, ExprNodeId)>,
        else_expr: ExprNodeId,
        span: Span,
        concrete_type: TyId,
    },
    Match {
        expr: ExprNodeId,
        cases: Vec<(CaseMode, ExprNodeId)>,
        default_case: ExprNodeId,
        span: Span,
        concrete_type: TyId,
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
    name: String,
    type_str: TypeNameString,
    span: Span,
}

#[derive(Debug, Clone)]
pub struct GenericVar {
    pub name: String,
    pub constraint: Vec<TypeNameString>,
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
        methods: Vec<DeclNodeId>,
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
    ADT {
        name: String,
        has_abst: Vec<String>,
        generic_vars: Vec<GenericVar>,
        ctors: Vec<DeclNode>,
        visibility: Visibility,
        span: Span,
    },
    Ctor {
        name: String,
        generic_vars: Vec<GenericVar>,
        return_type_str: TypeNameString,
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