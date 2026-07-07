use crate::ast::FileAst;
use crate::diagnostic::DiagMsg;
use crate::scope::{ScopeId, ScopePool, TopScopePool};

#[derive(Debug)]
pub enum NamePassError {
    UndefinedName,
    DuplicateDefinition
}

pub trait NamePassApi<'a> {
    fn new(ast: &'a FileAst) -> Self;
    fn pass_scope(&mut self) -> Result<(), DiagMsg>;
    fn pass_name(&mut self) -> Result<(), DiagMsg>;
    fn pass(&mut self) -> Result<(&TopScopePool, &ScopePool), DiagMsg>;
}