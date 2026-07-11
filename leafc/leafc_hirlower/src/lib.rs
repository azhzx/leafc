use std::collections::HashMap;
use leafc_coreapi::ast::{DeclNode, CrateAst, ExprNode, Visibility};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::{HirDecl, HirDeclId, HirDeclKind, HirModule, TyId, TypePool};
use leafc_coreapi::hir_lower::HirLowerApi;
use leafc_coreapi::name_pass::{DoScopeMap, FunScopeMap, NamePassResult};


pub struct HirLower<'a> {
    ast_module: &'a CrateAst,
    name_pass_result: &'a NamePassResult<'a>,
    hir: HirModule,
}

impl<'a> HirLower<'a> {
    pub fn find_type(&self, name: String) -> Option<TyId> {
        self.hir.name_type_id_map.get(&name).cloned()
    }
}

impl<'a> HirLowerApi<'a> for HirLower<'a> {
    fn new(
        ast_module: &'a CrateAst,
        name_pass_result: &'a NamePassResult,

        module_name: String
    ) -> Self {
        Self {
            ast_module,
            name_pass_result,
            hir: HirModule {
                name: module_name,
                main_fun: None,
                hir_expr_pool: vec![],
                hir_decl_pool: vec![],
                pub_decl_ids: vec![],
                name_type_id_map: HashMap::new(),
                type_pool: vec![],
            },
        }
    }


    fn lower(&mut self) -> Result<&HirModule, DiagMsg> {
        todo!();
        Ok(&self.hir)
    }

}