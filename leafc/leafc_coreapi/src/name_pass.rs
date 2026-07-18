use std::collections::HashMap;
use std::sync::Arc;
use crate::ast::{CrateAst, DeclNode, ExprNode};
use crate::diagnostic::DiagMsg;
use crate::scope::{ScopeId, ScopePool};

pub type DoScopeMap  = HashMap<Arc<ExprNode>, ScopeId>;
pub type FunScopeMap = HashMap<Arc<DeclNode>, ScopeId>;

#[derive(Debug)]
pub enum NamePassError {
    UndefinedName,
    DuplicateDefinition,
    UndefinedModule,
    InvalidMemberAccess,
    InvalidADTConstructor
}


#[derive(Debug, Clone)]
pub struct NamePassResult {
    pub pool: ScopePool,
    pub do_scope_map: DoScopeMap,
    pub fun_scope_map: FunScopeMap,
}

pub trait NamePassApi<'a> {
    fn new(ast: &'a CrateAst) -> Self;
    fn build_scope(&mut self) -> Result<(), DiagMsg>;
    fn resolve(&mut self) -> Result<(), DiagMsg>;
    fn pass(self) -> Result<NamePassResult, DiagMsg>;
}