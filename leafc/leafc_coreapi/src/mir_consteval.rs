use crate::diagnostic::DiagMsg;
use crate::mir::MirCrate;
use crate::type_system::TypeCtx;

pub trait MirConstEvalApi {
    fn new(mir: MirCrate, type_ctx: TypeCtx) -> Self;
    fn eval(self) -> Result<(MirCrate, TypeCtx), DiagMsg>;
}