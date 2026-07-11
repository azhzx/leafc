use std::collections::HashMap;
use crate::ast::{DeclNodeId, ExprNodeId, CrateAst};
use crate::diagnostic::DiagMsg;
use crate::scope::{ScopeId, ScopePool};

pub type DoScopeMap  = HashMap<ExprNodeId, ScopeId>;
pub type FunScopeMap = HashMap<DeclNodeId, ScopeId>;

#[derive(Debug)]
pub enum NamePassError {
    UndefinedName,
    DuplicateDefinition,
    UndefinedModule,
    InvalidMemberAccess,
}


pub struct NamePassResult<'a> {
    pub tree: &'a ScopePool,
    pub do_scope_map: &'a DoScopeMap,
    pub fun_scope_map: &'a FunScopeMap,
}

pub trait NamePassApi<'a> {
    fn new(ast: &'a CrateAst) -> Self;
    fn pass_scope(&mut self) -> Result<(), DiagMsg>;
    fn pass_name(&mut self) -> Result<(), DiagMsg>;
    fn pass(&mut self) -> Result<NamePassResult, DiagMsg>;
}