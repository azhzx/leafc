use crate::ast::FileAst;
use crate::diagnostic::DiagMsg;

pub enum HirLowerError {
    
}

pub trait HirLowerApi<'a> {
    fn new(file_ast: &'a FileAst) -> Self;
    fn lower() -> Result<(), DiagMsg>;
}