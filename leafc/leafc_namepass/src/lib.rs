use std::collections::HashMap;
use std::sync::Arc;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::ast::{CrateAst, AtomExprNode, DeclNode, DeclNodeKind, ExprNode, ExprNodeKind, ExprRedNode, DeclRedNode, FileRedUnit, Visibility, Require};
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
    fn collect_member_path(expr: &ExprRedNode) -> Option<(String, Vec<(String, Span)>)> {
        match &expr.inner.kind {
            ExprNodeKind::Atom { expr: AtomExprNode::Name { name } } => {
                Some((name.clone(), vec![]))
            }
            ExprNodeKind::Member { left, right } => {
                let (base, mut segs) = Self::collect_member_path(left)?;
                segs.push((right.clone(), expr.span.clone()));
                Some((base, segs))
            }
            _ => None, // 非简单路径，不静态检查
        }
    }

    fn check_member_path(
        &self,
        base_name: &str,
        segments: &[(String, Span)],
        current_scope: ScopeId,
    ) -> Result<(), DiagMsg> {
        // 查找基础名
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
                    // 获取文件对应的作用域
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
                        let has_public = file_unit.top_decls.iter().any(|decl_red| {
                            let decl = &decl_red.inner;
                            decl.name == *seg_name
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

    /// 创建作用域并填充符号
    fn build_expr_scope(
        &mut self,
        expr_red: &ExprRedNode,
        current_scope: ScopeId,
    ) -> Result<(), DiagMsg> {
        let main_expr = &expr_red.inner;
        match &main_expr.kind {
            ExprNodeKind::Atom { .. } => {}

            ExprNodeKind::Binary { left, right, .. } => {
                self.build_expr_scope(left, current_scope)?;
                self.build_expr_scope(right, current_scope)?;
            }
            ExprNodeKind::Unary { right, .. } => {
                self.build_expr_scope(right, current_scope)?;
            }
            ExprNodeKind::Call { callee, args, .. } => {
                self.build_expr_scope(callee, current_scope)?;
                for arg in args {
                    self.build_expr_scope(arg, current_scope)?;
                }
            }
            ExprNodeKind::UnsafeExternalCall { callee, args, .. } => {
                self.build_expr_scope(callee, current_scope)?;
                for arg in args {
                    self.build_expr_scope(arg, current_scope)?;
                }
            }
            ExprNodeKind::Member { left, .. } => {
                self.build_expr_scope(left, current_scope)?;
            }
            ExprNodeKind::TypeCast { expr, into_type } => {
                self.build_expr_scope(expr, current_scope)?;
                self.build_expr_scope(into_type, current_scope)?;
            }
            ExprNodeKind::Move { target, .. }
            | ExprNodeKind::Copy { target, .. }
            | ExprNodeKind::Ref { target, .. }
            | ExprNodeKind::MutRef { target, .. }
            | ExprNodeKind::Share { target, .. } => {
                self.build_expr_scope(target, current_scope)?;
            }

            ExprNodeKind::Do { exprs, .. } => {
                // 为 Do 表达式创建一个新的块作用域
                let new_scope_id = self.scope_pool.push_scope(
                    Some(current_scope),
                    ScopeKind::Block,
                    None,
                    Some(expr_red.span.clone()),
                );
                self.do_scope_map.insert(expr_red.inner.clone(), new_scope_id);

                for e in exprs {
                    self.build_expr_scope(e, new_scope_id)?;
                }
            }

            ExprNodeKind::Let { name, expr, .. } => {
                //
                self.scope_pool.add_symbol(
                    current_scope,
                    name.clone(),
                    expr.span.clone(),
                    SymbolKind::Local,
                );
                self.build_expr_scope(expr, current_scope)?;
            }

            ExprNodeKind::If {
                cond,
                then_expr,
                elifs,
                else_expr,
                ..
            } => {
                self.build_expr_scope(cond, current_scope)?;
                self.build_expr_scope(then_expr, current_scope)?;
                for elif in elifs {
                    self.build_expr_scope(&elif.cond, current_scope)?;
                    self.build_expr_scope(&elif.body, current_scope)?;
                }
                if let Some(else_e) = else_expr {
                    self.build_expr_scope(else_e, current_scope)?;
                }
            },
            ExprNodeKind::Return { expr } => {
                if let Some(e) = expr {
                    self.build_expr_scope(e, current_scope)?;
                }
            }
        }
        Ok(())
    }

    /// 解析所有标识符引用
    fn resolve_expr(
        &self,
        expr_red: &ExprRedNode,
        current_scope: ScopeId,
    ) -> Result<(), DiagMsg> {
        let expr = &expr_red.inner;
        match &expr.kind {
            ExprNodeKind::Atom { expr: atom_expr } => {
                if let AtomExprNode::Name { name } = atom_expr {
                    self.resolve_name(name, current_scope, expr_red.span.clone())?;
                }
            }
            ExprNodeKind::Binary { left, right, .. } => {
                self.resolve_expr(left, current_scope)?;
                self.resolve_expr(right, current_scope)?;
            }
            ExprNodeKind::Unary { right, .. } => {
                self.resolve_expr(right, current_scope)?;
            }
            ExprNodeKind::Call { callee, args, .. } => {
                self.resolve_expr(callee, current_scope)?;
                for arg in args {
                    self.resolve_expr(arg, current_scope)?;
                }
            }
            ExprNodeKind::UnsafeExternalCall { .. } => {
                // 外部调用不解析名称
            }
            ExprNodeKind::Member { left, .. } => {
                self.resolve_expr(left, current_scope)?;
                if let Some((base, segs)) = Self::collect_member_path(expr_red) {
                    if !segs.is_empty() {
                        self.check_member_path(&base, &segs, current_scope)?;
                    }
                }
            }
            ExprNodeKind::TypeCast { expr, into_type } => {
                self.resolve_expr(expr, current_scope)?;
                self.resolve_expr(into_type, current_scope)?;
            }
            ExprNodeKind::Move { target, .. }
            | ExprNodeKind::Copy { target, .. }
            | ExprNodeKind::Ref { target, .. }
            | ExprNodeKind::MutRef { target, .. }
            | ExprNodeKind::Share { target, .. } => {
                self.resolve_expr(target, current_scope)?;
            }
            ExprNodeKind::Do { .. } => {
                if let Some(&do_scope) = self.do_scope_map.get(&expr_red.inner) {
                    if let ExprNodeKind::Do { exprs, .. } = &expr.kind {
                        for e in exprs {
                            self.resolve_expr(e, do_scope)?;
                        }
                    }
                } else {
                    unreachable!()
                }
            }
            ExprNodeKind::Let { expr, .. } => {
                self.resolve_expr(expr, current_scope)?;
            }
            ExprNodeKind::If {
                cond,
                then_expr,
                elifs,
                else_expr,
                ..
            } => {
                self.resolve_expr(cond, current_scope)?;
                self.resolve_expr(then_expr, current_scope)?;
                for elif in elifs {
                    self.resolve_expr(&elif.cond, current_scope)?;
                    self.resolve_expr(&elif.body, current_scope)?;
                }
                if let Some(else_e) = else_expr {
                    self.resolve_expr(else_e, current_scope)?;
                }
            },
            ExprNodeKind::Return { expr } => {
                if let Some(e) = expr {
                    self.resolve_expr(e, current_scope)?;
                }
            }
        }
        Ok(())
    }

    /// 解析名称
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
                Some(file_unit.span.clone()),
            );

            self.source_id_to_scope.insert(file_unit.span.source_id, file_scope_id);

            self.scope_pool.add_symbol(
                crate_scope_id,
                file_unit.name.clone(),
                file_unit.span.clone(),
                SymbolKind::File { source_id: file_unit.span.source_id },
            );

            // 处理该文件内的顶层声明
            for decl_red in &file_unit.top_decls {
                let decl = &decl_red.inner;
                let decl_span = decl_red.span.clone();
                let decl_name = decl.name.clone();
                let visibility = decl.visibility.clone();

                match &decl.kind {
                    DeclNodeKind::Fun { params, block, .. } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::Function,
                        );

                        let fun_scope_id = self.scope_pool.push_scope(
                            Some(file_scope_id),
                            ScopeKind::Function,
                            Some(decl_red.inner.clone()),
                            Some(decl_span.clone()),
                        );
                        self.fun_scope_map.insert(decl_red.inner.clone(), fun_scope_id);

                        for param in params {
                            self.scope_pool.add_symbol(
                                fun_scope_id,
                                param.name.clone(),
                                param.span.clone(),
                                SymbolKind::Local,
                            );
                        }

                        for expr_red in block {
                            self.build_expr_scope(expr_red, fun_scope_id)?;
                        }
                    }

                    DeclNodeKind::TypeStruct { fields, generic_vars, .. } => {
                        let struct_scope_id = self.scope_pool.push_scope(
                            Some(file_scope_id),
                            ScopeKind::Struct,
                            Some(decl_red.inner.clone()),
                            Some(decl_span.clone()),
                        );

                        for gv in generic_vars {
                            self.scope_pool.add_symbol(
                                struct_scope_id,
                                gv.name.clone(),
                                decl_span.clone(),
                                SymbolKind::Generic,
                            );
                        }

                        let mut field_ids = vec![];
                        for field in fields {
                            field_ids.push(self.scope_pool.add_symbol_and_get_sym_id(
                                struct_scope_id,
                                field.name.clone(),
                                field.span.clone(),
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

                    DeclNodeKind::ADT { ctors, generic_vars, .. } => {
                        let adt_scope_id = self.scope_pool.push_scope(
                            Some(file_scope_id),
                            ScopeKind::Adt,
                            Some(decl_red.inner.clone()),
                            Some(decl_span.clone()),
                        );

                        for gv in generic_vars {
                            self.scope_pool.add_symbol(
                                adt_scope_id,
                                gv.name.clone(),
                                decl_span.clone(),
                                SymbolKind::Generic,
                            );
                        }

                        let mut constructors = vec![];
                        for ctor in ctors {
                            constructors.push(self.scope_pool.add_symbol_and_get_sym_id(
                                file_scope_id,
                                ctor.name.clone(),
                                ctor.span.clone(),
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

                    DeclNodeKind::TypeAlias { .. } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::TypeAlias,
                        );
                    }

                    DeclNodeKind::CType => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::CTypeDef,
                        );
                    }

                    DeclNodeKind::External { .. } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::External,
                        );
                    }

                    DeclNodeKind::FunDecl { .. } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::Function,
                        );
                    }

                    DeclNodeKind::Abstract { methods, .. } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.clone(),
                            decl_span.clone(),
                            SymbolKind::Abstract,
                        );
                        let abs_scope_id = self.scope_pool.push_scope(
                            Some(file_scope_id),
                            ScopeKind::Abstract,
                            Some(decl_red.inner.clone()),
                            Some(decl_span.clone()),
                        );
                        for method in methods {
                            self.scope_pool.add_symbol(
                                abs_scope_id,
                                method.name.clone(),
                                method.span.clone(),
                                SymbolKind::Method,
                            );
                        }
                    }

                    _ => {}
                }
            }

            for req in &file_unit.file_unit_requires {
                // todo: 暂只支持单级模块名
                let module_name = &req.path[0];
                if let Some((module_sym, _)) = self.scope_pool.lookup(crate_scope_id, module_name) {
                    if let SymbolKind::File { source_id: target_src } = &module_sym.kind {
                        let target_scope = self.source_id_to_scope[target_src];
                        let target_file = self.source_to_file_unit[target_src];

                        let names: Vec<String> = if req.only.is_empty() {
                            target_file.top_decls.iter()
                                .filter(|d| matches!(d.inner.visibility, Visibility::Public | Visibility::PublicExternal))
                                .map(|d| d.inner.name.clone())
                                .collect()
                        } else {
                            req.only.clone()
                        };

                        // open
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

        Ok(())
    }

    fn resolve(&mut self) -> Result<(), DiagMsg> {
        for file_unit in &self.ast_module.file_units {
            for decl_red in &file_unit.top_decls {
                let decl = &decl_red.inner;
                if let DeclNodeKind::Fun { block, .. } = &decl.kind {
                    if let Some(&fun_scope) = self.fun_scope_map.get(&decl_red.inner) {
                        for expr_red in block {
                            self.resolve_expr(expr_red, fun_scope)?;
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