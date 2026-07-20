use std::collections::{HashMap, HashSet};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::{
    HirBinOp, HirCrate, HirDeclId, HirDeclKind, HirExprId, HirExprKind,
    HirFieldDef, HirGenericParam, HirLit, HirName, HirParam, HirTypeName, HirUnaryOp,
};
use leafc_coreapi::name_pass::NamePassResult;
use leafc_coreapi::scope::SymId;
use leafc_coreapi::type_checker::{TypeCheckerApi, TypeCheckerResult};
use leafc_coreapi::source::Span;
use leafc_coreapi::type_system::{BuiltinType, HirDeclTypeMap, LetExprIdTypeMap, TyId, TypeNode, TypeNodeKind, TypeScheme};

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
    expr_type_map: HashMap<HirExprId, TyId>,
    name_type_map: HashMap<SymId, TypeScheme>,
    let_type_map: LetExprIdTypeMap,

    ty_pool: Vec<TypeNode>,
    current_level: u32,

    builtin: BuiltinTypes,
}

impl TypeChecker {
    fn create_builtins(ty_pool: &mut Vec<TypeNode>) -> BuiltinTypes {
        let mut push = |kind: TypeNodeKind| -> TyId {
            let id = ty_pool.len();
            ty_pool.push(TypeNode { kind, parent: id, level: 0 });
            id
        };
        let int8 = push(TypeNodeKind::Builtin(BuiltinType::Int8));
        let int16 = push(TypeNodeKind::Builtin(BuiltinType::Int16));
        let int32 = push(TypeNodeKind::Builtin(BuiltinType::Int32));
        let int64 = push(TypeNodeKind::Builtin(BuiltinType::Int64));
        let uint8 = push(TypeNodeKind::Builtin(BuiltinType::UInt8));
        let uint16 = push(TypeNodeKind::Builtin(BuiltinType::UInt16));
        let uint32 = push(TypeNodeKind::Builtin(BuiltinType::UInt32));
        let uint64 = push(TypeNodeKind::Builtin(BuiltinType::UInt64));
        let float32 = push(TypeNodeKind::Builtin(BuiltinType::Float32));
        let float64 = push(TypeNodeKind::Builtin(BuiltinType::Float64));
        let bool_ty = push(TypeNodeKind::Builtin(BuiltinType::Bool));
        let unit = push(TypeNodeKind::Unit);
        let never = push(TypeNodeKind::Never);
        BuiltinTypes {
            int8, int16, int32, int64, uint8, uint16, uint32, uint64,
            float32, float64, bool_ty, unit, never,
        }
    }

    pub fn new(mut hir_crate: HirCrate) -> Self {
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
            let_type_map: HashMap::new(),
            ty_pool,
            current_level: 0,
            builtin,
        }
    }

    fn sym_span(&self, sym_id: SymId, fallback: Span) -> Span {
        self.name_pass_result.pool
            .get_symbol_by_id(sym_id)
            .map(|sym| sym.def_span.clone())
            .unwrap_or(fallback)
    }

    /// 从 HirName 获取符号 span
    fn hir_name_span(&self, name: &HirName, fallback: Span) -> Span {
        self.sym_span(name.sym_id, fallback)
    }

    fn representative(&mut self, mut id: TyId) -> TyId {
        let parent = self.ty_pool[id].parent;
        if parent != id {
            let root = self.representative(parent);
            self.ty_pool[id].parent = root; // 路径压缩
            root
        } else {
            id
        }
    }

    fn new_type_var(&mut self) -> TyId {
        let id = self.ty_pool.len();
        self.ty_pool.push(TypeNode {
            kind: TypeNodeKind::Var,
            parent: id,
            level: self.current_level,
        });
        id
    }

    fn new_compound(&mut self, kind: TypeNodeKind) -> TyId {
        let id = self.ty_pool.len();
        self.ty_pool.push(TypeNode {
            kind,
            parent: id,
            level: 0,
        });
        id
    }

    /// 解析类型名称，支持泛型参数替换
    fn resolve_type_name(&mut self, name: &HirTypeName, span: Span) -> Result<TyId, DiagMsg> {

        if name.args.is_empty() {
            let builtin_ty = match name.name.name.as_str() {
                "Int8"    => Some(self.builtin.int8),
                "Int16"   => Some(self.builtin.int16),
                "Int32"   => Some(self.builtin.int32),
                "Int64"   => Some(self.builtin.int64),
                "UInt8"   => Some(self.builtin.uint8),
                "UInt16"  => Some(self.builtin.uint16),
                "UInt32"  => Some(self.builtin.uint32),
                "UInt64"  => Some(self.builtin.uint64),
                "Float32" => Some(self.builtin.float32),
                "Float64" => Some(self.builtin.float64),
                "Bool"    => Some(self.builtin.bool_ty),
                _ => None,
            };
            if let Some(ty) = builtin_ty {
                return Ok(ty);
            }
        }

        let decl = self.hir_crate.hir_decl_pool.iter()
            .find(|d| d.ident == name.name.name)
            .ok_or_else(|| DiagMsg {
                title: "UndefinedType".into(),
                msg: format!("unknown type `{}`", name.name.name),
                span: self.hir_name_span(&name.name, span.clone()),
            })?;

        let scheme = self.decl_type_map.get(&decl.hir_id)
            .ok_or_else(|| DiagMsg {
                title: "TypeNotChecked".into(),
                msg: format!("type `{}` not yet fully checked", name.name.name),
                span: self.hir_name_span(&name.name, span.clone()),
            })?.clone();
        if scheme.quantified.len() != name.args.len() {
            return Err(DiagMsg {
                title: "GenericArityMismatch".into(),
                msg: format!("expected {} type arguments, got {}",
                             scheme.quantified.len(), name.args.len()),
                span: self.hir_name_span(&name.name, span),
            });
        }
        // 实例化
        let mut subst_map = HashMap::new();
        for (&qv, arg_ty_name) in scheme.quantified.iter().zip(&name.args) {
            let arg_ty = self.resolve_type_name(arg_ty_name, span.clone())?;
            subst_map.insert(qv, arg_ty);
        }
        self.copy_type_with_subst(scheme.body.clone(), &subst_map)
    }

    fn copy_type_with_subst(&mut self, ty: TyId, subst: &HashMap<TyId, TyId>) -> Result<TyId, DiagMsg> {
        let root = self.representative(ty);
        match self.ty_pool[root].kind.clone() {
            TypeNodeKind::Var => {
                if let Some(&replacement) = subst.get(&root) {
                    Ok(replacement)
                } else {
                    Ok(self.new_type_var())
                }
            }
            TypeNodeKind::Builtin(_) | TypeNodeKind::Never | TypeNodeKind::Unit => Ok(root),
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
                    decl_id: decl_id,
                    subst: new_subst?,
                }))
            }
        }
    }

    fn unify(&mut self, t1: TyId, t2: TyId, span: Span) -> Result<(), DiagMsg> {
        let r1 = self.representative(t1);
        let r2 = self.representative(t2);
        if r1 == r2 { return Ok(()); }
        let k1 = self.ty_pool[r1].kind.clone();
        let k2 = self.ty_pool[r2].kind.clone();
        match (&k1, &k2) {
            (TypeNodeKind::Var, TypeNodeKind::Var) => {
                let lv1 = self.ty_pool[r1].level;
                let lv2 = self.ty_pool[r2].level;
                if lv1 <= lv2 {
                    self.ty_pool[r2].parent = r1;
                } else {
                    self.ty_pool[r1].parent = r2;
                }
                Ok(())
            }
            (TypeNodeKind::Var, _) => {
                self.check_occurs(r1, r2, span.clone())?;
                self.ty_pool[r1].parent = r2;
                Ok(())
            }
            (_, TypeNodeKind::Var) => {
                self.check_occurs(r2, r1, span.clone())?;
                self.ty_pool[r2].parent = r1;
                Ok(())
            }
            (TypeNodeKind::Builtin(b1), TypeNodeKind::Builtin(b2)) if b1 == b2 => Ok(()),
            (TypeNodeKind::Fun { param_tys: p1, return_ty: r1 },
                TypeNodeKind::Fun { param_tys: p2, return_ty: r2 }) => {
                if p1.len() != p2.len() {
                    return Err(DiagMsg {
                        title: "ArityMismatch".into(),
                        msg: format!("function arity mismatch: {} vs {}", p1.len(), p2.len()),
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
                        title: "TypeMismatch".into(),
                        msg: format!("tuple length mismatch: {} vs {}", e1.len(), e2.len()),
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
                        title: "TypeMismatch".into(),
                        msg: "struct generic parameter count mismatch".into(),
                        span,
                    });
                }
                for (&a, &b) in s1.iter().zip(s2.iter()) {
                    self.unify(a, b, span.clone())?;
                }
                Ok(())
            }
            _ => Err(DiagMsg {
                title: "TypeMismatch".into(),
                msg: format!("cannot unify {:?} with {:?}", k1, k2),
                span,
            }),
        }
    }

    fn check_occurs(&mut self, var: TyId, ty: TyId, span: Span) -> Result<(), DiagMsg> {
        let root = self.representative(ty);
        if root == var {
            return Err(DiagMsg {
                title: "InfiniteType".into(),
                msg: "infinite type detected".into(),
                span,
            });
        }
        match self.ty_pool[root].kind.clone() {
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

    // 泛化与实例化
    fn generalize(&mut self, body: TyId) -> TypeScheme {
        let mut free_vars = Vec::new();
        self.collect_free_vars(body, &mut free_vars);
        free_vars.sort_unstable();
        free_vars.dedup();
        let mut quantified = Vec::new();
        for &var in &free_vars {
            let root = self.representative(var);
            if self.ty_pool[root].level > self.current_level {
                quantified.push(root);
            }
        }
        TypeScheme { quantified, body }
    }

    fn instantiate(&mut self, scheme: &TypeScheme) -> TyId {
        let mut subst = HashMap::new();
        for &qv in &scheme.quantified {
            let new_var = self.new_type_var();
            subst.insert(qv, new_var);
        }
        self.copy_type_with_subst(scheme.body, &subst)
            .expect("instantiation should not fail")
    }

    fn collect_free_vars(&mut self, ty: TyId, out: &mut Vec<TyId>) {
        let root = self.representative(ty);
        match self.ty_pool[root].kind.clone() {
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

    // 表达式推断
    fn infer_expr(&mut self, expr_id: HirExprId, expected: Option<TyId>) -> Result<TyId, DiagMsg> {
        let expr = self.hir_crate.hir_expr_pool[expr_id].clone();
        let span = expr.span.clone();

        let ty = match &expr.kind {
            HirExprKind::Lit(lit) => self.infer_lit(lit)?,
            HirExprKind::Ident(name) => {
                let scheme = self.name_type_map.get(&name.sym_id)
                    .ok_or_else(|| DiagMsg {
                        title: "UndefinedVariable".into(),
                        msg: format!("undefined variable `{}`", name.name),
                        span: self.hir_name_span(name, span.clone()),
                    })?.clone();
                self.instantiate(&scheme)
            }
            HirExprKind::Binary { left, right, op } =>
                self.infer_binary(*left, *right, *op, &span)?,
            HirExprKind::Unary { op, right } =>
                self.infer_unary(*op, *right, &span)?,
            HirExprKind::Call { callee, args } =>
                self.infer_call(*callee, args, expected, &span)?,
            HirExprKind::Block { stmts } =>
                self.infer_block(stmts, expected, &span)?,
            HirExprKind::Let { name, type_ann, init, .. } =>
                self.infer_let(expr_id, name, type_ann.as_ref(), *init, &span)?,
            HirExprKind::If { cond, then, elifs, else_opt } =>
                self.infer_if(*cond, *then, elifs, *else_opt, expected, &span)?,
            HirExprKind::Tuple { elements } =>
                self.infer_tuple(elements, expected, &span)?,
            HirExprKind::Return { expr } =>
                self.infer_return(expr.as_ref(), &span)?,
            HirExprKind::TypeCast { expr, type_ann } =>
                self.infer_cast(*expr, type_ann, &span)?,
            HirExprKind::Move { target } | HirExprKind::Copy { target } |
            HirExprKind::Ref { target } | HirExprKind::MutRef { target } |
            HirExprKind::Share { target } => self.infer_expr(*target, expected)?,
            HirExprKind::FieldAccess { obj, field } => {
                let obj_ty = self.infer_expr(*obj, None)?;
                // TODO: 查找结构体字段类型
                let _ = field;
                self.new_type_var()
            }
            HirExprKind::Ellipsis => self.builtin.never,
            _ => todo!("expression kind not implemented"),
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

    fn infer_call(&mut self, callee: HirExprId, args: &[HirExprId], expected: Option<TyId>, span: &Span) -> Result<TyId, DiagMsg> {
        let callee_ty = self.infer_expr(callee, None)?;
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

    fn infer_let(&mut self, let_expr_id: HirExprId, name: &HirName, type_ann: Option<&HirTypeName>, init: HirExprId, span: &Span) -> Result<TyId, DiagMsg> {
        let init_ty = self.infer_expr(init, None)?;
        if let Some(ann) = type_ann {
            let ann_ty = self.resolve_type_name(ann, span.clone())?;
            self.unify(init_ty, ann_ty, self.hir_name_span(name, span.clone()))?;
        }
        let scheme = self.generalize(init_ty);
        self.name_type_map.insert(name.sym_id, scheme.clone());
        self.let_type_map.insert(let_expr_id, scheme.body);
        Ok(self.builtin.unit)
    }

    fn infer_if(&mut self, cond: HirExprId, then: HirExprId, elifs: &[(HirExprId, HirExprId)], else_opt: Option<HirExprId>, expected: Option<TyId>, span: &Span) -> Result<TyId, DiagMsg> {
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
            if let TypeNodeKind::Tuple(elem_tys) = &self.ty_pool[root].kind {
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

    fn infer_return(&mut self, expr: Option<&HirExprId>, _span: &Span) -> Result<TyId, DiagMsg> {
        if let Some(e) = expr {
            self.infer_expr(*e, None)?;
        }
        Ok(self.builtin.never)
    }

    fn infer_cast(&mut self, expr: HirExprId, type_ann: &HirTypeName, span: &Span) -> Result<TyId, DiagMsg> {
        let target_ty = self.resolve_type_name(type_ann, span.clone())?;
        self.infer_expr(expr, None)?;
        Ok(target_ty)
    }

    // 声明检查
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
            HirDeclKind::ADT { .. } => todo!("ADT not yet supported"),
            HirDeclKind::TypeAlias { .. } => todo!("type alias not yet supported"),
            HirDeclKind::Abstract { .. } => todo!("abstract type not yet supported"),
            HirDeclKind::CType => Ok(()),
        }
    }

    fn check_fun(&mut self, decl_id: HirDeclId, generic_params: &[HirGenericParam], params: &[HirParam], return_type: Option<&HirTypeName>, body: &[HirExprId]) -> Result<(), DiagMsg> {
        let saved_level = self.current_level;
        self.current_level += 1;
        let mut gen_vars = Vec::new();
        for gp in generic_params {
            let tv = self.new_type_var();
            gen_vars.push(tv);
            self.name_type_map.insert(gp.name.sym_id, TypeScheme { quantified: vec![], body: tv });
        }
        let mut param_tys = Vec::new();
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
        let scheme = self.generalize(fun_ty);
        for gp in generic_params {
            self.name_type_map.remove(&gp.name.sym_id);
        }
        for p in params {
            self.name_type_map.remove(&p.name.sym_id);
        }
        self.decl_type_map.insert(decl_id, scheme);
        self.current_level = saved_level;
        Ok(())
    }

    fn check_struct(&mut self, decl_id: HirDeclId, generic_params: &[HirGenericParam], fields: &[HirFieldDef]) -> Result<(), DiagMsg> {
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
        let scheme = self.generalize(struct_ty);
        for gp in generic_params {
            self.name_type_map.remove(&gp.name.sym_id);
        }
        self.decl_type_map.insert(decl_id, scheme);
        self.current_level = saved_level;
        Ok(())
    }

    fn check_external(&mut self, decl_id: HirDeclId, params: &[HirParam], return_type: &HirTypeName) -> Result<(), DiagMsg> {
        let mut param_tys = Vec::new();
        for p in params {
            let p_ty = if let Some(ann) = &p.type_ann {
                self.resolve_type_name(ann, p.span.clone())?
            } else {
                return Err(DiagMsg {
                    title: "MissingTypeAnnotation".into(),
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

    pub fn check(mut self) -> Result<TypeCheckerResult, DiagMsg> {
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
        Ok(TypeCheckerResult {
            decl_type_map: self.decl_type_map,
            expr_type_map: self.expr_type_map,
            let_type_map: self.let_type_map,
            hir: self.hir_crate,
        })
    }
}

impl TypeCheckerApi for TypeChecker {
    fn new(hir_crate: HirCrate) -> Self {
        TypeChecker::new(hir_crate)
    }

    fn check(mut self) -> Result<TypeCheckerResult, DiagMsg> {
        self.check()
    }
}