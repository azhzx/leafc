use std::collections::HashMap;
use std::sync::Arc;
use crate::ast::{CrateAst, GreenCatchClause, GreenDecl, GreenExpr, GreenMatchArm};
use crate::diagnostic::DiagMsg;
use crate::lang_items::LangItems;
use crate::scope::{ScopeId, ScopePool};
use crate::source::SourceId;



#[derive(Debug)]
pub enum NamePassError {
    UndefinedName,
    DuplicateDefinition,
    UndefinedModule,
    InvalidMemberAccess,
    InvalidADTConstructor
}

pub type DoScopeMap  = HashMap<Arc<GreenExpr>, ScopeId>;
pub type FunScopeMap = HashMap<Arc<GreenDecl>, ScopeId>;
pub type CatchScopeMap = HashMap<Arc<GreenCatchClause>, ScopeId>;
pub type ArmScopeMap = HashMap<Arc<GreenMatchArm>, ScopeId>;


#[derive(Debug, Clone)]
pub struct NamePassResult {
    pub pool: ScopePool,
    pub do_scope_map: DoScopeMap,
    pub fun_scope_map: FunScopeMap,
    pub arm_scope_map: ArmScopeMap,
    pub catch_scope_map: CatchScopeMap,
    pub source_id_to_scope: HashMap<SourceId, ScopeId>,
    pub lang_items: LangItems
}

pub trait NamePassApi<'a> {
    fn new(ast: &'a CrateAst) -> Self;
    fn build_scope(&mut self) -> Result<(), DiagMsg>;
    fn resolve(&mut self) -> Result<(), DiagMsg>;
    fn pass(self) -> Result<NamePassResult, DiagMsg>;
}