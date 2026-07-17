use std::collections::HashMap;
use crate::diagnostic::DiagMsg;
use crate::hir::{HirDecl, HirDeclId, HirExprId};
use crate::source::Span;
use crate::type_context::TypeKind::Var;

pub type TyId = usize;

/// 将声明 id 映射到其推导出的或标注的类型 id(HirLower阶段只做标记, 推导和检查留给TypeChecker)
pub type HirDeclTypeMap = HashMap<HirDeclId, TyId>;

/// Checker + Infer
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
    Var(TypeVarId),
    Tuple(Vec<TyId>),
}

#[derive(Clone, Debug)]
pub struct TypeUnit {
    pub decl: Option<HirDeclId>,
    pub kind: TypeKind,
    pub span: Span,
}

pub type TypeVarId = usize;

pub trait TypeContextApi {
    fn push_concrete(&mut self, ty: TypeUnit) -> TyId;
    fn new_ty_var(&mut self, span: Span) -> TyId;
    fn resolve(&self, id: TyId) -> Result<TyId, DiagMsg>;
    fn kind_of(&self, id: TyId) -> Result<TypeKind, DiagMsg>;
    /// 合一
    fn unify(&mut self, lhs: TyId, rhs: TyId, span: Span) -> Result<(), DiagMsg>;
    fn bind_var(&mut self, var: TyId, ty: TyId) -> Result<(), DiagMsg>;
    fn occurs(&self, var: TyId, ty: TyId) -> Result<bool, DiagMsg>;
}