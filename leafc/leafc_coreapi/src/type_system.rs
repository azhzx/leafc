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

/// id => type
pub type LocalBindingTypeMap = HashMap<SymId, TyId>;

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
    Tuple(Vec<TyId>),
    Struct {
        decl_id: HirDeclId,
        subst: Vec<TyId>,
        field_tys: Vec<TyId>,
    },
    ADT {
        decl_id: HirDeclId,
        subst: Vec<TyId>,
        variants: Vec<Option<TyId>>,
    },
    Never,
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

pub fn get_type_root(type_pool: &[TypeNode], id: TyId) -> TyId {
    let mut cur = id;
    while type_pool[cur].parent != cur {
        cur = type_pool[cur].parent;
    }
    cur
}

pub struct TypeCtx {
    pub decl_type_map: HirDeclTypeMap,
    pub expr_type_map: HirExprTypeMap,
    pub local_binding_map: LocalBindingTypeMap,
    pub name_type_map: NameTypeSchemeMap,
    pub sym_to_decl: HashMap<SymId, HirDeclId>,
    pub type_pool: Vec<TypeNode>,
}