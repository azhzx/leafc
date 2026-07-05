use crate::source::{SourceId, Span};
use crate::symbol::TyId;

pub type AtomExprNodeId = usize;
pub type ExprNodeId = usize;
pub type DeclNodeId = usize;

pub type TypeNameId = usize;

pub struct TypeNameString {
    name: String,
    generics: Option<Vec<TypeNameId>>,
    span: Span
}

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

pub enum Unpack {
    Tuple {
        tuple: Vec<Binder>
    },
    ADT {
        type_name: TypeNameId,
        binder: Vec<Binder>,
    },
    Struct {
        type_name: TypeNameId,
        binder: Vec<Binder>,
    }
}

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

pub struct Param {
    name: String,
    type_str: TypeNameString,
    span: Span,
}

pub struct Field {
    name: String,
    type_str: TypeNameString,
    span: Span,
}

pub struct GenericVar {
    name: String,
    impl_abst: Vec<TypeNameId>,
    only_type: Vec<TypeNameId>,
    expected_type: Vec<TypeNameId>,
}

pub enum AccessLevel {
    Private, Public, PublicExternal
}

pub enum DeclNode {
    Fun {
        name: String,
        params: Vec<Param>,
        return_type_str: TypeNameString,
        block: ExprNodeId,
        access_level: AccessLevel,
        span: Span,
    },
    FunDecl {
        name: String,
        params: Vec<Param>,
        return_type_str: TypeNameString,
        access_level: AccessLevel,
        span: Span,
    },
    Abstract {
        name: String,
        has_abst: Vec<String>,
        generic_vars: Vec<GenericVar>,
        access_level: AccessLevel,
        methods: Vec<DeclNodeId>,
        span: Span,
    },
    TypeStruct {
        name: String,
        fields: Vec<Field>,
        has_abst: Vec<String>,
        generic_vars: Vec<GenericVar>,
        access_level: AccessLevel,
        span: Span,
    },
    ADT {
        name: String,
        has_abst: Vec<String>,
        generic_vars: Vec<GenericVar>,
        ctors: Vec<DeclNode>,
        access_level: AccessLevel,
        span: Span,
    },
    Ctor {
        name: String,
        generic_vars: Vec<GenericVar>,
        return_type_str: TypeNameString,
        access_level: AccessLevel,
        span: Span,
    },
    CType {
        name: String,
        access_level: AccessLevel,
        span: Span,
    },
    External {
        name: String,
        sym_name: String,
        params: Vec<Param>,
        return_type_str: TypeNameString,
        access_level: AccessLevel,
        span: Span,
    }
}

pub struct FileAst {
    file: SourceId,
    atom_expr_pool: Vec<AtomExprNode>,
    expr_pool: Vec<ExprNode>,
    decl_pool: Vec<DeclNode>,
    type_name_pool: Vec<TypeNameString>,
}