use crate::ast::{FileAst, AstModule};
use crate::diagnostic::DiagMsg;
use crate::hir::{HirModule};
use crate::name_pass::NamePassResult;

pub enum HirLowerError {
    
}



pub trait HirLowerApi<'a> {
    fn new(
        ast_module: &'a AstModule,
        name_pass_result: &'a NamePassResult,

        module_name: String
    ) -> Self;

    fn lower(&mut self) -> Result<&HirModule, DiagMsg>;
}