use crate::diagnostic::DiagMsg;
use crate::mir::MirModule;

pub trait CodegenApi {
    type Output;
    fn emit(mir_module: MirModule) -> Result<Self::Output, DiagMsg>;
}