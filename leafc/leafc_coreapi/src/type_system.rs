use crate::hir::{HirDeclId, HirExprId};
use crate::lang_items::BuiltinType;
use crate::scope::SymId;
use std::collections::HashMap;
use crate::source::Span;

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
    RawPtr(TyId),
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

// ===----------------------------
// Type Definition
// ===----------------------------

#[derive(Debug, Clone)]
pub struct GenericParamDef {
    pub name: String,
    pub index: usize,
    pub default_ty: Option<TyId>,
    pub def_id: TyId,
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub ty: TyId,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct VariantDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub tag: Option<u64>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TypeDefKind {
    Struct {
        fields: Vec<FieldDef>,
    },
    Enum {
        variants: Vec<VariantDef>,
    },
}

#[derive(Debug, Clone)]
pub struct TypeDef {
    pub name: String,
    pub span: Span,
    pub generics: Vec<GenericParamDef>,
    pub kind: TypeDefKind,
}



pub struct TypeCtx {
    pub decl_type_map: HirDeclTypeMap,
    pub expr_type_map: HirExprTypeMap,
    pub local_binding_map: LocalBindingTypeMap,
    pub name_type_map: NameTypeSchemeMap,
    pub sym_to_decl: HashMap<SymId, HirDeclId>,
    pub type_pool: Vec<TypeNode>,
    pub generic_type_defs: HashMap<HirDeclId, TypeDef>,
    pub concrete_type_defs: HashMap<(HirDeclId, Vec<TyId>), TypeDef>,
}