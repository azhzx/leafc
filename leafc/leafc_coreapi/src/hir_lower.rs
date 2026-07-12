use crate::ast::{CrateAst};
use crate::diagnostic::DiagMsg;
use crate::hir::{HirCrate};
use crate::name_pass::NamePassResult;

pub enum HirLowerError {
    
}



pub trait HirLowerApi<'a> {
    fn new(
        crate_ast: &'a CrateAst,
        name_pass_result: &'a NamePassResult,

        crate_name: String
    ) -> Self;

    fn lower(&mut self) -> Result<&HirCrate, DiagMsg>;
}