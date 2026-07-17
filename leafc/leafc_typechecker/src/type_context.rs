use std::collections::HashSet;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::source::Span;
use leafc_coreapi::type_checker::TypeCheckerError;
use leafc_coreapi::type_context::{TyId, TypeContextApi, TypeKind, TypeUnit};
use leafc_coreapi::type_context::TypeKind::Var;
use crate::TypeChecker;

impl TypeContextApi for TypeChecker {
    fn push_concrete(&mut self, ty: TypeUnit) -> TyId {
        let id = self.hir_crate.type_pool.len();
        self.hir_crate.type_pool.push(ty);
        self.bindings.push(None);
        id
    }
    fn new_ty_var(&mut self, span: Span) -> TyId {
        let id = self.hir_crate.type_pool.len();
        let var_seq = self.hir_crate.type_pool.len();
        self.hir_crate.type_pool.push(TypeUnit {
            decl: None,
            kind: Var(var_seq),
            span,
        });
        self.bindings.push(None);
        id
    }
    fn resolve(&self, mut id: TyId) ->Result<TyId, DiagMsg> {
        let mut visited = HashSet::new();
        while let Some(bound_to) = self.bindings[id] {
            if !visited.insert(id) {
                return Err(DiagMsg {
                    title: format!("{:?}", TypeCheckerError::DuplicateType),
                    msg: "duplicate type id".to_string(),
                    span: self.hir_crate.type_pool[bound_to].span.clone(),
                })
            }
            id = bound_to;
        }
        Ok(id)
    }
    fn kind_of(&self, id: TyId) -> Result<TypeKind, DiagMsg> {
        let resolved = self.resolve(id)?;
        Ok(self.hir_crate.type_pool[resolved].kind.clone())
    }
    /// 合一
    fn unify(&mut self, lhs: TyId, rhs: TyId, span: Span) -> Result<(), DiagMsg> {
        let lhs = self.resolve(lhs)?;
        let rhs = self.resolve(rhs)?;

        if lhs == rhs {
            return Ok(());
        }

        let lhs_kind = self.hir_crate.type_pool[lhs].kind.clone();
        let rhs_kind = self.hir_crate.type_pool[rhs].kind.clone();

        match (lhs_kind, rhs_kind) {
            (Var(_), Var(_)) => {
                self.bind_var(lhs, rhs)?;
            }
            (Var(_), _) => {
                self.bind_var(lhs, rhs)?;
            }
            (_, Var(_)) => {
                self.bind_var(rhs, lhs)?;
            }
            (TypeKind::Builtin(b1), TypeKind::Builtin(b2)) => {
                if b1 == b2 {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::TypeMismatch),
                        msg: "type mismatch: builtin types differ".to_string(),
                        span,
                    });
                }
            }
            (TypeKind::Fun { param_tys: p1, return_ty: r1 },
                TypeKind::Fun { param_tys: p2, return_ty: r2 }) => {
                if p1.len() != p2.len() {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::TypeMismatch),
                        msg: format!("function arity mismatch: {} vs {}", p1.len(), p2.len()),
                        span,
                    });
                }
                for (a, b) in p1.iter().zip(p2.iter()) {
                    self.unify(*a, *b, span.clone())?;
                }
                self.unify(r1, r2, span.clone())?;
            }
            (TypeKind::Struct { fields: f1 },
                TypeKind::Struct { fields: f2 }) => {
                if f1.len() != f2.len() {
                    return Err(DiagMsg {
                        title: format!("{:?}", TypeCheckerError::TypeMismatch),
                        msg: format!("struct field count mismatch: {} vs {}", 
                            f1.len(),
                            f2.len()
                        ),
                        span,
                    });
                }
                for (a, b) in f1.iter().zip(f2.iter()) {
                    self.unify(*a, *b, span.clone())?;
                }
            }
            _ => {
                return Err(DiagMsg {
                    title: format!("{:?}", TypeCheckerError::TypeMismatch),
                    msg: "type mismatch: incompatible type shapes".to_string(),
                    span,
                });
            }
        }
        Ok(())
    }
    fn bind_var(&mut self, var: TyId, ty: TyId) -> Result<(), DiagMsg> {
        if self.occurs(var, ty)? {
            return Err(DiagMsg {
                title: format!("{:?}", TypeCheckerError::InfiniteType),
                msg: "infinite type: variable occurs inside its own binding".to_string(),
                span: self.hir_crate.type_pool[ty].span.clone(),
            })
        }
        self.bindings[var] = Some(ty);
        Ok(())
    }
    fn occurs(&self, var: TyId, ty: TyId) -> Result<bool, DiagMsg> {
        let ty = self.resolve(ty)?;
        if ty == var {
            return Ok(true);
        }

        let kind = &self.hir_crate.type_pool[ty].kind;
        match kind {
            TypeKind::Builtin(_) | TypeKind::Var(_) => Ok(false),
            TypeKind::Fun { param_tys, return_ty } => {

                let mut is_occur = false;
                for param_ty in param_tys {
                    is_occur = self.occurs(var, *param_ty)? || self.occurs(var, *return_ty)?
                }
                Ok(is_occur)
            }
            TypeKind::Struct { fields } => {
                let mut is_occur = false;
                for field in fields {
                    is_occur = self.occurs(var, *field)?
                }
                Ok(is_occur)
            }
            TypeKind::Tuple(tys) => {
                let mut is_occur = false;
                for ty in tys {
                    is_occur = self.occurs(var, *ty)?
                }
                Ok(is_occur)
            }
        }
    }
}