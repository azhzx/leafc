use std::collections::HashMap;
use crate::diagnostic::DiagMsg;
use crate::hir::{HirDeclId, HirExprId};
use crate::source::Span;

pub type TyId = usize;

/// 将声明 id 映射到其类型 id
pub type HirDeclTypeMap = HashMap<HirDeclId, TypeScheme>;
/// 将表达式 id 映射到其类型 id
pub type HirExprTypeMap = HashMap<HirExprId, TyId>;

/// let expr id => let decl type
pub type LetExprIdTypeMap = HashMap<HirExprId, TyId>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeNodeKind {
    Var,
    Builtin(BuiltinType),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinType {
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
    Bool,
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