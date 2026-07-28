use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::mir::MirCrate;
use leafc_coreapi::mir_lifetime_checker::MirLifetimeCheckerApi;
use leafc_coreapi::type_system::TypeCtx;

pub struct MirLifetimeChecker {
    mir: MirCrate,
    type_ctx: TypeCtx,
}

impl MirLifetimeChecker {

}

impl MirLifetimeCheckerApi for MirLifetimeChecker {
    fn new(mir: MirCrate, type_ctx: TypeCtx) -> Self {
        Self {
            mir,
            type_ctx,
        }
    }

    fn check(self) -> Result<(MirCrate, TypeCtx), DiagMsg> {
        Ok((self.mir, self.type_ctx))
    }
}