use std::collections::HashMap;
use crate::source::Span;

pub struct HirModule {
    pub name: String,
    pub main_fun: Option<HirDeclId>,
    pub hir_expr_pool: Vec<HirExpr>,
    pub hir_decl_pool: Vec<HirDecl>,
    /// 模块中对外公开的声明
    pub pub_decl_ids: Vec<HirDeclId>,
    pub name_type_id_map: HashMap<String, TyId>,
    pub type_pool: TypePool
}

/// 将声明 id 映射到其推导出的或标注的类型 id
pub type HirTypeMap = HashMap<HirDeclId, TyId>;


pub type HirDeclId = usize;
pub type HirExprId = usize;


#[derive(Debug, Clone)]
pub enum QPath {
    Builtin {
        item: BuiltinItemKind,
        span: Span,
    },
    Resolved {
        ty: Option<TyId>,
        concrete: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinItemKind {

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
        return_type: Option<QPath>,
        body: Vec<HirExprId>,
    },
    Struct {
        generic_params: Vec<HirGenericParam>,
        fields: Vec<HirFieldDef>,
        implemented_absts: Vec<QPath>,
    },
    TypeAlias {
        generic_params: Vec<HirGenericParam>,
        alias_for: QPath,
    },
    ADT {
        generic_params: Vec<HirGenericParam>,
        ctors: Vec<HirCtorDef>,
        implemented_absts: Vec<QPath>,
    },
    Abstract {
        generic_params: Vec<HirGenericParam>,
        methods: Vec<HirMethodDecl>,
        super_absts: Vec<QPath>,
    },
    CType,
    External {
        sym_name: String,
        params: Vec<HirParam>,
        return_type: Option<QPath>,
    },
}



#[derive(Debug, Clone)]
pub struct HirGenericParam {
    pub name: String,
    pub constraints: Vec<QPath>,
}

#[derive(Debug, Clone)]
pub struct HirFieldDef {
    pub name: String,
    pub type_ann: QPath,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct HirParam {
    pub name: String,
    pub type_ann: QPath,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct HirMethodDecl {
    pub name: String,
    pub generic_params: Vec<HirGenericParam>,
    pub params: Vec<HirParam>,
    pub return_type: Option<QPath>,
    pub is_pub_external: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct HirCtorDef {
    pub name: String,
    pub generic_params: Vec<HirGenericParam>,
    pub from_type: QPath,
    pub return_type: QPath,
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
    /// 路径引用
    Path(QPath),
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
        type_ann: QPath,
    },
    Block {
        stmts: Vec<HirExprId>,
    },
    Let {
        name: String,
        type_ann: Option<QPath>,
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

pub type TyId = usize;
pub enum TypeKind {
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Never,
    Ptr { ref_to: TyId },
    Ref { ref_to: TyId },
    MutRef { ref_to: TyId },

    Alias {
        ref_to: TyId,
        def: HirDeclId
    },
    Struct {
        def: HirDeclId
    },
    Union {
        unions: Vec<TyId>,
        def: HirDeclId
    },
    Tuple {
        members: Vec<TyId>,
    }
}

pub struct TypeSymbol {
    kind: TypeKind,
}

pub type TypePool = Vec<TypeSymbol>;