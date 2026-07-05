use crate::ast::FileAst;
use crate::diagnostic::DiagMsg;

pub enum NamePassError {
    
}

pub trait NamePassApi {
    fn name_pass(&self, ast: &mut FileAst) -> Result<(), DiagMsg>;
}