use std::collections::HashMap;
use std::sync::Arc;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::ast::{
    CrateAst, AtomExprNode, GreenDecl, GreenDeclKind, GreenExpr, GreenExprKind, GreenElseIf,
    GreenChild, GreenField, GreenGenericVar, GreenCtor, GreenMethodDecl,
    ExprRedNode, DeclRedNode, FileRedUnit, Visibility, RequireRedNode, TypeNameString,
};
use leafc_coreapi::name_pass::{
    DoScopeMap, FunScopeMap, NamePassApi, NamePassError, NamePassResult,
};
use leafc_coreapi::scope::{Scope, ScopeId, ScopeKind, ScopePool, SymId, Symbol, SymbolKind};
use leafc_coreapi::source::{SourceId, Span};

pub struct NamePass<'a> {
    ast_module: &'a CrateAst,
    scope_pool: ScopePool,

    /// DoExpr => Scope
    do_scope_map: DoScopeMap,

    /// FunDecl => Scope
    fun_scope_map: FunScopeMap,

    /// source_id => FileUnit
    source_to_file_unit: HashMap<SourceId, &'a FileRedUnit>,

    /// source_id => ScopeId
    source_id_to_scope: HashMap<SourceId, ScopeId>,
}

impl<'a> NamePass<'a> {
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

    fn collect_member_path(expr: &ExprRedNode) -> Option<(String, Vec<(String, Span)>)> {
        match &expr.inner.kind {
            GreenExprKind::Atom { expr: AtomExprNode::Name { name, .. } } => {
                Some((name.clone(), vec![]))
            }
            GreenExprKind::Member { left, right, .. } => {
                let left_red = Self::child_expr_red(&expr.span, left);
                let (base, mut segs) = Self::collect_member_path(&left_red)?;
                segs.push((right.node.as_ref().clone(), expr.span.clone()));
                Some((base, segs))
            }
            _ => None,
        }
    }

    fn check_member_path(
        &self,
        base_name: &str,
        segments: &[(String, Span)],
        current_scope: ScopeId,
    ) -> Result<(), DiagMsg> {
        let (mut sym, mut sym_scope) = self.scope_pool
            .lookup(current_scope, base_name)
            .ok_or_else(|| DiagMsg {
                title: format!("{:?}", NamePassError::UndefinedName),
                msg: format!("undefined name `{}`", base_name),
                span: segments.first()
                    .unwrap()
                    .clone()
                    .1
            })?;

        for (seg_name, seg_span) in segments {
            match &sym.kind {
                SymbolKind::File { source_id } => {
                    let file_scope = self.source_id_to_scope[source_id];
                    if let Some((found_sym, found_scope)) =
                        self.scope_pool.lookup(file_scope, seg_name)
                    {
                        let file_unit = self.source_to_file_unit.get(source_id).ok_or_else(|| {
                            DiagMsg {
                                title: format!("{:?}", NamePassError::UndefinedModule),
                                msg: format!("module `{}` not found", base_name),
                                span: seg_span.clone(),
                            }
                        })?;
                        let has_public = file_unit.green.top_decls.iter().any(|decl_child| {
                            let decl = &decl_child.node;
                            decl.name.node.as_ref() == seg_name
                                && (decl.visibility == Visibility::Public
                                || decl.visibility == Visibility::PublicExternal)
                        });
                        if !has_public {
                            return Err(DiagMsg {
                                title: format!("{:?}", NamePassError::UndefinedName),
                                msg: format!(
                                    "module `{}` has no public item `{}`",
                                    base_name, seg_name
                                ),
                                span: seg_span.clone(),
                            });
                        }
                        sym = found_sym;
                        sym_scope = found_scope;
                    } else {
                        return Err(DiagMsg {
                            title: format!("{:?}", NamePassError::UndefinedName),
                            msg: format!("module `{}` has no item `{}`", base_name, seg_name),
                            span: seg_span.clone(),
                        });
                    }
                }
                SymbolKind::Struct { fields } => {
                    let field_exists = fields.iter().any(|&sym_id| {
                        self.scope_pool
                            .get_symbol_by_id(sym_id)
                            .map_or(false, |s| s.name == *seg_name)
                    });
                    if !field_exists {
                        return Err(DiagMsg {
                            title: format!("{:?}", NamePassError::UndefinedName),
                            msg: format!("struct has no field `{}`", seg_name),
                            span: seg_span.clone(),
                        });
                    }
                    return Ok(());
                }
                SymbolKind::ADT { constructors } => {
                    let ctor_exists = constructors.iter().any(|&sym_id| {
                        self.scope_pool
                            .get_symbol_by_id(sym_id)
                            .map_or(false, |s| s.name == *seg_name)
                    });
                    if !ctor_exists {
                        return Err(DiagMsg {
                            title: format!("{:?}", NamePassError::InvalidADTConstructor),
                            msg: format!("ADT has no constructor `{}`", seg_name),
                            span: seg_span.clone(),
                        });
                    }
                    return Ok(());
                }
                _ => {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn build_expr_scope(
        &mut self,
        expr_red: &ExprRedNode,
        current_scope: ScopeId,
    ) -> Result<(), DiagMsg> {
        let expr = &expr_red.inner;
        match &expr.kind {
            GreenExprKind::Atom { .. } => {}

            GreenExprKind::Binary { left, op: _, right } => {
                self.build_expr_scope(&Self::child_expr_red(&expr_red.span, left), current_scope)?;
                self.build_expr_scope(&Self::child_expr_red(&expr_red.span, right), current_scope)?;
            }
            GreenExprKind::Unary { op: _, right } => {
                self.build_expr_scope(&Self::child_expr_red(&expr_red.span, right), current_scope)?;
            }
            GreenExprKind::Call { callee, args, .. } | GreenExprKind::UnsafeExternalCall { callee, args, .. } => {
                self.build_expr_scope(&Self::child_expr_red(&expr_red.span, callee), current_scope)?;
                for arg in args {
                    self.build_expr_scope(&Self::child_expr_red(&expr_red.span, arg), current_scope)?;
                }
            }
            GreenExprKind::Member { left, .. } => {
                self.build_expr_scope(&Self::child_expr_red(&expr_red.span, left), current_scope)?;
            }
            GreenExprKind::TypeCast { expr: e, into_type } => {
                self.build_expr_scope(&Self::child_expr_red(&expr_red.span, e), current_scope)?;
                self.build_expr_scope(&Self::child_expr_red(&expr_red.span, into_type), current_scope)?;
            }
            GreenExprKind::Move { target }
            | GreenExprKind::Copy { target }
            | GreenExprKind::Ref { target }
            | GreenExprKind::MutRef { target }
            | GreenExprKind::Share { target } => {
                self.build_expr_scope(&Self::child_expr_red(&expr_red.span, target), current_scope)?;
            }
            GreenExprKind::Do { exprs, .. } => {
                let new_scope_id = self.scope_pool.push_scope(
                    Some(current_scope),
                    ScopeKind::Block,
                    None,
                    Some(expr_red.span.clone()),
                );
                self.do_scope_map.insert(Arc::clone(&expr_red.inner), new_scope_id);

                for e in exprs {
                    self.build_expr_scope(&Self::child_expr_red(&expr_red.span, e), new_scope_id)?;
                }
            }
            GreenExprKind::Let { name, expr: e, .. } => {
                self.scope_pool.add_symbol(
                    current_scope,
                    name.node.as_ref().clone(),
                    expr_red.span.clone(), // 用 let 本身的 span
                    SymbolKind::Local,
                );
                self.build_expr_scope(&Self::child_expr_red(&expr_red.span, e), current_scope)?;
            }
            GreenExprKind::If {
                cond, then_expr, elifs, else_expr
            } => {
                self.build_expr_scope(&Self::child_expr_red(&expr_red.span, cond), current_scope)?;
                self.build_expr_scope(&Self::child_expr_red(&expr_red.span, then_expr), current_scope)?;
                for elif in elifs {
                    self.build_expr_scope(&Self::child_expr_red(&expr_red.span, &elif.cond), current_scope)?;
                    self.build_expr_scope(&Self::child_expr_red(&expr_red.span, &elif.body), current_scope)?;
                }
                if let Some(else_e) = else_expr {
                    self.build_expr_scope(&Self::child_expr_red(&expr_red.span, else_e), current_scope)?;
                }
            }
            GreenExprKind::Return { expr: opt_expr } => {
                if let Some(e) = opt_expr {
                    self.build_expr_scope(&Self::child_expr_red(&expr_red.span, e), current_scope)?;
                }
            }
        }
        Ok(())
    }

    fn resolve_expr(
        &self,
        expr_red: &ExprRedNode,
        current_scope: ScopeId,
    ) -> Result<(), DiagMsg> {
        let expr = &expr_red.inner;
        match &expr.kind {
            GreenExprKind::Atom { expr: atom_expr } => {
                if let AtomExprNode::Name { name, .. } = atom_expr {
                    self.resolve_name(name, current_scope, expr_red.span.clone())?;
                }
            }
            GreenExprKind::Binary { left, op: _, right } => {
                self.resolve_expr(&Self::child_expr_red(&expr_red.span, left), current_scope)?;
                self.resolve_expr(&Self::child_expr_red(&expr_red.span, right), current_scope)?;
            }
            GreenExprKind::Unary { op: _, right } => {
                self.resolve_expr(&Self::child_expr_red(&expr_red.span, right), current_scope)?;
            }
            GreenExprKind::Call { callee, args, .. } => {
                self.resolve_expr(&Self::child_expr_red(&expr_red.span, callee), current_scope)?;
                for arg in args {
                    self.resolve_expr(&Self::child_expr_red(&expr_red.span, arg), current_scope)?;
                }
            }
            GreenExprKind::UnsafeExternalCall { .. } => {
                // 外部调用不解析名称
            }
            GreenExprKind::Member { left, .. } => {
                self.resolve_expr(&Self::child_expr_red(&expr_red.span, left), current_scope)?;
                if let Some((base, segs)) = Self::collect_member_path(expr_red) {
                    if !segs.is_empty() {
                        self.check_member_path(&base, &segs, current_scope)?;
                    }
                }
            }
            GreenExprKind::TypeCast { expr: e, into_type } => {
                self.resolve_expr(&Self::child_expr_red(&expr_red.span, e), current_scope)?;
                self.resolve_expr(&Self::child_expr_red(&expr_red.span, into_type), current_scope)?;
            }
            GreenExprKind::Move { target }
            | GreenExprKind::Copy { target }
            | GreenExprKind::Ref { target }
            | GreenExprKind::MutRef { target }
            | GreenExprKind::Share { target } => {
                self.resolve_expr(&Self::child_expr_red(&expr_red.span, target), current_scope)?;
            }
            GreenExprKind::Do { .. } => {
                if let Some(&do_scope) = self.do_scope_map.get(&expr_red.inner) {
                    // 重新获取子表达式列表
                    if let GreenExprKind::Do { exprs, .. } = &expr.kind {
                        for e in exprs {
                            self.resolve_expr(&Self::child_expr_red(&expr_red.span, e), do_scope)?;
                        }
                    }
                } else {
                    unreachable!()
                }
            }
            GreenExprKind::Let { expr: e, .. } => {
                self.resolve_expr(&Self::child_expr_red(&expr_red.span, e), current_scope)?;
            }
            GreenExprKind::If {
                cond, then_expr, elifs, else_expr
            } => {
                self.resolve_expr(&Self::child_expr_red(&expr_red.span, cond), current_scope)?;
                self.resolve_expr(&Self::child_expr_red(&expr_red.span, then_expr), current_scope)?;
                for elif in elifs {
                    self.resolve_expr(&Self::child_expr_red(&expr_red.span, &elif.cond), current_scope)?;
                    self.resolve_expr(&Self::child_expr_red(&expr_red.span, &elif.body), current_scope)?;
                }
                if let Some(else_e) = else_expr {
                    self.resolve_expr(&Self::child_expr_red(&expr_red.span, else_e), current_scope)?;
                }
            }
            GreenExprKind::Return { expr: opt_expr } => {
                if let Some(e) = opt_expr {
                    self.resolve_expr(&Self::child_expr_red(&expr_red.span, e), current_scope)?;
                }
            }
        }
        Ok(())
    }

    fn resolve_name(
        &self,
        name: &str,
        current_scope: ScopeId,
        span: Span,
    ) -> Result<(), DiagMsg> {
        if self.scope_pool.lookup(current_scope, name).is_some() {
            Ok(())
        } else {
            Err(DiagMsg {
                title: format!("{:?}", NamePassError::UndefinedName),
                msg: "undefined name".to_string(),
                span,
            })
        }
    }
}

impl<'a> NamePassApi<'a> for NamePass<'a> {
    fn new(ast_module: &'a CrateAst) -> Self {
        Self {
            ast_module,
            scope_pool: ScopePool::new(),
            do_scope_map: HashMap::new(),
            fun_scope_map: HashMap::new(),
            source_to_file_unit: HashMap::new(),
            source_id_to_scope: HashMap::new(),
        }
    }

    fn build_scope(&mut self) -> Result<(), DiagMsg> {
        // 建立 source_id -> FileRedUnit 的映射
        for file_unit in &self.ast_module.file_units {
            self.source_to_file_unit.insert(file_unit.span.source_id, file_unit);
        }

        let crate_scope_id = self.scope_pool.push_scope(
            None,
            ScopeKind::Crate,
            None,
            None,
        );

        for file_unit in &self.ast_module.file_units {
            let file_scope_id = self.scope_pool.push_scope(
                Some(crate_scope_id),
                ScopeKind::File,
                None,
                None,
            );

            let file_source_id = file_unit.span.source_id;
            self.source_id_to_scope.insert(file_source_id, file_scope_id);

            // 添加模块符号
            let module_name = file_unit.green.name.node.as_ref().clone();
            self.scope_pool.add_symbol(
                crate_scope_id,
                module_name,
                Span {
                    source_id: file_source_id,
                    start_off: 0,
                    end_off: 0
                },
                SymbolKind::File { source_id: file_source_id },
            );

            // 处理顶层声明
            for decl_child in &file_unit.green.top_decls {
                let decl_red = Self::child_decl_red(&file_unit.span, decl_child);
                let decl = &decl_red.inner;
                let decl_span = decl_red.span.clone();
                let decl_name = decl.name.node.as_ref().clone();
                let visibility = decl.visibility.clone();

                match &decl.kind {
                    GreenDeclKind::Fun { params, block, .. } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::Function,
                        );

                        let fun_scope_id = self.scope_pool.push_scope(
                            Some(file_scope_id),
                            ScopeKind::Function,
                            Some(Arc::clone(&decl_red.inner)),
                            Some(decl_span.clone()),
                        );
                        self.fun_scope_map.insert(Arc::clone(&decl_red.inner), fun_scope_id);

                        // 参数符号
                        for param_child in params {
                            let param_span = {
                                let start = decl_red.span.start_off + param_child.relative_start;
                                Span {
                                    source_id: decl_red.span.source_id,
                                    start_off: start,
                                    end_off: start + param_child.node.text_len
                                }
                            };
                            let param_name = param_child.node.name.node.as_ref().clone();
                            self.scope_pool.add_symbol(
                                fun_scope_id,
                                param_name,
                                param_span,
                                SymbolKind::Local,
                            );
                        }

                        for stmt_child in block {
                            let stmt_red = Self::child_expr_red(&decl_red.span, stmt_child);
                            self.build_expr_scope(&stmt_red, fun_scope_id)?;
                        }
                    }

                    GreenDeclKind::FunDecl { params, .. } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::Function,
                        );
                    }

                    GreenDeclKind::TypeStruct { fields, generic_vars, .. } => {
                        let struct_scope_id = self.scope_pool.push_scope(
                            Some(file_scope_id),
                            ScopeKind::Struct,
                            Some(Arc::clone(&decl_red.inner)),
                            Some(decl_span.clone()),
                        );

                        for gv_child in generic_vars {
                            let gv_name = gv_child.node.name.node.as_ref().clone();
                            self.scope_pool.add_symbol(
                                struct_scope_id,
                                gv_name,
                                decl_span.clone(),
                                SymbolKind::Generic,
                            );
                        }

                        let mut field_ids = vec![];
                        for field_child in fields {
                            let field_span = {
                                let start = decl_red.span.start_off + field_child.relative_start;
                                Span{
                                    source_id: decl_red.span.source_id,
                                    start_off: start,
                                    end_off: start + field_child.node.text_len
                                }
                            };
                            let field_name = field_child.node.name.node.as_ref().clone();
                            field_ids.push(self.scope_pool.add_symbol_and_get_sym_id(
                                struct_scope_id,
                                field_name,
                                field_span,
                                SymbolKind::Field,
                            ));
                        }

                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::Struct { fields: field_ids },
                        );
                    }

                    GreenDeclKind::ADT { ctors, generic_vars, .. } => {
                        let adt_scope_id = self.scope_pool.push_scope(
                            Some(file_scope_id),
                            ScopeKind::Adt,
                            Some(Arc::clone(&decl_red.inner)),
                            Some(decl_span.clone()),
                        );

                        for gv_child in generic_vars {
                            let gv_name = gv_child.node.name.node.as_ref().clone();
                            self.scope_pool.add_symbol(
                                adt_scope_id,
                                gv_name,
                                decl_span.clone(),
                                SymbolKind::Generic,
                            );
                        }

                        let mut constructors = vec![];
                        for ctor_child in ctors {
                            let ctor_span = {
                                let start = decl_red.span.start_off + ctor_child.relative_start;
                                Span {
                                    source_id: decl_red.span.source_id,
                                    start_off: start,
                                    end_off: start + ctor_child.node.text_len
                                }
                            };
                            let ctor_name = ctor_child.node.name.node.as_ref().clone();
                            constructors.push(self.scope_pool.add_symbol_and_get_sym_id(
                                file_scope_id,
                                ctor_name,
                                ctor_span,
                                SymbolKind::Constructor,
                            ));
                        }
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::ADT { constructors },
                        );
                    }

                    GreenDeclKind::TypeAlias { .. } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::TypeAlias,
                        );
                    }

                    GreenDeclKind::CType => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::CTypeDef,
                        );
                    }

                    GreenDeclKind::External { .. } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::External,
                        );
                    }

                    GreenDeclKind::Abstract { methods, .. } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::Abstract,
                        );
                        let abs_scope_id = self.scope_pool.push_scope(
                            Some(file_scope_id),
                            ScopeKind::Abstract,
                            Some(Arc::clone(&decl_red.inner)),
                            Some(decl_span.clone()),
                        );
                        for method_child in methods {
                            let method_span = {
                                let start = decl_red.span.start_off + method_child.relative_start;
                                Span{
                                    source_id: decl_red.span.source_id,
                                    start_off: start,
                                    end_off: start + method_child.node.text_len
                                }
                            };
                            let method_name = method_child.node.name.node.as_ref().clone();
                            self.scope_pool.add_symbol(
                                abs_scope_id,
                                method_name,
                                method_span,
                                SymbolKind::Method,
                            );
                        }
                    }

                    _ => {}
                }
            }

            // 处理 require / use 导入
            for req_child in &file_unit.green.file_unit_requires {
                let req_red = {
                    let start = file_unit.span.start_off + req_child.relative_start;
                    let len = req_child.node.text_len;
                    RequireRedNode {
                        span: Span {
                            source_id: file_unit.span.source_id,
                            start_off: start,
                            end_off: start + len
                        },
                        green: Arc::clone(&req_child.node),
                    }
                };
                let req = &req_red.green;

                if let Some(first_seg) = req.path.first() {
                    let module_name = first_seg.node.as_ref().clone();
                    if let Some((module_sym, _)) = self.scope_pool.lookup(crate_scope_id, &module_name) {
                        if let SymbolKind::File { source_id: target_src } = &module_sym.kind {
                            let target_scope = self.source_id_to_scope[target_src];
                            let target_file = self.source_to_file_unit[target_src];

                            let names: Vec<String> = if req.only.is_empty() {
                                target_file.green.top_decls.iter()
                                    .filter(|d| matches!(d.node.visibility, Visibility::Public | Visibility::PublicExternal))
                                    .map(|d| d.node.name.node.as_ref().clone())
                                    .collect()
                            } else {
                                req.only.iter().map(|s| s.node.as_ref().clone()).collect()
                            };

                            if req.is_open {
                                let mut sym_ids = vec![];
                                for name in &names {
                                    if let Some((sym, _)) = self.scope_pool.lookup(target_scope, name) {
                                        sym_ids.push(sym.sym_id);
                                    }
                                }
                                let current_scope = self.scope_pool.get_scope_mut(file_scope_id);
                                for id in sym_ids {
                                    current_scope.symbols.push(id);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn resolve(&mut self) -> Result<(), DiagMsg> {
        for file_unit in &self.ast_module.file_units {
            for decl_child in &file_unit.green.top_decls {
                let decl_red = Self::child_decl_red(&file_unit.span, decl_child);
                let decl = &decl_red.inner;
                if let GreenDeclKind::Fun { block, .. } = &decl.kind {
                    if let Some(&fun_scope) = self.fun_scope_map.get(&decl_red.inner) {
                        for stmt_child in block {
                            let stmt_red = Self::child_expr_red(&decl_red.span, stmt_child);
                            self.resolve_expr(&stmt_red, fun_scope)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn pass(mut self) -> Result<NamePassResult, DiagMsg> {
        self.build_scope()?;
        self.resolve()?;
        Ok(NamePassResult {
            pool: self.scope_pool,
            do_scope_map: self.do_scope_map,
            fun_scope_map: self.fun_scope_map,
        })
    }
}