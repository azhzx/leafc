use crate::diagnostic::DiagMsg;
use crate::hir::HirModule;

pub enum HirPassError {

}

pub trait HirPassApi {
    fn pass(hir_module: &mut HirModule) -> Result<(), DiagMsg>;
}