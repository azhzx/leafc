use std::collections::HashMap;
use leafc_coreapi::ast::{DeclNode, FileAst, AstModule, ExprNode, Visibility};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::{HirDecl, HirDeclId, HirDeclKind, HirModule, TyId, TypePool};
use leafc_coreapi::hir_lower::HirLowerApi;
use leafc_coreapi::name_pass::{DoScopeMap, FunScopeMap, NamePassResult};
use leafc_coreapi::scope::{Scope, ScopePool, TopScopeIds};

pub struct HirLower<'a> {
    ast_module: &'a AstModule,
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
        ast_module: &'a AstModule,
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
        for file_ast in &self.ast_module.asts {
            let decl_pool = &file_ast.decl_pool;
            for decl in decl_pool {
                match decl.kind {
                    _ => todo!(),
                }
            }
        }
        Ok(&self.hir)
    }

}