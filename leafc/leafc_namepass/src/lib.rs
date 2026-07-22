use leafc_coreapi::ast::{child_decl_red, child_expr_red, child_span, AtomExprNode, CrateAst, DeclRedNode, ExprRedNode, FileRedUnit, GreenCatchClause, GreenChild, GreenDecl, GreenDeclKind, GreenExpr, GreenExprKind, GreenMatchArm, GreenPattern, GreenPureStaticPath, HasTextLen, RequireRedNode, TypeName, Visibility};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::name_pass::{ArmScopeMap, CatchScopeMap, DoScopeMap, FunScopeMap, NamePassApi, NamePassError, NamePassResult};
use leafc_coreapi::scope::{ScopeId, ScopeKind, ScopePool, SymbolKind};
use leafc_coreapi::source::{SourceId, Span};
use std::collections::HashMap;
use std::sync::Arc;
use leafc_coreapi::lang_items::{LangItems, STR_TO_BUILTIN};

pub struct NamePass<'a> {
    ast_module: &'a CrateAst,
    scope_pool: ScopePool,

    /// DoExpr => Scope
    do_scope_map: DoScopeMap,

    /// FunDecl => Scope
    fun_scope_map: FunScopeMap,

    /// MatchArm => Scope (for guard/body)
    arm_scope_map: ArmScopeMap,

    /// CatchClause => Scope (for body)
    catch_scope_map: CatchScopeMap,

    /// source_id => FileUnit
    source_to_file_unit: HashMap<SourceId, &'a FileRedUnit>,

    /// source_id => ScopeId
    source_id_to_scope: HashMap<SourceId, ScopeId>,

    /// lang item
    lang_items: LangItems,
}

impl<'a> NamePass<'a> {

    fn collect_pattern_bindings(pattern: &GreenPattern, bindings: &mut Vec<(String, Span)>) {
        match pattern {
            GreenPattern::Binding(ident) => {
                bindings.push((ident.name.clone(), Span {
                    source_id: 0,
                    start_off: 0,
                    end_off: 0,
                }));
            }
            GreenPattern::Constructor { args, .. } => {
                for arg in args {
                    Self::collect_pattern_bindings(&arg.node, bindings);
                }
            }
            _ => {}
        }
    }

     fn resolve_pattern(
        &self,
        pattern_child: &GreenChild<GreenPattern>,
        parent_span: &Span,
        current_scope: ScopeId,
    ) -> Result<(), DiagMsg> {

        let pattern = &pattern_child.node;

        match &**pattern {
            GreenPattern::Constructor { type_name, args, .. } => {
                if let TypeName::Named { path, .. } = type_name.node.as_ref() {

                    let path_child_span = child_span(parent_span, type_name.relative_start, type_name.node.text_len());
                    self.resolve_static_path_with_span(&path.node, &path_child_span, current_scope)?;

                }
                for arg in args {
                    let arg_child_span = child_span(parent_span, arg.relative_start, arg.node.text_len());
                    self.resolve_pattern(arg, &arg_child_span, current_scope)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn resolve_static_path_with_span(
        &self,
        path: &GreenPureStaticPath,
        path_span: &Span,
        current_scope: ScopeId,
    ) -> Result<(), DiagMsg> {

        let segments = &path.segments;
        if segments.is_empty() {
            return Ok(());
        }

        let first_seg = &segments[0];
        let first_name = &first_seg.node.name;

        let first_span = Span {
            source_id: path_span.source_id,
            start_off: path_span.start_off + first_seg.relative_start,
            end_off: path_span.start_off + first_seg.relative_start + first_seg.node.text_len(),
        };

        let (mut sym, mut sym_scope) = self.scope_pool
            .lookup(current_scope, first_name)
            .ok_or_else(|| DiagMsg {
                title: format!("{:?}", NamePassError::UndefinedName),
                msg: format!("undefined name `{}`", first_name),
                span: first_span.clone(),
            })?;

        for seg in segments.iter().skip(1) {

            let seg_name = &seg.node.name;
            let seg_span = Span {
                source_id: path_span.source_id,
                start_off: path_span.start_off + seg.relative_start,
                end_off: path_span.start_off + seg.relative_start + seg.node.text_len(),
            };

            match &sym.kind {

                SymbolKind::File { source_id } => {

                    let file_scope = self.source_id_to_scope[source_id];
                    if let Some((found_sym, found_scope)) = self.scope_pool.lookup(file_scope, seg_name) {
                        sym = found_sym;
                        sym_scope = found_scope;

                    } else {
                        return Err(DiagMsg {
                            title: format!("{:?}", NamePassError::UndefinedName),
                            msg: format!("module `{}` has no item `{}`", first_name, seg_name),
                            span: seg_span,
                        });
                    }
                }
                SymbolKind::Struct { .. } | SymbolKind::ADT { .. } => {
                    break;
                }
                _ => break,
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
                self.build_expr_scope(&child_expr_red(&expr_red.span, left), current_scope)?;
                self.build_expr_scope(&child_expr_red(&expr_red.span, right), current_scope)?;
            }

            GreenExprKind::Unary { op: _, right } => {
                self.build_expr_scope(&child_expr_red(&expr_red.span, right), current_scope)?;
            }

            GreenExprKind::Call { callee, args, .. } | GreenExprKind::UnsafeExternalCall { callee, args, .. } => {
                self.build_expr_scope(&child_expr_red(&expr_red.span, callee), current_scope)?;
                for arg in args {
                    self.build_expr_scope(&child_expr_red(&expr_red.span, arg), current_scope)?;
                }
            }

            GreenExprKind::StaticPath { .. } => {}

            GreenExprKind::MemberAccess { left, .. } => {
                self.build_expr_scope(&child_expr_red(&expr_red.span, left), current_scope)?;
            }

            GreenExprKind::MakeStruct { path, fields } => {
                self.build_expr_scope(&child_expr_red(&expr_red.span, path), current_scope)?;
                for field in fields {
                    self.build_expr_scope(&child_expr_red(&expr_red.span, &field.node.value), current_scope)?;
                }
            }

            GreenExprKind::TypeCast { expr: e, into_type } => {
                self.build_expr_scope(&child_expr_red(&expr_red.span, e), current_scope)?;
            }

            GreenExprKind::Move { target }
            | GreenExprKind::Copy { target }
            | GreenExprKind::Ref { target }
            | GreenExprKind::MutRef { target }
            | GreenExprKind::Share { target } => {
                self.build_expr_scope(&child_expr_red(&expr_red.span, target), current_scope)?;
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
                    self.build_expr_scope(&child_expr_red(&expr_red.span, e), new_scope_id)?;
                }
            }

            GreenExprKind::Let { name, expr: e, .. } => {
                let name_span = child_span(&expr_red.span, name.relative_start, name.node.text_len());
                self.scope_pool.add_symbol(
                    current_scope,
                    name.node.as_ref().clone().name,
                    name_span,
                    SymbolKind::Local,
                );
                self.build_expr_scope(&child_expr_red(&expr_red.span, e), current_scope)?;
            }

            GreenExprKind::If {
                cond, then_expr, elifs, else_expr
            } => {
                self.build_expr_scope(&child_expr_red(&expr_red.span, cond), current_scope)?;
                self.build_expr_scope(&child_expr_red(&expr_red.span, then_expr), current_scope)?;
                for elif in elifs {
                    self.build_expr_scope(&child_expr_red(&expr_red.span, &elif.cond), current_scope)?;
                    self.build_expr_scope(&child_expr_red(&expr_red.span, &elif.body), current_scope)?;
                }
                if let Some(else_e) = else_expr {
                    self.build_expr_scope(&child_expr_red(&expr_red.span, else_e), current_scope)?;
                }
            }

            GreenExprKind::Return { expr: opt_expr } => {
                if let Some(e) = opt_expr {
                    self.build_expr_scope(&child_expr_red(&expr_red.span, e), current_scope)?;
                }
            }

            GreenExprKind::Match { for_match, arms } => {

                self.build_expr_scope(&child_expr_red(&expr_red.span, for_match), current_scope)?;

                for arm_child in arms {

                    let arm = &arm_child.node;
                    let arm_span = child_span(&expr_red.span, arm_child.relative_start, arm.text_len);
                    let arm_scope = self.scope_pool.push_scope(
                        Some(current_scope),
                        ScopeKind::Block,
                        None,
                        Some(arm_span.clone()),
                    );
                    self.arm_scope_map.insert(arm.clone(), arm_scope);

                    let mut bindings = Vec::new();

                    Self::collect_pattern_bindings(&arm.pattern.node, &mut bindings);

                    let pattern_child = &arm.pattern;
                    let pattern_abs_span = child_span(&arm_span, pattern_child.relative_start, pattern_child.node.text_len());
                    for (name, _) in bindings {
                        self.scope_pool.add_symbol(arm_scope, name, pattern_abs_span.clone(), SymbolKind::Local);
                    }

                    if let Some(guard) = &arm.guard {
                        self.build_expr_scope(&child_expr_red(&arm_span, guard), arm_scope)?;
                    }
                    self.build_expr_scope(&child_expr_red(&arm_span, &arm.body), arm_scope)?;
                }
            }
            GreenExprKind::Is { expr: e, pattern } => {

                self.build_expr_scope(&child_expr_red(&expr_red.span, e), current_scope)?;

                let pattern_abs_span = child_span(&expr_red.span, pattern.relative_start, pattern.node.text_len());
                let mut bindings = Vec::new();
                Self::collect_pattern_bindings(&pattern.node, &mut bindings);
                for (name, _) in bindings {
                    self.scope_pool.add_symbol(current_scope, name, pattern_abs_span.clone(), SymbolKind::Local);
                }
            }
            GreenExprKind::Raise { effect_path, control_name, args } => {
                for arg in args {
                    self.build_expr_scope(&child_expr_red(&expr_red.span, arg), current_scope)?;
                }
            }
            GreenExprKind::With { handler_expr, clauses } => {
                self.build_expr_scope(&child_expr_red(&expr_red.span, handler_expr), current_scope)?;

                for clause_child in clauses {
                    let clause = &clause_child.node;
                    let clause_span = child_span(&expr_red.span, clause_child.relative_start, clause.text_len);
                    let catch_scope = self.scope_pool.push_scope(
                        Some(current_scope),
                        ScopeKind::Block,
                        None,
                        Some(clause_span.clone()),
                    );
                    self.catch_scope_map.insert(clause.clone(), catch_scope);

                    for param_child in &clause.params {
                        let param_abs_span = child_span(&clause_span, param_child.relative_start, param_child.node.text_len());
                        let mut bindings = Vec::new();
                        Self::collect_pattern_bindings(&param_child.node, &mut bindings);
                        for (name, _) in bindings {
                            self.scope_pool.add_symbol(catch_scope, name, param_abs_span.clone(), SymbolKind::Local);
                        }
                    }

                    self.build_expr_scope(&child_expr_red(&clause_span, &clause.body), catch_scope)?;
                }
            }
            GreenExprKind::Resume { expr: e } => {
                self.build_expr_scope(&child_expr_red(&expr_red.span, e), current_scope)?;
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
                self.resolve_expr(&child_expr_red(&expr_red.span, left), current_scope)?;
                self.resolve_expr(&child_expr_red(&expr_red.span, right), current_scope)?;
            }
            GreenExprKind::Unary { op: _, right } => {
                self.resolve_expr(&child_expr_red(&expr_red.span, right), current_scope)?;
            }
            GreenExprKind::Call { callee, args, .. } => {
                self.resolve_expr(&child_expr_red(&expr_red.span, callee), current_scope)?;
                for arg in args {
                    self.resolve_expr(&child_expr_red(&expr_red.span, arg), current_scope)?;
                }
            }
            GreenExprKind::UnsafeExternalCall { .. } => {}
            GreenExprKind::StaticPath { path } => {
                let path_span = child_span(&expr_red.span, path.relative_start, path.node.text_len);
                self.resolve_static_path_with_span(&path.node, &path_span, current_scope)?;
            }
            GreenExprKind::MemberAccess { left, .. } => {
                self.resolve_expr(&child_expr_red(&expr_red.span, left), current_scope)?;
            }
            GreenExprKind::MakeStruct { path, fields } => {
                self.resolve_expr(&child_expr_red(&expr_red.span, path), current_scope)?;
                for field in fields {
                    self.resolve_expr(&child_expr_red(&expr_red.span, &field.node.value), current_scope)?;
                }
            }
            GreenExprKind::TypeCast { expr: e, into_type } => {
                self.resolve_expr(&child_expr_red(&expr_red.span, e), current_scope)?;
            }
            GreenExprKind::Move { target }
            | GreenExprKind::Copy { target }
            | GreenExprKind::Ref { target }
            | GreenExprKind::MutRef { target }
            | GreenExprKind::Share { target } => {
                self.resolve_expr(&child_expr_red(&expr_red.span, target), current_scope)?;
            }
            GreenExprKind::Do { .. } => {
                if let Some(&do_scope) = self.do_scope_map.get(&expr_red.inner) {
                    if let GreenExprKind::Do { exprs, .. } = &expr.kind {
                        for e in exprs {
                            self.resolve_expr(&child_expr_red(&expr_red.span, e), do_scope)?;
                        }
                    }
                } else {
                    unreachable!()
                }
            }
            GreenExprKind::Let { expr: e, .. } => {
                self.resolve_expr(&child_expr_red(&expr_red.span, e), current_scope)?;
            }

            GreenExprKind::If {
                cond, then_expr, elifs, else_expr
            } => {

                self.resolve_expr(&child_expr_red(&expr_red.span, cond), current_scope)?;
                self.resolve_expr(&child_expr_red(&expr_red.span, then_expr), current_scope)?;
                for elif in elifs {
                    self.resolve_expr(&child_expr_red(&expr_red.span, &elif.cond), current_scope)?;
                    self.resolve_expr(&child_expr_red(&expr_red.span, &elif.body), current_scope)?;
                }
                if let Some(else_e) = else_expr {
                    self.resolve_expr(&child_expr_red(&expr_red.span, else_e), current_scope)?;
                }
            }

            GreenExprKind::Return { expr: opt_expr } => {
                if let Some(e) = opt_expr {
                    self.resolve_expr(&child_expr_red(&expr_red.span, e), current_scope)?;
                }
            }

            GreenExprKind::Match { for_match, arms } => {
                self.resolve_expr(&child_expr_red(&expr_red.span, for_match), current_scope)?;

                for arm_child in arms {

                    let arm = &arm_child.node;
                    let arm_span = child_span(&expr_red.span, arm_child.relative_start, arm.text_len);
                    let arm_scope = self.arm_scope_map.get(&arm.clone())
                        .expect("arm scope must be recorded during build");
                    let pattern_child = &arm.pattern;
                    let pattern_abs_span = child_span(&arm_span, pattern_child.relative_start, pattern_child.node.text_len());
                    self.resolve_pattern(pattern_child, &pattern_abs_span, *arm_scope)?;

                    if let Some(guard) = &arm.guard {
                        self.resolve_expr(&child_expr_red(&arm_span, guard), *arm_scope)?;
                    }
                    self.resolve_expr(&child_expr_red(&arm_span, &arm.body), *arm_scope)?;
                }
            }
            GreenExprKind::Is { expr: e, pattern } => {
                self.resolve_expr(&child_expr_red(&expr_red.span, e), current_scope)?;
                let pattern_abs_span = child_span(&expr_red.span, pattern.relative_start, pattern.node.text_len());
                self.resolve_pattern(pattern, &pattern_abs_span, current_scope)?;
            }
            GreenExprKind::Raise { effect_path, control_name, args } => {
                let path_span = child_span(&expr_red.span, effect_path.relative_start, effect_path.node.text_len);
                self.resolve_static_path_with_span(&effect_path.node, &path_span, current_scope)?;
                for arg in args {
                    self.resolve_expr(&child_expr_red(&expr_red.span, arg), current_scope)?;
                }
            }
            GreenExprKind::With { handler_expr, clauses } => {
                self.resolve_expr(&child_expr_red(&expr_red.span, handler_expr), current_scope)?;

                for clause_child in clauses {
                    let clause = &clause_child.node;
                    let clause_span = child_span(&expr_red.span, clause_child.relative_start, clause.text_len);
                    let catch_scope = self.catch_scope_map.get(&clause.clone())
                        .expect("catch scope must be recorded during build");

                    let control_path_child = &clause.control_static_path;
                    let path_span = child_span(&clause_span, control_path_child.relative_start, control_path_child.node.text_len);
                    self.resolve_static_path_with_span(&control_path_child.node, &path_span, *catch_scope)?;

                    // pattern
                    for param_child in &clause.params {
                        let param_abs_span = child_span(&clause_span, param_child.relative_start, param_child.node.text_len());
                        self.resolve_pattern(param_child, &param_abs_span, *catch_scope)?;
                    }

                    // body
                    self.resolve_expr(&child_expr_red(&clause_span, &clause.body), *catch_scope)?;
                }
            }
            GreenExprKind::Resume { expr: e } => {
                self.resolve_expr(&child_expr_red(&expr_red.span, e), current_scope)?;
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
            arm_scope_map: HashMap::new(),
            catch_scope_map: HashMap::new(),
            source_to_file_unit: HashMap::new(),
            source_id_to_scope: HashMap::new(),
            lang_items: LangItems::new(),
        }
    }

    fn build_scope(&mut self) -> Result<(), DiagMsg> {
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

            // Add module symbol
            let module_name = file_unit.green.name.node.as_ref().clone();
            self.scope_pool.add_symbol(
                crate_scope_id,
                module_name.name.clone(),
                Span {
                    source_id: file_source_id,
                    start_off: 0,
                    end_off: 0,
                },
                SymbolKind::File { source_id: file_source_id },
            );

            // Process top-level declarations
            for decl_child in &file_unit.green.top_decls {

                let decl_red = child_decl_red(&file_unit.span, decl_child);
                let decl = &decl_red.inner;
                let decl_span = decl_red.span.clone();
                let decl_name = decl.name.node.as_ref().clone();

                match &decl.kind {
                    GreenDeclKind::Fun { params, generic_vars, block, where_clause, .. } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.name.clone(),
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

                        // Generic params
                        for gv in generic_vars {
                            let gv_span = child_span(&decl_span, gv.relative_start, gv.node.text_len);
                            self.scope_pool.add_symbol(
                                fun_scope_id,
                                gv.node.name.node.as_ref().name.clone(),
                                gv_span,
                                SymbolKind::Generic,
                            );
                        }

                        // Function parameters
                        for param_child in params {
                            let param_span = child_span(&decl_span, param_child.relative_start, param_child.node.text_len);
                            let param_name = param_child.node.name.node.as_ref().clone();
                            self.scope_pool.add_symbol(
                                fun_scope_id,
                                param_name.name,
                                param_span,
                                SymbolKind::Local,
                            );
                        }

                        // Body
                        for stmt_child in block {
                            let stmt_red = child_expr_red(&decl_span, stmt_child);
                            self.build_expr_scope(&stmt_red, fun_scope_id)?;
                        }
                    }

                    GreenDeclKind::FunDecl { .. } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.name.clone(),
                            decl_span.clone(),
                            SymbolKind::Function,
                        );
                    }

                    GreenDeclKind::TypeStruct { fields, generic_vars, has_abst, where_clause } => {
                        let struct_scope_id = self.scope_pool.push_scope(
                            Some(file_scope_id),
                            ScopeKind::Struct,
                            Some(Arc::clone(&decl_red.inner)),
                            Some(decl_span.clone()),
                        );

                        for gv in generic_vars {
                            let gv_span = child_span(&decl_span, gv.relative_start, gv.node.text_len);
                            self.scope_pool.add_symbol(
                                struct_scope_id,
                                gv.node.name.node.as_ref().name.clone(),
                                gv_span,
                                SymbolKind::Generic,
                            );
                        }

                        let mut field_ids = vec![];
                        for field_child in fields {
                            let field_span = child_span(&decl_span, field_child.relative_start, field_child.node.text_len);
                            let field_name = field_child.node.name.node.as_ref().clone();
                            field_ids.push(self.scope_pool.add_symbol_and_get_sym_id(
                                struct_scope_id,
                                field_name.name,
                                field_span,
                                SymbolKind::Field,
                            ));
                        }

                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.name.clone(),
                            decl_span.clone(),
                            SymbolKind::Struct { fields: field_ids },
                        );
                    }

                    GreenDeclKind::ADT { ctors, generic_vars, has_abst, where_clause } => {
                        let adt_scope_id = self.scope_pool.push_scope(
                            Some(file_scope_id),
                            ScopeKind::Adt,
                            Some(Arc::clone(&decl_red.inner)),
                            Some(decl_span.clone()),
                        );

                        for gv in generic_vars {
                            let gv_span = child_span(&decl_span, gv.relative_start, gv.node.text_len);
                            self.scope_pool.add_symbol(
                                adt_scope_id,
                                gv.node.name.node.as_ref().name.clone(),
                                gv_span,
                                SymbolKind::Generic,
                            );
                        }

                        let mut constructors = vec![];
                        for ctor_child in ctors {
                            let ctor_span = child_span(&decl_span, ctor_child.relative_start, ctor_child.node.text_len);
                            let ctor_name = ctor_child.node.name.node.as_ref().clone();
                            constructors.push(self.scope_pool.add_symbol_and_get_sym_id(
                                file_scope_id,
                                ctor_name.name,
                                ctor_span,
                                SymbolKind::Constructor,
                            ));
                        }
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.name.clone(),
                            decl_span.clone(),
                            SymbolKind::ADT { constructors },
                        );
                    }

                    GreenDeclKind::TypeAlias { ref_to, generic_vars, has_abst, where_clause } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.name.clone(),
                            decl_span.clone(),
                            SymbolKind::TypeAlias,
                        );
                    }

                    GreenDeclKind::CType => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.name.clone(),
                            decl_span.clone(),
                            SymbolKind::CTypeDef,
                        );
                    }

                    GreenDeclKind::External { sym_name, params, return_type_str } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.name.clone(),
                            decl_span.clone(),
                            SymbolKind::External,
                        );
                    }

                    GreenDeclKind::Abstract { super_abst, generic_vars, methods, where_clause } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.name.clone(),
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
                            let method_span = child_span(&decl_span, method_child.relative_start, method_child.node.text_len);
                            let method_name = method_child.node.name.node.as_ref().clone();
                            self.scope_pool.add_symbol(
                                abs_scope_id,
                                method_name.name,
                                method_span,
                                SymbolKind::Method,
                            );
                        }
                    }

                    GreenDeclKind::Effect { controls } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.name.clone(),
                            decl_span.clone(),
                            SymbolKind::Effect,
                        );
                        let effect_scope_id = self.scope_pool.push_scope(
                            Some(file_scope_id),
                            ScopeKind::Effect,
                            Some(Arc::clone(&decl_red.inner)),
                            Some(decl_span.clone()),
                        );
                        for ctrl_child in controls {
                            let ctrl_span = child_span(&decl_span, ctrl_child.relative_start, ctrl_child.node.text_len);
                            let ctrl_name = ctrl_child.node.name.node.as_ref().clone();
                            self.scope_pool.add_symbol(
                                effect_scope_id,
                                ctrl_name.name,
                                ctrl_span,
                                SymbolKind::Control,
                            );
                        }
                    }

                    GreenDeclKind::Const { expr: e } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.name.clone(),
                            decl_span.clone(),
                            SymbolKind::Const,
                        );
                        let expr_red = child_expr_red(&decl_span, &e);
                        self.build_expr_scope(&expr_red, file_scope_id)?;
                    }

                    GreenDeclKind::Global { expr: e } => {
                        self.scope_pool.add_symbol(
                            file_scope_id,
                            decl_name.name.clone(),
                            decl_span.clone(),
                            SymbolKind::Global,
                        );
                        let expr_red = child_expr_red(&decl_span, e);
                        self.build_expr_scope(&expr_red, file_scope_id)?;
                    }

                    GreenDeclKind::TypeDecl => {
                        let lang_type = decl_child.node.annotations.iter()
                            .find(|ann| ann.node.name == "lang" && ann.node.args.len() == 1)
                            .and_then(|ann| ann.node.args.first())
                            .and_then(|arg| STR_TO_BUILTIN.get(arg)); // 适配 String

                        let sym_id = self.scope_pool.add_symbol_and_get_sym_id(
                            file_scope_id,
                            decl_name.name.clone(),
                            decl_span.clone(),
                            SymbolKind::TypeDecl,
                        );

                        if let Some(ty) = lang_type {
                            self.lang_items.register_builtin_sym_by_type(*ty, sym_id);
                        }
                    }
                }
            }

            // Process require / use imports
            for req_child in &file_unit.green.file_unit_requires {
                let req_red = {
                    let start = file_unit.span.start_off + req_child.relative_start;
                    let len = req_child.node.text_len;
                    RequireRedNode {
                        span: Span {
                            source_id: file_unit.span.source_id,
                            start_off: start,
                            end_off: start + len,
                        },
                        green: Arc::clone(&req_child.node),
                    }
                };
                let req = &req_red.green;

                if let Some(first_seg) = req.path.first() {

                    let module_name = first_seg.node.as_ref().clone();

                    if let Some((module_sym, _)) = self.scope_pool.lookup(crate_scope_id, &module_name.name) {

                        if let SymbolKind::File { source_id: target_src } = &module_sym.kind {
                            let target_scope = self.source_id_to_scope[target_src];
                            let target_file = self.source_to_file_unit[target_src];

                            let names: Vec<String> = if req.only.is_empty() {
                                target_file.green.top_decls.iter()
                                    .filter(|d| matches!(d.node.visibility, Visibility::Public | Visibility::PublicExternal))
                                    .map(|d| d.node.name.node.as_ref().clone().name)
                                    .collect()
                            } else {
                                req.only.iter().map(|s| s.node.as_ref().clone().name).collect()
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
                let decl_red = child_decl_red(&file_unit.span, decl_child);
                let decl = &decl_red.inner;

                match &decl.kind {
                    GreenDeclKind::Fun { block, .. } => {
                        if let Some(&fun_scope) = self.fun_scope_map.get(&decl_red.inner) {
                            for stmt_child in block {
                                let stmt_red = child_expr_red(&decl_red.span, stmt_child);
                                self.resolve_expr(&stmt_red, fun_scope)?;
                            }
                        }
                    }
                    GreenDeclKind::Const { expr: e } => {
                        let expr_red = child_expr_red(&decl_red.span, e);
                        // Resolve in file scope (constant initializer can't refer to local variables)
                        let file_scope = self.source_id_to_scope[&decl_red.span.source_id];
                        self.resolve_expr(&expr_red, file_scope)?;
                    }
                    GreenDeclKind::Global { expr: e } => {
                        let expr_red = child_expr_red(&decl_red.span, e);
                        let file_scope = self.source_id_to_scope[&decl_red.span.source_id];
                        self.resolve_expr(&expr_red, file_scope)?;
                    }
                    _ => {}
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
            arm_scope_map: self.arm_scope_map,
            catch_scope_map: self.catch_scope_map,
            source_id_to_scope: self.source_id_to_scope,
            lang_items: self.lang_items,
        })
    }
}