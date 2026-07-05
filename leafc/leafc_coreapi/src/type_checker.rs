use crate::ast::FileAst;
use crate::diagnostic::DiagMsg;

pub enum TypeCheckerError {
    
}

pub trait TypeCheckerApi {
    fn check(&self, ast: &mut FileAst) -> Result<(), DiagMsg>;
}