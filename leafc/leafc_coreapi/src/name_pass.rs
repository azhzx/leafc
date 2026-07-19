use std::collections::HashMap;
use std::sync::Arc;
use crate::ast::{CrateAst, GreenDecl, GreenExpr};
use crate::diagnostic::DiagMsg;
use crate::scope::{ScopeId, ScopePool};
use crate::source::SourceId;

pub type DoScopeMap  = HashMap<Arc<GreenExpr>, ScopeId>;
pub type FunScopeMap = HashMap<Arc<GreenDecl>, ScopeId>;

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
    pub source_id_to_scope: HashMap<SourceId, ScopeId>,
}

pub trait NamePassApi<'a> {
    fn new(ast: &'a CrateAst) -> Self;
    fn build_scope(&mut self) -> Result<(), DiagMsg>;
    fn resolve(&mut self) -> Result<(), DiagMsg>;
    fn pass(self) -> Result<NamePassResult, DiagMsg>;
}