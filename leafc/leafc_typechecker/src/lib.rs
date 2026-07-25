use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::{HirBinOp, HirCrate, HirCtorDef, HirDeclId, HirDeclKind, HirExprId, HirExprKind, HirFieldDef, HirGenericParam, HirLit, HirName, HirParam, HirPattern, HirTypeName, HirUnaryOp};
use leafc_coreapi::lang_items::BuiltinType;
use leafc_coreapi::name_pass::NamePassResult;
use leafc_coreapi::scope::SymId;
use leafc_coreapi::source::Span;
use leafc_coreapi::type_checker::{TypeCheckerApi, TypeCheckerError, TypeCheckerResult};
use leafc_coreapi::type_system::{HirDeclTypeMap, HirExprTypeMap, LocalBindingTypeMap, NameTypeSchemeMap, TyId, TypeNode, TypeNodeKind, TypeScheme};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct BuiltinTypes {
    pub int8: TyId,
    pub int16: TyId,
    pub int32: TyId,
    pub int64: TyId,
    pub uint8: TyId,
    pub uint16: TyId,
    pub uint32: TyId,
    pub uint64: TyId,
    pub float32: TyId,
    pub float64: TyId,
    pub bool_ty: TyId,
    pub unit: TyId,
    pub never: TyId,
}

pub struct TypeChecker {
    hir_crate: HirCrate,
    name_pass_result: NamePassResult,

    decl_type_map: HirDeclTypeMap,
    expr_type_map: HirExprTypeMap,
    name_type_map: NameTypeSchemeMap,

    local_binding_map: LocalBindingTypeMap,

    sym_to_decl: HashMap<SymId, HirDeclId>,

    type_pool: Vec<TypeNode>,
    current_level: u32,

    // ResumeType, catch body has met resume?
    current_resume_ty: Option<(TyId, bool)>,

    builtin: BuiltinTypes,
}

impl TypeChecker {
    fn create_builtins(ty_pool: &mut Vec<TypeNode>) -> BuiltinTypes {
        let mut push = |kind: TypeNodeKind| -> TyId {
            let id = ty_pool.len();
            ty_pool.push(TypeNode { kind, parent: id, level: 0 });
            id
        };
        let int8 = push(TypeNodeKind::Builtin(BuiltinType::I8));
        let int16 = push(TypeNodeKind::Builtin(BuiltinType::I16));
        let int32 = push(TypeNodeKind::Builtin(BuiltinType::I32));
        let int64 = push(TypeNodeKind::Builtin(BuiltinType::I64));
        let uint8 = push(TypeNodeKind::Builtin(BuiltinType::U8));
        let uint16 = push(TypeNodeKind::Builtin(BuiltinType::U16));
        let uint32 = push(TypeNodeKind::Builtin(BuiltinType::U32));
        let uint64 = push(TypeNodeKind::Builtin(BuiltinType::U64));
        let float32 = push(TypeNodeKind::Builtin(BuiltinType::F32));
        let float64 = push(TypeNodeKind::Builtin(BuiltinType::F64));
        let bool_ty = push(TypeNodeKind::Builtin(BuiltinType::Bool));
        let never = push(TypeNodeKind::Never);
        let unit = push(TypeNodeKind::Tuple(vec![]));
        BuiltinTypes {
            int8, int16, int32, int64, uint8, uint16, uint32, uint64,
            float32, float64, bool_ty, unit, never,
        }
    }

    fn sym_span(&self, sym_id: SymId, fallback: Span) -> Span {
        self.name_pass_result.pool
            .get_symbol_by_id(sym_id)
            .map(|sym| sym.def_span.clone())
            .unwrap_or(fallback)
    }

    fn hir_name_span(&self, name: &HirName, fallback: Span) -> Span {
        self.sym_span(name.sym_id, fallback)
    }

    fn representative(&mut self, id: TyId) -> TyId {
        let parent = self.type_pool[id].parent;
        if parent != id {
            let root = self.representative(parent);
            self.type_pool[id].parent = root; // 路径压缩
            root
        } else {
            id
        }
    }

    fn new_type_var(&mut self) -> TyId {
        let id = self.type_pool.len();
        self.type_pool.push(TypeNode {
            kind: TypeNodeKind::Var,
            parent: id,
            level: self.current_level,
        });
        id
    }

    fn new_compound(&mut self, kind: TypeNodeKind) -> TyId {
        let id = self.type_pool.len();
        self.type_pool.push(TypeNode {
            kind,
            parent: id,
            level: 0,
        });
        id
    }

    /// resolve type name
    fn resolve_type_name(&mut self, name: &HirTypeName, span: Span) -> Result<TyId, DiagMsg> {
        match name {

            HirTypeName::Named { path, generics } => {

                if let Some(scheme) = self.name_type_map.get(&path.sym_id).cloned() {
                    if !generics.is_empty() {
                        return Err(DiagMsg {
                            title: format!("{:?}", TypeCheckerError::GenericArityMismatch),
                            msg: format!("expected 0 type arguments, got {}", generics.len()),
                            span: span.clone(),
                        });
                    }
                    return Ok(self.instantiate(&scheme));
                }

                if let Some(builtin_ty) =
                    self.name_pass_result.lang_items.get_builtin_type_by_sym(path.sym_id) {

                    if !generics.is_empty() {
                        return Err(DiagMsg {
                            title: format!("{:?}", TypeCheckerError::GenericArityMismatch),
                            msg: format!("built-in type does not accept generic arguments, got {}", generics.len()),
                            span,
                        });
                    }

                    let ty_id = match builtin_ty {
                        BuiltinType::I8  => self.builtin.int8,
                        BuiltinType::I16 => self.builtin.int16,
                        BuiltinType::I32 => self.builtin.int32,
                        BuiltinType::I64 => self.builtin.int64,
                        BuiltinType::U8  => self.builtin.uint8,
                        BuiltinType::U16 => self.builtin.uint16,
                        BuiltinType::U32 => self.builtin.uint32,
                        BuiltinType::U64 => self.builtin.uint64,
                        BuiltinType::F32 => self.builtin.float32,
                        BuiltinType::F64 => self.builtin.float64,
                        BuiltinType::Bool => self.builtin.bool_ty,
                        BuiltinType::Never => self.builtin.never,
                        BuiltinType::Ptr => todo!("pointer type not yet implemented"),
                    };
                    return Ok(ty_id);
                }

                let decl_id = *self.sym_to_decl.get(&path.sym_id)
                    .ok_or_else(|| DiagMsg {
                        title: format!("{:?}", TypeCheckerError::UndefinedType),
                        msg: format!("type `{}` (sym {}) not declared in this crate", path.name, path.sym_id),
                        span: span.clone(),
                    })?;

                let scheme = self.decl_type_map.get(&decl_id)
                    .ok_or_else(|| DiagMsg {
                        title: format!("{:?}", TypeCheckerError::TypeNotChecked),
                        msg: format!("type `{}` not yet fully checked", path.name),
                        span: span.clone(),
                    })?.clone();

                if scheme.quantified.len() != generics.len() {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::GenericArityMismatch),
                        msg: format!(
                            "expected {} type arguments, got {}",
                            scheme.quantified.len(),
                            generics.len()
                        ),
                        span: span.clone(),
                    });
                }

                let mut subst_map = HashMap::new();
                for (&qv, arg_ty) in scheme.quantified.iter().zip(generics) {
                    let arg_id = self.resolve_type_name(arg_ty, span.clone())?;
                    subst_map.insert(qv, arg_id);
                }

                let result_ty = self.copy_type_with_subst(scheme.body, &subst_map)?;

                self.check_generic_constraints(decl_id, &scheme.quantified, &subst_map, &span)?;

                Ok(result_ty)
            }

            HirTypeName::Ref(inner) => {
                let inner_ty = self.resolve_type_name(inner, span.clone())?;
                Ok(self.new_compound(TypeNodeKind::Ref(inner_ty)))
            }
            HirTypeName::MutRef(inner) => {
                let inner_ty = self.resolve_type_name(inner, span.clone())?;
                Ok(self.new_compound(TypeNodeKind::MutRef(inner_ty)))
            }
            HirTypeName::Share(inner) => {
                let inner_ty = self.resolve_type_name(inner, span.clone())?;
                Ok(self.new_compound(TypeNodeKind::Share(inner_ty)))
            }
            HirTypeName::Tuple(elements) => {
                let types: Vec<TyId> = elements.iter()
                    .map(|e| self.resolve_type_name(e, span.clone()))
                    .collect::<Result<_, _>>()?;
                Ok(self.new_compound(TypeNodeKind::Tuple(types)))
            }
            HirTypeName::Fun { params, return_type } => {
                let param_tys: Vec<TyId> = params.iter()
                    .map(|p| self.resolve_type_name(p, span.clone()))
                    .collect::<Result<_, _>>()?;
                let ret_ty = self.resolve_type_name(return_type, span.clone())?;
                Ok(self.new_compound(TypeNodeKind::Fun { param_tys, return_ty: ret_ty }))
            }
            HirTypeName::Impl(inner) => {
                todo!()
            }
        }
    }

    fn copy_type_with_subst(&mut self, ty: TyId, subst: &HashMap<TyId, TyId>) -> Result<TyId, DiagMsg> {
        let root = self.representative(ty);
        match self.type_pool[root].kind.clone() {
            TypeNodeKind::Var => {
                if let Some(&replacement) = subst.get(&root) {
                    Ok(replacement)
                } else {
                    Ok(root)
                }
            }
            TypeNodeKind::Builtin(_) | TypeNodeKind::Never => Ok(root),
            TypeNodeKind::Fun { param_tys, return_ty } => {
                let new_params: Result<Vec<_>, _> = param_tys.iter()
                    .map(|&p| self.copy_type_with_subst(p, subst))
                    .collect();
                let new_ret = self.copy_type_with_subst(return_ty, subst)?;
                Ok(self.new_compound(TypeNodeKind::Fun {
                    param_tys: new_params?,
                    return_ty: new_ret,
                }))
            }
            TypeNodeKind::Tuple(elems) => {
                let new_elems: Result<Vec<_>, _> = elems.iter()
                    .map(|&e| self.copy_type_with_subst(e, subst))
                    .collect();
                Ok(self.new_compound(TypeNodeKind::Tuple(new_elems?)))
            }
            TypeNodeKind::Struct { decl_id, subst: existing_subst } => {
                let new_subst: Result<Vec<_>, _> = existing_subst.iter()
                    .map(|&s| self.copy_type_with_subst(s, subst))
                    .collect();
                Ok(self.new_compound(TypeNodeKind::Struct {
                    decl_id,
                    subst: new_subst?,
                }))
            },
            TypeNodeKind::Ref(_) => todo!(),
            TypeNodeKind::MutRef(_) => todo!(),
            TypeNodeKind::Share(_) => todo!(),
            TypeNodeKind::ADT { decl_id, subst: existing_subst } => {
                let new_subst: Result<Vec<_>, _> = existing_subst.iter()
                    .map(|&s| self.copy_type_with_subst(s, subst))
                    .collect();
                Ok(self.new_compound(TypeNodeKind::ADT {
                    decl_id,
                    subst: new_subst?,
                }))
            }
        }
    }

    fn unify(&mut self, t1: TyId, t2: TyId, span: Span) -> Result<(), DiagMsg> {
        let r1 = self.representative(t1);
        let r2 = self.representative(t2);
        if r1 == r2 { return Ok(()); }
        let k1 = self.type_pool[r1].kind.clone();
        let k2 = self.type_pool[r2].kind.clone();
        match (&k1, &k2) {
            (TypeNodeKind::Never, _) => {
                // Never <: T
                self.type_pool[r1].parent = r2;
                Ok(())
            }
            (TypeNodeKind::Var, TypeNodeKind::Var) => {
                let lv1 = self.type_pool[r1].level;
                let lv2 = self.type_pool[r2].level;
                if lv1 <= lv2 {
                    self.type_pool[r2].parent = r1;
                } else {
                    self.type_pool[r1].parent = r2;
                }
                Ok(())
            }
            (TypeNodeKind::Var, _) => {
                self.check_occurs(r1, r2, span.clone())?;
                self.type_pool[r1].parent = r2;
                Ok(())
            }
            (_, TypeNodeKind::Var) => {
                self.check_occurs(r2, r1, span.clone())?;
                self.type_pool[r2].parent = r1;
                Ok(())
            }
            (TypeNodeKind::Builtin(b1), TypeNodeKind::Builtin(b2)) if b1 == b2 => Ok(()),
            (TypeNodeKind::Fun { param_tys: p1, return_ty: r1 },
                TypeNodeKind::Fun { param_tys: p2, return_ty: r2 }) => {
                if p1.len() != p2.len() {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::ArityMismatch),
                        msg: format!("function arity mismatch: expect {}, got {}", p1.len(), p2.len()),
                        span,
                    });
                }
                for (&a, &b) in p1.iter().zip(p2.iter()) {
                    self.unify(a, b, span.clone())?;
                }
                self.unify(*r1, *r2, span)
            }
            (TypeNodeKind::Tuple(e1), TypeNodeKind::Tuple(e2)) => {
                if e1.len() != e2.len() {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::ArityMismatch),
                        msg: format!("tuple arity mismatch: expect {}, got {}", e1.len(), e2.len()),
                        span,
                    });
                }
                for (&a, &b) in e1.iter().zip(e2.iter()) {
                    self.unify(a, b, span.clone())?;
                }
                Ok(())
            }
            (TypeNodeKind::Struct { decl_id: d1, subst: s1 },
                TypeNodeKind::Struct { decl_id: d2, subst: s2 }) if d1 == d2 => {
                if s1.len() != s2.len() {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::ArityMismatch),
                        msg: format!("struct arity mismatch: expect {}, got {}", s1.len(), s2.len()),
                        span,
                    });
                }
                for (&a, &b) in s1.iter().zip(s2.iter()) {
                    if let Err(_) = self.unify(a, b, span.clone()) {
                        return Err(DiagMsg {
                            title: format!("{:?}", TypeCheckerError::TypeMismatch),
                            msg: format!(
                                "expected {}, but got {}",
                                self.ty_to_string(t2),
                                self.ty_to_string(t1)
                            ),
                            span,
                        });
                    }
                }
                Ok(())
            }
            (TypeNodeKind::ADT { decl_id: d1, subst: s1 },
                TypeNodeKind::ADT { decl_id: d2, subst: s2 }) if d1 == d2 => {
                if s1.len() != s2.len() {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::ArityMismatch),
                        msg: format!("ADT arity mismatch: expect {}, got {}", s1.len(), s2.len()),
                        span,
                    });
                }
                for (&a, &b) in s1.iter().zip(s2.iter()) {
                    if let Err(_) = self.unify(a, b, span.clone()) {
                        return Err(DiagMsg {
                            title: format!("{:?}", TypeCheckerError::TypeMismatch),
                            msg: format!(
                                "expected {}, but got {}",
                                self.ty_to_string(t2),
                                self.ty_to_string(t1)
                            ),
                            span,
                        });
                    }
                }
                Ok(())
            }
            _ => Err(DiagMsg {
                title: format!("{:?}", TypeCheckerError::TypeMismatch),
                msg: format!(
                    "expected \"{}\", but got \"{}\"",
                    self.ty_to_string(r2),
                    self.ty_to_string(r1)
                ),
                span,
            }),
        }
    }

    fn check_occurs(&mut self, var: TyId, ty: TyId, span: Span) -> Result<(), DiagMsg> {
        let root = self.representative(ty);
        if root == var {
            return Err(DiagMsg {
                title: format!("{:?}", TypeCheckerError::InfiniteType),
                msg: "infinite type detected".into(),
                span,
            });
        }
        match self.type_pool[root].kind.clone() {
            TypeNodeKind::Fun { param_tys, return_ty } => {
                for p in param_tys {
                    self.check_occurs(var, p, span.clone())?;
                }
                self.check_occurs(var, return_ty, span)
            }
            TypeNodeKind::Tuple(elems) => {
                for e in elems {
                    self.check_occurs(var, e, span.clone())?;
                }
                Ok(())
            }
            TypeNodeKind::Struct { subst, .. } => {
                for s in subst {
                    self.check_occurs(var, s, span.clone())?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn generalize(&mut self, body: TyId) -> TypeScheme {
        let mut free_vars = Vec::new();
        self.collect_free_vars(body, &mut free_vars);
        free_vars.sort_unstable();
        free_vars.dedup();
        let mut quantified = Vec::new();
        for &var in &free_vars {
            let root = self.representative(var);
            if self.type_pool[root].level > self.current_level {
                quantified.push(root);
            }
        }
        TypeScheme { quantified, body }
    }

    fn instantiate(&mut self, scheme: &TypeScheme) -> TyId {
        if scheme.quantified.is_empty() {
            return scheme.body;
        }
        let mut subst = HashMap::new();
        for &qv in &scheme.quantified {
            let new_var = self.new_type_var();
            subst.insert(qv, new_var);
        }
        self.copy_type_with_subst(scheme.body, &subst)
            .expect("instantiation should not fail")
    }

    fn instantiate_with_map(&mut self, scheme: &TypeScheme) -> (TyId, HashMap<TyId, TyId>) {
        if scheme.quantified.is_empty() {
            return (scheme.body, HashMap::new());
        }
        let mut subst = HashMap::new();
        for &qv in &scheme.quantified {
            let new_var = self.new_type_var();
            subst.insert(qv, new_var);
        }
        let ty = self.copy_type_with_subst(scheme.body, &subst)
            .expect("instantiation should not fail");
        (ty, subst)
    }

    fn get_adt_decl_id_for_constructor(&self, ctor_sym_id: SymId) -> Option<HirDeclId> {
        for (decl_id, decl) in self.hir_crate.hir_decl_pool.iter().enumerate() {
            if let HirDeclKind::ADT { ctors, .. } = &decl.kind {
                if ctors.iter().any(|c| c.name.sym_id == ctor_sym_id) {
                    return Some(decl_id);
                }
            }
        }
        None
    }

    /// 检查泛型约束
    fn check_generic_constraints(
        &mut self,
        decl_id: HirDeclId,
        quantified: &[TyId],
        subst: &HashMap<TyId, TyId>,
        span: &Span,
    ) -> Result<(), DiagMsg> {
        let generic_params = match &self.hir_crate.hir_decl_pool[decl_id].kind {
            HirDeclKind::Fun { generic_params, .. }
            | HirDeclKind::Struct { generic_params, .. }
            | HirDeclKind::ADT { generic_params, .. }
            | HirDeclKind::TypeAlias { generic_params, .. }
            | HirDeclKind::Abstract { generic_params, .. } => generic_params.clone(),
            _ => return Ok(()),
        };

        if generic_params.is_empty() {
            return Ok(());
        }

        if quantified.len() != generic_params.len() {
            return Err(DiagMsg {
                title: format!("{:?}", TypeCheckerError::InternalError),
                msg: "mismatched quantified vars and generic params".into(),
                span: span.clone(),
            });
        }

        for (i, gp) in generic_params.iter().enumerate() {
            let qv = quantified[i];
            let actual_ty = subst.get(&qv).copied().unwrap_or(qv);

            let old = self.name_type_map.insert(
                gp.name.sym_id,
                TypeScheme { quantified: vec![], body: actual_ty },
            );

            for constraint in &gp.constraints {
                let c_ty = self.resolve_type_name(constraint, span.clone())?;
                self.unify(actual_ty, c_ty, span.clone())?;
            }

            if let Some(old_scheme) = old {
                self.name_type_map.insert(gp.name.sym_id, old_scheme);
            } else {
                self.name_type_map.remove(&gp.name.sym_id);
            }
        }

        Ok(())
    }

    fn collect_free_vars(&mut self, ty: TyId, out: &mut Vec<TyId>) {
        let root = self.representative(ty);
        match self.type_pool[root].kind.clone() {
            TypeNodeKind::Var => { out.push(root); }
            TypeNodeKind::Fun { param_tys, return_ty } => {
                for p in param_tys {
                    self.collect_free_vars(p, out);
                }
                self.collect_free_vars(return_ty, out);
            }
            TypeNodeKind::Tuple(elems) => {
                for e in elems {
                    self.collect_free_vars(e, out);
                }
            }
            TypeNodeKind::Struct { subst, .. } => {
                for s in subst {
                    self.collect_free_vars(s, out);
                }
            }
            _ => {}
        }
    }

    fn bind_pattern(
        &mut self,
        pat: &HirPattern,
        ty: TyId,
        bound_symbols: &mut Vec<SymId>,
    ) -> Result<(), DiagMsg> {
        match pat {
            HirPattern::Binding(name) => {
                self.name_type_map.insert(
                    name.sym_id,
                    TypeScheme { quantified: vec![], body: ty },
                );
                bound_symbols.push(name.sym_id);
                Ok(())
            }
            _ => todo!()
        }
    }

    fn check_match_exhaustiveness(
        &mut self,
        scrutinee_ty: TyId,
        patterns: &[HirPattern],
        span: &Span,
    ) -> Result<(), DiagMsg> {
        let mut matrix: Vec<Vec<&HirPattern>> = Vec::new();

        for (idx, pat) in patterns.iter().enumerate() {
            let q = vec![pat];

            // unreachable
            if !self.is_useful(&matrix, &q) {
                return Err(DiagMsg {
                    title: format!("{:?}", TypeCheckerError::UnreachablePattern),
                    msg: format!("pattern at arm {} is unreachable", idx + 1),
                    span: span.clone(),
                });
            }

            matrix.push(q);
        }

        // exhaustiveness)
        let wildcard = HirPattern::Wildcard;
        let q_wildcard = vec![&wildcard];

        if self.is_useful(&matrix, &q_wildcard) {
            return Err(DiagMsg {
                title: format!("{:?}", TypeCheckerError::NonExhaustiveMatch),
                msg: "match expression is non-exhaustive, add `_ => ...` to cover all cases".into(),
                span: span.clone(),
            });
        }

        Ok(())
    }

    fn literals_equal(a: &HirLit, b: &HirLit) -> bool {
        match (a, b) {
            (HirLit::Decimal(s1), HirLit::Decimal(s2)) => s1 == s2,
            (HirLit::Int(s1), HirLit::Int(s2)) => s1 == s2,
            (HirLit::Str(s1), HirLit::Str(s2)) => s1 == s2,
            (HirLit::Bool(b1), HirLit::Bool(b2)) => b1 == b2,
            _ => false,
        }
    }

    fn check_pattern(
        &mut self,
        ty: TyId,
        pattern: &HirPattern,
        span: &Span,
    ) -> Result<Vec<(SymId, TyId)>, DiagMsg> {
        match pattern {
            HirPattern::Wildcard | HirPattern::Rest => Ok(vec![]),

            HirPattern::Binding(name) => Ok(vec![(name.sym_id, ty)]),

            HirPattern::Literal(lit) => {
                let lit_ty = self.infer_lit(lit)?;
                self.unify(ty, lit_ty, span.clone())?;
                Ok(vec![])
            }

            HirPattern::Tuple { elements, span: pat_span } => {
                let root = self.representative(ty);
                let elem_tys = match &self.type_pool[root].kind.clone() {
                    TypeNodeKind::Tuple(tys) => tys.clone(),
                    _ => {
                        let tys: Vec<TyId> = (0..elements.len()).map(|_| self.new_type_var()).collect();
                        let tuple_ty = self.new_compound(TypeNodeKind::Tuple(tys.clone()));
                        self.unify(ty, tuple_ty, pat_span.clone())?;
                        tys
                    }
                };

                if elements.len() != elem_tys.len() {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::ArityMismatch),
                        msg: format!("expected {} elements in tuple pattern, got {}", elem_tys.len(), elements.len()),
                        span: pat_span.clone(),
                    });
                }

                let mut bindings = Vec::new();
                for (elem_pat, elem_ty) in elements.iter().zip(elem_tys) {
                    let mut b = self.check_pattern(elem_ty, elem_pat, pat_span)?;
                    bindings.append(&mut b);
                }
                Ok(bindings)
            }

            HirPattern::Constructor { type_name, args, span: pat_span } => {
                let ctor_name = match type_name {
                    HirTypeName::Named { path, .. } => path,
                    _ => {
                        return Err(DiagMsg {
                            title: format!("{:?}", TypeCheckerError::TypeMismatch),
                            msg: "constructor pattern requires a named type".into(),
                            span: pat_span.clone(),
                        });
                    }
                };

                let scheme = self
                    .name_type_map
                    .get(&ctor_name.sym_id)
                    .cloned()
                    .ok_or_else(|| DiagMsg {
                        title: format!("{:?}", TypeCheckerError::UndefinedVariable),
                        msg: format!("constructor `{}` not found", ctor_name.name),
                        span: self.hir_name_span(ctor_name, pat_span.clone()),
                    })?;

                let ctor_ty = self.instantiate(&scheme);
                let root = self.representative(ctor_ty);

                let (param_tys, adt_ty) = match &self.type_pool[root].kind {
                    TypeNodeKind::Fun { param_tys, return_ty } => (param_tys.clone(), *return_ty),
                    TypeNodeKind::ADT { .. } => (vec![], ctor_ty),
                    _ => {
                        return Err(DiagMsg {
                            title: format!("{:?}", TypeCheckerError::TypeMismatch),
                            msg: format!("`{}` is not a constructor", ctor_name.name),
                            span: pat_span.clone(),
                        });
                    }
                };

                self.unify(ty, adt_ty, pat_span.clone())?;

                let adt_root = self.representative(adt_ty);
                if let TypeNodeKind::ADT { decl_id, subst } = &self.type_pool[adt_root].kind.clone() {
                    let adt_scheme = self.decl_type_map.get(decl_id)
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", TypeCheckerError::InternalError),
                            msg: "ADT type scheme not found".into(),
                            span: pat_span.clone(),
                        })?
                        .clone();
                    let mut subst_map = HashMap::new();
                    for (i, &qv) in adt_scheme.quantified.iter().enumerate() {
                        if i < subst.len() {
                            subst_map.insert(qv, subst[i]);
                        }
                    }
                    self.check_generic_constraints(*decl_id, &adt_scheme.quantified, &subst_map, pat_span)?;
                }

                if args.len() != param_tys.len() {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::ArityMismatch),
                        msg: format!(
                            "constructor `{}` expects {} arguments, got {}",
                            ctor_name.name,
                            param_tys.len(),
                            args.len()
                        ),
                        span: pat_span.clone(),
                    });
                }

                let mut bindings = Vec::new();
                for (arg_pat, &param_ty) in args.iter().zip(param_tys.iter()) {
                    let mut inner = self.check_pattern(param_ty, arg_pat, pat_span)?;
                    bindings.append(&mut inner);
                }

                Ok(bindings)
            }

            HirPattern::Or { left, right, span: pat_span } => {
                let left_bindings = self.check_pattern(ty, left, pat_span)?;
                let _right_bindings = self.check_pattern(ty, right, pat_span)?;
                Ok(left_bindings)
            }

            HirPattern::Alias { pattern, name, span: pat_span } => {
                let mut bindings = self.check_pattern(ty, pattern, pat_span)?;
                bindings.push((name.sym_id, ty));
                Ok(bindings)
            }

            HirPattern::Struct { path, fields, rest: _, span: pat_span } => {
                let decl_id = *self.sym_to_decl.get(&path.sym_id).ok_or_else(|| DiagMsg {
                    title: format!("{:?}", TypeCheckerError::UndefinedType),
                    msg: format!("struct `{}` not found", path.name),
                    span: pat_span.clone(),
                })?;

                let scheme = self.decl_type_map.get(&decl_id).ok_or_else(|| DiagMsg {
                    title: format!("{:?}", TypeCheckerError::TypeNotChecked),
                    msg: format!("type `{}` not checked yet", path.name),
                    span: pat_span.clone(),
                })?.clone();

                let struct_ty = self.instantiate(&scheme);
                self.unify(ty, struct_ty, pat_span.clone())?;

                let decl = self.hir_crate.hir_decl_pool[decl_id].clone();
                let field_defs = match &decl.kind {
                    HirDeclKind::Struct { fields, .. } => fields.clone(),
                    _ => return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::TypeMismatch),
                        msg: format!("`{}` is not a struct", path.name),
                        span: pat_span.clone(),
                    }),
                };

                let mut bindings = Vec::new();
                for field_pat in fields {
                    let def = field_defs.iter().find(|f| f.name.name == field_pat.field_name.name)
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", TypeCheckerError::FieldNotFound),
                            msg: format!("struct `{}` has no field `{}`", path.name, field_pat.field_name.name),
                            span: field_pat.span.clone(),
                        })?;
                    let field_ty = self.resolve_type_name(&def.type_ann, field_pat.span.clone())?;
                    let mut b = self.check_pattern(field_ty, &field_pat.pattern, &field_pat.span)?;
                    bindings.append(&mut b);
                }
                Ok(bindings)
            }
        }
    }

    fn is_same_type_name(a: &HirTypeName, b: &HirTypeName) -> bool {
        match (a, b) {
            (HirTypeName::Named { path: p1, .. }, HirTypeName::Named { path: p2, .. }) => {
                p1.sym_id == p2.sym_id
            }
            _ => false,
        }
    }

    /// U(P, q): check q is useful for P ?
    fn is_useful(&self, p: &[Vec<&HirPattern>], q: &[&HirPattern]) -> bool {
        if p.is_empty() {
            return true;
        }

        if q.is_empty() {
            return false;
        }

        let q1 = q[0];
        let q_rest = &q[1..];

        match q1 {
            HirPattern::Constructor { args, .. } => {
                let s_p = self.specialize(p, q1);
                let mut s_q: Vec<&HirPattern> = args.iter().collect();
                s_q.extend_from_slice(q_rest);

                self.is_useful(&s_p, &s_q)
            }
            HirPattern::Tuple { elements, .. } => {
                let s_p = self.specialize(p, q1);
                let mut s_q: Vec<&HirPattern> = elements.iter().collect();
                s_q.extend_from_slice(q_rest);

                self.is_useful(&s_p, &s_q)
            }
            HirPattern::Literal(_) => {
                let s_p = self.specialize(p, q1);
                self.is_useful(&s_p, q_rest)
            }
            HirPattern::Wildcard | HirPattern::Binding(_) | HirPattern::Rest => {
                let constructors_in_p = self.get_constructors(p);
                let is_exhaustive_constructors = self.is_all_constructors_covered(&constructors_in_p);

                if is_exhaustive_constructors {
                    for ctor in constructors_in_p {
                        let s_p = self.specialize(p, ctor);
                        let arity = self.get_ctor_arity(ctor);

                        let mut s_q = vec![&HirPattern::Wildcard; arity];
                        s_q.extend_from_slice(q_rest);

                        if self.is_useful(&s_p, &s_q) {
                            return true;
                        }
                    }
                    false
                } else {
                    let d_p = self.default_matrix(p);
                    self.is_useful(&d_p, q_rest)
                }
            }
            HirPattern::Or { left, right, .. } => {
                [left.as_ref(), right.as_ref()].iter().any(|pat| {
                    let mut expanded_q = vec![*pat];
                    expanded_q.extend_from_slice(q_rest);
                    self.is_useful(p, &expanded_q)
                })
            }
            HirPattern::Alias { pattern, .. } => {
                let mut expanded_q = vec![pattern.as_ref()];
                expanded_q.extend_from_slice(q_rest);
                self.is_useful(p, &expanded_q)
            }
            HirPattern::Struct { .. } => {
                let s_p = self.specialize(p, q1);
                self.is_useful(&s_p, q_rest)
            }
        }
    }

    /// 特化矩阵
    fn specialize<'a>(&self, p: &[Vec<&'a HirPattern>], ctor: &HirPattern) -> Vec<Vec<&'a HirPattern>> {
        let mut s_p = Vec::new();
        for row in p {
            if row.is_empty() { continue; }

            match (row[0], ctor) {
                (
                    HirPattern::Constructor { type_name: r_name, args: r_args, .. },
                    HirPattern::Constructor { type_name: c_name, .. },
                ) => {
                    if Self::is_same_type_name(r_name, c_name) {
                        let mut new_row = r_args.iter().collect::<Vec<_>>();
                        new_row.extend_from_slice(&row[1..]);
                        s_p.push(new_row);
                    }
                }
                // Tuple
                (
                    HirPattern::Tuple { elements: r_elems, .. },
                    HirPattern::Tuple { .. },
                ) => {
                    let mut new_row = r_elems.iter().collect::<Vec<_>>();
                    new_row.extend_from_slice(&row[1..]);
                    s_p.push(new_row);
                }
                // Literal
                (HirPattern::Literal(r_lit), HirPattern::Literal(c_lit)) => {
                    if Self::literals_equal(r_lit, c_lit) {
                        s_p.push(row[1..].to_vec());
                    }
                }
                (HirPattern::Struct { path: r_path, fields: r_fields, .. },
                    HirPattern::Struct { path: c_path, .. }) =>
                    {
                        if r_path.sym_id == c_path.sym_id {
                            let mut new_row: Vec<&HirPattern> = r_fields.iter()
                                .map(|f| &f.pattern)
                                .collect();
                            new_row.extend_from_slice(&row[1..]);
                            s_p.push(new_row);
                        }
                    }
                // Wildcards / Bindings / Rest
                (HirPattern::Wildcard | HirPattern::Binding(_) | HirPattern::Rest, _) => {
                    let arity = self.get_ctor_arity(ctor);
                    let mut new_row = vec![&HirPattern::Wildcard; arity];
                    new_row.extend_from_slice(&row[1..]);
                    s_p.push(new_row);
                }
                // Or pattern
                (HirPattern::Or { left, right, .. }, _) => {
                    for pat in [left.as_ref(), right.as_ref()] {
                        let mut expanded_row = vec![pat];
                        expanded_row.extend_from_slice(&row[1..]);
                        s_p.extend(self.specialize(&[expanded_row], ctor));
                    }
                }
                // Alias
                (HirPattern::Alias { pattern, .. }, _) => {
                    let mut expanded_row = vec![pattern.as_ref()];
                    expanded_row.extend_from_slice(&row[1..]);
                    s_p.extend(self.specialize(&[expanded_row], ctor));
                }
                _ => {}
            }
        }
        s_p
    }

    fn default_matrix<'a>(&self, p: &[Vec<&'a HirPattern>]) -> Vec<Vec<&'a HirPattern>> {
        let mut d_p = Vec::new();
        for row in p {
            if row.is_empty() { continue; }
            match row[0] {
                HirPattern::Wildcard | HirPattern::Binding(_) | HirPattern::Rest => {
                    d_p.push(row[1..].to_vec());
                }
                HirPattern::Or { left, right, .. } => {
                    for pat in [left.as_ref(), right.as_ref()] {
                        let mut expanded_row = vec![pat];
                        expanded_row.extend_from_slice(&row[1..]);
                        d_p.extend(self.default_matrix(&[expanded_row]));
                    }
                }
                HirPattern::Alias { pattern, .. } => {
                    let mut expanded_row = vec![pattern.as_ref()];
                    expanded_row.extend_from_slice(&row[1..]);
                    d_p.extend(self.default_matrix(&[expanded_row]));
                }
                _ => {}
            }
        }
        d_p
    }

    fn get_ctor_arity(&self, ctor: &HirPattern) -> usize {
        match ctor {
            HirPattern::Constructor { args, .. } => args.len(),
            HirPattern::Tuple { elements, .. } => elements.len(),
            HirPattern::Struct { fields, .. } => fields.len(),
            HirPattern::Literal(_) => 0,
            _ => 0,
        }
    }

    fn is_same_ctor(&self, a: &HirPattern, b: &HirPattern) -> bool {
        match (a, b) {
            (
                HirPattern::Constructor { type_name: t1, .. },
                HirPattern::Constructor { type_name: t2, .. },
            ) => Self::is_same_type_name(t1, t2),
            (HirPattern::Literal(l1), HirPattern::Literal(l2)) => Self::literals_equal(l1, l2),
            (
                HirPattern::Tuple { elements: e1, .. },
                HirPattern::Tuple { elements: e2, .. },
            ) => e1.len() == e2.len(),
            (
                HirPattern::Struct { path: p1, .. },
                HirPattern::Struct { path: p2, .. },
            ) => p1.sym_id == p2.sym_id,
            _ => false,
        }
    }

    fn get_constructors<'a>(&self, p: &[Vec<&'a HirPattern>]) -> Vec<&'a HirPattern> {
        let mut ctors: Vec<&'a HirPattern> = Vec::new();

        for row in p {
            if row.is_empty() { continue; }
            match row[0] {
                pat @ (
                HirPattern::Constructor { .. }
                | HirPattern::Literal(_)
                | HirPattern::Tuple { .. }
                | HirPattern::Struct { .. }
                ) => {
                    if !ctors.iter().any(|c| self.is_same_ctor(c, pat)) {
                        ctors.push(pat);
                    }
                }
                HirPattern::Or { left, right, .. } => {
                    let left_row = vec![left.as_ref()];
                    let right_row = vec![right.as_ref()];
                    ctors.extend(self.get_constructors(&[left_row]));
                    ctors.extend(self.get_constructors(&[right_row]));
                }
                HirPattern::Alias { pattern, .. } => {
                    ctors.extend(self.get_constructors(&[vec![pattern.as_ref()]]));
                }
                _ => {}
            }
        }
        ctors
    }

    fn is_all_constructors_covered(&self, ctors: &[&HirPattern]) -> bool {
        if ctors.is_empty() {
            return false;
        }

        match ctors[0] {
            HirPattern::Constructor { type_name, .. } => {
                if let HirTypeName::Named { path, .. } = type_name {
                    if let Some(decl_id) = self.get_adt_decl_id_for_constructor(path.sym_id) {
                        let decl = &self.hir_crate.hir_decl_pool[decl_id];
                        if let HirDeclKind::ADT { ctors: def_ctors, .. } = &decl.kind {
                            return ctors.len() >= def_ctors.len();
                        }
                    }
                }
                false
            }
            HirPattern::Tuple { .. } => true,
            HirPattern::Struct { .. } => true,
            HirPattern::Literal(HirLit::Bool(_)) => {
                let has_true = ctors.iter().any(|c| matches!(c, HirPattern::Literal(HirLit::Bool(true))));
                let has_false = ctors.iter().any(|c| matches!(c, HirPattern::Literal(HirLit::Bool(false))));
                has_true && has_false
            }
            HirPattern::Literal(_) => false,
            _ => false,
        }
    }

    fn infer_expr(&mut self, expr_id: HirExprId, expected: Option<TyId>) -> Result<TyId, DiagMsg> {
        let expr = self.hir_crate.hir_expr_pool[expr_id].clone();
        let span = expr.span.clone();

        let ty = match &expr.kind {
            HirExprKind::Lit(lit) => self.infer_lit(lit)?,
            HirExprKind::Ident(name) => {
                let ty = if let Some(scheme) = self.name_type_map.get(&name.sym_id).cloned() {
                    self.instantiate(&scheme)
                } else {
                    let decl_id = *self.sym_to_decl.get(&name.sym_id)
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", TypeCheckerError::UndefinedVariable),
                            msg: format!("undefined variable `{}`", name.name),
                            span: self.hir_name_span(name, span.clone()),
                        })?;
                    let scheme = self.decl_type_map.get(&decl_id)
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", TypeCheckerError::TypeNotChecked),
                            msg: format!("type of `{}` not yet checked", name.name),
                            span: self.hir_name_span(name, span.clone()),
                        })?.clone();
                    self.instantiate(&scheme)
                };
                ty
            }
            HirExprKind::Binary { left, right, op } =>
                self.infer_binary(*left, *right, *op, &span)?,
            HirExprKind::Unary { op, right } =>
                self.infer_unary(*op, *right, &span)?,
            HirExprKind::Call { callee, args } =>
                self.infer_call(*callee, args, expected, &span)?,
            HirExprKind::Block { stmts } =>
                self.infer_block(stmts, expected, &span)?,
            HirExprKind::Let {
                name, type_ann, init, .. } =>
                self.infer_let(expr_id, name, type_ann.as_ref(), *init, &span)?,
            HirExprKind::If { cond, then, elifs, else_opt } =>
                self.infer_if(*cond, *then, elifs, *else_opt, expected, &span)?,
            HirExprKind::Tuple { elements } =>
                self.infer_tuple(elements, expected, &span)?,
            HirExprKind::Return { expr } =>
                self.infer_return(expr.as_ref(), expected, &span)?,
            HirExprKind::TypeCast { expr, type_ann } =>
                self.infer_cast(*expr, type_ann, &span)?,
            HirExprKind::Move { target } | HirExprKind::Copy { target } => {
                self.infer_expr(*target, expected)?
            }
            HirExprKind::Ref { target } => {
                let target_ty = self.infer_expr(*target, None)?;
                self.new_compound(TypeNodeKind::Ref(target_ty))
            }
            HirExprKind::MutRef { target } => {
                let target_ty = self.infer_expr(*target, None)?;
                self.new_compound(TypeNodeKind::MutRef(target_ty))
            }
            HirExprKind::Share { target } => {
                let target_ty = self.infer_expr(*target, None)?;
                self.new_compound(TypeNodeKind::Share(target_ty))
            }

            HirExprKind::FieldAccess { obj, field } => {

                let obj_ty = self.infer_expr(*obj, None)?;
                let obj_root = self.representative(obj_ty);

                if let TypeNodeKind::Struct { decl_id, subst } = &self.type_pool[obj_root].kind.clone() {
                    let (field_type, subst_clone, generic_params) = {
                        let decl = &self.hir_crate.hir_decl_pool[*decl_id];
                        match &decl.kind {
                            HirDeclKind::Struct { fields, generic_params, .. } => {
                                let field_def = fields.iter()
                                    .find(|f| f.name.name == *field)
                                    .ok_or_else(|| DiagMsg {
                                        title: format!("{:?}", TypeCheckerError::FieldNotFound),
                                        msg: format!("struct `{}` has no field named `{}`", decl.ident, field),
                                        span: span.clone(),
                                    })?;
                                (field_def.type_ann.clone(), subst.clone(), generic_params.clone())
                            }
                            _ => return Err(DiagMsg {
                                title: format!("{:?}", TypeCheckerError::TypeMismatch),
                                msg: "type is not a struct".into(),
                                span: span.clone(),
                            }),
                        }
                    };

                    // SymId => TyId
                    let mut var_map = HashMap::new();
                    for (gp, &actual_ty) in generic_params.iter().zip(subst_clone.iter()) {
                        var_map.insert(gp.name.sym_id, actual_ty);
                    }

                    let mut inserted_symbols = Vec::new();
                    for (&sym_id, &ty) in &var_map {
                        self.name_type_map.insert(sym_id, TypeScheme { quantified: vec![], body: ty });
                        inserted_symbols.push(sym_id);
                    }

                    let field_ty = self.resolve_type_name(&field_type, span.clone())?;

                    // 清理
                    for sym_id in inserted_symbols {
                        self.name_type_map.remove(&sym_id);
                    }

                    return Ok(field_ty);
                }

                return Err(DiagMsg {
                    title: format!("{:?}", TypeCheckerError::TypeMismatch),
                    msg: format!("cannot access field `{}` on non‑struct type", field),
                    span: span.clone(),
                })
            }

            HirExprKind::MakeStruct { path, fields } => {
                let struct_ty = self.infer_expr(*path, None)?;
                let struct_root = self.representative(struct_ty);
                let (decl_id, subst) = if let TypeNodeKind::Struct { decl_id, subst } = &self.type_pool[struct_root].kind {
                    (*decl_id, subst.clone())
                } else {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::TypeMismatch),
                        msg: "expected struct type".into(),
                        span: span.clone(),
                    });
                };

                let decl = self.hir_crate.hir_decl_pool[decl_id].clone();
                let (generic_params, struct_fields) = match &decl.kind {
                    HirDeclKind::Struct { generic_params, fields, .. } => (generic_params.clone(), fields.clone()),
                    _ => return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::InternalError),
                        msg: "struct decl_id points to non‑struct".into(),
                        span: span.clone(),
                    }),
                };

                if fields.len() != struct_fields.len() {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::ArityMismatch),
                        msg: format!(
                            "struct `{}` has {} fields, but {} were provided",
                            decl.ident,
                            struct_fields.len(),
                            fields.len()
                        ),
                        span: span.clone(),
                    });
                }

                let struct_scheme = self.decl_type_map.get(&decl_id)
                    .ok_or_else(|| DiagMsg {
                        title: format!("{:?}", TypeCheckerError::InternalError),
                        msg: "struct type scheme not found".into(),
                        span: span.clone(),
                    })?
                    .clone();
                let mut subst_map = HashMap::new();
                for (i, &qv) in struct_scheme.quantified.iter().enumerate() {
                    if i < subst.len() {
                        subst_map.insert(qv, subst[i]);
                    }
                }
                self.check_generic_constraints(decl_id, &struct_scheme.quantified, &subst_map, &span)?;

                let mut var_map: HashMap<SymId, TyId> = HashMap::new();
                for (gp, &actual_ty) in generic_params.iter().zip(subst.iter()) {
                    var_map.insert(gp.name.sym_id, actual_ty);
                }

                let mut inserted_symbols = Vec::new();
                for (sym_id, &ty) in &var_map {
                    self.name_type_map.insert(*sym_id, TypeScheme { quantified: vec![], body: ty });
                    inserted_symbols.push(*sym_id);
                }

                for (field_name, field_expr) in fields {
                    let def = struct_fields.iter()
                        .find(|f| f.name.name == *field_name)
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", TypeCheckerError::UnknownField),
                            msg: format!("struct `{}` has no field `{}`", decl.ident, field_name),
                            span: span.clone(),
                        })?;
                    let field_ty = self.resolve_type_name(&def.type_ann, span.clone())?;
                    self.infer_expr(*field_expr, Some(field_ty))?;
                }

                // 清理临时泛型绑定
                for sym_id in inserted_symbols {
                    self.name_type_map.remove(&sym_id);
                }

                struct_ty
            },
            HirExprKind::BuildVariant { variant_name, target } => {
                let scheme = self.name_type_map.get(&variant_name.sym_id)
                    .ok_or_else(|| DiagMsg {
                        title: format!("{:?}", TypeCheckerError::UndefinedVariable),
                        msg: format!("constructor `{}` not found", variant_name.name),
                        span: self.hir_name_span(variant_name, span.clone()),
                    })?
                    .clone();
                let ctor_ty = self.instantiate(&scheme);

                let arg_ty = self.infer_expr(*target, None)?;
                let root = self.representative(ctor_ty);

                let (param_ty, result_ty) = match &self.type_pool[root].kind {
                    TypeNodeKind::ADT { .. } => (self.builtin.unit, ctor_ty),
                    TypeNodeKind::Fun { param_tys, return_ty } => {
                        if param_tys.len() != 1 {
                            return Err(DiagMsg {
                                title: format!("{:?}", TypeCheckerError::ArityMismatch),
                                msg: format!("constructor expects exactly 1 argument, got {}", param_tys.len()),
                                span: span.clone(),
                            });
                        }
                        (param_tys[0], *return_ty)
                    }
                    _ => {
                        return Err(DiagMsg {
                            title: format!("{:?}", TypeCheckerError::TypeMismatch),
                            msg: format!("constructor `{}` has unexpected type", variant_name.name),
                            span: span.clone(),
                        });
                    }
                };

                self.unify(arg_ty, param_ty, span.clone())?;

                let result_root = self.representative(result_ty);
                if let TypeNodeKind::ADT { decl_id, subst } = &self.type_pool[result_root].kind.clone() {
                    let adt_scheme = self.decl_type_map.get(decl_id)
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", TypeCheckerError::InternalError),
                            msg: "ADT type scheme not found".into(),
                            span: span.clone(),
                        })?
                        .clone();
                    let mut subst_map = HashMap::new();
                    for (i, &qv) in adt_scheme.quantified.iter().enumerate() {
                        if i < subst.len() {
                            subst_map.insert(qv, subst[i]);
                        }
                    }
                    self.check_generic_constraints(*decl_id, &adt_scheme.quantified, &subst_map, &span)?;
                }

                result_ty
            }
            HirExprKind::Raise { control_name, args } => {
                let scheme = self.name_type_map.get(&control_name.sym_id)
                    .ok_or_else(|| DiagMsg {
                        title: format!("{:?}", TypeCheckerError::UndefinedVariable),
                        msg: format!("effect control `{}` not found", control_name.name),
                        span: self.hir_name_span(control_name, span.clone()),
                    })?
                    .clone();

                let ctrl_ty = self.instantiate(&scheme);

                let arg_tys: Vec<TyId> = args.iter()
                    .map(|&a| self.infer_expr(a, None))
                    .collect::<Result<_, _>>()?;

                let root = self.representative(ctrl_ty);
                let (param_tys, ret_ty) = if let TypeNodeKind::Fun { param_tys, return_ty } = &self.type_pool[root].kind {
                    if param_tys.len() != arg_tys.len() {
                        return Err(DiagMsg {
                            title: format!("{:?}", TypeCheckerError::ArityMismatch),
                            msg: format!("control expects {} arguments, got {}", param_tys.len(), arg_tys.len()),
                            span: span.clone(),
                        });
                    }
                    (param_tys.clone(), *return_ty)
                } else {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::TypeMismatch),
                        msg: format!("control `{}` is not a function type", control_name.name),
                        span: span.clone(),
                    });
                };

                for (arg_ty, &param_ty) in arg_tys.iter().zip(param_tys.iter()) {
                    self.unify(*arg_ty, param_ty, span.clone())?;
                }

                ret_ty
            }

            HirExprKind::UnsafeExternalCall { callee, args } =>
                self.infer_call(*callee, args, expected, &span)?,

            HirExprKind::Resume { expr } => {
                let resume_fun_ty = {
                    let (ty, called) = self.current_resume_ty.as_mut()
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", TypeCheckerError::InternalError),
                            msg: "`resume` used outside of a catch clause".into(),
                            span: span.clone(),
                        })?;

                    *called = true;
                    *ty
                };

                let root = self.representative(resume_fun_ty);

                let (arg_ty, ret_ty) = match &self.type_pool[root].kind {
                    TypeNodeKind::Fun { param_tys, return_ty } =>
                        (param_tys[0], *return_ty),
                    _ => unreachable!(),
                };

                let val_ty = self.infer_expr(*expr, Some(arg_ty))?;
                self.unify(val_ty, arg_ty, span.clone())?;
                ret_ty
            }

            HirExprKind::With { handler, clauses } => {
                let body_ty = self.infer_expr(*handler, expected)?;

                for clause in clauses {
                    let scheme = self.name_type_map.get(&clause.control_path.sym_id)
                        .cloned()
                        .ok_or_else(|| DiagMsg {
                            title: format!("{:?}", TypeCheckerError::UndefinedVariable),
                            msg: format!("control `{}` not found", clause.control_path.name),
                            span: clause.span.clone(),
                        })?;

                    let ctrl_ty = self.instantiate(&scheme);
                    let ctrl_root = self.representative(ctrl_ty);

                    let (ctrl_param_tys, ctrl_ret_ty) =
                        if let TypeNodeKind::Fun { param_tys, return_ty } =
                            &self.type_pool[ctrl_root].kind
                        {
                            (param_tys.clone(), *return_ty)
                        } else {
                            return Err(DiagMsg {
                                title: format!("{:?}", TypeCheckerError::InvalidControlType),
                                msg: format!("control `{}` is not a function", clause.control_path.name),
                                span: clause.span.clone(),
                            });
                        };

                    if clause.params.len() != ctrl_param_tys.len() {
                        return Err(DiagMsg {
                            title: format!("{:?}", TypeCheckerError::ArityMismatch),
                            msg: format!(
                                "control `{}` expects {} arguments, got {}",
                                clause.control_path.name,
                                ctrl_param_tys.len(),
                                clause.params.len()
                            ),
                            span: clause.span.clone(),
                        });
                    }

                    // 新作用域
                    let saved_level = self.current_level;
                    self.current_level += 1;

                    let mut bound_symbols = Vec::new();
                    for (pat, &pty) in clause.params.iter().zip(&ctrl_param_tys) {
                        if let HirPattern::Binding(name) = pat {
                            self.local_binding_map.insert(name.sym_id, pty);
                        }
                        self.bind_pattern(pat, pty, &mut bound_symbols)?;
                    }

                    let resume_fun_ty = self.new_compound(TypeNodeKind::Fun {
                        param_tys: vec![ctrl_ret_ty],
                        return_ty: body_ty,
                    });

                    self.current_resume_ty = Some((resume_fun_ty, false));

                    let clause_ty = self.infer_expr(clause.body, Some(body_ty))?;
                    self.unify(clause_ty, body_ty, clause.span.clone())?;

                    let (_, resume_called) = self.current_resume_ty.unwrap();

                    let ctrl_ret_root = self.representative(ctrl_ret_ty);
                    let is_unit = matches!(
                        &self.type_pool[ctrl_ret_root].kind,
                        TypeNodeKind::Tuple(elems) if elems.is_empty()
                    );

                    if !is_unit && !resume_called {
                        return Err(DiagMsg {
                            title: format!("{:?}", TypeCheckerError::MissingResume),
                            msg: format!(
                                "control `{}` expects a return value of type `{}`, but `resume` was never called in the handler",
                                clause.control_path.name,
                                self.ty_to_string(ctrl_ret_ty)
                            ),
                            span: clause.span.clone(),
                        });
                    }

                    // 清理
                    self.current_resume_ty = None;
                    for sym_id in bound_symbols {
                        self.name_type_map.remove(&sym_id);
                    }
                    self.current_level = saved_level;
                }

                body_ty
            }

            HirExprKind::Match { scrutinee, arms } => {
                let scrutinee_ty = self.infer_expr(*scrutinee, None)?;

                let mut match_res_ty = expected;
                let mut patterns_for_check = Vec::new();

                for arm in arms {
                    patterns_for_check.push(arm.pattern.clone());

                    // new scope
                    let saved_level = self.current_level;
                    self.current_level += 1;

                    let bindings = self.check_pattern(scrutinee_ty, &arm.pattern, &arm.span)?;

                    let mut bound_symbols = Vec::new();
                    for (sym_id, ty) in bindings {
                        self.name_type_map.insert(sym_id, TypeScheme { quantified: vec![], body: ty });
                        self.local_binding_map.insert(sym_id, ty);
                        bound_symbols.push(sym_id);
                    }

                    // guard
                    if let Some(guard) = arm.guard {
                        self.infer_expr(guard, Some(self.builtin.bool_ty))?;
                    }

                    let arm_ty = self.infer_expr(arm.body, match_res_ty)?;
                    if match_res_ty.is_none() {
                        match_res_ty = Some(arm_ty);
                    }

                    for sym_id in bound_symbols {
                        self.name_type_map.remove(&sym_id);
                    }
                    self.current_level = saved_level;
                }

                // exhaustiveness and usefulness
                self.check_match_exhaustiveness(scrutinee_ty, &patterns_for_check, &span)?;

                match_res_ty.unwrap_or(self.builtin.unit)
            }

            HirExprKind::Is { expr, pattern } => {
                let expr_ty = self.infer_expr(*expr, None)?;
                let bindings = self.check_pattern(expr_ty, pattern, &span)?;

                for (sym_id, ty) in bindings {
                    self.name_type_map
                        .insert(sym_id, TypeScheme { quantified: vec![], body: ty });
                    self.local_binding_map.insert(sym_id, ty);
                }

                self.builtin.bool_ty
            }

            HirExprKind::Ellipsis => todo!("maybe deprecated?"),

        };

        if let Some(expected_ty) = expected {
            self.unify(ty, expected_ty, span)?;
        }
        self.expr_type_map.insert(expr_id, ty);
        Ok(ty)
    }

    fn infer_lit(&mut self, lit: &HirLit) -> Result<TyId, DiagMsg> {
        match lit {
            HirLit::Int(_) => Ok(self.builtin.int32),
            HirLit::Decimal(_) => Ok(self.builtin.float64),
            HirLit::Str(_) => todo!("string literals not yet implemented"),
            HirLit::Bool(_) => Ok(self.builtin.bool_ty),
        }
    }

    fn infer_binary(&mut self, left: HirExprId, right: HirExprId, op: HirBinOp, span: &Span) -> Result<TyId, DiagMsg> {
        let lt = self.infer_expr(left, None)?;
        let rt = self.infer_expr(right, None)?;
        match op {
            HirBinOp::Add | HirBinOp::Sub | HirBinOp::Mul | HirBinOp::Div | HirBinOp::Mod => {
                self.unify(lt, rt, span.clone())?;
                Ok(lt)
            }
            HirBinOp::And | HirBinOp::Or => {
                self.unify(lt, self.builtin.bool_ty, span.clone())?;
                self.unify(rt, self.builtin.bool_ty, span.clone())?;
                Ok(self.builtin.bool_ty)
            }
            HirBinOp::Eq | HirBinOp::Neq | HirBinOp::Lt | HirBinOp::Gt | HirBinOp::Le | HirBinOp::Ge => {
                self.unify(lt, rt, span.clone())?;
                Ok(self.builtin.bool_ty)
            }
        }
    }

    fn infer_unary(&mut self, op: HirUnaryOp, right: HirExprId, span: &Span) -> Result<TyId, DiagMsg> {
        let rt = self.infer_expr(right, None)?;
        match op {
            HirUnaryOp::Neg => Ok(rt),
            HirUnaryOp::Not => {
                self.unify(rt, self.builtin.bool_ty, span.clone())?;
                Ok(self.builtin.bool_ty)
            }
        }
    }

    fn infer_call(
        &mut self,
        callee: HirExprId,
        args: &[HirExprId],
        expected: Option<TyId>,
        span: &Span,
    ) -> Result<TyId, DiagMsg> {
        let callee_kind = self.hir_crate.hir_expr_pool[callee].kind.clone();

        let (callee_ty, fun_info) = if let HirExprKind::Ident(ref name) = callee_kind {
            if let Some(scheme) = self.name_type_map.get(&name.sym_id).cloned() {
                let (inst_ty, subst) = self.instantiate_with_map(&scheme);
                let decl_id = self.sym_to_decl.get(&name.sym_id).copied();
                let is_fun = decl_id.map_or(false, |id| {
                    matches!(&self.hir_crate.hir_decl_pool[id].kind, HirDeclKind::Fun { .. })
                });
                if is_fun {
                    let fun_decl_id = decl_id.unwrap();
                    (inst_ty, Some((fun_decl_id, scheme.quantified, subst)))
                } else {
                    (inst_ty, None)
                }
            } else {
                (self.infer_expr(callee, None)?, None)
            }
        } else {
            (self.infer_expr(callee, None)?, None)
        };

        let arg_tys: Vec<TyId> = (0..args.len()).map(|_| self.new_type_var()).collect();
        let ret_ty = expected.unwrap_or_else(|| self.new_type_var());
        let fun_ty = self.new_compound(TypeNodeKind::Fun {
            param_tys: arg_tys.clone(),
            return_ty: ret_ty,
        });
        self.unify(callee_ty, fun_ty, span.clone())?;

        for (arg_id, &expected_arg_ty) in args.iter().zip(&arg_tys) {
            self.infer_expr(*arg_id, Some(expected_arg_ty))?;
        }

        // 检查约束
        if let Some((decl_id, quantified, subst)) = fun_info {
            let mut actual_subst = HashMap::new();
            for (&qv, &fresh_var) in &subst {
                let actual_ty = self.representative(fresh_var);
                actual_subst.insert(qv, actual_ty);
            }
            self.check_generic_constraints(decl_id, &quantified, &actual_subst, span)?;
        }

        Ok(ret_ty)
    }

    fn infer_block(&mut self, stmts: &[HirExprId], expected: Option<TyId>, _span: &Span) -> Result<TyId, DiagMsg> {
        if stmts.is_empty() {
            return Ok(self.builtin.unit);
        }
        let last_idx = stmts.len() - 1;
        for &stmt_id in &stmts[..last_idx] {
            self.infer_expr(stmt_id, Some(self.builtin.unit))?;
        }
        self.infer_expr(stmts[last_idx], expected)
    }

    fn infer_let(
        &mut self,
        let_expr_id: HirExprId,
        name: &HirName,
        type_ann: Option<&HirTypeName>,
        init: HirExprId,
        span: &Span
    ) -> Result<TyId, DiagMsg> {
        let init_ty = self.infer_expr(init, None)?;
        let init_ty = self.representative(init_ty);

        if let Some(ann) = type_ann {
            let ann_ty = self.resolve_type_name(ann, span.clone())?;
            self.unify(init_ty, ann_ty, self.hir_name_span(name, span.clone()))?;
        }
        let scheme = self.generalize(init_ty);
        self.name_type_map.insert(name.sym_id, scheme.clone());
        self.local_binding_map.insert(name.sym_id, init_ty);
        Ok(self.builtin.unit)
    }

    fn infer_if(
        &mut self,
        cond: HirExprId,
        then: HirExprId,
        elifs: &[(HirExprId, HirExprId)],
        else_opt: Option<HirExprId>,
        expected: Option<TyId>,
        span: &Span
    ) -> Result<TyId, DiagMsg> {
        self.infer_expr(cond, Some(self.builtin.bool_ty))?;
        let then_ty = self.infer_expr(then, expected)?;
        for &(elif_cond, elif_body) in elifs {
            self.infer_expr(elif_cond, Some(self.builtin.bool_ty))?;
            let elif_ty = self.infer_expr(elif_body, expected)?;
            self.unify(then_ty, elif_ty, span.clone())?;
        }
        if let Some(else_expr) = else_opt {
            let else_ty = self.infer_expr(else_expr, expected)?;
            self.unify(then_ty, else_ty, span.clone())?;
        } else {
            self.unify(then_ty, self.builtin.unit, span.clone())?;
        }
        Ok(then_ty)
    }

    fn infer_tuple(&mut self, elements: &[HirExprId], expected: Option<TyId>, _span: &Span) -> Result<TyId, DiagMsg> {
        if let Some(exp) = expected {
            let root = self.representative(exp);
            if let TypeNodeKind::Tuple(elem_tys) = &self.type_pool[root].kind {
                if elem_tys.len() == elements.len() {
                    let elem_tys = elem_tys.clone();
                    for (&e, et) in elements.iter().zip(elem_tys) {
                        self.infer_expr(e, Some(et))?;
                    }
                    return Ok(exp);
                }
            }
        }
        let mut elem_tys = Vec::new();
        for &e in elements {
            elem_tys.push(self.infer_expr(e, None)?);
        }
        Ok(self.new_compound(TypeNodeKind::Tuple(elem_tys)))
    }

    fn infer_return(&mut self, expr: Option<&HirExprId>, expected: Option<TyId>, _span: &Span) -> Result<TyId, DiagMsg> {
        if let Some(e) = expr {
            self.infer_expr(*e, expected)?;
        }
        Ok(self.builtin.never)
    }

    fn infer_cast(&mut self, expr: HirExprId, type_ann: &HirTypeName, span: &Span) -> Result<TyId, DiagMsg> {
        let target_ty = self.resolve_type_name(type_ann, span.clone())?;
        self.infer_expr(expr, None)?;
        Ok(target_ty)
    }

    fn check_decl(&mut self, decl_id: HirDeclId) -> Result<(), DiagMsg> {
        let decl = self.hir_crate.hir_decl_pool[decl_id].clone();
        match &decl.kind {
            HirDeclKind::Fun { generic_params, params, return_type, body } => {
                self.check_fun(decl_id, generic_params, params, return_type.as_ref(), body)
            }
            HirDeclKind::Struct { generic_params, fields, .. } => {
                self.check_struct(decl_id, generic_params, fields)
            }
            HirDeclKind::External { params, return_type, .. } => {
                self.check_external(decl_id, params, return_type)
            }
            HirDeclKind::ADT { generic_params, ctors, .. } => {
                self.check_adt(decl_id, generic_params, ctors)
            }
            HirDeclKind::TypeAlias { generic_params, alias_for } => {
                self.check_type_alias(decl_id, generic_params, alias_for)
            }
            HirDeclKind::Abstract { .. } => todo!("abstract type not yet supported"),

            HirDeclKind::CType => Ok(()),

            HirDeclKind::TypeDecl => Ok(()),

            HirDeclKind::Effect { controls } => {
                self.check_effect(decl_id, controls)
            }

            HirDeclKind::Const { expr, type_ann } => {
                self.check_global_value_decl(decl_id, *expr, type_ann.as_ref())
            }
            HirDeclKind::Global { expr, type_ann } => {
                self.check_global_value_decl(decl_id, *expr, type_ann.as_ref())
            }
        }
    }

    fn check_fun(&mut self, decl_id: HirDeclId, generic_params: &[HirGenericParam], params: &[HirParam], return_type: Option<&HirTypeName>, body: &[HirExprId]) -> Result<(), DiagMsg> {
        let saved_level = self.current_level;

        self.current_level += 1;

        for gp in generic_params {
            let tv = self.new_type_var();
            self.name_type_map.insert(gp.name.sym_id, TypeScheme { quantified: vec![], body: tv });
        }

        let mut param_tys = Vec::new();
        let has_all_param_ty_ann = params.iter().all(|p|
            p.type_ann.is_some()
        );
        for p in params {
            let p_ty = if let Some(ann) = &p.type_ann {
                self.resolve_type_name(ann, p.span.clone())?
            } else {
                self.new_type_var()
            };
            self.name_type_map.insert(p.name.sym_id, TypeScheme { quantified: vec![], body: p_ty });
            param_tys.push(p_ty);
        }

        let ret_ty = if let Some(rt) = return_type {
            self.resolve_type_name(rt, self.hir_crate.hir_decl_pool[decl_id].span.clone())?
        } else {
            self.new_type_var()
        };
        let has_ret_ty_ann = return_type.is_some();


        let body_ty = if body.is_empty() {
            self.builtin.unit
        } else {
            let last_idx = body.len() - 1;
            for &stmt_id in &body[..last_idx] {
                self.infer_expr(stmt_id, Some(self.builtin.unit))?;
            }
            self.infer_expr(body[last_idx], Some(ret_ty))?
        };


        self.unify(body_ty, ret_ty, self.hir_crate.hir_decl_pool[decl_id].span.clone())?;
        let fun_ty = self.new_compound(TypeNodeKind::Fun {
            param_tys,
            return_ty: ret_ty,
        });


        for gp in generic_params {
            self.name_type_map.remove(&gp.name.sym_id);
        }

        for p in params {
            self.name_type_map.remove(&p.name.sym_id);
        }

        // 先清理环境再泛化
        self.current_level = saved_level;
        let scheme = self.generalize(fun_ty);
        self.decl_type_map.insert(decl_id, scheme);


        // pub external fun 不能省ann
        if self.hir_crate.pub_decl_ids.contains(&decl_id)
            && (!has_all_param_ty_ann || !has_ret_ty_ann) {

            return Err(DiagMsg {
                title: format!("{:?}", TypeCheckerError::MissingTypeAnnotation),
                msg: "pub(external) function parameters must have type annotations".into(),
                span: self.hir_crate.hir_decl_pool[decl_id].span.clone(),
            });
        }

        Ok(())
    }

    fn check_adt(
        &mut self,
        decl_id: HirDeclId,
        generic_params: &[HirGenericParam],
        ctors: &[HirCtorDef],
    ) -> Result<(), DiagMsg> {
        let saved_level = self.current_level;
        self.current_level += 1;

        let mut gen_vars = Vec::new();
        for gp in generic_params {
            let tv = self.new_type_var();
            gen_vars.push(tv);
            self.name_type_map.insert(gp.name.sym_id, TypeScheme { quantified: vec![], body: tv });
        }

        let adt_ty = self.new_compound(TypeNodeKind::ADT {
            decl_id,
            subst: gen_vars.clone(),
        });

        for ctor in ctors {
            let param_ty = if let Some(from_type) = &ctor.from_type {
                self.resolve_type_name(from_type, ctor.span.clone())?
            } else {
                self.builtin.unit
            };
            let ctor_ty = if ctor.from_type.is_some() {
                self.new_compound(TypeNodeKind::Fun {
                    param_tys: vec![param_ty],
                    return_ty: adt_ty,
                })
            } else {
                adt_ty
            };
            let scheme = TypeScheme {
                quantified: gen_vars.clone(),
                body: ctor_ty,
            };
            self.name_type_map.insert(ctor.name.sym_id, scheme);
        }

        for gp in generic_params {
            self.name_type_map.remove(&gp.name.sym_id);
        }

        self.current_level = saved_level;

        let scheme = TypeScheme {
            quantified: gen_vars,
            body: adt_ty,
        };
        self.decl_type_map.insert(decl_id, scheme);

        Ok(())
    }

    fn check_struct(
        &mut self,
        decl_id: HirDeclId,
        generic_params: &[HirGenericParam],
        fields: &[HirFieldDef]
    ) -> Result<(), DiagMsg> {
        let saved_level = self.current_level;
        self.current_level += 1;
        let mut gen_vars = Vec::new();
        for gp in generic_params {
            let tv = self.new_type_var();
            gen_vars.push(tv);
            self.name_type_map.insert(gp.name.sym_id, TypeScheme { quantified: vec![], body: tv });
        }
        for f in fields {
            // 仅检查字段类型是否可解析
            self.resolve_type_name(&f.type_ann, f.span.clone())?;
        }
        let struct_ty = self.new_compound(TypeNodeKind::Struct {
            decl_id,
            subst: gen_vars.clone(),
        });

        // 直接构建类型方案
        let scheme = TypeScheme {
            quantified: gen_vars.clone(),
            body: struct_ty,
        };

        for gp in generic_params {
            self.name_type_map.remove(&gp.name.sym_id);
        }
        self.decl_type_map.insert(decl_id, scheme);
        self.current_level = saved_level;
        Ok(())
    }

    fn check_external(
        &mut self,
        decl_id: HirDeclId,
        params: &[HirParam],
        return_type: &HirTypeName
    ) -> Result<(), DiagMsg> {
        let mut param_tys = Vec::new();
        for p in params {
            let p_ty = if let Some(ann) = &p.type_ann {
                self.resolve_type_name(ann, p.span.clone())?
            } else {
                return Err(DiagMsg {
                    title: format!("{:?}", TypeCheckerError::MissingTypeAnnotation),
                    msg: "external function parameters must have type annotations".into(),
                    span: p.span.clone(),
                });
            };
            param_tys.push(p_ty);
        }
        let ret_ty = self.resolve_type_name(return_type, self.hir_crate.hir_decl_pool[decl_id].span.clone())?;
        let fun_ty = self.new_compound(TypeNodeKind::Fun { param_tys, return_ty: ret_ty });
        self.decl_type_map.insert(decl_id, TypeScheme { quantified: vec![], body: fun_ty });
        Ok(())
    }

    fn check_type_alias(
        &mut self,
        decl_id: HirDeclId,
        generic_params: &[HirGenericParam],
        alias_for: &HirTypeName,
    ) -> Result<(), DiagMsg> {
        let saved_level = self.current_level;
        self.current_level += 1;

        let decl = &self.hir_crate.hir_decl_pool[decl_id];
        let self_sym_id = self.sym_to_decl.iter()
            .find_map(|(&sym, &id)| if id == decl_id { Some(sym) } else { None })
            .ok_or_else(|| DiagMsg {
                title: format!("{:?}", TypeCheckerError::InternalError),
                msg: "self symbol not found".to_string(),
                span: decl.span.clone(),
            })?;

        // 递归检测 alias_for 中是否包含自身引用
        if Self::contains_self_ref(alias_for, self_sym_id) {
            return Err(DiagMsg {
                title: format!("{:?}", TypeCheckerError::RecursiveTypeAlias),
                msg: format!("type alias `{}` recursively references itself", decl.ident),
                span: decl.span.clone(),
            });
        }

        let mut gen_vars = Vec::new();
        for gp in generic_params {
            let tv = self.new_type_var();
            gen_vars.push(tv);
            self.name_type_map.insert(
                gp.name.sym_id,
                TypeScheme { quantified: vec![], body: tv },
            );
        }

        let span = self.hir_crate.hir_decl_pool[decl_id].span.clone();
        let body_ty = self.resolve_type_name(alias_for, span)?;

        for gp in generic_params {
            self.name_type_map.remove(&gp.name.sym_id);
        }

        self.current_level = saved_level;

        let scheme = TypeScheme {
            quantified: gen_vars,
            body: body_ty,
        };
        self.decl_type_map.insert(decl_id, scheme);

        Ok(())
    }

    fn check_global_value_decl(
        &mut self,
        decl_id: HirDeclId,
        expr: HirExprId,
        type_ann: Option<&HirTypeName>
    ) -> Result<(), DiagMsg> {
        let saved_level = self.current_level;
        self.current_level += 1;

        let init_ty = self.infer_expr(expr, None)?;
        let init_ty = self.representative(init_ty);

        if let Some(ann) = type_ann {
            let span = self.hir_crate.hir_decl_pool[decl_id].span.clone();
            let ann_ty = self.resolve_type_name(ann, span.clone())?;
            self.unify(init_ty, ann_ty, span)?;
        }

        self.current_level = saved_level;
        let scheme = self.generalize(init_ty);
        self.decl_type_map.insert(decl_id, scheme.clone());

        let decl = &self.hir_crate.hir_decl_pool[decl_id];
        if let Some(&scope_id) = self.name_pass_result.source_id_to_scope.get(&decl.span.source_id) {
            if let Some((sym, _)) = self.name_pass_result.pool.lookup(scope_id, &decl.ident) {
                self.name_type_map.insert(sym.sym_id, scheme);
            }
        }

        Ok(())
    }

    fn check_effect(
        &mut self,
        decl_id: HirDeclId,
        controls: &[(HirName, Vec<HirParam>, Option<HirTypeName>)]
    ) -> Result<(), DiagMsg> {

        for (ctrl_name, params, ret_opt) in controls {
            let mut param_tys = Vec::new();
            for p in params {
                if let Some(ann) = &p.type_ann {
                    let ty = self.resolve_type_name(ann, p.span.clone())?;
                    param_tys.push(ty);
                } else {
                    param_tys.push(self.new_type_var());
                }
            }

            let ret_ty = if let Some(rt) = ret_opt {
                self.resolve_type_name(rt, self.hir_crate.hir_decl_pool[decl_id].span.clone())?
            } else {
                self.builtin.unit
            };

            let fun_ty = self.new_compound(TypeNodeKind::Fun {
                param_tys,
                return_ty: ret_ty,
            });

            self.name_type_map.insert(
                ctrl_name.sym_id,
                TypeScheme { quantified: vec![], body: fun_ty },
            );
        }
        Ok(())
    }

    fn contains_self_ref(ty: &HirTypeName, target_sym: SymId) -> bool {
        match ty {
            HirTypeName::Named { path, generics } => {
                if path.sym_id == target_sym { return true; }
                generics.iter().any(|g| Self::contains_self_ref(g, target_sym))
            }
            HirTypeName::Ref(inner) => Self::contains_self_ref(inner, target_sym),
            HirTypeName::MutRef(inner) => Self::contains_self_ref(inner, target_sym),
            HirTypeName::Share(inner) => Self::contains_self_ref(inner, target_sym),
            HirTypeName::Tuple(elems) => elems.iter().any(|e| Self::contains_self_ref(e, target_sym)),
            HirTypeName::Fun { params, return_type } => {
                params.iter().any(|p| Self::contains_self_ref(p, target_sym)) ||
                    Self::contains_self_ref(return_type, target_sym)
            }
            HirTypeName::Impl(inner) => Self::contains_self_ref(inner, target_sym),
        }
    }

    fn build_sym_to_decl(&mut self) {
        for (id, decl) in self.hir_crate.hir_decl_pool.iter().enumerate() {
            if let Some(&scope_id) = self.name_pass_result
                .source_id_to_scope
                .get(&decl.span.source_id)
            {
                if let Some((sym, _)) = self.name_pass_result.pool.lookup(scope_id, &decl.ident) {
                    self.sym_to_decl.insert(sym.sym_id, id);
                }
            }
        }
    }

    fn ty_kind_to_string(&self, kind: &TypeNodeKind) -> String {
        match kind {
            TypeNodeKind::Builtin(b) => format!("{:?}", b),
            TypeNodeKind::Var => "_".to_string(),
            TypeNodeKind::Never => "Never".to_string(),
            TypeNodeKind::Fun { param_tys, return_ty } => {
                let params: Vec<String> = param_tys.iter()
                    .map(|&p| self.ty_to_string(p))
                    .collect();
                format!("({}) -> {}", params.join(", "), self.ty_to_string(*return_ty))
            }
            TypeNodeKind::Tuple(elems) => {
                let elems: Vec<String> = elems.iter()
                    .map(|&e| self.ty_to_string(e))
                    .collect();
                format!("({})", elems.join(", "))
            }
            TypeNodeKind::Struct { decl_id, subst } => {
                let ident = self.hir_crate.hir_decl_pool[*decl_id].ident.clone();
                let subst_str: Vec<String> = subst.iter().map(|&s| self.ty_to_string(s)).collect();
                format!("{}{}", ident, if subst_str.is_empty() {
                    String::new()
                } else {
                    format!("[{}]", subst_str.join(", "))
                })
            }
            TypeNodeKind::Ref(inner) => format!("ref {}", self.ty_to_string(*inner)),
            TypeNodeKind::MutRef(inner) => format!("ref mut {}", self.ty_to_string(*inner)),
            TypeNodeKind::Share(inner) => format!("share {}", self.ty_to_string(*inner)),
            TypeNodeKind::ADT { decl_id, subst } => {
                let ident = self.hir_crate.hir_decl_pool[*decl_id].ident.clone();
                let subst_str: Vec<String> = subst.iter().map(|&s| self.ty_to_string(s)).collect();
                format!("{}{}", ident, if subst_str.is_empty() {
                    String::new()
                } else {
                    format!("[{}]", subst_str.join(", "))
                })
            }
        }
    }

    fn get_root(&self, mut id: TyId) -> TyId {
        while self.type_pool[id].parent != id {
            id = self.type_pool[id].parent;
        }
        id
    }

    fn ty_to_string(&self, ty: TyId) -> String {
        let root = self.get_root(ty);
        self.ty_kind_to_string(&self.type_pool[root].kind)
    }
}

impl TypeCheckerApi for TypeChecker {
    fn new(mut hir_crate: HirCrate) -> Self {
        let name_pass_result = hir_crate.name_pass_result.take()
            .expect("name pass must be run before type checking");
        let mut ty_pool = Vec::new();
        let builtin = Self::create_builtins(&mut ty_pool);
        TypeChecker {
            hir_crate,
            name_pass_result,
            decl_type_map: HashMap::new(),
            expr_type_map: HashMap::new(),
            name_type_map: HashMap::new(),
            local_binding_map: HashMap::new(),
            sym_to_decl: HashMap::new(),
            type_pool: ty_pool,
            current_level: 0,
            current_resume_ty: None,
            builtin,
        }
    }

    fn check(mut self) -> Result<TypeCheckerResult, DiagMsg> {

        self.build_sym_to_decl();

        for decl_id in 0..self.hir_crate.hir_decl_pool.len() {
            let decl = &self.hir_crate.hir_decl_pool[decl_id];
            let gen_count = match &decl.kind {
                HirDeclKind::Fun { generic_params, .. } => generic_params.len(),
                HirDeclKind::Struct { generic_params, .. } => generic_params.len(),
                HirDeclKind::ADT { generic_params, .. } => generic_params.len(),
                HirDeclKind::TypeAlias { generic_params, .. } => generic_params.len(),
                HirDeclKind::Abstract { generic_params, .. } => generic_params.len(),
                _ => 0,
            };
            let gen_vars: Vec<TyId> = (0..gen_count).map(|_| self.new_type_var()).collect();
            let body = self.new_type_var();
            self.decl_type_map.insert(decl_id, TypeScheme { quantified: gen_vars, body });
        }
        for decl_id in 0..self.hir_crate.hir_decl_pool.len() {
            self.check_decl(decl_id)?;
        }
        self.hir_crate.type_pool = self.type_pool;

        Ok(TypeCheckerResult {
            decl_type_map: self.decl_type_map,
            expr_type_map: self.expr_type_map,
            hir: self.hir_crate,
            local_binding_map: self.local_binding_map,
        })
    }
}