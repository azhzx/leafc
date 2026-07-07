use std::any::TypeId;
use std::collections::HashMap;
use leafc_coreapi::ast::FileAst;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::{HirId, HirModule};
use leafc_coreapi::hir_lower::HirLowerApi;
use leafc_coreapi::scope::TopScopePool;

struct HirLower<'a> {
    file_ast: &'a FileAst,
    type_node_map: HashMap<TypeId, HirId>,
    scope: &'a TopScopePool,
    hir: &'a HirModule,
}

impl<'a> HirLower<'a> {

}

impl<'a> HirLowerApi<'a> for HirLower<'a> {
    fn new(file_ast: &'a FileAst) -> Self {
        todo!()
    }

    fn lower() -> Result<(), DiagMsg> {
        todo!()
    }
}