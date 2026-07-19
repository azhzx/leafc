
pub mod type_context;

use std::collections::HashMap;

use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::{
    HirBinOp, HirCrate, HirDeclKind, HirExprId, HirExprKind, HirLit, HirName, HirTypeName,
    HirUnaryOp,
};
use leafc_coreapi::type_checker::{TypeCheckerApi, TypeCheckerError, TypeCheckerResult};
use leafc_coreapi::type_context::{
    BuiltinType, HirDeclTypeMap, HirExprTypeMap, TyId, TypeContextApi, TypeKind, TypeUnit,
};
use leafc_coreapi::source::Span;

pub struct TypeChecker {
    hir_crate: HirCrate,
    bindings: Vec<Option<TyId>>,
    decl_type_map: HirDeclTypeMap,
    expr_type_map: HirExprTypeMap,
    env: HashMap<String, TyId>,
    return_ty: Option<TyId>,
}

impl TypeChecker {
    fn insert_type(&mut self, kind: TypeKind, span: Option<Span>) -> TyId {
        let id = self.hir_crate.type_pool.len();
        self.hir_crate.type_pool.push(TypeUnit {
            decl: None,
            kind,
            span,
        });
        self.bindings.push(None);
        id
    }

    fn builtin_ty(&mut self, b: BuiltinType) -> TyId {
        self.insert_type(TypeKind::Builtin(b), None)
    }

    fn fresh_var(&mut self, span: Span) -> TyId {
        self.insert_type(TypeKind::Var, Some(span))
    }

    fn resolve_type_name(&mut self, name: &HirTypeName) -> Result<TyId, DiagMsg> {
        match name.name.name.as_str() {
            "Int" => return Ok(self.builtin_ty(BuiltinType::Int32)),
            "Float" => return Ok(self.builtin_ty(BuiltinType::Float64)),
            "Bool" => return Ok(self.builtin_ty(BuiltinType::Int8)),
            "Never" => return Ok(self.builtin_ty(BuiltinType::Never)),
            "Int64" => return Ok(self.builtin_ty(BuiltinType::Int64)),
            "UInt8" => return Ok(self.builtin_ty(BuiltinType::UInt8)),
            "UInt16" => return Ok(self.builtin_ty(BuiltinType::UInt16)),
            "UInt32" => return Ok(self.builtin_ty(BuiltinType::UInt32)),
            "UInt64" => return Ok(self.builtin_ty(BuiltinType::UInt64)),
            "Float32" => return Ok(self.builtin_ty(BuiltinType::Float32)),
            "Float64" => return Ok(self.builtin_ty(BuiltinType::Float64)),
            _ => {}
        }

        if let Some(&ty_id) = self.env.get(&name.name.name) {
            return Ok(ty_id);
        }

        for (decl_id, &ty) in &self.decl_type_map {
            let decl = &self.hir_crate.hir_decl_pool[*decl_id];
            if decl.ident == name.name.name {
                return Ok(ty);
            }
        }

        // Err(DiagMsg {
        //     title: "UnknownType".into(),
        //     msg: format!("unknown type `{}`", name.name.name),
        //     span: ,
        // })
        todo!()
    }

    fn optional_type_ann(&mut self, ann: &Option<HirTypeName>) -> Result<Option<TyId>, DiagMsg> {
        match ann {
            Some(tn) => Ok(Some(self.resolve_type_name(tn)?)),
            None => Ok(None),
        }
    }
}


impl TypeCheckerApi for TypeChecker {
    fn new(hir_crate: HirCrate) -> Self {
        TypeChecker {
            hir_crate,
            bindings: Vec::new(),
            decl_type_map: HashMap::new(),
            expr_type_map: HashMap::new(),
            env: HashMap::new(),
            return_ty: None,
        }
    }

    fn check(mut self) -> Result<TypeCheckerResult, DiagMsg> {
        self.create_decl_types()?;

        for (i, decl) in self.hir_crate.hir_decl_pool.iter().enumerate() {
            let ty = self.decl_type_map[&i];
            self.env.insert(decl.ident.clone(), ty);
        }

        let mut fun_bodies: Vec<(Vec<HirExprId>, Vec<(String, TyId)>, TyId)> = Vec::new();

        for (i, decl) in self.hir_crate.hir_decl_pool.iter().enumerate() {
            if let HirDeclKind::Fun { params, body, .. } = &decl.kind {
                let fun_ty = self.decl_type_map[&i];
                let resolved_fun_ty = self.resolve(fun_ty)?;
                if let TypeKind::Fun {
                    param_tys,
                    return_ty,
                } = self.hir_crate.type_pool[resolved_fun_ty].kind.clone()
                {
                    let mut param_bindings = Vec::new();
                    for (p, pt) in params.iter().zip(param_tys.iter()) {
                        param_bindings.push((p.name.name.clone(), *pt));
                    }
                    fun_bodies.push((body.clone(), param_bindings, return_ty));
                } else {
                    return Err(DiagMsg {
                        title: "InvalidFunctionType".into(),
                        msg: "function type is not a function type".into(),
                        span: decl.span.clone(),
                    });
                }
            }
        }

        for (body, param_bindings, return_ty) in fun_bodies {
            let saved_env = self.env.clone();
            let saved_return = self.return_ty;

            for (name, ty) in param_bindings {
                self.env.insert(name, ty);
            }
            self.return_ty = Some(return_ty);

            for expr_id in body {
                self.check_stmt(expr_id)?;
            }

            self.env = saved_env;
            self.return_ty = saved_return;
        }

        Ok(TypeCheckerResult {
            decl_type_map: self.decl_type_map,
            expr_type_map: self.expr_type_map,
            hir: self.hir_crate,
        })
    }
}

impl TypeChecker {
    fn create_decl_type(&mut self, kind: &HirDeclKind, span: Span) -> Result<TyId, DiagMsg> {
        match kind {
            HirDeclKind::Fun { params, return_type, .. } => {
                let mut param_tys = Vec::new();
                for p in params {
                    let p_ty = self.optional_type_ann(&p.type_ann)?
                        .unwrap_or_else(|| self.fresh_var(p.span.clone()));
                    param_tys.push(p_ty);
                }
                let ret_ty = self.optional_type_ann(return_type)?
                    .unwrap_or_else(|| self.fresh_var(span.clone()));
                Ok(self.insert_type(
                    TypeKind::Fun { param_tys, return_ty: ret_ty },
                    Some(span),
                ))
            }
            _ => Ok(self.fresh_var(span)),
        }
    }

    /// 为所有声明生成初始类型
    fn create_decl_types(&mut self) -> Result<(), DiagMsg> {
        // 先克隆所有需要的声明信息，释放不可变引用
        let decl_info: Vec<(usize, HirDeclKind, Span)> = self.hir_crate.hir_decl_pool
            .iter()
            .map(|decl| (decl.hir_id, decl.kind.clone(), decl.span.clone()))
            .collect();

        for (i, kind, span) in decl_info {
            let ty = self.create_decl_type(&kind, span)?;
            self.decl_type_map.insert(i, ty);
        }
        Ok(())
    }

    fn infer_expr(&mut self, expr_id: HirExprId) -> Result<TyId, DiagMsg> {
        let expr = self.hir_crate.hir_expr_pool[expr_id].clone();
        let span = expr.span.clone();

        let ty = match &expr.kind {
            HirExprKind::Lit(lit) => match lit {
                HirLit::Int(_) => self.builtin_ty(BuiltinType::Int32),
                HirLit::Decimal(_) => self.builtin_ty(BuiltinType::Float64),
                HirLit::Str(_) => self.fresh_var(span),
                HirLit::Bool(_) => self.builtin_ty(BuiltinType::Int8),
            },
            HirExprKind::Ident(name) => self.env.get(&name.name).copied().ok_or_else(|| DiagMsg {
                title: "UndefinedVariable".into(),
                msg: format!("undefined variable `{}`", name.name),
                span: span.clone(),
            })?,
            HirExprKind::Binary { left, right, op } => {
                let left_ty = self.infer_expr(*left)?;
                let right_ty = self.infer_expr(*right)?;
                match op {
                    HirBinOp::Add | HirBinOp::Sub | HirBinOp::Mul | HirBinOp::Div | HirBinOp::Mod => {
                        let int_ty = self.builtin_ty(BuiltinType::Int32);
                        self.unify(left_ty, int_ty, self.hir_crate.hir_expr_pool[*left].span.clone())?;
                        self.unify(right_ty, int_ty, self.hir_crate.hir_expr_pool[*right].span.clone())?;
                        int_ty
                    }
                    HirBinOp::And | HirBinOp::Or => {
                        let bool_ty = self.builtin_ty(BuiltinType::Int8);
                        self.unify(left_ty, bool_ty, self.hir_crate.hir_expr_pool[*left].span.clone())?;
                        self.unify(right_ty, bool_ty, self.hir_crate.hir_expr_pool[*right].span.clone())?;
                        bool_ty
                    }
                    HirBinOp::Eq | HirBinOp::Neq | HirBinOp::Lt | HirBinOp::Gt | HirBinOp::Le | HirBinOp::Ge => {
                        self.unify(left_ty, right_ty, self.hir_crate.hir_expr_pool[*right].span.clone())?;
                        self.builtin_ty(BuiltinType::Int8)
                    }
                }
            }
            HirExprKind::Unary { op, right } => {
                let right_ty = self.infer_expr(*right)?;
                match op {
                    HirUnaryOp::Neg => {
                        let int_ty = self.builtin_ty(BuiltinType::Int32);
                        self.unify(right_ty, int_ty, self.hir_crate.hir_expr_pool[*right].span.clone())?;
                        int_ty
                    }
                    HirUnaryOp::Not => {
                        let bool_ty = self.builtin_ty(BuiltinType::Int8);
                        self.unify(right_ty, bool_ty, self.hir_crate.hir_expr_pool[*right].span.clone())?;
                        bool_ty
                    }
                }
            }
            HirExprKind::Call { callee, args } => {
                let callee_ty = self.infer_expr(*callee)?;
                let callee_resolved = self.resolve(callee_ty)?;
                match self.hir_crate.type_pool[callee_resolved].kind.clone() {
                    TypeKind::Fun { param_tys, return_ty } => {
                        if param_tys.len() != args.len() {
                            return Err(DiagMsg {
                                title: "ArityMismatch".into(),
                                msg: format!("expected {} arguments, got {}", param_tys.len(), args.len()),
                                span: span.clone(),
                            });
                        }
                        for (arg_id, &p_ty) in args.iter().zip(param_tys.iter()) {
                            let arg_ty = self.infer_expr(*arg_id)?;
                            self.unify(arg_ty, p_ty, self.hir_crate.hir_expr_pool[*arg_id].span.clone())?;
                        }
                        return_ty
                    }
                    _ => return Err(DiagMsg {
                        title: "NotAFunction".into(),
                        msg: "callee is not a function".into(),
                        span: span.clone(),
                    }),
                }
            }
            HirExprKind::Let { name, type_ann, init, mutable: _ } => {
                let init_ty = self.infer_expr(*init)?;
                if let Some(expected) = self.optional_type_ann(type_ann)? {
                    self.unify(init_ty, expected, self.hir_crate.hir_expr_pool[*init].span.clone())?;
                    self.env.insert(name.name.clone(), expected);
                    expected
                } else {
                    self.env.insert(name.name.clone(), init_ty);
                    init_ty
                }
            }
            HirExprKind::Block { stmts } => {
                let mut last_ty = self.builtin_ty(BuiltinType::Never);
                for &stmt_id in stmts {
                    last_ty = self.infer_expr(stmt_id)?;
                }
                last_ty
            }
            HirExprKind::Return { expr } => {
                let ret_ty = self.return_ty.ok_or_else(|| DiagMsg {
                    title: "ReturnOutsideFunction".into(),
                    msg: "return statement outside function".into(),
                    span: span.clone(),
                })?;
                if let Some(e) = expr {
                    let expr_ty = self.infer_expr(*e)?;
                    self.unify(expr_ty, ret_ty, self.hir_crate.hir_expr_pool[*e].span.clone())?;
                }
                self.builtin_ty(BuiltinType::Never)
            }
            HirExprKind::If { cond, then, elifs, else_opt } => {
                let cond_ty = self.infer_expr(*cond)?;
                let bool_ty = self.builtin_ty(BuiltinType::Int8);
                self.unify(cond_ty, bool_ty, self.hir_crate.hir_expr_pool[*cond].span.clone())?;
                let then_ty = self.infer_expr(*then)?;
                for (elif_cond, elif_body) in elifs {
                    let ec_ty = self.infer_expr(*elif_cond)?;
                    self.unify(ec_ty, bool_ty, self.hir_crate.hir_expr_pool[*elif_cond].span.clone())?;
                    let eb_ty = self.infer_expr(*elif_body)?;
                    self.unify(then_ty, eb_ty, self.hir_crate.hir_expr_pool[*elif_body].span.clone())?;
                }
                if let Some(else_expr) = else_opt {
                    let else_ty = self.infer_expr(*else_expr)?;
                    self.unify(then_ty, else_ty, self.hir_crate.hir_expr_pool[*else_expr].span.clone())?;
                }
                then_ty
            }
            HirExprKind::Tuple { elements } => {
                let mut tys = Vec::new();
                for &e in elements {
                    tys.push(self.infer_expr(e)?);
                }
                self.insert_type(TypeKind::Tuple(tys), None)
            }
            _ => self.fresh_var(span),
        };

        self.expr_type_map.insert(expr_id, ty);
        Ok(ty)
    }

    fn check_stmt(&mut self, expr_id: HirExprId) -> Result<TyId, DiagMsg> {
        self.infer_expr(expr_id)
    }
}