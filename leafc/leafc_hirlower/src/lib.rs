use std::collections::{HashMap, HashSet};
use leafc_coreapi::ast::{child_decl_red, child_expr_red, child_span_of, AtomExprNode, CrateAst, DeclRedNode, ExprRedNode, GreenCatchClause, GreenChild, GreenCtor, GreenDecl, GreenDeclKind, GreenExpr, GreenExprKind, GreenField, GreenGenericVar, GreenMatchArm, GreenMethodDecl, GreenParam, GreenPattern, GreenPureStaticPath, GreenWhereClause, HasTextLen, TypeName, Visibility};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::{HirBinOp, HirCatchClause, HirCrate, HirCtorDef, HirDecl, HirDeclId, HirDeclKind, HirExpr, HirExprId, HirExprKind, HirFieldDef, HirGenericParam, HirLit, HirMatchArm, HirMethodDecl, HirName, HirParam, HirPattern, HirStructPatternField, HirTypeName, HirUnaryOp};
use leafc_coreapi::hir_lower::{HirLowerApi, HirLowerError};
use leafc_coreapi::name_pass::NamePassResult;
use leafc_coreapi::operators::Operator;
use leafc_coreapi::scope::{ScopeId, SymId, SymbolKind};
use leafc_coreapi::source::Span;
use std::sync::Arc;

pub struct HirLower<'a> {
    ast_crate: &'a CrateAst,
    name_pass_result: NamePassResult,
    hir: HirCrate,
    /// Function Name SymId => ParamNames
    param_names_map: HashMap<SymId, Vec<String>>,
}

impl<'a> HirLower<'a> {
    fn lookup_symbol(&self, scope_id: ScopeId, name: &str) -> Option<&leafc_coreapi::scope::Symbol> {
        self.name_pass_result.pool.lookup(scope_id, name)
            .map(|(sym, _)| sym)
    }


    fn resolve_static_path(
        &self,
        path: &GreenPureStaticPath,
        scope_id: ScopeId,
        span: &Span,
    ) -> Result<HirName, DiagMsg> {

        let segments = &path.segments;
        if segments.is_empty() {
            return Err(DiagMsg {
                title: format!("{:?}", HirLowerError::EmptyPath),
                msg: "empty path in expression".into(),
                span: span.clone(),
            });
        }

        let first = &segments[0];
        let first_name = &first.node.name;
        let first_span = child_span_of(span, first);
        let (mut sym, mut current_scope) = self
            .name_pass_result
            .pool
            .lookup(scope_id, first_name)
            .ok_or_else(|| DiagMsg {
                title: format!("{:?}", HirLowerError::PathNotFound),
                msg: format!("name `{}` not found", first_name),
                span: first_span.clone(),
            })?;

        for seg in segments.iter().skip(1) {
            let seg_name = &seg.node.name;
            let seg_span = child_span_of(span, seg);
            match &sym.kind {
                SymbolKind::File { source_id } => {
                    let file_scope = *self.name_pass_result.source_id_to_scope.get(source_id)
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", HirLowerError::ModuleScopeNotFound),
                            msg: format!("module scope not found for source_id {:?}", source_id),
                            span: seg_span.clone(),
                        })?;
                    let (found_sym, found_scope) = self.name_pass_result.pool
                        .lookup(file_scope, seg_name)
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", HirLowerError::PathNotFound),
                            msg: format!("name `{}` not found in module", seg_name),
                            span: seg_span.clone(),
                        })?;
                    sym = found_sym;
                    current_scope = found_scope;
                }
                SymbolKind::Struct { fields } => {
                    let field_sym = fields
                        .iter()
                        .filter_map(|&sid| self.name_pass_result.pool.get_symbol_by_id(sid))
                        .find(|s| s.name == *seg_name)
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", HirLowerError::FieldNotFound),
                            msg: format!("struct `{}` has no field named `{}`", sym.name, seg_name),
                            span: seg_span.clone(),
                        })?;
                    sym = field_sym;
                }
                SymbolKind::ADT { constructors } => {
                    let ctor_sym = constructors
                        .iter()
                        .filter_map(|&sid| self.name_pass_result.pool.get_symbol_by_id(sid))
                        .find(|s| s.name == *seg_name)
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", HirLowerError::ConstructorNotFound),
                            msg: format!("ADT `{}` has no constructor named `{}`", sym.name, seg_name),
                            span: seg_span.clone(),
                        })?;
                    sym = ctor_sym;
                }
                SymbolKind::Effect { scope_id } => {
                    let effect_scope = *scope_id;
                    let (found_sym, found_scope) = self.name_pass_result.pool
                        .lookup(effect_scope, seg_name)
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", HirLowerError::ControlNotFound),
                            msg: format!("effect `{}` has no control named `{}`", sym.name, seg_name),
                            span: seg_span.clone(),
                        })?;
                    sym = found_sym;
                    current_scope = found_scope;
                }
                SymbolKind::Generic => {
                    // 泛型参数不能有后续路径段
                    return Err(DiagMsg {
                        title: format!("{:?}", HirLowerError::InvalidPath),
                        msg: format!("generic parameter `{}` cannot have members", sym.name),
                        span: seg_span.clone(),
                    });
                }
                _ => {
                    return Err(DiagMsg {
                        title: format!("{:?}", HirLowerError::InvalidPath),
                        msg: format!("cannot access member of `{}`", sym.name),
                        span: seg_span.clone(),
                    });
                }
            }
        }

        Ok(HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        })
    }


    fn lower_type_name(
        &self,
        type_name: &TypeName,
        scope_id: ScopeId,
        span: Span,
    ) -> Result<HirTypeName, DiagMsg> {
        match type_name {
            TypeName::Named { path, generics, .. } => {
                let path_span = child_span_of(&span, path);
                let name = self.resolve_static_path(&path.node, scope_id, &path_span)?;
                let args = generics
                    .iter()
                    .map(|t| self.lower_type_name(t, scope_id, span.clone()))
                    .collect::<Result<_, _>>()?;
                Ok(HirTypeName::Named { path: name, generics: args })
            }
            TypeName::Ref { inner, .. } => {
                let inner_span = child_span_of(&span, inner);
                let inner = self.lower_type_name(&inner.node, scope_id, inner_span)?;
                Ok(HirTypeName::Ref(Box::new(inner)))
            }
            TypeName::MutRef { inner, .. } => {
                let inner_span = child_span_of(&span, inner);
                let inner = self.lower_type_name(&inner.node, scope_id, inner_span)?;
                Ok(HirTypeName::MutRef(Box::new(inner)))
            }
            TypeName::Share { inner, .. } => {
                let inner_span = child_span_of(&span, inner);
                let inner = self.lower_type_name(&inner.node, scope_id, inner_span)?;
                Ok(HirTypeName::Share(Box::new(inner)))
            }
            TypeName::Tuple { elements, .. } => {
                let types = elements
                    .iter()
                    .map(|el| {
                        let el_span = child_span_of(&span, &el.ty);
                        self.lower_type_name(&el.ty.node, scope_id, el_span)
                    })
                    .collect::<Result<_, _>>()?;
                Ok(HirTypeName::Tuple(types))
            }
            TypeName::Fun { params, return_type, .. } => {
                let param_types = params
                    .iter()
                    .map(|p| {
                        let p_span = child_span_of(&span, p);
                        self.lower_type_name(&p.node, scope_id, p_span)
                    })
                    .collect::<Result<_, _>>()?;
                let ret_span = child_span_of(&span, return_type);
                let ret = self.lower_type_name(&return_type.node, scope_id, ret_span)?;
                Ok(HirTypeName::Fun {
                    params: param_types,
                    return_type: Box::new(ret),
                })
            }
            TypeName::Impl { trait_type, .. } => {
                let inner_span = child_span_of(&span, trait_type);
                let inner = self.lower_type_name(&trait_type.node, scope_id, inner_span)?;
                Ok(HirTypeName::Impl(Box::new(inner)))
            }
        }
    }


    /// for type ann
    fn is_type_empty(type_name: &TypeName) -> bool {
        match type_name {
            TypeName::Named { path, .. } => path.node.segments.is_empty(),
            _ => false,
        }
    }


    /// generic params
    fn lower_generic_params(
        &self,
        generic_vars: &[GreenChild<GreenGenericVar>],
        where_clause: Option<&GreenWhereClause>,
        scope_id: ScopeId,
        parent_span: &Span,
    ) -> Result<Vec<HirGenericParam>, DiagMsg> {
        generic_vars
            .iter()
            .map(|gv_child| {
                let gv = &gv_child.node;
                let gv_span = child_span_of(parent_span, gv_child);
                let sym = self
                    .lookup_symbol(scope_id, &gv.name.node.name)
                    .ok_or_else(|| DiagMsg {
                        title: format!("{:?}", HirLowerError::GenericNotFound),
                        msg: format!("generic `{}` not found", gv.name.node.name),
                        span: gv_span.clone(),
                    })?;
                let name = HirName {
                    name: sym.name.clone(),
                    sym_id: sym.sym_id,
                };

                let mut constraints: Vec<HirTypeName> = gv
                    .constraint
                    .iter()
                    .map(|c| {
                        let c_span = child_span_of(parent_span, c);
                        self.lower_type_name(&c.node, scope_id, c_span)
                    })
                    .collect::<Result<_, _>>()?;

                // 2. 来自 where 子句的约束
                if let Some(wc) = where_clause {
                    for c_child in &wc.constraints {
                        let c = &c_child.node;
                        if c.name.node.name == gv.name.node.name {
                            let bound_tys: Result<Vec<_>, _> = c.bounds
                                .iter()
                                .map(|b| {
                                    let b_span = child_span_of(parent_span, b);
                                    self.lower_type_name(&b.node, scope_id, b_span)
                                })
                                .collect();
                            constraints.extend(bound_tys?);
                            break;
                        }
                    }
                }

                Ok(HirGenericParam { name, constraints })
            })
            .collect()
    }


    /// pattern
    fn lower_pattern(
        &self,
        pattern: &GreenPattern,
        scope_id: ScopeId,
        span: &Span,
    ) -> Result<HirPattern, DiagMsg> {
        match pattern {
            GreenPattern::Wildcard => Ok(HirPattern::Wildcard),
            GreenPattern::Rest => Ok(HirPattern::Rest),
            GreenPattern::Literal(lit) => {
                let hir_lit = self.lower_lit(lit);
                Ok(HirPattern::Literal(hir_lit))
            }
            GreenPattern::Binding(ident) => {
                let sym = self
                    .lookup_symbol(scope_id, &ident.name)
                    .ok_or_else(|| DiagMsg {
                        title: format!("{:?}", HirLowerError::BindingNotFound),
                        msg: format!("binding `{}` not found", ident.name),
                        span: span.clone(),
                    })?;
                Ok(HirPattern::Binding(HirName {
                    name: sym.name.clone(),
                    sym_id: sym.sym_id,
                }))
            }
            GreenPattern::Constructor { type_name, args, .. } => {
                let type_span = child_span_of(span, type_name);
                let hir_type = self.lower_type_name(&type_name.node, scope_id, type_span)?;
                let hir_args = args
                    .iter()
                    .map(|a| {
                        let a_span = child_span_of(span, a);
                        self.lower_pattern(&a.node, scope_id, &a_span)
                    })
                    .collect::<Result<_, _>>()?;
                Ok(HirPattern::Constructor {
                    type_name: hir_type,
                    args: hir_args,
                    span: span.clone(),
                })
            }
            GreenPattern::Tuple { elements, .. } => {
                let hir_elements = elements
                    .iter()
                    .map(|e| {
                        let e_span = child_span_of(span, e);
                        self.lower_pattern(&e.node, scope_id, &e_span)
                    })
                    .collect::<Result<_, _>>()?;
                Ok(HirPattern::Tuple {
                    elements: hir_elements,
                    span: span.clone(),
                })
            }
            GreenPattern::Struct { path, fields, rest, .. } => {
                let path_span = child_span_of(span, path);
                let hir_path = self.resolve_static_path(&path.node, scope_id, &path_span)?;
                let hir_fields = fields
                    .iter()
                    .map(|f| {
                        let field_pat_span = child_span_of(span, &f.pattern);
                        let pattern = self.lower_pattern(&f.pattern.node, scope_id, &field_pat_span)?;
                        let field_name_sym = self
                            .lookup_symbol(scope_id, &f.field_name.name)
                            .ok_or_else(|| DiagMsg {
                                title: format!("{:?}", HirLowerError::FieldNotFound),
                                msg: format!("field `{}` not found", f.field_name.name),
                                span: field_pat_span.clone(),
                            })?;
                        Ok(HirStructPatternField {
                            field_name: HirName {
                                name: field_name_sym.name.clone(),
                                sym_id: field_name_sym.sym_id,
                            },
                            pattern,
                            span: field_pat_span,
                        })
                    })
                    .collect::<Result<_, _>>()?;
                Ok(HirPattern::Struct {
                    path: hir_path,
                    fields: hir_fields,
                    rest: *rest,
                    span: span.clone(),
                })
            }
            GreenPattern::Alias { pattern, name, .. } => {
                let inner_span = child_span_of(span, pattern);
                let inner = self.lower_pattern(&pattern.node, scope_id, &inner_span)?;
                let sym = self
                    .lookup_symbol(scope_id, &name.name)
                    .ok_or_else(|| DiagMsg {
                        title: format!("{:?}", HirLowerError::BindingNotFound),
                        msg: format!("alias `{}` not found", name.name),
                        span: span.clone(),
                    })?;
                Ok(HirPattern::Alias {
                    pattern: Box::new(inner),
                    name: HirName {
                        name: sym.name.clone(),
                        sym_id: sym.sym_id,
                    },
                    span: span.clone(),
                })
            }
            GreenPattern::Or { left, right, .. } => {
                let left_span = child_span_of(span, left);
                let right_span = child_span_of(span, right);
                let hir_left = self.lower_pattern(&left.node, scope_id, &left_span)?;
                let hir_right = self.lower_pattern(&right.node, scope_id, &right_span)?;
                Ok(HirPattern::Or {
                    left: Box::new(hir_left),
                    right: Box::new(hir_right),
                    span: span.clone(),
                })
            }
        }
    }


    /// literal
    fn lower_lit(&self, atom: &AtomExprNode) -> HirLit {
        match atom {
            AtomExprNode::Decimal { dec, .. } => HirLit::Decimal(dec.clone()),
            AtomExprNode::Int { int, .. } => HirLit::Int(int.clone()),
            AtomExprNode::Str { string, .. } => HirLit::Str(string.clone()),
            _ => unreachable!()
        }
    }


    /// field def
    fn lower_field_def(
        &self,
        field: &GreenChild<GreenField>,
        scope_id: ScopeId,
        parent_span: &Span,
    ) -> Result<HirFieldDef, DiagMsg> {
        let green_field = &field.node;
        let field_span = child_span_of(parent_span, field);
        let type_span = child_span_of(&field_span, &green_field.type_str);
        let type_ann = self.lower_type_name(&green_field.type_str.node, scope_id, type_span)?;
        let sym = self
            .lookup_symbol(scope_id, &green_field.name.node.name)
            .ok_or_else(|| DiagMsg {
                title: format!("{:?}", HirLowerError::FieldNotFound),
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


    /// param
    fn lower_param(
        &self,
        param: &GreenChild<GreenParam>,
        scope_id: ScopeId,
        parent_span: &Span,
    ) -> Result<HirParam, DiagMsg> {
        let green_param = &param.node;
        let param_span = child_span_of(parent_span, param);
        let sym = self
            .lookup_symbol(scope_id, &green_param.name.node.name)
            .ok_or_else(|| DiagMsg {
                title: format!("{:?}", HirLowerError::ParamNotFound),
                msg: format!("parameter `{}` not found", green_param.name.node.name),
                span: param_span.clone(),
            })?;
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };
        let type_ann = if Self::is_type_empty(&green_param.type_str.node) {
            None
        } else {
            let type_span = child_span_of(&param_span, &green_param.type_str);
            Some(self.lower_type_name(&green_param.type_str.node, scope_id, type_span)?)
        };
        Ok(HirParam { name, type_ann, span: param_span })
    }


    /// method of abstract decl
    fn lower_method_decl(
        &self,
        method: &GreenChild<GreenMethodDecl>,
        scope_id: ScopeId,
        parent_span: &Span,
    ) -> Result<HirMethodDecl, DiagMsg> {
        let green_method = &method.node;
        let method_span = child_span_of(parent_span, method);
        let method_name = green_method.name.node.as_ref();
        let sym = self
            .lookup_symbol(scope_id, &method_name.name)
            .ok_or_else(|| DiagMsg {
                title: format!("{:?}", HirLowerError::MethodNotFound),
                msg: format!("method `{}` not found", method_name.name),
                span: method_span.clone(),
            })?;
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };
        let params = green_method
            .params
            .iter()
            .map(|p| self.lower_param(p, scope_id, &method_span))
            .collect::<Result<_, _>>()?;
        let return_type = if Self::is_type_empty(&green_method.return_type_str.node) {
            None
        } else {
            let ret_span = child_span_of(&method_span, &green_method.return_type_str);
            Some(self.lower_type_name(&green_method.return_type_str.node, scope_id, ret_span)?)
        };
        Ok(HirMethodDecl {
            name,
            generic_params: vec![],
            params,
            return_type,
            is_pub_external: green_method.visibility == Visibility::PublicExternal,
            span: method_span,
        })
    }


    /// ctor def of ADT decl
    fn lower_ctor_def(
        &self,
        ctor: &GreenChild<GreenCtor>,
        scope_id: ScopeId,
        parent_span: &Span,
    ) -> Result<HirCtorDef, DiagMsg> {
        let green_ctor = &ctor.node;
        let ctor_span = child_span_of(parent_span, ctor);
        let ctor_name = green_ctor.name.node.as_ref();
        let sym = self
            .lookup_symbol(scope_id, &ctor_name.name)
            .ok_or_else(|| DiagMsg {
                title: format!("{:?}", HirLowerError::CtorNotFound),
                msg: format!("constructor `{}` not found", ctor_name.name),
                span: ctor_span.clone(),
            })?;
        let name = HirName {
            name: sym.name.clone(),
            sym_id: sym.sym_id,
        };
        let generic_params = self.lower_generic_params(&green_ctor.generic_vars, None, scope_id, &ctor_span)?;

        let from_type = if Self::is_type_empty(&green_ctor.from_type_str.node) {
            None
        } else {
            let from_span = child_span_of(&ctor_span, &green_ctor.from_type_str);
            Some(self.lower_type_name(&green_ctor.from_type_str.node, scope_id, from_span)?)
        };
        let return_type = if Self::is_type_empty(&green_ctor.return_type_str.node) {
            None
        } else {
            let ret_span = child_span_of(&ctor_span, &green_ctor.return_type_str);
            Some(self.lower_type_name(&green_ctor.return_type_str.node, scope_id, ret_span)?)
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


    /// pattern => expr
    fn lower_match_arm(
        &mut self,
        arm: &GreenMatchArm,
        arm_scope: ScopeId,
        parent_span: &Span,
    ) -> Result<HirMatchArm, DiagMsg> {
        let pattern_span = Span {
            source_id: parent_span.source_id,
            start_off: parent_span.start_off,
            end_off: parent_span.start_off + arm.pattern.node.text_len(),
        };
        let pattern = self.lower_pattern(&arm.pattern.node, arm_scope, &pattern_span)?;

        let guard = if let Some(g) = &arm.guard {
            let guard_red = child_expr_red(parent_span, g);
            Some(self.lower_expr(&guard_red, arm_scope)?)
        } else {
            None
        };
        let body_red = child_expr_red(parent_span, &arm.body);
        let body = self.lower_expr(&body_red, arm_scope)?;

        let arm_span = Span {
            source_id: parent_span.source_id,
            start_off: parent_span.start_off,
            end_off: parent_span.start_off + arm.text_len,
        };
        Ok(HirMatchArm {
            pattern,
            guard,
            body,
            span: arm_span,
        })
    }

    /// effect handler
    fn lower_catch_clause(
        &mut self,
        clause: &GreenCatchClause,
        catch_scope: ScopeId,
        parent_span: &Span,
    ) -> Result<HirCatchClause, DiagMsg> {
        let path_span = child_span_of(parent_span, &clause.control_static_path);
        let control_name = self.resolve_static_path(&clause.control_static_path.node, catch_scope, &path_span)?;
        let params = clause.params
            .iter()
            .map(|p| {
                let p_span = child_span_of(parent_span, p);
                self.lower_pattern(&p.node, catch_scope, &p_span)
            })
            .collect::<Result<_, _>>()?;
        let body_red = child_expr_red(parent_span, &clause.body);
        let body = self.lower_expr(&body_red, catch_scope)?;
        let clause_span = Span {
            source_id: parent_span.source_id,
            start_off: parent_span.start_off + clause.control_static_path.relative_start,
            end_off: parent_span.start_off + clause.control_static_path.relative_start + clause.text_len,
        };
        Ok(HirCatchClause {
            control_path: control_name,
            params,
            body,
            span: clause_span,
        })
    }


    /// expr
    fn lower_expr(
        &mut self,
        expr_red: &ExprRedNode,
        scope_id: ScopeId,
    ) -> Result<HirExprId, DiagMsg> {
        let span = expr_red.span.clone();

        let kind = match &expr_red.inner.kind {
            GreenExprKind::Atom { expr: atom } => match atom {
                AtomExprNode::Decimal { .. } | AtomExprNode::Int { .. } | AtomExprNode::Str { .. } => {
                    HirExprKind::Lit(self.lower_lit(atom))
                }
                AtomExprNode::Name { name, .. } => {
                    let sym = self
                        .lookup_symbol(scope_id, name)
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", HirLowerError::NameNotFound),
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
                        .map(|e| self.lower_expr(&child_expr_red(&span, e), scope_id))
                        .collect::<Result<_, _>>()?;
                    HirExprKind::Tuple { elements }
                }
                AtomExprNode::Ellipsis { .. } => HirExprKind::Ellipsis,
            },

            GreenExprKind::Binary { left, op, right } => {
                let left_id = self.lower_expr(&expr_red.child_to_red(left), scope_id)?;
                let right_id = self.lower_expr(&expr_red.child_to_red(right), scope_id)?;
                let op = Self::convert_binop(&op.node);
                HirExprKind::Binary { left: left_id, right: right_id, op }
            }

            GreenExprKind::Unary { op, right } => {
                let right_id = self.lower_expr(&expr_red.child_to_red(right), scope_id)?;
                let op = Self::convert_unary(&op.node)?;
                HirExprKind::Unary { op, right: right_id }
            }

            GreenExprKind::Move { target } => {
                let target_id = self.lower_expr(&expr_red.child_to_red(target), scope_id)?;
                HirExprKind::Move { target: target_id }
            }
            GreenExprKind::Copy { target } => {
                let target_id = self.lower_expr(&expr_red.child_to_red(target), scope_id)?;
                HirExprKind::Copy { target: target_id }
            }
            GreenExprKind::Ref { target } => {
                let target_id = self.lower_expr(&expr_red.child_to_red(target), scope_id)?;
                HirExprKind::Ref { target: target_id }
            }
            GreenExprKind::MutRef { target } => {
                let target_id = self.lower_expr(&expr_red.child_to_red(target), scope_id)?;
                HirExprKind::MutRef { target: target_id }
            }
            GreenExprKind::Share { target } => {
                let target_id = self.lower_expr(&expr_red.child_to_red(target), scope_id)?;
                HirExprKind::Share { target: target_id }
            }

            GreenExprKind::Call { callee, args } => {
                let maybe_func_sym = if let GreenExprKind::StaticPath { path } = &callee.node.kind {
                    let callee_span = child_span_of(&span, callee);
                    self.resolve_static_path(&path.node, scope_id, &callee_span).ok()
                } else {
                    None
                };

                let has_named = args.iter().any(|a| a.node.name.is_some());
                let param_names = if has_named {
                    maybe_func_sym.as_ref()
                        .and_then(|hir_name| self.param_names_map.get(&hir_name.sym_id))
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", HirLowerError::CannotResolveFunction),
                            msg: "cannot resolve function for named arguments".into(),
                            span: span.clone(),
                        })?
                } else {
                    &vec![]
                };

                let ordered_args = if has_named {
                    let mut positional = Vec::new();
                    let mut named = HashMap::new();
                    let mut seen_names = HashSet::new();

                    for arg in args {
                        if let Some(name_child) = &arg.node.name {
                            let name = name_child.node.name.clone();
                            if !seen_names.insert(name.clone()) {
                                return Err(DiagMsg {
                                    title: format!("{:?}", HirLowerError::DuplicateKeywordArg),
                                    msg: format!("duplicate keyword argument `{}`", name),
                                    span: span.clone(),
                                });
                            }
                            named.insert(name, arg);
                        } else {
                            positional.push(arg);
                        }
                    }

                    // 检查命名参数是否都在形参列表中
                    for name in named.keys() {
                        if !param_names.contains(name) {
                            return Err(DiagMsg {
                                title: format!("{:?}", HirLowerError::UnexpectedKeywordArg),
                                msg: format!("unexpected keyword argument `{}`", name),
                                span: span.clone(),
                            });
                        }
                    }

                    let total_params = param_names.len();
                    if positional.len() + named.len() > total_params {
                        return Err(DiagMsg {
                            title: format!("{:?}", HirLowerError::TooManyArguments),
                            msg: "too many arguments".into(),
                            span: span.clone(),
                        });
                    }

                    let mut ordered = vec![None; total_params];
                    let mut pos_idx = 0;

                    // 填充位置参数
                    for (i, param) in param_names.iter().enumerate() {
                        if named.contains_key(param) {
                            continue;
                        }
                        if pos_idx < positional.len() {
                            ordered[i] = Some(positional[pos_idx]);
                            pos_idx += 1;
                        }
                    }

                    // 填充命名参数
                    for (name, arg) in named {
                        let idx = param_names.iter().position(|p| p == &name).unwrap();
                        if ordered[idx].is_some() {
                            return Err(DiagMsg {
                                title: format!("{:?}", HirLowerError::ArgumentConflict),
                                msg: format!("argument `{}` specified multiple times", name),
                                span: span.clone(),
                            });
                        }
                        ordered[idx] = Some(arg);
                    }

                    if ordered.iter().any(|o| o.is_none()) {
                        return Err(DiagMsg {
                            title: format!("{:?}", HirLowerError::MissingArguments),
                            msg: "missing required arguments".into(),
                            span: span.clone(),
                        });
                    }

                    ordered.into_iter().map(|o| o.unwrap().clone()).collect::<Vec<_>>()
                } else {
                    args.to_vec()
                };

                if let Some(func_sym) = maybe_func_sym {
                    if let Some(sym) = self.name_pass_result.pool.get_symbol_by_id(func_sym.sym_id) {
                        if let SymbolKind::Constructor = sym.kind {
                            if ordered_args.len() == 1 {
                                let arg_id = self.lower_expr(
                                    &expr_red.child_to_red(&ordered_args[0].node.value), scope_id)?;
                                let kind = HirExprKind::BuildVariant {
                                    variant_name: func_sym,
                                    target: arg_id,
                                };
                                let hir_id = self.hir.hir_expr_pool.len();
                                self.hir.hir_expr_pool.push(HirExpr { kind, hir_id, span });
                                return Ok(hir_id);
                            } else {
                                return Err(DiagMsg {
                                    title: format!("{:?}", HirLowerError::ArityMismatch),
                                    msg: format!("constructor expects 1 argument, got {}", ordered_args.len()),
                                    span: span.clone(),
                                });
                            }
                        }
                    }
                }

                let callee_id = self.lower_expr(&expr_red.child_to_red(callee), scope_id)?;
                let arg_ids = ordered_args
                    .iter()
                    .map(|a| self.lower_expr(&expr_red.child_to_red(&a.node.value), scope_id))
                    .collect::<Result<_, _>>()?;

                HirExprKind::Call { callee: callee_id, args: arg_ids }
            }
            GreenExprKind::UnsafeExternalCall { callee, args } => {
                let callee_id = self.lower_expr(&expr_red.child_to_red(callee), scope_id)?;
                let arg_ids = args
                    .iter()
                    .map(|a| self.lower_expr(&expr_red.child_to_red(a), scope_id))
                    .collect::<Result<_, _>>()?;
                HirExprKind::UnsafeExternalCall { callee: callee_id, args: arg_ids }
            }

            GreenExprKind::StaticPath { path } => {
                if let Ok(path_name) = self.resolve_static_path(&path.node, scope_id, &span) {
                    if let Some(sym) = self.name_pass_result.pool.get_symbol_by_id(path_name.sym_id) {
                        if let SymbolKind::Constructor = sym.kind {
                            let unit_id = self.hir.hir_expr_pool.len();
                            self.hir.hir_expr_pool.push(HirExpr {
                                kind: HirExprKind::Tuple { elements: vec![] },
                                hir_id: unit_id,
                                span: span.clone(),
                            });
                            let kind = HirExprKind::BuildVariant {
                                variant_name: path_name,
                                target: unit_id,
                            };
                            let hir_id = self.hir.hir_expr_pool.len();
                            self.hir.hir_expr_pool.push(HirExpr { kind, hir_id, span: span.clone() });
                            return Ok(hir_id);
                        }
                    }
                }

                let segments = &path.node.segments;
                if segments.is_empty() {
                    return Err(DiagMsg {
                        title: format!("{:?}", HirLowerError::EmptyPath),
                        msg: "empty static path".into(),
                        span: span.clone(),
                    });
                }

                // 贪心解析命名空间段，直到遇到第一个非命名空间符号
                let mut ns_end = 0;
                let mut current_scope = scope_id;
                for (i, seg) in segments.iter().enumerate() {
                    let seg_name = &seg.node.name;
                    let seg_span = child_span_of(&span, seg);
                    match self.name_pass_result.pool.lookup(current_scope, seg_name) {
                        Some((sym, found_scope)) => {
                            match &sym.kind {
                                SymbolKind::File { .. }
                                | SymbolKind::Struct { .. }
                                | SymbolKind::ADT { .. }
                                | SymbolKind::Effect { .. } => {
                                    current_scope = found_scope;
                                    ns_end = i + 1;
                                }
                                _ => break,
                            }
                        }
                        None => {
                            return Err(DiagMsg {
                                title: format!("{:?}", HirLowerError::PathNotFound),
                                msg: format!("name `{}` not found", seg_name),
                                span: seg_span.clone(),
                            });
                        }
                    }
                }

                if ns_end == segments.len() {
                    let name = self.resolve_static_path(&path.node, scope_id, &span)?;
                    let ident_kind = HirExprKind::Ident(name);
                    let hir_id = self.hir.hir_expr_pool.len();
                    self.hir.hir_expr_pool.push(HirExpr {
                        kind: ident_kind,
                        hir_id,
                        span: span.clone(),
                    });
                    return Ok(hir_id);
                }


                let base_obj_id = if ns_end > 0 {
                    let ns_path = GreenPureStaticPath {
                        segments: segments[..ns_end].to_vec(),
                        text_len: 0,
                    };
                    let ns_name = self.resolve_static_path(&ns_path, scope_id, &span)?;
                    let ident_kind = HirExprKind::Ident(ns_name);
                    let hir_id = self.hir.hir_expr_pool.len();
                    self.hir.hir_expr_pool.push(HirExpr {
                        kind: ident_kind,
                        hir_id,
                        span: span.clone(),
                    });
                    hir_id
                } else {
                    let first_seg = &segments[0];
                    let first_seg_span = child_span_of(&span, first_seg);
                    let first_expr_red = ExprRedNode {
                        span: first_seg_span.clone(),
                        inner: Arc::new(GreenExpr {
                            kind: GreenExprKind::Atom {
                                expr: AtomExprNode::Name {
                                    name: first_seg.node.name.clone(),
                                    text_len: first_seg.node.text_len(),
                                },
                            },
                            text_len: first_seg.node.text_len(),
                        }),
                    };
                    self.lower_expr(&first_expr_red, scope_id)?
                };

                // 为剩余段逐段生成 FieldAccess
                let start_field_idx = if ns_end == 0 { 1 } else { ns_end };
                let mut current_obj_id = base_obj_id;
                for seg in segments.iter().skip(start_field_idx) {
                    let field_name = seg.node.name.clone();
                    let field_span = child_span_of(&span, seg);
                    let field_kind = HirExprKind::FieldAccess {
                        obj: current_obj_id,
                        field: field_name,
                    };
                    let field_hir_id = self.hir.hir_expr_pool.len();
                    self.hir.hir_expr_pool.push(HirExpr {
                        kind: field_kind,
                        hir_id: field_hir_id,
                        span: field_span,
                    });
                    current_obj_id = field_hir_id;
                }

                return Ok(current_obj_id);
            }

            GreenExprKind::MemberAccess { left, member } => {
                let obj_id = self.lower_expr(&expr_red.child_to_red(left), scope_id)?;
                HirExprKind::FieldAccess {
                    obj: obj_id,
                    field: member.node.name.clone(),
                }
            }
            GreenExprKind::TupleIndex { expr, index } => {
                let expr_id = self.lower_expr(&expr_red.child_to_red(expr), scope_id)?;
                HirExprKind::TupleIndex {
                    expr: expr_id,
                    index: *index,
                }
            }

            GreenExprKind::MakeStruct { path, fields } => {
                let path_id = self.lower_expr(&expr_red.child_to_red(path), scope_id)?;
                let hir_fields = fields
                    .iter()
                    .map(|f| {
                        let init = &f.node;
                        let field_name = init.name.node.name.clone();
                        let value_id = self.lower_expr(&expr_red.child_to_red(&init.value), scope_id)?;
                        Ok((field_name, value_id))
                    })
                    .collect::<Result<_, DiagMsg>>()?;
                HirExprKind::MakeStruct {
                    path: path_id,
                    fields: hir_fields,
                }
            }

            GreenExprKind::TypeCast { expr: e, into_type } => {
                let expr_id = self.lower_expr(&expr_red.child_to_red(e), scope_id)?;
                let type_span = child_span_of(&span, into_type);
                let type_ann = self.lower_type_name(&into_type.node, scope_id, type_span)?;
                HirExprKind::TypeCast { expr: expr_id, type_ann }
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
                let init_id = self.lower_expr(&expr_red.child_to_red(e), scope_id)?;
                let sym = self
                    .lookup_symbol(scope_id, &name.node.name)
                    .ok_or_else(|| DiagMsg {
                        title: format!("{:?}", HirLowerError::LetNameNotFound),
                        msg: format!("variable `{}` not found", name.node.name),
                        span: span.clone(),
                    })?;
                let var_name = HirName {
                    name: sym.name.clone(),
                    sym_id: sym.sym_id,
                };
                let type_ann = if let Some(ts) = type_str {
                    let ts_span = child_span_of(&span, ts);
                    Some(self.lower_type_name(&ts.node, scope_id, ts_span)?)
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
                let cond_id = self.lower_expr(&expr_red.child_to_red(cond), scope_id)?;
                let then_id = self.lower_expr(&expr_red.child_to_red(then_expr), scope_id)?;
                let elifs_ids = elifs
                    .iter()
                    .map(|elif| {
                        let c = self.lower_expr(&expr_red.child_to_red(&elif.cond), scope_id)?;
                        let b = self.lower_expr(&expr_red.child_to_red(&elif.body), scope_id)?;
                        Ok((c, b))
                    })
                    .collect::<Result<_, DiagMsg>>()?;
                let else_opt = if let Some(else_e) = else_expr {
                    Some(self.lower_expr(&expr_red.child_to_red(else_e), scope_id)?)
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
                    Some(self.lower_expr(&expr_red.child_to_red(e), scope_id)?)
                } else {
                    None
                };
                HirExprKind::Return { expr: expr_id }
            }

            GreenExprKind::Match { for_match, arms } => {
                let scrutinee_id = self.lower_expr(&expr_red.child_to_red(for_match), scope_id)?;
                let hir_arms = arms
                    .iter()
                    .map(|arm_child| {
                        let arm = &arm_child.node;
                        let arm_scope = *self.name_pass_result.arm_scope_map.get(arm)
                            .expect("match arm scope not found"); // 使用精确作用域
                        let arm_span = Span {
                            source_id: span.source_id,
                            start_off: span.start_off + arm_child.relative_start,
                            end_off: span.start_off + arm_child.relative_start + arm.text_len,
                        };
                        self.lower_match_arm(arm, arm_scope, &arm_span)
                    })
                    .collect::<Result<_, _>>()?;
                HirExprKind::Match {
                    scrutinee: scrutinee_id,
                    arms: hir_arms,
                }
            }

            GreenExprKind::Is { expr: e, pattern } => {
                let expr_id = self.lower_expr(&expr_red.child_to_red(e), scope_id)?;
                let pattern_span = child_span_of(&span, pattern);
                let hir_pattern = self.lower_pattern(&pattern.node, scope_id, &pattern_span)?;
                HirExprKind::Is { expr: expr_id, pattern: hir_pattern }
            }

            GreenExprKind::Raise { effect_path, control_name, args } => {
                // 将 effect_path 和 control_name 合并为完整路径，然后统一解析
                let mut full_segments = effect_path.node.segments.clone();
                full_segments.push(GreenChild {
                    relative_start: control_name.relative_start,
                    node: control_name.node.clone(),
                });
                let full_path = GreenPureStaticPath {
                    segments: full_segments,
                    text_len: 0,
                };
                // 构造一个临时 GreenChild 用来计算 span
                let tmp_child = GreenChild {
                    relative_start: effect_path.relative_start,
                    node: Arc::new(full_path.clone()),
                };
                let path_span = child_span_of(&span, &tmp_child);
                let control_name_resolved = self.resolve_static_path(&full_path, scope_id, &path_span)?;
                let args_ids = args
                    .iter()
                    .map(|a| self.lower_expr(&expr_red.child_to_red(a), scope_id))
                    .collect::<Result<_, _>>()?;
                HirExprKind::Raise {
                    control_name: control_name_resolved,
                    args: args_ids,
                }
            }

            GreenExprKind::With { handler_expr, clauses } => {
                let handler_id = self.lower_expr(&expr_red.child_to_red(handler_expr), scope_id)?;
                let hir_clauses = clauses
                    .iter()
                    .map(|clause_child| {
                        let clause = &clause_child.node;
                        let catch_scope = *self.name_pass_result.catch_scope_map.get(clause)
                            .expect("catch clause scope not found"); // 使用精确作用域
                        let clause_span = Span {
                            source_id: span.source_id,
                            start_off: span.start_off + clause_child.relative_start,
                            end_off: span.start_off + clause_child.relative_start + clause.text_len,
                        };
                        self.lower_catch_clause(clause, catch_scope, &clause_span)
                    })
                    .collect::<Result<_, _>>()?;
                HirExprKind::With {
                    handler: handler_id,
                    clauses: hir_clauses,
                }
            }

            GreenExprKind::Resume { expr: e } => {
                let expr_id = self.lower_expr(&expr_red.child_to_red(e), scope_id)?;
                HirExprKind::Resume { expr: expr_id }
            }
        };

        let hir_id = self.hir.hir_expr_pool.len();
        let hir_expr = HirExpr { kind, hir_id, span };
        self.hir.hir_expr_pool.push(hir_expr);
        Ok(hir_id)
    }


    /// decl
    fn lower_decl(
        &mut self,
        decl_red: &DeclRedNode,
        file_scope_id: ScopeId,
    ) -> Result<HirDeclId, DiagMsg> {

        let decl = &decl_red.inner;
        let hir_id = self.hir.hir_decl_pool.len();
        let span = decl_red.span.clone();
        let ident = decl.name.node.as_ref().clone();
        let is_pub_external = decl.visibility == Visibility::PublicExternal;

        let decl_scope = self.find_decl_scope(Arc::clone(&decl_red.inner))
            .unwrap_or(file_scope_id);

        let kind = match &decl.kind {
            GreenDeclKind::Fun {
                params,
                return_type_str,
                generic_vars,
                block,
                where_clause
            } => {
                let generic_params = self.lower_generic_params(
                    generic_vars,
                    where_clause.as_ref().map(|wc| &wc.node).map(|v| &**v),
                    decl_scope, &span,
                )?;
                let hir_params = params
                    .iter()
                    .map(|p| self.lower_param(p, decl_scope, &span))
                    .collect::<Result<_, _>>()?;
                let return_type = if Self::is_type_empty(&return_type_str.node) {
                    None
                } else {
                    let ret_span = child_span_of(&span, return_type_str);
                    Some(self.lower_type_name(&return_type_str.node, decl_scope, ret_span)?)
                };
                let body = block
                    .iter()
                    .map(|stmt| self.lower_expr(&child_expr_red(&span, stmt), decl_scope))
                    .collect::<Result<_, _>>()?;
                HirDeclKind::Fun {
                    generic_params,
                    params: hir_params,
                    return_type,
                    body,
                }
            }

            GreenDeclKind::FunDecl {
                params,
                return_type_str,
                generic_vars,
                where_clause
            } => {
                let generic_params = self.lower_generic_params(
                    generic_vars,
                    where_clause.as_ref().map(|wc| &wc.node).map(|v| &**v),
                    decl_scope, &span,
                )?;
                let hir_params = params
                    .iter()
                    .map(|p| self.lower_param(p, decl_scope, &span))
                    .collect::<Result<_, _>>()?;
                let return_type = if Self::is_type_empty(&return_type_str.node) {
                    None
                } else {
                    let ret_span = child_span_of(&span, return_type_str);
                    Some(self.lower_type_name(&return_type_str.node, decl_scope, ret_span)?)
                };
                HirDeclKind::Fun {
                    generic_params,
                    params: hir_params,
                    return_type,
                    body: vec![],
                }
            }

            GreenDeclKind::TypeStruct {
                fields,
                has_abst,
                generic_vars,
                where_clause
            } => {
                let generic_params = self.lower_generic_params(
                    generic_vars,
                    where_clause.as_ref().map(|wc| &wc.node).map(|v| &**v),
                    decl_scope, &span,
                )?;
                let hir_fields = fields
                    .iter()
                    .map(|f| self.lower_field_def(f, decl_scope, &span))
                    .collect::<Result<_, _>>()?;
                let implemented_abstracts = has_abst
                    .iter()
                    .map(|name_child| {
                        let name_span = child_span_of(&span, name_child);
                        let path = GreenPureStaticPath {
                            segments: vec![name_child.clone()],
                            text_len: name_child.node.text_len(),
                        };
                        self.resolve_static_path(&path, decl_scope, &name_span)
                            .map(|name| HirTypeName::Named { path: name, generics: vec![] })
                    })
                    .collect::<Result<_, _>>()?;
                HirDeclKind::Struct {
                    generic_params,
                    fields: hir_fields,
                    implemented_abstracts,
                }
            }

            GreenDeclKind::TypeAlias {
                ref_to,
                generic_vars,
                where_clause,
                ..
            } => {
                let alias_scope = self.name_pass_result.pool.decl_node_scope_map
                    .get(&decl_red.inner)
                    .copied()
                    .unwrap_or(file_scope_id); // fallback

                let alias_span = child_span_of(&span, ref_to);
                let alias_for = self.lower_type_name(&ref_to.node, alias_scope, alias_span)?;

                let generic_params = self.lower_generic_params(
                    generic_vars,
                    where_clause.as_ref().map(|wc| &wc.node).map(|v| &**v),
                    alias_scope,
                    &span,
                )?;

                HirDeclKind::TypeAlias {
                    generic_params,
                    alias_for,
                }
            }

            GreenDeclKind::ADT {
                has_abst,
                generic_vars,
                ctors,
                where_clause,
            } => {
                let generic_params = self.lower_generic_params(
                    generic_vars,
                    where_clause.as_ref().map(|wc| &wc.node).map(|v| &**v),
                    decl_scope, &span,
                )?;
                let implemented_abstracts = has_abst
                    .iter()
                    .map(|name_child| {
                        let name_span = child_span_of(&span, name_child);
                        let path = GreenPureStaticPath {
                            segments: vec![name_child.clone()],
                            text_len: name_child.node.text_len(),
                        };
                        self.resolve_static_path(&path, decl_scope, &name_span)
                            .map(|name| HirTypeName::Named { path: name, generics: vec![] })
                    })
                    .collect::<Result<_, _>>()?;

                let hir_ctors = ctors
                    .iter()
                    .map(|c| self.lower_ctor_def(c, decl_scope, &span))   // 使用 decl_scope
                    .collect::<Result<_, _>>()?;

                HirDeclKind::ADT {
                    generic_params,
                    ctors: hir_ctors,
                    implemented_abstracts,
                }
            }

            GreenDeclKind::Abstract {
                super_abst,
                generic_vars,
                methods,
                where_clause
            } => {
                let generic_params = self.lower_generic_params(
                    generic_vars,
                    where_clause.as_ref().map(|wc| &wc.node).map(|v| &**v),
                    decl_scope, &span,
                )?;
                let super_abstracts = super_abst
                    .iter()
                    .map(|name_child| {
                        let name_span = child_span_of(&span, name_child);
                        let path = GreenPureStaticPath {
                            segments: vec![name_child.clone()],
                            text_len: name_child.node.text_len(),
                        };
                        self.resolve_static_path(&path, decl_scope, &name_span)
                            .map(|name| HirTypeName::Named { path: name, generics: vec![] })
                    })
                    .collect::<Result<_, _>>()?;
                let hir_methods = methods
                    .iter()
                    .map(|m| self.lower_method_decl(m, decl_scope, &span))
                    .collect::<Result<_, _>>()?;
                HirDeclKind::Abstract {
                    generic_params,
                    methods: hir_methods,
                    super_abstracts,
                }
            }

            GreenDeclKind::Effect { controls } => {
                let controls_hir = controls
                    .iter()
                    .map(|ctrl| {
                        let ctrl_span = child_span_of(&span, ctrl);
                        let ctrl_name_str = ctrl.node.name.node.name.clone();
                        let sym = self.lookup_symbol(decl_scope, &ctrl_name_str)
                            .ok_or_else(|| DiagMsg {
                                title: format!("{:?}", HirLowerError::ControlNotFound),
                                msg: format!("control `{}` not found", ctrl_name_str),
                                span: ctrl_span.clone(),
                            })?;
                        let ctrl_name = HirName {
                            name: sym.name.clone(),
                            sym_id: sym.sym_id,
                        };
                        let params = ctrl.node.params
                            .iter()
                            .map(|p| self.lower_param(p, decl_scope, &ctrl_span))
                            .collect::<Result<_, DiagMsg>>()?;
                        let return_type = if Self::is_type_empty(&ctrl.node.return_type.node) {
                            None
                        } else {
                            let ret_span = child_span_of(&ctrl_span, &ctrl.node.return_type);
                            Some(self.lower_type_name(&ctrl.node.return_type.node, decl_scope, ret_span)?)
                        };
                        Ok((ctrl_name, params, return_type))
                    })
                    .collect::<Result<_, DiagMsg>>()?;
                HirDeclKind::Effect { controls: controls_hir }
            }

            GreenDeclKind::Const { expr, type_str } => {
                let expr_id = self.lower_expr(&child_expr_red(&span, expr), decl_scope)?;
                let type_ann = if let Some(ts) = type_str {
                    let ts_span = child_span_of(&span, ts);
                    Some(self.lower_type_name(&ts.node, decl_scope, ts_span)?)
                } else {
                    None
                };
                HirDeclKind::Const { expr: expr_id, type_ann }
            }

            GreenDeclKind::Global { expr, type_str } => {
                let expr_id = self.lower_expr(&child_expr_red(&span, expr), decl_scope)?;
                let type_ann = if let Some(ts) = type_str {
                    let ts_span = child_span_of(&span, ts);
                    Some(self.lower_type_name(&ts.node, decl_scope, ts_span)?)
                } else {
                    None
                };
                HirDeclKind::Global { expr: expr_id, type_ann }
            }

            GreenDeclKind::TypeDecl => HirDeclKind::TypeDecl,

            GreenDeclKind::CType => HirDeclKind::CType,

            GreenDeclKind::External { sym_name, params, return_type_str, is_variadic } => {
                let hir_params = params
                    .iter()
                    .map(|p| self.lower_param(p, decl_scope, &span))
                    .collect::<Result<_, _>>()?;
                let return_type = self.lower_type_name(&return_type_str.node, decl_scope,
                                                       child_span_of(&span, return_type_str))?;
                HirDeclKind::External {
                    sym_name: sym_name.node.name.clone(),
                    params: hir_params,
                    return_type,
                    is_variadic: *is_variadic,
                }
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
                name_pass_result: None,
            },
            param_names_map: HashMap::new(),
        }
    }

    fn lower(mut self) -> Result<HirCrate, DiagMsg> {

        // build param_names_map
        for file_unit in &self.ast_crate.file_units {
            let file_scope_id = self.name_pass_result.source_id_to_scope
                .get(&file_unit.span.source_id)
                .copied()
                .expect("file scope not found");

            for decl_child in &file_unit.green.top_decls {
                let decl = &decl_child.node;
                if let GreenDeclKind::Fun { params, .. } | GreenDeclKind::FunDecl { params, .. } = &decl.kind {
                    let func_name = decl.name.node.as_ref().name.clone();
                    if let Some((sym, _)) = self.name_pass_result.pool.lookup(file_scope_id, &func_name) {
                        let param_names: Vec<String> = params
                            .iter()
                            .map(|p| p.node.name.node.as_ref().name.clone())
                            .collect();
                        self.param_names_map.insert(sym.sym_id, param_names);
                    }
                }
            }
        }


        for file_unit in &self.ast_crate.file_units {
            let file_source_id = file_unit.span.source_id;
            let file_scope_id = self.name_pass_result.source_id_to_scope
                .get(&file_source_id)
                .copied()
                .expect("file scope not found");

            for decl_child in &file_unit.green.top_decls {
                let decl_red = child_decl_red(&file_unit.span, decl_child);
                let hir_id = self.lower_decl(&decl_red, file_scope_id)?;
                if decl_child.node.visibility == Visibility::Public
                    || decl_child.node.visibility == Visibility::PublicExternal
                {
                    self.hir.pub_decl_ids.push(hir_id);
                }
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