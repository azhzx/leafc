use crate::hir::{HirDeclId, HirExprId};
use crate::lang_items::BuiltinType;
use crate::scope::SymId;
use std::collections::HashMap;
use crate::source::Span;

pub type TyId = usize;

pub type HirDeclTypeMap = HashMap<HirDeclId, TypeScheme>;
pub type HirExprTypeMap = HashMap<HirExprId, TyId>;
pub type NameTypeSchemeMap = HashMap<SymId, TypeScheme>;
pub type LocalBindingTypeMap = HashMap<SymId, TyId>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeNodeKind {
    Var,
    RigidVar,
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
    MutRawPtr(TyId),
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
    Struct { fields: Vec<FieldDef> },
    Enum { variants: Vec<VariantDef> },
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
    pub type_intern: HashMap<TypeNodeKind, TyId>,
}

impl TypeCtx {
    pub fn push_raw(&mut self, kind: TypeNodeKind) -> TyId {
        let id = self.type_pool.len();
        self.type_pool.push(TypeNode {
            kind,
            parent: id,
            level: 0,
        });
        id
    }

    pub fn intern_type(&mut self, kind: TypeNodeKind) -> TyId {
        let mut has_var = false;
        let canonical = self.canonicalize_kind(kind, &mut has_var);

        if has_var {
            return self.push_raw(canonical);
        }

        if let Some(&existing) = self.type_intern.get(&canonical) {
            return existing;
        }

        let id = self.push_raw(canonical.clone());
        self.type_intern.insert(canonical, id);
        id
    }

    fn canonicalize_kind(&self, kind: TypeNodeKind, has_var: &mut bool) -> TypeNodeKind {
        let mut rep = |ty: TyId| -> TyId {
            let r = get_type_root(&self.type_pool, ty);
            if matches!(self.type_pool[r].kind, TypeNodeKind::Var) {
                *has_var = true;
            }
            r
        };
        match kind {
            TypeNodeKind::Var => {
                *has_var = true;
                TypeNodeKind::Var
            }
            TypeNodeKind::Fun { param_tys, return_ty } => {
                let new_params: Vec<TyId> = param_tys.iter().map(|&t| rep(t)).collect();
                let new_ret = rep(return_ty);
                TypeNodeKind::Fun { param_tys: new_params, return_ty: new_ret }
            }
            TypeNodeKind::Tuple(elems) => {
                let new_elems: Vec<TyId> = elems.iter().map(|&t| rep(t)).collect();
                TypeNodeKind::Tuple(new_elems)
            }
            TypeNodeKind::Struct { decl_id, subst, field_tys } => {
                let new_subst: Vec<TyId> = subst.iter().map(|&s| rep(s)).collect();
                let new_fields: Vec<TyId> = field_tys.iter().map(|&f| rep(f)).collect();
                TypeNodeKind::Struct { decl_id, subst: new_subst, field_tys: new_fields }
            }
            TypeNodeKind::ADT { decl_id, subst, variants } => {
                let new_subst: Vec<TyId> = subst.iter().map(|&s| rep(s)).collect();
                let new_variants: Vec<Option<TyId>> = variants
                    .iter()
                    .map(|opt| opt.map(|t| rep(t)))
                    .collect();
                TypeNodeKind::ADT { decl_id, subst: new_subst, variants: new_variants }
            }
            TypeNodeKind::Ref(inner) => TypeNodeKind::Ref(rep(inner)),
            TypeNodeKind::MutRef(inner) => TypeNodeKind::MutRef(rep(inner)),
            TypeNodeKind::Share(inner) => TypeNodeKind::Share(rep(inner)),
            TypeNodeKind::RawPtr(inner) => TypeNodeKind::RawPtr(rep(inner)),
            other => other,
        }
    }
}