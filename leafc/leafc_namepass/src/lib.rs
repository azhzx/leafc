use std::collections::HashMap;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::ast::{
    CrateAst, AtomExprNode, DeclNode, DeclNodeId, DeclNodeKind, ExprNode, ExprNodeId,
    ExprNodeKind, Visibility,
};
use leafc_coreapi::name_pass::{DoScopeMap, FunScopeMap, NamePassApi, NamePassError, NamePassResult};
use leafc_coreapi::scope::{FieldDef, Scope, ScopeId, ScopeKind, ScopePool, Symbol, SymbolKind};
use leafc_coreapi::source::{SourceId, Span};

pub struct NamePass<'a> {
    ast_module: &'a CrateAst,
    scope_pool: ScopePool,

    /// DoExpr => Scope
    do_scope_map: DoScopeMap,

    /// FunDecl => Scope
    fun_scope_map: FunScopeMap,

    /// source_id => FileUnit
    source_to_file_unit: HashMap<SourceId, DeclNodeId>,
}

impl<'a> NamePass<'a> {
    fn source_id_from_scope(&self, scope_id: ScopeId) -> SourceId {
        let mut current = Some(scope_id);
        while let Some(sid) = current {
            let scope = self.scope_pool.get_scope(sid);
            if scope.kind == ScopeKind::File {
                let file_unit = &self.ast_module.decl_pool[scope.bind_to_ast.unwrap()];
                return file_unit.source_id;
            }
            current = scope.parent;
        }
        unreachable!()
    }
}

impl<'a> NamePass<'a> {
    /// 创建作用域并填充符号
    fn handle_expr(
        &mut self,
        expr_id: ExprNodeId,
        current_scope: ScopeId,
    ) -> Result<(), DiagMsg> {
        let main_expr = &self.ast_module.expr_pool[expr_id];
        match &main_expr.kind {
            ExprNodeKind::Atom { .. } => {}

            ExprNodeKind::Binary { left, right, .. } => {
                self.handle_expr(*left, current_scope)?;
                self.handle_expr(*right, current_scope)?;
            }
            ExprNodeKind::Unary { right, .. } => {
                self.handle_expr(*right, current_scope)?;
            }
            ExprNodeKind::Call { callee, args, .. } => {
                self.handle_expr(*callee, current_scope)?;
                for arg in args {
                    self.handle_expr(*arg, current_scope)?;
                }
            }
            ExprNodeKind::UnsafeExternalCall { callee, args, .. } => {
                self.handle_expr(*callee, current_scope)?;
                for arg in args {
                    self.handle_expr(*arg, current_scope)?;
                }
            }
            ExprNodeKind::Member { left, .. } => {
                self.handle_expr(*left, current_scope)?;
            }
            ExprNodeKind::TypeCast { expr, .. } => {
                self.handle_expr(*expr, current_scope)?;
            }
            ExprNodeKind::Move { target, .. }
            | ExprNodeKind::Copy { target, .. }
            | ExprNodeKind::Ref { target, .. }
            | ExprNodeKind::MutRef { target, .. }
            | ExprNodeKind::Share { target, .. } => {
                self.handle_expr(*target, current_scope)?;
            }

            ExprNodeKind::Do { exprs, .. } => {
                // 为 Do 表达式创建一个新的块作用域
                let new_scope_id = self.scope_pool.push_scope(
                    Some(current_scope),
                    ScopeKind::Block,
                    Some(expr_id),
                );
                self.do_scope_map.insert(expr_id, new_scope_id);

                for e in exprs {
                    self.handle_expr(*e, new_scope_id)?;
                }
            }

            ExprNodeKind::Let { name, expr, .. } => {
                // 在当前作用域添加局部符号
                self.scope_pool.add_symbol(
                    current_scope,
                    name.clone(),
                    main_expr.span.clone(),
                    SymbolKind::Local,
                );
                self.handle_expr(*expr, current_scope)?;
            }

            ExprNodeKind::If {
                cond,
                then_expr,
                elifs,
                else_expr,
                ..
            } => {
                self.handle_expr(*cond, current_scope)?;
                self.handle_expr(*then_expr, current_scope)?;
                for elif in elifs {
                    self.handle_expr(elif.cond, current_scope)?;
                    self.handle_expr(elif.body, current_scope)?;
                }
                if let Some(else_e) = else_expr {
                    self.handle_expr(*else_e, current_scope)?;
                }
            }
        }
        Ok(())
    }

    /// 解析所有标识符引用
    fn resolve_expr(
        &self,
        expr_id: ExprNodeId,
        current_scope: ScopeId,
    ) -> Result<(), DiagMsg> {
        let expr = &self.ast_module.expr_pool[expr_id];
        match &expr.kind {
            ExprNodeKind::Atom { expr: atom_expr } => {
                if let AtomExprNode::Name { name, span } = atom_expr {
                    self.resolve_name(name, current_scope, span.clone())?;
                }
            }
            ExprNodeKind::Binary { left, right, .. } => {
                self.resolve_expr(*left, current_scope)?;
                self.resolve_expr(*right, current_scope)?;
            }
            ExprNodeKind::Unary { right, .. } => {
                self.resolve_expr(*right, current_scope)?;
            }
            ExprNodeKind::Call { callee, args, .. } => {
                self.resolve_expr(*callee, current_scope)?;
                for &arg in args {
                    self.resolve_expr(arg, current_scope)?;
                }
            }
            ExprNodeKind::UnsafeExternalCall { .. } => {
                // 外部调用不解析名称
            }
            ExprNodeKind::Member { left, right } => {
                self.resolve_expr(*left, current_scope)?;

                let left_expr = &self.ast_module.expr_pool[*left];
                if let ExprNodeKind::Atom {
                    expr: AtomExprNode::Name { name, .. }
                } = &left_expr.kind {
                    if let Some((sym, _scope_id)) = self.scope_pool.lookup(
                        current_scope, name
                    ) {
                        match &sym.kind {
                            SymbolKind::File { source_id } => {
                                let file_unit_id = self.source_to_file_unit.get(source_id).ok_or_else(|| {
                                    DiagMsg {
                                        title: format!("{:?}", NamePassError::UndefinedModule),
                                        msg: format!("module `{}` not found", name),
                                        span: expr.span.clone(),
                                        source: self.source_id_from_scope(current_scope),
                                    }
                                })?;
                                let file_unit = &self.ast_module.decl_pool[*file_unit_id];
                                if let DeclNodeKind::FileUnit { top_decls } = &file_unit.kind {
                                    let has_public_member = top_decls.iter().any(|&decl_id| {
                                        let decl = &self.ast_module.decl_pool[decl_id];
                                        decl.name == *right && (decl.visibility == Visibility::Public || decl.visibility == Visibility::PublicExternal)
                                    });
                                    if !has_public_member {
                                        return Err(DiagMsg {
                                            title: format!("{:?}", NamePassError::UndefinedName),
                                            msg: format!("module `{}` has no public item `{}`", name, right),
                                            span: expr.span.clone(),
                                            source: self.source_id_from_scope(current_scope),
                                        });
                                    }
                                } else {
                                    return Err(DiagMsg {
                                        title: format!("{:?}", NamePassError::UndefinedModule),
                                        msg: format!("`{}` is not a module", name),
                                        span: expr.span.clone(),
                                        source: self.source_id_from_scope(current_scope),
                                    });
                                }
                            },
                            SymbolKind::Struct { fields } => {
                                // 在结构体字段中查找 right
                                if !fields.iter().any(|f| f.name == *right) {
                                    return Err(DiagMsg {
                                        title: format!("{:?}", NamePassError::UndefinedName),
                                        msg: format!("struct `{}` has no field `{}`", name, right),
                                        span: expr.span.clone(),
                                        source: self.source_id_from_scope(current_scope),
                                    });
                                }
                            },
                            SymbolKind::ADT => {
                                todo!()
                            },
                            _ => {
                                return Err(DiagMsg {
                                    title: format!("{:?}", NamePassError::InvalidMemberAccess),
                                    msg: format!("invalid member access"),
                                    span: expr.span.clone(),
                                    source: self.source_id_from_scope(current_scope),
                                });
                            }
                        }
                    }
                }
            }
            ExprNodeKind::TypeCast { expr, .. } => {
                self.resolve_expr(*expr, current_scope)?;
            }
            ExprNodeKind::Move { target, .. }
            | ExprNodeKind::Copy { target, .. }
            | ExprNodeKind::Ref { target, .. }
            | ExprNodeKind::MutRef { target, .. }
            | ExprNodeKind::Share { target, .. } => {
                self.resolve_expr(*target, current_scope)?;
            }
            ExprNodeKind::Do { .. } => {
                if let Some(&do_scope) = self.do_scope_map.get(&expr_id) {
                    if let ExprNodeKind::Do { exprs, .. } = &self.ast_module.expr_pool[expr_id].kind
                    {
                        for &e in exprs {
                            self.resolve_expr(e, do_scope)?;
                        }
                    }
                } else {
                    unreachable!()
                }
            }
            ExprNodeKind::Let { expr, .. } => {
                self.resolve_expr(*expr, current_scope)?;
            }
            ExprNodeKind::If {
                cond,
                then_expr,
                elifs,
                else_expr,
                ..
            } => {
                self.resolve_expr(*cond, current_scope)?;
                self.resolve_expr(*then_expr, current_scope)?;
                for elif in elifs {
                    self.resolve_expr(elif.cond, current_scope)?;
                    self.resolve_expr(elif.body, current_scope)?;
                }
                if let Some(else_e) = else_expr {
                    self.resolve_expr(*else_e, current_scope)?;
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
                source: self.source_id_from_scope(current_scope),
            })
        }
    }

    fn decl_kind_to_symbol_kind(kind: &DeclNodeKind) -> SymbolKind {
        match kind {
            DeclNodeKind::Fun { .. } => SymbolKind::Function,
            DeclNodeKind::FunDecl { .. } => SymbolKind::Function,
            DeclNodeKind::TypeStruct { fields, .. } => SymbolKind::Struct {
                fields: fields.iter().map(|f| FieldDef {
                    name: f.name.clone(),
                    def_span: f.span.clone(),
                }).collect(),
            },
            DeclNodeKind::ADT { ctors, .. } => SymbolKind::ADT,
            DeclNodeKind::TypeAlias { .. } => SymbolKind::TypeAlias,
            DeclNodeKind::CType => SymbolKind::CTypeDef,
            DeclNodeKind::External { .. } => SymbolKind::External,
            DeclNodeKind::Abstract { .. } => SymbolKind::Abstract,
            _ => SymbolKind::Local, // fallback
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
        }
    }

    fn pass_scope(&mut self) -> Result<(), DiagMsg> {
        // 建立 source_id -> FileUnit 声明的映射
        for (decl_id, decl) in self.ast_module.decl_pool.iter().enumerate() {
            if let DeclNodeKind::FileUnit { .. } = &decl.kind {
                self.source_to_file_unit.insert(decl.source_id, decl_id);
            }
        }

        let crate_scope_id = self.scope_pool.push_scope(
            None,
            ScopeKind::Crate,
            None,
        );

        for (decl_id, decl) in self.ast_module.decl_pool.iter().enumerate() {
            if let DeclNodeKind::FileUnit { top_decls } = &decl.kind {
                let file_scope_id = self.scope_pool.push_scope(
                    Some(crate_scope_id),
                    ScopeKind::File,
                    Some(decl_id),
                );

                self.scope_pool.add_symbol(
                    crate_scope_id,
                    decl.name.clone(),
                    decl.span.clone(),
                   SymbolKind::File { source_id: decl.source_id },
                );

                // 处理该文件内的顶层声明
                for &inner_decl_id in top_decls {
                    let inner_decl = &self.ast_module.decl_pool[inner_decl_id];
                    let decl_span = inner_decl.span.clone();
                    let decl_name = inner_decl.name.clone();
                    let visibility = inner_decl.visibility.clone();

                    match &inner_decl.kind {
                        DeclNodeKind::Fun { params, block, .. } => {
                            self.scope_pool.add_symbol(
                                file_scope_id,
                                decl_name.clone(),
                                decl_span.clone(),
                                SymbolKind::Function
                            );

                            let fun_scope_id = self.scope_pool.push_scope(
                                Some(file_scope_id),
                                ScopeKind::Function,
                                Some(inner_decl_id),
                            );
                            self.fun_scope_map.insert(inner_decl_id, fun_scope_id);

                            // 添加参数符号
                            for param in params {
                                self.scope_pool.add_symbol(
                                    fun_scope_id,
                                    param.name.clone(),
                                    param.span.clone(),
                                    SymbolKind::Local,
                                );
                            }

                            // 处理函数体
                            for &expr_id in block {
                                self.handle_expr(expr_id, fun_scope_id)?;
                            }
                        }

                        DeclNodeKind::TypeStruct { fields, .. } => {
                            self.scope_pool.add_symbol(
                                file_scope_id,
                                decl_name.clone(),
                                decl_span.clone(),
                                SymbolKind::Struct {
                                        fields: fields.iter().map(|f| FieldDef {
                                            name: f.name.clone(),
                                            def_span: f.span.clone(),
                                        }).collect(),
                                },
                            );
                        }

                        DeclNodeKind::ADT { ctors, .. } => {
                            self.scope_pool.add_symbol(
                                file_scope_id,
                                decl_name.clone(),
                                decl_span.clone(),
                               SymbolKind::ADT,
                            );
                            let adt_scope_id = self.scope_pool.push_scope(
                                Some(file_scope_id),
                                ScopeKind::Adt,
                                Some(inner_decl_id),
                            );
                            for ctor in ctors {
                                self.scope_pool.add_symbol(
                                    adt_scope_id,
                                    ctor.name.clone(),
                                    ctor.span.clone(),
                                    SymbolKind::Constructor,
                                );
                            }
                        }

                        DeclNodeKind::TypeAlias { .. } => {
                            self.scope_pool.add_symbol(
                                file_scope_id,
                                decl_name.clone(),
                                decl_span.clone(),
                                SymbolKind::TypeAlias
                            );
                        }

                        DeclNodeKind::CType => {
                            self.scope_pool.add_symbol(
                                file_scope_id,
                                decl_name.clone(),
                                decl_span.clone(),
                                SymbolKind::CTypeDef
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
                                Some(inner_decl_id),
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
            }
        }

        // 外部 require在后续阶段处理
        Ok(())
    }

    fn pass_name(&mut self) -> Result<(), DiagMsg> {
        for (decl_id, decl) in self.ast_module.decl_pool.iter().enumerate() {
            if let DeclNodeKind::Fun { block, .. } = &decl.kind {
                if let Some(&fun_scope) = self.fun_scope_map.get(&decl_id) {
                    for &expr_id in block {
                        self.resolve_expr(expr_id, fun_scope)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn pass(&mut self) -> Result<NamePassResult, DiagMsg> {
        self.pass_scope()?;
        self.pass_name()?;
        Ok(NamePassResult {
            pool: &self.scope_pool,
            do_scope_map: &self.do_scope_map,
            fun_scope_map: &self.fun_scope_map,
        })
    }
}