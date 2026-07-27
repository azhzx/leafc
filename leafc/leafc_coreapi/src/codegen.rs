use crate::diagnostic::DiagMsg;
use crate::mir::MirCrate;
use crate::type_system::TypeCtx;

pub trait CodegenApi {
    type Output;
    fn new(mir: MirCrate, type_checker_result: TypeCtx) -> Self;
    fn emit(self) -> Result<Self::Output, DiagMsg>;
}