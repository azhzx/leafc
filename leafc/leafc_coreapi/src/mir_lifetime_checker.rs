use crate::diagnostic::DiagMsg;
use crate::mir::MirCrate;
use crate::type_system::TypeCtx;

pub trait MirLifetimeCheckerApi {
    fn new(mir: MirCrate, type_ctx: TypeCtx) -> Self;
    fn check(self) -> Result<(MirCrate, TypeCtx), DiagMsg>;
}