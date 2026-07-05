use crate::diagnostic::DiagMsg;
use crate::hir::HirModule;

pub enum MirLowerError {

}

pub trait MirLowerApi {
    fn lower(hir_module: HirModule) -> Result<(), DiagMsg>;
}