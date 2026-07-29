use crate::diagnostic::DiagMsg;
use crate::hir::HirCrate;
use crate::mir::MirCrate;
use crate::type_system::TypeCtx;

pub enum MirLowerError {
    UndefinedVariable,
}

pub trait MirLowerApi {
    fn new(ty_ck_result: TypeCtx, hir_crate: HirCrate) -> Self;
    fn lower(self) -> Result<(MirCrate, TypeCtx), DiagMsg>;
}