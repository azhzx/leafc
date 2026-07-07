use std::collections::HashMap;
use leafc_coreapi::ast::{
    AtomExprNode, CaseMode, DeclNode, DeclNodeId, ExprNode, ExprNodeId, FileAst, Unpack,
};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::name_pass::{NamePassApi, NamePassError};
use leafc_coreapi::scope::{
    LocalSymbol, Scope, ScopeId, ScopePool, TopScopePool,
};
use leafc_coreapi::source::{SourceId, Span};

pub struct NamePass<'a> {
    ast: &'a FileAst,
    top_scope: TopScopePool,
    scope_pool: ScopePool,
    // DoExpr => Scope
    do_scope_map: HashMap<ExprNodeId, ScopeId>,
}

impl<'a> NamePass<'a> {
    fn handle_expr(&mut self, expr_id: ExprNodeId, current_scope: ScopeId) -> Result<(), DiagMsg> {
        let expr = self.ast.expr_pool[expr_id].clone();
        match expr {
            ExprNode::Atom { .. } => {}

            ExprNode::Binary { left, right, .. } => {
                self.handle_expr(left, current_scope)?;
                self.handle_expr(right, current_scope)?;
            }
            ExprNode::Unary { right, .. } => {
                self.handle_expr(right, current_scope)?;
            }
            ExprNode::Call { callee, args, .. } => {
                self.handle_expr(callee, current_scope)?;
                for arg in args {
                    self.handle_expr(arg, current_scope)?;
                }
            }
            ExprNode::UnsafeExternalCall { callee, args, .. } => {
                self.handle_expr(callee, current_scope)?;
                for arg in args {
                    self.handle_expr(arg, current_scope)?;
                }
            }
            ExprNode::Member { left, .. } => {
                self.handle_expr(left, current_scope)?;
            }
            ExprNode::TypeCast { expr, .. } => {
                self.handle_expr(expr, current_scope)?;
            }
            ExprNode::Move { target, .. }
            | ExprNode::Copy { target, .. }
            | ExprNode::Ref { target, .. }
            | ExprNode::MutRef { target, .. }
            | ExprNode::Share { target, .. } => {
                self.handle_expr(target, current_scope)?;
            }

            ExprNode::Do { exprs, .. } => {
                let new_scope = Scope::Scope {
                    parent: Some(current_scope),
                    symbols: vec![],
                    children: vec![],
                    bind_to_ast: expr_id,
                };
                let new_scope_id = self.scope_pool.len();
                self.scope_pool.push(new_scope);

                self.do_scope_map.insert(expr_id, new_scope_id);

                if let Scope::Scope { children, .. } = &mut self.scope_pool[current_scope] {
                    children.push(new_scope_id);
                }

                for e in exprs {
                    self.handle_expr(e, new_scope_id)?;
                }
            }

            ExprNode::Let { name, expr, span, .. } => {
                if let Scope::Scope { symbols, .. } = &mut self.scope_pool[current_scope] {
                    symbols.push(LocalSymbol {
                        name: name.clone(),
                        def_span: span.clone(),
                    });
                }
                self.handle_expr(expr, current_scope)?;
            }

            ExprNode::If { cond, then_expr, elifs, else_expr, .. } => {
                self.handle_expr(cond, current_scope)?;
                self.handle_expr(then_expr, current_scope)?;
                for (elif_cond, elif_body) in elifs {
                    self.handle_expr(elif_cond, current_scope)?;
                    self.handle_expr(elif_body, current_scope)?;
                }
                if let Some(else_e) = else_expr {
                    self.handle_expr(else_e, current_scope)?;
                }
            }

            ExprNode::Match { .. } => {
                todo!()
            }
        }
        Ok(())
    }

    /// 遍历表达式，解析所有标识符引用
    fn resolve_expr(&self, expr_id: ExprNodeId, current_scope: ScopeId) -> Result<(), DiagMsg> {
        let expr = &self.ast.expr_pool[expr_id];
        match expr {
            ExprNode::Atom { expr: atom_id, span } => {
                if let AtomExprNode::Name { name, .. } = &self.ast.atom_expr_pool[*atom_id] {
                    self.resolve_name(name, current_scope, span.clone())?;
                }
            }
            ExprNode::Binary { left, right, .. } => {
                self.resolve_expr(*left, current_scope)?;
                self.resolve_expr(*right, current_scope)?;
            }
            ExprNode::Unary { right, .. } => self.resolve_expr(*right, current_scope)?,
            ExprNode::Call { callee, args, .. } => {
                self.resolve_expr(*callee, current_scope)?;
                for &arg in args {
                    self.resolve_expr(arg, current_scope)?;
                }
            }
            ExprNode::UnsafeExternalCall { callee, args, .. } => {
                // self.resolve_expr(*callee, current_scope)?;
                // for &arg in args {
                //     self.resolve_expr(arg, current_scope)?;
                // }
            }
            ExprNode::Member { left, .. } => self.resolve_expr(*left, current_scope)?,
            ExprNode::TypeCast { expr, .. } => self.resolve_expr(*expr, current_scope)?,
            ExprNode::Move { target, .. }
            | ExprNode::Copy { target, .. }
            | ExprNode::Ref { target, .. }
            | ExprNode::MutRef { target, .. }
            | ExprNode::Share { target, .. } => self.resolve_expr(*target, current_scope)?,
            ExprNode::Do { .. } => {
                if let Some(&do_scope) = self.do_scope_map.get(&expr_id) {
                    if let ExprNode::Do { exprs, .. } = &self.ast.expr_pool[expr_id] {
                        for &e in exprs {
                            self.resolve_expr(e, do_scope)?;
                        }
                    }
                } else {
                    unreachable!()
                }
            }
            ExprNode::Let { expr, .. } => self.resolve_expr(*expr, current_scope)?,
            ExprNode::If { cond, then_expr, elifs, else_expr, .. } => {
                self.resolve_expr(*cond, current_scope)?;
                self.resolve_expr(*then_expr, current_scope)?;
                for (elif_cond, elif_body) in elifs {
                    self.resolve_expr(*elif_cond, current_scope)?;
                    self.resolve_expr(*elif_body, current_scope)?;
                }
                if let Some(else_e) = else_expr {
                    self.resolve_expr(*else_e, current_scope)?;
                }
            }
            ExprNode::Match { expr, cases, default_case, .. } => {
                todo!()
            }
        }
        Ok(())
    }

    fn resolve_name(&self, name: &str, current_scope: ScopeId, span: Span) -> Result<(), DiagMsg> {
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

        for &top_id in &self.top_scope {
            let scope = &self.scope_pool[top_id];
            match scope {
                Scope::Struct { name: n, .. }
                | Scope::ADT { name: n, .. }
                | Scope::TypeAlias { name: n, .. }
                | Scope::CTypeDef { name: n, .. }
                | Scope::External { name: n, .. }
                | Scope::FunDecl { name: n, .. }
                | Scope::Abstract { name: n, .. } => {
                    if n == name {
                        return Ok(());
                    }
                }
                Scope::Scope { bind_to_ast, .. } => {
                    // 在这里只能是 DeclNodeId函数声明 (by azhz)
                    if let Some(decl) = self.ast.decl_pool.get(*bind_to_ast) {
                        if let DeclNode::Fun { name: n, .. } = decl {
                            if n == name {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }

        Err(DiagMsg {
            title: format!("{:?}", NamePassError::UndefinedName),
            msg: "undefined name".to_string(),
            span,
            source: self.ast.file,
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
    fn new(ast: &'a FileAst) -> Self {
        Self {
            ast,
            top_scope: vec![],
            scope_pool: vec![],
            do_scope_map: HashMap::new(),
        }
    }

    fn pass_scope(&mut self) -> Result<(), DiagMsg> {
        for index in 0..self.ast.decl_pool.len() {
            match self.ast.decl_pool[index].clone() {
                DeclNode::Fun { name: _, params, block, .. } => {
                    let fun_scope = Scope::Scope {
                        parent: None,
                        symbols: params.iter().map(|p| LocalSymbol {
                            name: p.name.clone(),
                            def_span: p.span.clone(),
                        }).collect(),
                        children: vec![],
                        bind_to_ast: index, // 记录对应的声明索引
                    };
                    let fun_scope_id = self.scope_pool.len();
                    self.scope_pool.push(fun_scope);
                    self.top_scope.push(fun_scope_id);

                    for expr_id in block {
                        self.handle_expr(expr_id, fun_scope_id)?;
                    }
                }
                DeclNode::TypeStruct { name, fields, .. } => {
                    let scope = Scope::Struct {
                        name: name.clone(),
                        bind_to_ast: index,
                        fields: fields.iter().map(|f| leafc_coreapi::scope::FieldSymbol {
                            name: f.name.clone(),
                            def_span: f.span.clone(),
                        }).collect(),
                    };
                    let id = self.scope_pool.len();
                    self.scope_pool.push(scope);
                    self.top_scope.push(id);
                }
                DeclNode::ADT { name, ctors, .. } => {
                    let scope = Scope::ADT {
                        name: name.clone(),
                        bind_to_ast: index,
                        ctors: ctors.iter().map(|c| leafc_coreapi::scope::CtorSymbol {
                            name: c.name.clone(),
                            def_span: c.span.clone(),
                        }).collect(),
                    };
                    let id = self.scope_pool.len();
                    self.scope_pool.push(scope);
                    self.top_scope.push(id);
                }
                DeclNode::TypeAlias { name, .. } => {
                    let scope = Scope::TypeAlias { name: name.clone(), bind_to_ast: index };
                    let id = self.scope_pool.len();
                    self.scope_pool.push(scope);
                    self.top_scope.push(id);
                }
                DeclNode::External { name, .. } => {
                    let scope = Scope::External { name: name.clone(), bind_to_ast: index };
                    let id = self.scope_pool.len();
                    self.scope_pool.push(scope);
                    self.top_scope.push(id);
                }
                DeclNode::CType { name, .. } => {
                    let scope = Scope::CTypeDef { name: name.clone(), bind_to_ast: index };
                    let id = self.scope_pool.len();
                    self.scope_pool.push(scope);
                    self.top_scope.push(id);
                }
                DeclNode::FunDecl { name, .. } => {
                    let scope = Scope::FunDecl { name: name.clone(), bind_to_ast: index };
                    let id = self.scope_pool.len();
                    self.scope_pool.push(scope);
                    self.top_scope.push(id);
                }
                DeclNode::Abstract { name, methods, .. } => {
                    let scope = Scope::Abstract {
                        name: name.clone(),
                        bind_to_ast: index,
                        methods: methods.iter().map(|m| leafc_coreapi::scope::MethodSymbol {
                            name: m.name.clone(),
                            def_span: m.span.clone(),
                        }).collect(),
                    };
                    let id = self.scope_pool.len();
                    self.scope_pool.push(scope);
                    self.top_scope.push(id);
                }
                _ => continue,
            }
        }
        Ok(())
    }

    fn pass_name(&mut self) -> Result<(), DiagMsg> {
        // 收集所有函数作用域及其体
        let fun_scopes: Vec<(ScopeId, Vec<ExprNodeId>)> = self
            .top_scope
            .iter()
            .filter_map(|&id| {
                if let Scope::Scope { bind_to_ast, .. } = &self.scope_pool[id] {
                    if let DeclNode::Fun { block, .. } = &self.ast.decl_pool[*bind_to_ast] {
                        return Some((id, block.clone()));
                    }
                }
                None
            })
            .collect();

        for (scope_id, body) in fun_scopes {
            for expr_id in body {
                self.resolve_expr(expr_id, scope_id)?;
            }
        }
        Ok(())
    }

    fn pass(&mut self) -> Result<(&TopScopePool, &ScopePool), DiagMsg> {
        self.pass_scope()?;
        self.pass_name()?;
        Ok((&self.top_scope, &self.scope_pool))
    }
}