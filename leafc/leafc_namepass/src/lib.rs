use std::collections::HashMap;
use leafc_coreapi::ast::{AstModule, AtomExprNode, DeclNode, DeclNodeId, DeclNodeKind, ElseIf, ExprNode, ExprNodeId, ExprNodeKind, FileAst, Visibility};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::name_pass::{DoScopeMap, FunScopeMap, NamePassApi, NamePassError, NamePassResult};
use leafc_coreapi::scope::{LocalSymbol, Scope, ScopeId, ScopePool, TopScopeIds, FieldSymbol, CtorSymbol, MethodSymbol};
use leafc_coreapi::source::{SourceId, Span};

pub struct NamePass<'a> {
    ast_module: &'a AstModule,
    scope_pool: ScopePool,
    file_root_scopes: Vec<ScopeId>,          // 每个文件的根作用域
    file_top_decl_scopes: Vec<Vec<ScopeId>>, // 每个文件的顶层声明作用域列表
    file_symbols: Vec<HashMap<String, (usize, DeclNodeId)>>, // 每个文件的符号表，值表示 (file_id, decl_id)
    do_scope_map: DoScopeMap,
    fun_scope_map: FunScopeMap,
    current_file_id: usize, // 当前正在处理的文件索引
}

impl<'a> NamePass<'a> {
    fn handle_expr(&mut self, expr_id: ExprNodeId, current_scope: ScopeId, file_id: usize) -> Result<(), DiagMsg> {
        let main_expr = &self.ast_module.asts[file_id].expr_pool[expr_id];
        match &main_expr.kind {
            ExprNodeKind::Atom { .. } => {}

            ExprNodeKind::Binary { left, right, .. } => {
                self.handle_expr(*left, current_scope, file_id)?;
                self.handle_expr(*right, current_scope, file_id)?;
            }
            ExprNodeKind::Unary { right, .. } => {
                self.handle_expr(*right, current_scope, file_id)?;
            }
            ExprNodeKind::Call { callee, args, .. } => {
                self.handle_expr(*callee, current_scope, file_id)?;
                for arg in args {
                    self.handle_expr(*arg, current_scope, file_id)?;
                }
            }
            ExprNodeKind::UnsafeExternalCall { callee, args, .. } => {
                self.handle_expr(*callee, current_scope, file_id)?;
                for arg in args {
                    self.handle_expr(*arg, current_scope, file_id)?;
                }
            }
            ExprNodeKind::Member { left, .. } => {
                self.handle_expr(*left, current_scope, file_id)?;
            }
            ExprNodeKind::TypeCast { expr, .. } => {
                self.handle_expr(*expr, current_scope, file_id)?;
            }
            ExprNodeKind::Move { target, .. }
            | ExprNodeKind::Copy { target, .. }
            | ExprNodeKind::Ref { target, .. }
            | ExprNodeKind::MutRef { target, .. }
            | ExprNodeKind::Share { target, .. } => {
                self.handle_expr(*target, current_scope, file_id)?;
            }

            ExprNodeKind::Do { exprs, .. } => {
                let new_scope = Scope::Scope {
                    parent: Some(current_scope),
                    symbols: vec![],
                    children: vec![],
                    bind_to_ast: 0, // Do 表达式无对应声明，设为0
                };
                let new_scope_id = self.scope_pool.len();
                self.scope_pool.push(new_scope);

                self.do_scope_map.insert(expr_id, new_scope_id);

                if let Scope::Scope { children, .. } = &mut self.scope_pool[current_scope] {
                    children.push(new_scope_id);
                }

                for e in exprs {
                    self.handle_expr(*e, new_scope_id, file_id)?;
                }
            }

            ExprNodeKind::Let { name, expr, .. } => {
                if let Scope::Scope { symbols, .. } = &mut self.scope_pool[current_scope] {
                    symbols.push(LocalSymbol {
                        name: name.clone(),
                        def_span: main_expr.span.clone(),
                    });
                }
                self.handle_expr(*expr, current_scope, file_id)?;
            }

            ExprNodeKind::If { cond, then_expr, elifs, else_expr, .. } => {
                self.handle_expr(*cond, current_scope, file_id)?;
                self.handle_expr(*then_expr, current_scope, file_id)?;
                for ElseIf{cond, body} in elifs {
                    self.handle_expr(*cond, current_scope, file_id)?;
                    self.handle_expr(*body, current_scope, file_id)?;
                }
                if let Some(else_e) = else_expr {
                    self.handle_expr(*else_e, current_scope, file_id)?;
                }
            }
        }
        Ok(())
    }

    /// 遍历表达式，解析所有标识符引用
    fn resolve_expr(&self, expr_id: ExprNodeId, current_scope: ScopeId, file_id: usize) -> Result<(), DiagMsg> {
        let expr = &self.ast_module.asts[file_id].expr_pool[expr_id];
        match &expr.kind {
            ExprNodeKind::Atom { expr: atom_expr } => {
                if let AtomExprNode::Name { name, span } = atom_expr {
                    self.resolve_name(name, current_scope, span.clone(), file_id)?;
                }
            }
            ExprNodeKind::Binary { left, right, .. } => {
                self.resolve_expr(*left, current_scope, file_id)?;
                self.resolve_expr(*right, current_scope, file_id)?;
            }
            ExprNodeKind::Unary { right, .. } => self.resolve_expr(*right, current_scope, file_id)?,
            ExprNodeKind::Call { callee, args, .. } => {
                self.resolve_expr(*callee, current_scope, file_id)?;
                for &arg in args {
                    self.resolve_expr(arg, current_scope, file_id)?;
                }
            }
            ExprNodeKind::UnsafeExternalCall { callee, args, .. } => {
                // 外部调用不解析名称
            }
            ExprNodeKind::Member { left, .. } => self.resolve_expr(*left, current_scope, file_id)?,
            ExprNodeKind::TypeCast { expr, .. } => self.resolve_expr(*expr, current_scope, file_id)?,
            ExprNodeKind::Move { target, .. }
            | ExprNodeKind::Copy { target, .. }
            | ExprNodeKind::Ref { target, .. }
            | ExprNodeKind::MutRef { target, .. }
            | ExprNodeKind::Share { target, .. } => self.resolve_expr(*target, current_scope, file_id)?,
            ExprNodeKind::Do { .. } => {
                if let Some(&do_scope) = self.do_scope_map.get(&expr_id) {
                    if let ExprNodeKind::Do { exprs, .. } = &self.ast_module.asts[file_id].expr_pool[expr_id].kind {
                        for &e in exprs {
                            self.resolve_expr(e, do_scope, file_id)?;
                        }
                    }
                } else {
                    unreachable!()
                }
            }
            ExprNodeKind::Let { expr, .. } => self.resolve_expr(*expr, current_scope, file_id)?,
            ExprNodeKind::If { cond, then_expr, elifs, else_expr, .. } => {
                self.resolve_expr(*cond, current_scope, file_id)?;
                self.resolve_expr(*then_expr, current_scope, file_id)?;
                for ElseIf{cond, body} in elifs {
                    self.resolve_expr(*cond, current_scope, file_id)?;
                    self.resolve_expr(*body, current_scope, file_id)?;
                }
                if let Some(else_e) = else_expr {
                    self.resolve_expr(*else_e, current_scope, file_id)?;
                }
            }
        }
        Ok(())
    }

    fn resolve_name(&self, name: &str, current_scope: ScopeId, span: Span, file_id: usize) -> Result<(), DiagMsg> {
        // 1. 查找局部变量（沿作用域链）
        let mut scope_id = Some(current_scope);
        while let Some(id) = scope_id {
            let scope = &self.scope_pool[id];
            match scope {
                Scope::Scope { symbols, parent, .. } => {
                    if symbols.iter().rev().any(|s| s.name == name) {
                        return Ok(());
                    }
                    scope_id = *parent;
                }
                _ => break,
            }
        }

        // 2. 查找当前文件的顶层符号（本文件声明 + 导入的公开声明）
        if let Some(&(_src_file, _decl_id)) = self.file_symbols[file_id].get(name) {
            return Ok(());
        }

        // 3. 未找到
        Err(DiagMsg {
            title: format!("{:?}", NamePassError::UndefinedName),
            msg: "undefined name".to_string(),
            span,
            source: self.ast_module.asts[file_id].file,
        })
    }

    fn lookup_local_symbol(&self, name: &str, current_scope: ScopeId) -> Option<&LocalSymbol> {
        let mut scope_id = Some(current_scope);
        while let Some(id) = scope_id {
            let scope = &self.scope_pool[id];
            match scope {
                Scope::Scope { symbols, parent, .. } => {
                    // shadow
                    if let Some(sym) = symbols.iter().rev().find(|s| s.name == name) {
                        return Some(sym);
                    }
                    scope_id = *parent;
                }
                _ => break,
            }
        }
        None
    }
}

impl<'a> NamePassApi<'a> for NamePass<'a> {
    fn new(ast_module: &'a AstModule) -> Self {
        Self {
            ast_module,
            scope_pool: vec![],
            file_root_scopes: vec![],
            file_top_decl_scopes: vec![],
            file_symbols: vec![],
            do_scope_map: HashMap::new(),
            fun_scope_map: HashMap::new(),
            current_file_id: 0,
        }
    }

    fn pass_scope(&mut self) -> Result<(), DiagMsg> {
        let num_files = self.ast_module.asts.len();
        self.file_root_scopes = Vec::with_capacity(num_files);
        self.file_top_decl_scopes = Vec::with_capacity(num_files);
        self.file_symbols = Vec::with_capacity(num_files);

        let mut source_to_file = HashMap::new();

        // 第一步：为每个文件建立作用域和符号表
        for (file_id, file_ast) in self.ast_module.asts.iter().enumerate() {
            source_to_file.insert(file_ast.file, file_id);

            // 创建根作用域
            let root_scope = Scope::Scope {
                parent: None,
                symbols: vec![],
                children: vec![],
                bind_to_ast: usize::MAX, // 特殊值表示文件根
            };
            let root_scope_id = self.scope_pool.len();
            self.scope_pool.push(root_scope);
            self.file_root_scopes.push(root_scope_id);

            let mut top_decl_scopes = Vec::new();
            let mut symbol_map = HashMap::new();

            // 处理每个顶层声明
            for (decl_id, decl) in file_ast.decl_pool.iter().enumerate() {
                let scope = match &decl.kind {
                    DeclNodeKind::Fun { params, .. } => {
                        let symbols = params.iter().map(|p| LocalSymbol {
                            name: p.name.clone(),
                            def_span: p.span.clone(),
                        }).collect();
                        Scope::Scope {
                            parent: Some(root_scope_id),
                            symbols,
                            children: vec![],
                            bind_to_ast: decl_id,
                        }
                    }
                    DeclNodeKind::FunDecl { .. } => {
                        Scope::FunDecl {
                            name: decl.name.clone(),
                            bind_to_ast: decl_id,
                        }
                    }
                    DeclNodeKind::TypeStruct { fields, .. } => {
                        Scope::Struct {
                            name: decl.name.clone(),
                            bind_to_ast: decl_id,
                            fields: fields.iter().map(|f| FieldSymbol {
                                name: f.name.clone(),
                                def_span: f.span.clone(),
                            }).collect(),
                        }
                    }
                    DeclNodeKind::ADT { ctors, .. } => {
                        Scope::ADT {
                            name: decl.name.clone(),
                            bind_to_ast: decl_id,
                            ctors: ctors.iter().map(|c| CtorSymbol {
                                name: c.name.clone(),
                                def_span: c.span.clone(),
                            }).collect(),
                        }
                    }
                    DeclNodeKind::TypeAlias { .. } => {
                        Scope::TypeAlias {
                            name: decl.name.clone(),
                            bind_to_ast: decl_id,
                        }
                    }
                    DeclNodeKind::CType => {
                        Scope::CTypeDef {
                            name: decl.name.clone(),
                            bind_to_ast: decl_id,
                        }
                    }
                    DeclNodeKind::External { .. } => {
                        Scope::External {
                            name: decl.name.clone(),
                            bind_to_ast: decl_id,
                        }
                    }
                    DeclNodeKind::Abstract { methods, .. } => {
                        Scope::Abstract {
                            name: decl.name.clone(),
                            bind_to_ast: decl_id,
                            methods: methods.iter().map(|m| MethodSymbol {
                                name: m.name.clone(),
                                def_span: m.span.clone(),
                            }).collect(),
                        }
                    }
                    _ => continue,
                };

                let scope_id = self.scope_pool.len();
                self.scope_pool.push(scope);

                if let DeclNodeKind::Fun { .. } = &decl.kind {
                    self.fun_scope_map.insert(decl_id, scope_id);
                }

                if let Scope::Scope { children, .. } = &mut self.scope_pool[root_scope_id] {
                    children.push(scope_id);
                }

                top_decl_scopes.push(scope_id);
                symbol_map.insert(decl.name.clone(), (file_id, decl_id));

                if let DeclNodeKind::Fun { block, .. } = &decl.kind {
                    for &expr_id in block {
                        self.handle_expr(expr_id, scope_id, file_id)?;
                    }
                }
            }

            self.file_top_decl_scopes.push(top_decl_scopes);
            self.file_symbols.push(symbol_map);
        }

        // 处理 require，导入公开声明
        for (file_id, file_ast) in self.ast_module.asts.iter().enumerate() {
            for require in &file_ast.requires {
                // if require.is_open {
                //     if let Some(&target_file_id) = source_to_file.get(&require.target_source_id) {
                //         let target_decls = &self.ast_module.asts[target_file_id].decl_pool;
                //         for (decl_id, decl) in target_decls.iter().enumerate() {
                //             if let Visibility::Public | Visibility::PublicExternal = decl.visibility {
                //                 let name = &decl.name;
                //                 if self.file_symbols[file_id].contains_key(name) {
                //                     return Err(DiagMsg {
                //                         title: format!("{:?}", NamePassError::DuplicateDefinition),
                //                         msg: format!("duplicate definition of `{}`", name),
                //                         span: require.span.clone(),
                //                         source: file_ast.file,
                //                     });
                //                 }
                //                 self.file_symbols[file_id].insert(name.clone(), (target_file_id, decl_id));
                //             }
                //         }
                //     } else {
                //         return Err(DiagMsg {
                //             title: format!("{:?}", NamePassError::UndefinedModule),
                //             msg: "module not found".to_string(),
                //             span: require.span.clone(),
                //             source: file_ast.file,
                //         });
                //     }
                // }
                todo!()
            }
        }

        Ok(())
    }

    fn pass_name(&mut self) -> Result<(), DiagMsg> {
        for file_id in 0..self.ast_module.asts.len() {
            self.current_file_id = file_id;
            let file_ast = &self.ast_module.asts[file_id];
            for (decl_id, decl) in file_ast.decl_pool.iter().enumerate() {
                if let DeclNodeKind::Fun { block, .. } = &decl.kind {
                    // 获取该函数的作用域 ID
                    let scope_id = *self.fun_scope_map.get(&decl_id)
                        .expect("Function scope should have been recorded");
                    for &expr_id in block {
                        self.resolve_expr(expr_id, scope_id, file_id)?;
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
            top_scope_ids: &self.file_root_scopes,
            scope_pool: &self.scope_pool,
            do_scope_map: &self.do_scope_map,
            fun_scope_map: &self.fun_scope_map,
        })
    }
}