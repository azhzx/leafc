use crate::diagnostic::DiagMsg;
use crate::hir::HirCrate;
use crate::mir::MirCrate;
use crate::type_checker::TypeCheckerResult;

pub enum MirLowerError {

}

pub trait MirLowerApi {
    fn new(hir_and_types: TypeCheckerResult) -> Self;
    fn lower(self) -> Result<MirCrate, DiagMsg>;
}