use std::collections::HashMap;

use leafc_coreapi::ast::{AtomExprNode, CrateAst, DeclNode, DeclNodeId, DeclNodeKind, ElseIf, ExprNode, ExprNodeKind, Field, GenericVar, MethodDecl, Operator, Param, TypeNameString, Visibility};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::{
    HirBinOp, HirCtorDef, HirDecl, HirDeclId, HirDeclKind, HirCrate, HirExpr,
    HirExprId, HirExprKind, HirFieldDef, HirGenericParam, HirLit, HirMethodDecl,
    HirName, HirParam, HirTypeName, HirUnaryOp,
};
use leafc_coreapi::hir_lower::HirLowerApi;
use leafc_coreapi::name_pass::{DoScopeMap, FunScopeMap, NamePassResult};
use leafc_coreapi::scope::{ScopeId, ScopePool, Symbol};

pub struct HirLower<'a> {
    ast_crate: &'a CrateAst,
    hir: HirCrate,
}

impl<'a> HirLower<'a> {
    fn type_name_to_path(
        &self,
        type_name: &TypeNameString,
        scope_id: ScopeId,
    ) -> Result<HirTypeName, DiagMsg> {
        let sym = self.lookup_symbol(scope_id, &type_name.name).unwrap();
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };

        let args: Vec<HirTypeName> = type_name
            .generics
            .iter()
            .map(|t| self.type_name_to_path(t, scope_id))
            .collect::<Result<_, _>>()?;

        Ok(HirTypeName { name, args })
    }

    fn lookup_symbol(
        &self,
        scope_id: ScopeId,
        name: &str,
    ) -> Option<&Symbol> {
        self.hir.name_pass_result
            .pool
            .lookup(scope_id, name)
            .map(|(sym, _)| sym)
    }

    fn lower_generic_params(
        &self,
        generic_vars: &[GenericVar],
        scope_id: ScopeId,
    ) -> Result<Vec<HirGenericParam>, DiagMsg> {
        generic_vars
            .iter()
            .map(|gv| {
                let sym = self.lookup_symbol(scope_id, &gv.name).unwrap();
                let name = HirName {
                    name: sym.name.clone(),
                    sym_id: sym.sym_id,
                };
                let constraints = gv
                    .constraint
                    .iter()
                    .map(|c| self.type_name_to_path(c, scope_id))
                    .collect::<Result<_, _>>()?;
                Ok(HirGenericParam { name, constraints })
            })
            .collect()
    }

    /// 获取声明自身的作用域
    fn get_decl_scope(&self, decl_id: usize) -> Option<ScopeId> {
        self.hir.name_pass_result.pool
            .decl_node_scope_map
            .get(&decl_id)
            .copied()
    }

    fn lower_decl(&mut self, decl_id: usize) -> Result<HirDeclId, DiagMsg> {
        let decl = self.ast_crate.decl_pool[decl_id].clone();
        let hir_id = self.hir.hir_decl_pool.len();
        let is_pub_external = decl.visibility == Visibility::PublicExternal;
        let ident = decl.name.clone();
        let span = decl.span;
        let decl_scope = self.get_decl_scope(decl_id).unwrap();

        let kind = match &decl.kind {
            DeclNodeKind::Fun {
                params,
                return_type_str,
                block,
            } => {
                let scope_id =
                    decl_scope;
                let generic_params = vec![]; // for future

                let hir_params = params
                    .iter()
                    .map(|p| self.lower_param(p, scope_id))
                    .collect::<Result<_, _>>()?;

                let return_type = if return_type_str.name.is_empty() {
                    None
                } else {
                    Some(self.type_name_to_path(return_type_str, scope_id)?)
                };

                let body: Vec<HirExprId> = block
                    .iter()
                    .map(|&expr_id| self.lower_expr(expr_id, scope_id))
                    .collect::<Result<Vec<_>, _>>()?;

                HirDeclKind::Fun {
                    generic_params,
                    params: hir_params,
                    return_type,
                    body,
                }
            }
            DeclNodeKind::FunDecl {
                params,
                return_type_str,
            } => {
                todo!()
            }
            DeclNodeKind::Abstract {
                has_abst,
                generic_vars,
                methods,
            } => {
                let scope_id =
                    decl_scope;
                let generic_params = self.lower_generic_params(generic_vars, scope_id)?;

                let super_absts = has_abst
                    .iter()
                    .map(|name| self.lower_plain_type_name(name, scope_id))
                    .collect::<Result<_, _>>()?;

                let hir_methods = methods
                    .iter()
                    .map(|m| self.lower_method_decl(m, scope_id))
                    .collect::<Result<_, _>>()?;

                HirDeclKind::Abstract {
                    generic_params,
                    methods: hir_methods,
                    super_absts,
                }
            }
            DeclNodeKind::TypeStruct {
                fields,
                has_abst,
                generic_vars,
            } => {
                let scope_id =
                    decl_scope;
                let generic_params = self.lower_generic_params(generic_vars, scope_id)?;

                let hir_fields = fields
                    .iter()
                    .map(|f| self.lower_field_def(f, scope_id))
                    .collect::<Result<_, _>>()?;

                let implemented_absts = has_abst
                    .iter()
                    .map(|name| self.lower_plain_type_name(name, scope_id))
                    .collect::<Result<_, _>>()?;

                HirDeclKind::Struct {
                    generic_params,
                    fields: hir_fields,
                    implemented_absts,
                }
            }
            DeclNodeKind::TypeAlias {
                ref_to,
                has_abst: _,
                generic_vars,
            } => {
                let scope_id =
                    decl_scope;
                let generic_params = self.lower_generic_params(generic_vars, scope_id)?;
                let alias_for = self.type_name_to_path(ref_to, scope_id)?;

                HirDeclKind::TypeAlias {
                    generic_params,
                    alias_for,
                }
            }
            DeclNodeKind::ADT {
                has_abst,
                generic_vars,
                ctors,
            } => {
                let scope_id =
                    decl_scope;
                let generic_params = self.lower_generic_params(generic_vars, scope_id)?;
                let implemented_absts = has_abst
                    .iter()
                    .map(|name| self.lower_plain_type_name(name, scope_id))
                    .collect::<Result<_, _>>()?;

                let hir_ctors = ctors
                    .iter()
                    .map(|c| self.lower_ctor_def(c, scope_id))
                    .collect::<Result<_, _>>()?;

                HirDeclKind::ADT {
                    generic_params,
                    ctors: hir_ctors,
                    implemented_absts,
                }
            }
            DeclNodeKind::CType => HirDeclKind::CType,
            DeclNodeKind::External {
                sym_name,
                params,
                return_type_str,
            } => {
                todo!();

                // HirDeclKind::External {
                //     sym_name: sym_name.clone(),
                //     params: hir_params,
                //     return_type,
                // }
            }
            DeclNodeKind::FileUnit { .. } => unreachable!("FileUnit should be flattened"),
        };

        let hir_decl = HirDecl {
            ident,
            kind,
            is_pub_external,
            hir_id,
            span,
        };
        self.hir.hir_decl_pool.push(hir_decl);
        Ok(hir_id)
    }

    fn lower_param(&self, param: &Param, scope_id: ScopeId) -> Result<HirParam, DiagMsg> {
        let sym = self.lookup_symbol(scope_id, &param.name).unwrap();
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };
        let type_ann = if param.type_str.name.is_empty() {
            None
        } else {
            Some(self.type_name_to_path(&param.type_str, scope_id)?)
        };
        Ok(HirParam {
            name,
            type_ann,
            span: param.span.clone(),
        })
    }

    fn lower_field_def(&self, field: &Field, scope_id: ScopeId) -> Result<HirFieldDef, DiagMsg> {
        let type_ann = self.type_name_to_path(&field.type_str, scope_id)?;
        let sym = self.lookup_symbol(scope_id, &field.name).unwrap();
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };
        Ok(HirFieldDef {
            name,
            type_ann,
            span: field.span.clone(),
        })
    }

    fn lower_method_decl(
        &self,
        method: &MethodDecl,
        scope_id: ScopeId,
    ) -> Result<HirMethodDecl, DiagMsg> {
        let sym = self.lookup_symbol(scope_id, &method.name).unwrap();
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };
        let params = method
            .params
            .iter()
            .map(|p| self.lower_param(p, scope_id))
            .collect::<Result<_, _>>()?;
        let return_type = if method.return_type_str.name.is_empty() {
            None
        } else {
            Some(self.type_name_to_path(&method.return_type_str, scope_id)?)
        };
        Ok(HirMethodDecl {
            name,
            generic_params: vec![], // for future
            params,
            return_type,
            is_pub_external: method.visibility == Visibility::PublicExternal,
            span: method.span.clone(),
        })
    }

    fn lower_ctor_def(
        &self,
        ctor: &leafc_coreapi::ast::Ctor,
        scope_id: ScopeId,
    ) -> Result<HirCtorDef, DiagMsg> {
        let sym = self.lookup_symbol(scope_id, &ctor.name).unwrap();
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };
        let generic_params = self.lower_generic_params(&ctor.generic_vars, scope_id)?;

        let from_type = if ctor.from_type_str.name.is_empty() {
            None
        } else {
            Some(self.type_name_to_path(&ctor.from_type_str, scope_id)?)
        };

        let return_type = if ctor.return_type_str.name.is_empty() {
            None
        } else {
            Some(self.type_name_to_path(&ctor.return_type_str, scope_id)?)
        };

        Ok(HirCtorDef {
            name,
            generic_params,
            from_type,
            return_type,
            is_pub_external: ctor.visibility == Visibility::PublicExternal,
            span: ctor.span.clone(),
        })
    }

    /// 将名字提升为 HirTypeName
    fn lower_plain_type_name(
        &self,
        name: &str,
        scope_id: ScopeId,
    ) -> Result<HirTypeName, DiagMsg> {
        let sym = self.lookup_symbol(scope_id, name).unwrap();
        Ok(HirTypeName {
            name: HirName {
                name: sym.name.clone(),
                sym_id: sym.sym_id,
            },
            args: vec![],
        })
    }

    /// Lower 表达式
    fn lower_expr(
        &mut self,
        expr_id: usize,
        scope_id: ScopeId,
    ) -> Result<HirExprId, DiagMsg> {
        let expr_node = self.ast_crate.expr_pool[expr_id].clone();
        let hir_id = self.hir.hir_expr_pool.len();
        let span = expr_node.span;

        let kind = match &expr_node.kind {
            ExprNodeKind::Atom { expr: atom } => match atom {

                AtomExprNode::Decimal { dec, .. } =>
                    HirExprKind::Lit(HirLit::Decimal(dec.clone())),

                AtomExprNode::Int { int, .. } =>
                    HirExprKind::Lit(HirLit::Int(int.clone())),

                AtomExprNode::Str { string, .. } =>
                    HirExprKind::Lit(HirLit::Str(string.clone())),

                AtomExprNode::Name { name, .. } => {
                    let sym = self.lookup_symbol(scope_id, name).unwrap();
                    HirExprKind::Ident(HirName {
                        name: sym.name.clone(),
                        sym_id: sym.sym_id,
                    })
                }
                AtomExprNode::Tuple { exprs, .. } => {
                    let elements = exprs
                        .iter()
                        .map(|&e| self.lower_expr(e, scope_id))
                        .collect::<Result<_, _>>()?;
                    HirExprKind::Tuple { elements }
                }
                AtomExprNode::Ellipsis { .. } => HirExprKind::Ellipsis,
            },

            ExprNodeKind::Binary { left, right, op } => {
                let left_id = self.lower_expr(*left, scope_id)?;
                let right_id = self.lower_expr(*right, scope_id)?;
                let op = Self::convert_binop(op);
                HirExprKind::Binary {
                    left: left_id,
                    right: right_id,
                    op,
                }
            }
            ExprNodeKind::Unary { op, right } => {
                let right_id = self.lower_expr(*right, scope_id)?;
                let op = Self::convert_unary(op)?;
                HirExprKind::Unary {
                    op,
                    right: right_id,
                }
            }
            ExprNodeKind::Move { target } => {
                let target_id = self.lower_expr(*target, scope_id)?;
                HirExprKind::Move { target: target_id }
            }
            ExprNodeKind::Copy { target } => {
                let target_id = self.lower_expr(*target, scope_id)?;
                HirExprKind::Copy { target: target_id }
            }
            ExprNodeKind::Ref { target } => {
                let target_id = self.lower_expr(*target, scope_id)?;
                HirExprKind::Ref { target: target_id }
            }
            ExprNodeKind::MutRef { target } => {
                let target_id = self.lower_expr(*target, scope_id)?;
                HirExprKind::MutRef { target: target_id }
            }
            ExprNodeKind::Share { target } => {
                let target_id = self.lower_expr(*target, scope_id)?;
                HirExprKind::Share { target: target_id }
            }
            ExprNodeKind::Call { callee, args } => {
                let callee_id = self.lower_expr(*callee, scope_id)?;
                let arg_ids = args
                    .iter()
                    .map(|&a| self.lower_expr(a, scope_id))
                    .collect::<Result<_, _>>()?;
                HirExprKind::Call {
                    callee: callee_id,
                    args: arg_ids,
                }
            }
            ExprNodeKind::UnsafeExternalCall { callee, args } => {
                let callee_id = self.lower_expr(*callee, scope_id)?;
                let arg_ids = args
                    .iter()
                    .map(|&a| self.lower_expr(a, scope_id))
                    .collect::<Result<_, _>>()?;
                HirExprKind::UnsafeExternalCall {
                    callee: callee_id,
                    args: arg_ids,
                }
            }
            ExprNodeKind::Member { left, right } => {
                let obj_id = self.lower_expr(*left, scope_id)?;
                HirExprKind::FieldAccess {
                    obj: obj_id,
                    field: right.clone(),
                }
            }
            ExprNodeKind::TypeCast {
                expr,
                into_type_str,
            } => {
                let expr_id = self.lower_expr(*expr, scope_id)?;
                let type_ann = self.type_name_to_path(into_type_str, scope_id)?;
                HirExprKind::TypeCast {
                    expr: expr_id,
                    type_ann,
                }
            }
            ExprNodeKind::Do { exprs } => {
                let do_scope = self
                    .hir
                    .name_pass_result
                    .do_scope_map
                    .get(&expr_id)
                    .copied()
                    .unwrap_or(scope_id);
                let stmts = exprs
                    .iter()
                    .map(|&e| self.lower_expr(e, do_scope))
                    .collect::<Result<_, _>>()?;
                HirExprKind::Block { stmts }
            }
            ExprNodeKind::Let {
                name,
                expr,
                type_str,
                mutable,
            } => {
                let init_id = self.lower_expr(*expr, scope_id)?;
                let sym = self.lookup_symbol(scope_id, name).unwrap();
                let var_name = HirName {
                    name: sym.name.clone(),
                    sym_id: sym.sym_id,
                };
                let type_ann = if type_str.name.is_empty() {
                    None
                } else {
                    Some(self.type_name_to_path(type_str, scope_id)?)
                };
                HirExprKind::Let {
                    name: var_name,
                    type_ann,
                    init: init_id,
                    mutable: *mutable,
                }
            }
            ExprNodeKind::If {
                cond,
                then_expr,
                elifs,
                else_expr,
            } => {
                let cond_id = self.lower_expr(*cond, scope_id)?;
                let then_id = self.lower_expr(*then_expr, scope_id)?;
                let elifs_ids = elifs
                    .iter()
                    .map(|elif| {
                        let c = self.lower_expr(elif.cond, scope_id)?;
                        let b = self.lower_expr(elif.body, scope_id)?;
                        Ok((c, b))
                    })
                    .collect::<Result<_, DiagMsg>>()?;
                let else_opt = if let Some(else_id) = else_expr {
                    Some(self.lower_expr(*else_id, scope_id)?)
                } else {
                    None
                };
                HirExprKind::If {
                    cond: cond_id,
                    then: then_id,
                    elifs: elifs_ids,
                    else_opt,
                }
            },
            ExprNodeKind::Return { expr } => {
                if expr.is_none() {
                    HirExprKind::Return {
                        expr: None,
                    }
                } else {
                    HirExprKind::Return {
                        expr: Some(self.lower_expr(expr.unwrap(), scope_id)?),
                    }
                }
            }
        };

        let hir_expr = HirExpr {
            kind,
            hir_id,
            span,
        };
        self.hir.hir_expr_pool.push(hir_expr);
        Ok(hir_id)
    }

    fn convert_binop(op: &Operator) -> HirBinOp {
        match op {
            Operator::Add => HirBinOp::Add,
            Operator::Sub => HirBinOp::Sub,
            Operator::Mul => HirBinOp::Mul,
            Operator::Div => HirBinOp::Div,
            Operator::Mod => HirBinOp::Mod,
            Operator::And => HirBinOp::And,
            Operator::Or => HirBinOp::Or,
            Operator::Eq => HirBinOp::Eq,
            Operator::Neq => HirBinOp::Neq,
            Operator::Lt => HirBinOp::Lt,
            Operator::Gt => HirBinOp::Gt,
            Operator::Le => HirBinOp::Le,
            Operator::Ge => HirBinOp::Ge,
            _ => unreachable!(),
        }
    }

    fn convert_unary(op: &Operator) -> Result<HirUnaryOp, DiagMsg> {
        match op {
            Operator::Not => Ok(HirUnaryOp::Not),
            Operator::Sub => Ok(HirUnaryOp::Neg),
            _ => unreachable!(),
        }
    }
}

impl<'a> HirLowerApi<'a> for HirLower<'a> {
    fn new(
        ast_module: &'a CrateAst,
        name_pass_result: NamePassResult,
        module_name: String,
    ) -> Self {


        Self {
            ast_crate: ast_module,
            hir: HirCrate {
                name: module_name,
                main_fun: None,
                hir_expr_pool: vec![],
                hir_decl_pool: vec![],
                pub_decl_ids: vec![],
                type_map: HashMap::new(),
                name_pass_result,
            },
        }
    }

    fn lower(mut self) -> Result<HirCrate, DiagMsg> {
        let mut top_decls: Vec<(usize, &DeclNode)> = Vec::new();

        for decl in &self.ast_crate.decl_pool {
            if let DeclNodeKind::FileUnit { top_decls: inner } = &decl.kind {
                for &inner_id in inner {
                    top_decls.push((inner_id, &self.ast_crate.decl_pool[inner_id]));
                }
            }
        }

        for (decl_id, decl) in top_decls {
            let hir_id = self.lower_decl(decl_id)?;
            if decl.visibility == Visibility::PublicExternal {
                self.hir.pub_decl_ids.push(hir_id);
            }
        }

        Ok(self.hir)
    }
}