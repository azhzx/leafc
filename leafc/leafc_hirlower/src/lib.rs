use std::collections::HashMap;
use std::sync::Arc;
use leafc_coreapi::ast::{AtomExprNode, CrateAst, DeclRedNode, ExprRedNode, FieldRedNode, FileRedUnit, GreenChild, GreenDecl, GreenDeclKind, GreenExpr, GreenExprKind, GreenElseIf, GreenField, GreenGenericVar, GreenParam, Operator, TypeNameString, Visibility, GreenCtor, GreenMethodDecl, HasTextLen, IdentName};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::{
    HirBinOp, HirCtorDef, HirDecl, HirDeclId, HirDeclKind, HirCrate, HirExpr,
    HirExprId, HirExprKind, HirFieldDef, HirGenericParam, HirLit, HirMethodDecl,
    HirName, HirParam, HirTypeName, HirUnaryOp,
};
use leafc_coreapi::hir_lower::HirLowerApi;
use leafc_coreapi::name_pass::{NamePassResult, FunScopeMap, DoScopeMap};
use leafc_coreapi::scope::{Scope, ScopeId, ScopePool, Symbol, SymbolKind};
use leafc_coreapi::source::Span;


pub struct HirLower<'a> {
    ast_crate: &'a CrateAst,
    name_pass_result: NamePassResult,
    hir: HirCrate,
}

impl<'a> HirLower<'a> {
    fn child_expr_red(parent_span: &Span, child: &GreenChild<GreenExpr>) -> ExprRedNode {
        let start = parent_span.start_off + child.relative_start;
        let len = child.node.text_len;
        ExprRedNode {
            span: Span {
                source_id: parent_span.source_id,
                start_off: start,
                end_off: start + len,
            },
            inner: Arc::clone(&child.node),
        }
    }

    fn child_decl_red(parent_span: &Span, child: &GreenChild<GreenDecl>) -> DeclRedNode {
        let start = parent_span.start_off + child.relative_start;
        let len = child.node.text_len;
        DeclRedNode {
            span: Span {
                source_id: parent_span.source_id,
                start_off: start,
                end_off: start + len,
            },
            inner: Arc::clone(&child.node),
        }
    }

    /// 计算 GreenChild 的绝对 Span
    fn child_span<T>(base: &Span, child: &GreenChild<T>) -> Span
    where T: HasTextLen {

        let start = base.start_off + child.relative_start;
        let len = child.node.text_len();
        Span {
            source_id: base.source_id,
            start_off: start,
            end_off: start + len,
        }
    }

    /// 符号查找
    fn lookup_symbol(&self, scope_id: ScopeId, name: &str) -> Option<&Symbol> {
        self.name_pass_result.pool.lookup(scope_id, name)
            .map(|(sym, _)| sym)
    }

    /// 类型名
    fn type_name_to_path(
        &self,
        type_name: &TypeNameString,
        scope_id: ScopeId,
        span: Span,
    ) -> Result<HirTypeName, DiagMsg> {
        let sym = self.lookup_symbol(scope_id, &type_name.name)
            .ok_or_else(|| DiagMsg {
                title: "TypeNotFound".into(),
                msg: format!("type `{}` not found", type_name.name),
                span: span.clone(),
            })?;
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };

        let args = type_name.generics
            .iter()
            .map(|t| self.type_name_to_path(t, scope_id, span.clone())) // 泛型参数共享父类型名的 span
            .collect::<Result<_, _>>()?;

        Ok(HirTypeName { name, args })
    }

    /// 泛型参数
    fn lower_generic_params(
        &self,
        generic_vars: &[GreenChild<GreenGenericVar>],
        scope_id: ScopeId,
        parent_span: &Span,
    ) -> Result<Vec<HirGenericParam>, DiagMsg> {
        generic_vars
            .iter()
            .map(|gv_child| {
                let gv = &gv_child.node;
                let gv_span = Self::child_span(parent_span, gv_child);
                let sym = self.lookup_symbol(scope_id, &gv.name.node.name)
                    .ok_or_else(|| DiagMsg {
                        title: "GenericNotFound".into(),
                        msg: format!("generic `{}` not found", gv.name.node.name),
                        span: gv_span.clone(),
                    })?;
                let name = HirName {
                    name: sym.name.clone(),
                    sym_id: sym.sym_id,
                };
                let constraints = gv.constraint
                    .iter()
                    .map(|c| {
                        let c_span = Self::child_span(parent_span, c);
                        self.type_name_to_path(&c.node, scope_id, c_span)
                    })
                    .collect::<Result<_, _>>()?;
                Ok(HirGenericParam { name, constraints })
            })
            .collect()
    }

    /// 字段
    fn lower_field_def(
        &self,
        field: &GreenChild<GreenField>,
        scope_id: ScopeId,
        parent_span: &Span,
    ) -> Result<HirFieldDef, DiagMsg> {
        let green_field = &field.node;
        let field_span = Self::child_span(parent_span, field);
        let type_span = Self::child_span(&field_span, &green_field.type_str);
        let type_ann = self.type_name_to_path(&green_field.type_str.node, scope_id, type_span)?;
        let sym = self.lookup_symbol(scope_id, &green_field.name.node.name)
            .ok_or_else(|| DiagMsg {
                title: "FieldNotFound".into(),
                msg: format!("field `{}` not found", green_field.name.node.name),
                span: field_span.clone(),
            })?;
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };
        Ok(HirFieldDef {
            name,
            type_ann,
            span: field_span,
        })
    }

    /// 构造子
    fn lower_ctor_def(
        &self,
        ctor: &GreenChild<GreenCtor>,
        scope_id: ScopeId,
        parent_span: &Span,
    ) -> Result<HirCtorDef, DiagMsg> {
        let green_ctor = &ctor.node;
        let ctor_span = Self::child_span(parent_span, ctor);
        let ctor_name = green_ctor.name.node.as_ref();
        let sym = self.lookup_symbol(scope_id, &*ctor_name.name)
            .ok_or_else(|| DiagMsg {
                title: "CtorNotFound".into(),
                msg: format!("constructor `{}` not found", ctor_name.name),
                span: ctor_span.clone(),
            })?;
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };

        let generic_params = self.lower_generic_params(&green_ctor.generic_vars, scope_id, &ctor_span)?;

        let from_type = if green_ctor.from_type_str.node.name.is_empty() {
            None
        } else {
            let from_span = Self::child_span(&ctor_span, &green_ctor.from_type_str);
            Some(self.type_name_to_path(&green_ctor.from_type_str.node, scope_id, from_span)?)
        };

        let return_type = if green_ctor.return_type_str.node.name.is_empty() {
            None
        } else {
            let ret_span = Self::child_span(&ctor_span, &green_ctor.return_type_str);
            Some(self.type_name_to_path(&green_ctor.return_type_str.node, scope_id, ret_span)?)
        };

        Ok(HirCtorDef {
            name,
            generic_params,
            from_type,
            return_type,
            is_pub_external: green_ctor.visibility == Visibility::PublicExternal,
            span: ctor_span,
        })
    }

    /// 方法声明
    fn lower_method_decl(
        &self,
        method: &GreenChild<GreenMethodDecl>,
        scope_id: ScopeId,       // 抽象作用域
        parent_span: &Span,
    ) -> Result<HirMethodDecl, DiagMsg> {
        let green_method = &method.node;
        let method_span = Self::child_span(parent_span, method);
        let method_name = green_method.name.node.as_ref();
        let sym = self.lookup_symbol(scope_id, &*method_name.name)
            .ok_or_else(|| DiagMsg {
                title: "MethodNotFound".into(),
                msg: format!("method `{}` not found", method_name.name),
                span: method_span.clone(),
            })?;
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };

        let params = green_method.params
            .iter()
            .map(|p| self.lower_param(p, scope_id, &method_span))
            .collect::<Result<_, _>>()?;

        let return_type = if green_method.return_type_str.node.name.is_empty() {
            None
        } else {
            let ret_span = Self::child_span(&method_span, &green_method.return_type_str);
            Some(self.type_name_to_path(&green_method.return_type_str.node, scope_id, ret_span)?)
        };

        Ok(HirMethodDecl {
            name,
            generic_params: vec![],   // 未来扩展
            params,
            return_type,
            is_pub_external: green_method.visibility == Visibility::PublicExternal,
            span: method_span,
        })
    }

    /// 参数
    fn lower_param(
        &self,
        param: &GreenChild<GreenParam>,
        scope_id: ScopeId,
        parent_span: &Span,
    ) -> Result<HirParam, DiagMsg> {
        let green_param = &param.node;
        let param_span = Self::child_span(parent_span, param);
        let sym = self.lookup_symbol(scope_id, &green_param.name.node.name)
            .ok_or_else(|| DiagMsg {
                title: "ParamNotFound".into(),
                msg: format!("parameter `{}` not found", green_param.name.node.name),
                span: param_span.clone(),
            })?;
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };
        let type_ann = if green_param.type_str.node.name.is_empty() {
            None
        } else {
            let type_span = Self::child_span(&param_span, &green_param.type_str);
            Some(self.type_name_to_path(&green_param.type_str.node, scope_id, type_span)?)
        };
        Ok(HirParam { name, type_ann, span: param_span })
    }

    /// 表达式
    fn lower_expr(
        &mut self,
        expr_red: &ExprRedNode,
        scope_id: ScopeId,
    ) -> Result<HirExprId, DiagMsg> {
        let span = expr_red.span.clone();

        let kind = match &expr_red.inner.kind {
            GreenExprKind::Atom { expr: atom } => match atom {
                AtomExprNode::Decimal { dec, .. } =>
                    HirExprKind::Lit(HirLit::Decimal(dec.clone())),
                AtomExprNode::Int { int, .. } =>
                    HirExprKind::Lit(HirLit::Int(int.clone())),
                AtomExprNode::Str { string, .. } =>
                    HirExprKind::Lit(HirLit::Str(string.clone())),
                AtomExprNode::Name { name, .. } => {
                    let sym = self.lookup_symbol(scope_id, &*name)
                        .ok_or_else(|| DiagMsg {
                            title: "NameNotFound".into(),
                            msg: format!("name `{}` not found", name),
                            span: span.clone(),
                        })?;
                    HirExprKind::Ident(HirName {
                        name: sym.name.clone(),
                        sym_id: sym.sym_id,
                    })
                }
                AtomExprNode::Tuple { exprs, .. } => {
                    let elements = exprs
                        .iter()
                        .map(|e| self.lower_expr(&Self::child_expr_red(&span, e), scope_id))
                        .collect::<Result<_, _>>()?;
                    HirExprKind::Tuple { elements }
                }
                AtomExprNode::Ellipsis { .. } => HirExprKind::Ellipsis,
            },

            GreenExprKind::Binary { left, op, right } => {
                let left_id = self.lower_expr(&expr_red.child_to_red(&left), scope_id)?;
                let right_id = self.lower_expr(&expr_red.child_to_red(&right), scope_id)?;
                let op = Self::convert_binop(&op.node);
                HirExprKind::Binary { left: left_id, right: right_id, op }
            }

            GreenExprKind::Unary { op, right } => {
                let right_id = self.lower_expr(&expr_red.child_to_red(&right), scope_id)?;
                let op = Self::convert_unary(&op.node)?;
                HirExprKind::Unary { op, right: right_id }
            }

            GreenExprKind::Move { target } => {
                let target_id = self.lower_expr(&expr_red.child_to_red(&target), scope_id)?;
                HirExprKind::Move { target: target_id }
            }
            GreenExprKind::Copy { target } => {
                let target_id = self.lower_expr(&expr_red.child_to_red(&target), scope_id)?;
                HirExprKind::Copy { target: target_id }
            }
            GreenExprKind::Ref { target } => {
                let target_id = self.lower_expr(&expr_red.child_to_red(&target), scope_id)?;
                HirExprKind::Ref { target: target_id }
            }
            GreenExprKind::MutRef { target } => {
                let target_id = self.lower_expr(&expr_red.child_to_red(&target), scope_id)?;
                HirExprKind::MutRef { target: target_id }
            }
            GreenExprKind::Share { target } => {
                let target_id = self.lower_expr(&expr_red.child_to_red(&target), scope_id)?;
                HirExprKind::Share { target: target_id }
            }

            GreenExprKind::Call { callee, args } => {
                let callee_id = self.lower_expr(&expr_red.child_to_red(&callee), scope_id)?;
                let arg_ids = args
                    .iter()
                    .map(|a| self.lower_expr(&expr_red.child_to_red(a), scope_id))
                    .collect::<Result<_, _>>()?;
                HirExprKind::Call { callee: callee_id, args: arg_ids }
            }

            GreenExprKind::UnsafeExternalCall { callee, args } => {
                let callee_id = self.lower_expr(&expr_red.child_to_red(&callee), scope_id)?;
                let arg_ids = args
                    .iter()
                    .map(|a| self.lower_expr(&expr_red.child_to_red(a), scope_id))
                    .collect::<Result<_, _>>()?;
                HirExprKind::UnsafeExternalCall { callee: callee_id, args: arg_ids }
            }

            GreenExprKind::Member { left, right } => {
                let obj_id = self.lower_expr(&expr_red.child_to_red(&left), scope_id)?;
                HirExprKind::FieldAccess {
                    obj: obj_id,
                    field: right.node.as_ref().clone().name,
                }
            }

            GreenExprKind::TypeCast { expr: e, into_type } => {
                // into_type 目前是 GreenChild<GreenExpr>，待后续解析为类型
                todo!("TypeCast into_type lowering not yet implemented")
            }

            GreenExprKind::Do { exprs } => {
                let do_scope = self.name_pass_result.do_scope_map
                    .get(&expr_red.inner)
                    .copied()
                    .unwrap_or(scope_id);
                let stmts = exprs
                    .iter()
                    .map(|e| self.lower_expr(&expr_red.child_to_red(e), do_scope))
                    .collect::<Result<_, _>>()?;
                HirExprKind::Block { stmts }
            }

            GreenExprKind::Let { name, expr: e, type_str, mutable } => {
                let init_id = self.lower_expr(&expr_red.child_to_red(&e), scope_id)?;
                let sym = self.lookup_symbol(scope_id, &name.node.name)
                    .ok_or_else(|| DiagMsg {
                        title: "LetNameNotFound".into(),
                        msg: format!("variable `{}` not found", name.node.name),
                        span: span.clone(),
                    })?;
                let var_name = HirName {
                    name: sym.name.clone(),
                    sym_id: sym.sym_id,
                };
                let type_ann = if let Some(ts) = type_str {
                    let ts_span = Self::child_span(&span, &ts);
                    Some(self.type_name_to_path(&ts.node, scope_id, ts_span)?)
                } else {
                    None
                };
                HirExprKind::Let {
                    name: var_name,
                    type_ann,
                    init: init_id,
                    mutable: *mutable,
                }
            }

            GreenExprKind::If { cond, then_expr, elifs, else_expr } => {
                let cond_id = self.lower_expr(&expr_red.child_to_red(&cond), scope_id)?;
                let then_id = self.lower_expr(&expr_red.child_to_red(&then_expr), scope_id)?;
                let elifs_ids = elifs
                    .iter()
                    .map(|elif| {
                        let c = self.lower_expr(&expr_red.child_to_red(&elif.cond), scope_id)?;
                        let b = self.lower_expr(&expr_red.child_to_red(&elif.body), scope_id)?;
                        Ok((c, b))
                    })
                    .collect::<Result<_, DiagMsg>>()?;
                let else_opt = if let Some(else_e) = else_expr {
                    Some(self.lower_expr(&expr_red.child_to_red(&else_e), scope_id)?)
                } else {
                    None
                };
                HirExprKind::If {
                    cond: cond_id,
                    then: then_id,
                    elifs: elifs_ids,
                    else_opt,
                }
            }

            GreenExprKind::Return { expr: opt_expr } => {
                let expr_id = if let Some(e) = opt_expr {
                    Some(self.lower_expr(&expr_red.child_to_red(&e), scope_id)?)
                } else {
                    None
                };
                HirExprKind::Return { expr: expr_id }
            }
        };

        let hir_id = self.hir.hir_expr_pool.len();
        let hir_expr = HirExpr { kind, hir_id, span };
        self.hir.hir_expr_pool.push(hir_expr);
        Ok(hir_id)
    }

    /// 声明
    fn lower_decl(&mut self, decl_red: &DeclRedNode, file_scope_id: ScopeId) -> Result<HirDeclId, DiagMsg> {
        let decl = &decl_red.inner;
        let hir_id = self.hir.hir_decl_pool.len();
        let span = decl_red.span.clone();
        let ident = decl.name.node.as_ref().clone();
        let is_pub_external = decl.visibility == Visibility::PublicExternal;

        // 获取声明自身的作用域
        let decl_scope = self.find_decl_scope(Arc::clone(&decl_red.inner))
            .unwrap_or_else(|| {
                panic!("decl scope not found for {:?}", ident)
            });

        let kind = match &decl.kind {
            GreenDeclKind::Fun { params, block, return_type_str } => {
                let scope_id = decl_scope;
                let generic_params = vec![];

                let hir_params = params
                    .iter()
                    .map(|p| self.lower_param(p, scope_id, &span))
                    .collect::<Result<_, _>>()?;

                let return_type = if return_type_str.node.name.is_empty() {
                    None
                } else {
                    let ret_span = Self::child_span(&span, return_type_str);
                    Some(self.type_name_to_path(&return_type_str.node, scope_id, ret_span)?)
                };

                let body = block
                    .iter()
                    .map(|stmt| self.lower_expr(&Self::child_expr_red(&span, stmt), scope_id))
                    .collect::<Result<_, _>>()?;

                HirDeclKind::Fun {
                    generic_params,
                    params: hir_params,
                    return_type,
                    body,
                }
            }

            GreenDeclKind::FunDecl { params, return_type_str } => {
                todo!()
            }

            GreenDeclKind::Abstract { super_abst: has_abst, generic_vars, methods } => {
                let scope_id = decl_scope;
                let generic_params = self.lower_generic_params(generic_vars, scope_id, &span)?;
                let super_abstracts = has_abst
                    .iter()
                    .map(|name_child| {
                        let name_span = Self::child_span(&span, name_child);
                        self.lower_plain_type_name(&name_child.node, scope_id, name_span)
                    })
                    .collect::<Result<_, _>>()?;
                let hir_methods = methods
                    .iter()
                    .map(|m| self.lower_method_decl(m, scope_id, &span))
                    .collect::<Result<_, _>>()?;
                HirDeclKind::Abstract {
                    generic_params,
                    methods: hir_methods,
                    super_abstracts,
                }
            }

            GreenDeclKind::TypeStruct { fields, has_abst, generic_vars } => {
                let scope_id = decl_scope;
                let generic_params = self.lower_generic_params(generic_vars, scope_id, &span)?;
                let hir_fields = fields
                    .iter()
                    .map(|f| self.lower_field_def(f, scope_id, &span))
                    .collect::<Result<_, _>>()?;
                let implemented_abstracts = has_abst
                    .iter()
                    .map(|name_child| {
                        let name_span = Self::child_span(&span, name_child);
                        self.lower_plain_type_name(&name_child.node, scope_id, name_span)
                    })
                    .collect::<Result<_, _>>()?;
                HirDeclKind::Struct {
                    generic_params,
                    fields: hir_fields,
                    implemented_abstracts,
                }
            }

            GreenDeclKind::TypeAlias { ref_to, has_abst, generic_vars } => {
                let scope_id = decl_scope;
                let generic_params = self.lower_generic_params(generic_vars, scope_id, &span)?;
                let alias_span = Self::child_span(&span, ref_to);
                let alias_for = self.type_name_to_path(&ref_to.node, scope_id, alias_span)?;
                HirDeclKind::TypeAlias {
                    generic_params,
                    alias_for,
                }
            }

            GreenDeclKind::ADT { has_abst, generic_vars, ctors } => {
                let scope_id = decl_scope;
                let generic_params = self.lower_generic_params(generic_vars, scope_id, &span)?;
                let implemented_abstracts = has_abst
                    .iter()
                    .map(|name_child| {
                        let name_span = Self::child_span(&span, name_child);
                        self.lower_plain_type_name(&name_child.node, scope_id, name_span)
                    })
                    .collect::<Result<_, _>>()?;
                let hir_ctors = ctors
                    .iter()
                    .map(|c| self.lower_ctor_def(c, file_scope_id, &span))  // 构造函数符号在文件作用域中
                    .collect::<Result<_, _>>()?;
                HirDeclKind::ADT {
                    generic_params,
                    ctors: hir_ctors,
                    implemented_abstracts,
                }
            }

            GreenDeclKind::CType => HirDeclKind::CType,

            GreenDeclKind::External { sym_name, params, return_type_str } => {
                let scope_id = decl_scope;
                let hir_params = params
                    .iter()
                    .map(|p| self.lower_param(p, scope_id, &span))
                    .collect::<Result<_, _>>()?;
                let return_type = {
                    let ret_span = Self::child_span(&span, return_type_str);
                    self.type_name_to_path(&return_type_str.node, scope_id, ret_span)?
                };
                HirDeclKind::External {
                    sym_name: sym_name.node.as_ref().clone().name,
                    params: hir_params,
                    return_type,
                }
            }

            GreenDeclKind::TypeDecl => {
                todo!()
            }
        };

        let hir_decl = HirDecl {
            ident: ident.name,
            kind,
            is_pub_external,
            hir_id,
            span,
        };
        self.hir.hir_decl_pool.push(hir_decl);
        Ok(hir_id)
    }

    fn lower_plain_type_name(
        &self,
        name: &Arc<IdentName>,
        scope_id: ScopeId,
        span: Span,
    ) -> Result<HirTypeName, DiagMsg> {
        let sym = self.lookup_symbol(scope_id, &*name.name)
            .ok_or_else(|| DiagMsg {
                title: "NameNotFound".into(),
                msg: format!("name `{}` not found", name.name),
                span: span.clone(),
            })?;
        Ok(HirTypeName {
            name: HirName {
                name: sym.name.clone(),
                sym_id: sym.sym_id,
            },
            args: vec![],
        })
    }

    fn find_decl_scope(&self, decl: Arc<GreenDecl>) -> Option<ScopeId> {
        self.name_pass_result.pool.decl_node_scope_map.get(&decl).copied()
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
            _ => panic!("unexpected binary operator"),
        }
    }

    fn convert_unary(op: &Operator) -> Result<HirUnaryOp, DiagMsg> {
        match op {
            Operator::Not => Ok(HirUnaryOp::Not),
            Operator::Sub => Ok(HirUnaryOp::Neg),
            _ => panic!("unexpected unary operator"),
        }
    }
}

impl<'a> HirLowerApi<'a> for HirLower<'a> {
    fn new(
        ast_module: &'a CrateAst,
        name_pass_result: NamePassResult,
        module_name: String,
    ) -> Self {
        HirLower {
            ast_crate: ast_module,
            name_pass_result,
            hir: HirCrate {
                name: module_name,
                main_fun: None,
                hir_expr_pool: vec![],
                hir_decl_pool: vec![],
                pub_decl_ids: vec![],
                type_pool: vec![],
                name_pass_result: None,
            },
        }
    }

    fn lower(mut self) -> Result<HirCrate, DiagMsg> {
        for file_unit in &self.ast_crate.file_units {
            let file_source_id = file_unit.span.source_id;
            // 获取文件作用域
            let file_scope_id = self.name_pass_result.source_id_to_scope
                .get(&file_source_id)
                .copied()
                .expect("file scope not found");

            for decl_child in &file_unit.green.top_decls {
                let decl_red = HirLower::child_decl_red(&file_unit.span, decl_child);
                let hir_id = self.lower_decl(&decl_red, file_scope_id)?;
                if decl_child.node.visibility == Visibility::Public
                    || decl_child.node.visibility == Visibility::PublicExternal
                {
                    self.hir.pub_decl_ids.push(hir_id);
                }
                // 查找 main 函数
                if decl_child.node.name.node.as_ref().name == "main" {
                    if let GreenDeclKind::Fun { .. } = &decl_child.node.kind {
                        self.hir.main_fun = Some(hir_id);
                    }
                }
            }
        }
        self.hir.name_pass_result = Some(self.name_pass_result);
        Ok(self.hir)
    }
}