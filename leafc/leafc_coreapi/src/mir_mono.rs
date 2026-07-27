use crate::diagnostic::DiagMsg;
use crate::mir::MirCrate;
use crate::type_system::TypeCtx;

pub trait MirMonoApi {
    fn new(mono_mir: MirCrate, type_checker_result: TypeCtx) -> Self;
    fn mono(self) -> Result<(MirCrate, TypeCtx), DiagMsg>;
}