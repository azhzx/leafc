use crate::ast::FileAst;
use crate::diagnostic::DiagMsg;

pub enum HirLowerError {
    
}

pub trait HirLowerApi {
    fn lower(file_ast: FileAst) -> Result<(), DiagMsg>;
}