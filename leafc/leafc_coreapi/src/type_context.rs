use std::collections::HashMap;
use crate::diagnostic::DiagMsg;
use crate::hir::{HirDeclId, HirExprId};
use crate::source::Span;

pub type TyId = usize;

/// 将声明 id 映射到其类型 id
pub type HirDeclTypeMap = HashMap<HirDeclId, TyId>;
/// 将表达式 id 映射到其类型 id
pub type HirExprTypeMap = HashMap<HirExprId, TyId>;

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
    Never,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum TypeKind {
    Builtin(BuiltinType),
    Fun {
        param_tys: Vec<TyId>,
        return_ty: TyId,
    },
    Struct {
        fields: Vec<TyId>,
    },
    /// 类型变量（无额外参数，通过 TyId 区分）
    Var,
    Tuple(Vec<TyId>),
}

#[derive(Clone, Debug)]
pub struct TypeUnit {
    pub decl: Option<HirDeclId>,
    pub kind: TypeKind,
    pub span: Option<Span>,
}

pub trait TypeContextApi {
    fn push_concrete(&mut self, ty: TypeUnit) -> TyId;
    fn new_ty_var(&mut self, span: Span) -> TyId;
    fn resolve(&self, id: TyId) -> Result<TyId, DiagMsg>;
    fn kind_of(&self, id: TyId) -> Result<TypeKind, DiagMsg>;
    fn unify(&mut self, lhs: TyId, rhs: TyId, span: Span) -> Result<(), DiagMsg>;
    fn bind_var(&mut self, var: TyId, ty: TyId) -> Result<(), DiagMsg>;
    fn occurs(&self, var: TyId, ty: TyId) -> Result<bool, DiagMsg>;
}