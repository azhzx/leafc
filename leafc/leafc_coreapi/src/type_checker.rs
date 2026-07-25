use crate::diagnostic::DiagMsg;
use crate::hir::HirCrate;
use crate::type_system::{HirDeclTypeMap, HirExprTypeMap, LocalBindingTypeMap};

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
}

pub struct TypeCheckerResult {
    pub decl_type_map: HirDeclTypeMap,
    pub expr_type_map: HirExprTypeMap,
    pub local_binding_map: LocalBindingTypeMap,
    pub hir: HirCrate
}

pub trait TypeCheckerApi {
    fn new(hir_crate: HirCrate) -> Self;
    fn check(self) -> Result<TypeCheckerResult, DiagMsg>;
}