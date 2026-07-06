use crate::ast::FileAst;
use crate::diagnostic::DiagMsg;

pub enum NamePassError {
    
}

pub trait NamePassApi<'a> {
    fn new(ast: &'a mut FileAst) -> Self;
    fn pass(&self,) -> Result<(), DiagMsg>;
}