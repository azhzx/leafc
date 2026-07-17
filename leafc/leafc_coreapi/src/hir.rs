use std::collections::HashMap;
use crate::name_pass::NamePassResult;
use crate::scope::{ScopeId, SymId};
use crate::source::Span;
use crate::type_context::{HirDeclTypeMap, TyId, TypeUnit};

#[derive(Debug, Clone)]
pub struct HirCrate {
    pub name: String,
    
    pub main_fun: Option<HirDeclId>,
    
    pub hir_expr_pool: Vec<HirExpr>,
    
    pub hir_decl_pool: Vec<HirDecl>,
    
    /// 模块中对外公开的声明(PublicExternal)
    pub pub_decl_ids: Vec<HirDeclId>,
    
    pub type_map: HirDeclTypeMap,
    
    pub type_pool: Vec<TypeUnit>,

    pub name_pass_result: NamePassResult
}

pub type HirDeclId = usize;
pub type HirExprId = usize;


#[derive(Debug, Clone)]
pub struct HirName { // 具体的SymbolId
    pub name: String,
    pub sym_id: SymId,
}

#[derive(Debug, Clone)]
pub struct HirTypeName {
    pub name: HirName,
    pub args: Vec<HirTypeName>,
}

#[derive(Debug, Clone)]
pub struct HirGenericParam {
    pub name: HirName,
    pub constraints: Vec<HirTypeName>,
}


#[derive(Debug, Clone)]
pub struct HirDecl {
    pub ident: String,
    pub kind: HirDeclKind,
    pub is_pub_external: bool, // public, private在NamePass已被处理, 只剩pub(external)
    pub hir_id: HirDeclId,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum HirDeclKind {
    Fun {
        generic_params: Vec<HirGenericParam>,
        params: Vec<HirParam>,
        return_type: Option<HirTypeName>,
        body: Vec<HirExprId>,
    },
    Struct {
        generic_params: Vec<HirGenericParam>,
        fields: Vec<HirFieldDef>,
        implemented_abstracts: Vec<HirTypeName>,
    },
    TypeAlias {
        generic_params: Vec<HirGenericParam>,
        alias_for: HirTypeName,
    },
    ADT {
        generic_params: Vec<HirGenericParam>,
        ctors: Vec<HirCtorDef>,
        implemented_abstracts: Vec<HirTypeName>,
    },
    Abstract {
        generic_params: Vec<HirGenericParam>,
        methods: Vec<HirMethodDecl>,
        super_abstracts: Vec<HirTypeName>,
    },
    CType,
    External {
        sym_name: String,
        params: Vec<HirParam>,
        return_type: HirTypeName,
    },
}


#[derive(Debug, Clone)]
pub struct HirFieldDef {
    pub name: HirName,
    pub type_ann: HirTypeName,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct HirParam {
    pub name: HirName,
    pub type_ann: Option<HirTypeName>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct HirMethodDecl {
    pub name: HirName,
    pub generic_params: Vec<HirGenericParam>,
    pub params: Vec<HirParam>,
    pub return_type: Option<HirTypeName>,
    pub is_pub_external: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct HirCtorDef {
    pub name: HirName,
    pub generic_params: Vec<HirGenericParam>,
    pub from_type: Option<HirTypeName>,
    pub return_type: Option<HirTypeName>,
    pub is_pub_external: bool,
    pub span: Span,
}


#[derive(Debug, Clone)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub hir_id: HirExprId,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum HirExprKind {
    Lit(HirLit),
    Ident(HirName),
    Binary {
        left: HirExprId,
        right: HirExprId,
        op: HirBinOp,
    },
    Unary {
        op: HirUnaryOp,
        right: HirExprId,
    },
    Move {
        target: HirExprId,
    },
    Copy {
        target: HirExprId,
    },
    Ref {
        target: HirExprId,
    },
    MutRef {
        target: HirExprId,
    },
    Share {
        target: HirExprId,
    },
    Call {
        callee: HirExprId,
        args: Vec<HirExprId>,
    },
    UnsafeExternalCall {
        callee: HirExprId,
        args: Vec<HirExprId>,
    },
    FieldAccess {
        obj: HirExprId,
        field: String,
    },
    TypeCast {
        expr: HirExprId,
        type_ann: HirTypeName,
    },
    Block {
        stmts: Vec<HirExprId>,
    },
    Let {
        name: HirName,
        type_ann: Option<HirTypeName>,
        init: HirExprId,
        mutable: bool,
    },
    If {
        cond: HirExprId,
        then: HirExprId,
        elifs: Vec<(HirExprId, HirExprId)>,
        else_opt: Option<HirExprId>,
    },
    Tuple {
        elements: Vec<HirExprId>,
    },
    Return {
        expr: Option<HirExprId>,
    },
    Ellipsis,
}

#[derive(Debug, Clone)]
pub enum HirLit {
    Decimal(String),
    Int(String),
    Str(String),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirUnaryOp {
    Not,
    Neg,
}