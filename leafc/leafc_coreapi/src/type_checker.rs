use crate::ast::CrateAst;
use crate::diagnostic::DiagMsg;
use crate::hir::HirCrate;
use crate::name_pass::NamePassResult;
use crate::type_context::{HirDeclTypeMap, HirExprTypeMap};

#[derive(Debug)]
pub enum TypeCheckerError {
   DuplicateType,
   InfiniteType,
   TypeMismatch
}

pub struct TypeCheckerResult {
    pub decl_type_map: HirDeclTypeMap,
    pub expr_type_map: HirExprTypeMap,
    pub hir: HirCrate
}

pub trait TypeCheckerApi {
    fn new(hir_crate: HirCrate) -> Self;
    fn check(self) -> Result<TypeCheckerResult, DiagMsg>;
}