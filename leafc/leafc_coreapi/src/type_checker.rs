use std::collections::HashMap;
use crate::diagnostic::DiagMsg;
use crate::hir::{HirCrate, HirDeclId};
use crate::scope::SymId;
use crate::type_system::{HirDeclTypeMap, HirExprTypeMap, LocalBindingTypeMap, NameTypeSchemeMap, TypeNode};

#[derive(Debug)]
pub enum TypeCheckerError {
    DuplicateType,
    InfiniteType,
    TypeMismatch,
    GenericArityMismatch,
    ArityMismatch,
    UndefinedVariable,
    TypeNotChecked,
    FieldNotFound,
    InternalError,
    UnknownField,
    MissingTypeAnnotation,
    UndefinedType,
    RecursiveTypeAlias,
    MissingResume,
    InvalidControlType,
    UnreachablePattern,
    NonExhaustiveMatch,
}

pub struct TypeCheckerResult {
    pub decl_type_map: HirDeclTypeMap,
    pub expr_type_map: HirExprTypeMap,
    pub local_binding_map: LocalBindingTypeMap,
    pub name_type_map: NameTypeSchemeMap,
    pub sym_to_decl: HashMap<SymId, HirDeclId>,
    pub type_pool: Vec<TypeNode>,
}

pub trait TypeCheckerApi {
    fn new(hir_crate: HirCrate) -> Self;
    fn check(self) -> Result<(TypeCheckerResult, HirCrate), DiagMsg>;
}