use crate::hir::{HirDeclId, HirExprId};
use crate::lang_items::BuiltinType;
use crate::scope::SymId;
use std::collections::HashMap;

pub type TyId = usize;

/// 将声明 id 映射到其类型 id
pub type HirDeclTypeMap = HashMap<HirDeclId, TypeScheme>;

/// 将表达式 id 映射到其类型 id
pub type HirExprTypeMap = HashMap<HirExprId, TyId>;

/// sym => scheme
pub type NameTypeSchemeMap = HashMap<SymId, TypeScheme>;

/// let expr id => let decl type
pub type LetExprIdTypeMap = HashMap<HirExprId, TyId>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeNodeKind {
    Var,
    Builtin(BuiltinType),
    Ref(TyId),
    MutRef(TyId),
    Share(TyId),
    Fun {
        param_tys: Vec<TyId>,
        return_ty: TyId,
    },
    Struct {
        decl_id: HirDeclId,
        subst: Vec<TyId>,
    },
    Tuple(Vec<TyId>),
    Never,
    Unit,
}

#[derive(Debug, Clone)]
pub struct TypeNode {
    pub kind: TypeNodeKind,
    pub parent: TyId,
    pub level: u32,
}

#[derive(Debug, Clone)]
pub struct TypeScheme {
    pub quantified: Vec<TyId>,
    pub body: TyId,
}