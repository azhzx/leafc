use crate::diagnostic::DiagMsg;
use crate::hir::HirCrate;
use crate::type_system::TypeCtx;

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
    MultipleResume,
}

pub trait TypeCheckerApi {
    fn new(hir_crate: HirCrate) -> Self;
    fn check(self) -> Result<(TypeCtx, HirCrate), DiagMsg>;
}