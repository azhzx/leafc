use std::collections::HashMap;
use crate::ast::{AstModule, DeclNodeId, ExprNodeId, FileAst};
use crate::diagnostic::DiagMsg;
use crate::scope::{ScopeId, ScopePool, TopScopeIds};

pub type DoScopeMap  = HashMap<ExprNodeId, ScopeId>;
pub type FunScopeMap = HashMap<DeclNodeId, ScopeId>;

#[derive(Debug)]
pub enum NamePassError {
    UndefinedName,
    DuplicateDefinition,
    UndefinedModule,
}


pub struct NamePassResult<'a> {
    pub top_scope_ids: &'a TopScopeIds,
    pub scope_pool: &'a ScopePool,
    pub do_scope_map: &'a DoScopeMap,
    pub fun_scope_map: &'a FunScopeMap,
}

pub trait NamePassApi<'a> {
    fn new(ast: &'a AstModule) -> Self;
    fn pass_scope(&mut self) -> Result<(), DiagMsg>;
    fn pass_name(&mut self) -> Result<(), DiagMsg>;
    fn pass(&mut self) -> Result<NamePassResult, DiagMsg>;
}