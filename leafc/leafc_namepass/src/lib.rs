use leafc_coreapi::ast::FileAst;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::name_pass::NamePassApi;
use leafc_coreapi::source::SourceId;

pub struct NamePass<'a> {
    ast: &'a mut FileAst,
}

impl<'a> NamePassApi<'a> for NamePass<'a> {
    fn new(ast: &'a mut FileAst) -> Self {
        todo!()
    }
    
    fn pass(&self) -> Result<(), DiagMsg> {
        todo!()
    }
}
