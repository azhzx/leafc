use crate::ast::{CrateAst};
use crate::diagnostic::DiagMsg;
use crate::hir::{HirCrate};
use crate::name_pass::NamePassResult;

#[derive(Debug)]
pub enum HirLowerError {
    EmptyPath,
    PathNotFound,
    ModuleScopeNotFound,
    FieldNotFound,
    ConstructorNotFound,
    ControlNotFound,
    InvalidPath,
    GenericNotFound,
    BindingNotFound,
    ParamNotFound,
    MethodNotFound,
    CtorNotFound,
    NameNotFound,
    LetNameNotFound,
}



pub trait HirLowerApi<'a> {
    fn new(
        crate_ast: &'a CrateAst,
        name_pass_result: NamePassResult,

        crate_name: String
    ) -> Self;

    fn lower(self) -> Result<HirCrate, DiagMsg>;
}