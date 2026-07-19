use crate::diagnostic::DiagMsg;
use crate::hir::HirCrate;
use crate::mir::MirCrate;

pub enum MirLowerError {

}

pub trait MirLowerApi {
    fn new(
        hir_crate: HirCrate
    ) -> Self;
    fn lower(self) -> Result<MirCrate, DiagMsg>;
}