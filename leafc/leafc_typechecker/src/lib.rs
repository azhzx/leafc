pub mod type_context;

use std::collections::HashMap;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::HirCrate;
use leafc_coreapi::name_pass::NamePassResult;
use leafc_coreapi::source::Span;
use leafc_coreapi::type_checker::{TypeCheckerApi, TypeCheckerResult};
use leafc_coreapi::type_context::{HirDeclTypeMap, HirExprTypeMap, TyId, TypeContextApi, TypeKind, TypeUnit};
use leafc_coreapi::type_context::TypeKind::Var;

pub struct TypeChecker {
    hir_crate: HirCrate,
    type_pool: Vec<TypeUnit>,
    bindings: Vec<Option<TyId>>,
    decl_type_map: HirDeclTypeMap,
    expr_type_map: HirExprTypeMap,
}


impl TypeCheckerApi for TypeChecker {
    fn new(hir_crate: HirCrate) -> Self {
        Self {
            hir_crate,
            type_pool: vec![],
            bindings: vec![],
            decl_type_map: HashMap::new(),
            expr_type_map: HashMap::new(),
        }
    }

    fn check(mut self) -> Result<TypeCheckerResult, DiagMsg> {
        Ok(TypeCheckerResult {
            decl_type_map: self.decl_type_map,
            expr_type_map: self.expr_type_map,
        })
    }
}