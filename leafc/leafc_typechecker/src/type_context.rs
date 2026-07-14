use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::source::Span;
use leafc_coreapi::type_context::{TyId, TypeContextApi, TypeKind, TypeUnit};
use leafc_coreapi::type_context::TypeKind::Var;
use crate::TypeChecker;

impl TypeContextApi for TypeChecker {
    fn push_concrete(&mut self, ty: TypeUnit) -> TyId {
        let id = self.type_pool.len();
        self.type_pool.push(ty);
        self.bindings.push(None);
        id
    }
    fn new_ty_var(&mut self, span: Span) -> TyId {
        let id = self.type_pool.len();
        let var_seq = self.type_pool.len();
        self.type_pool.push(TypeUnit {
            decl: None,
            kind: Var(var_seq),
            span,
        });
        self.bindings.push(None);
        id
    }
    fn resolve(&self, mut id: TyId) -> TyId {
        let mut visited = std::collections::HashSet::new();
        while let Some(bound_to) = self.bindings[id] {
            if !visited.insert(id) {
                panic!("duplicate type id");
            }
            id = bound_to;
        }
        id
    }
    fn kind_of(&self, id: TyId) -> &TypeKind {
        let resolved = self.resolve(id);
        &self.type_pool[resolved].kind
    }
    /// 合一
    fn unify(&mut self, lhs: TyId, rhs: TyId) -> Result<(), DiagMsg> {
        let lhs = self.resolve(lhs);
        let rhs = self.resolve(rhs);

        if lhs == rhs {
            return Ok(());
        }

        let lhs_kind = self.type_pool[lhs].kind.clone();
        let rhs_kind = self.type_pool[rhs].kind.clone();

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
                    return panic!("type mismatch: builtin types differ");
                }
            }
            (TypeKind::Fun { param_tys: p1, return_ty: r1 },
                TypeKind::Fun { param_tys: p2, return_ty: r2 }) => {
                if p1.len() != p2.len() {
                    todo!()
                    // return Err(panic!("{}", (
                    //     "function arity mismatch: {} vs {}",
                    //     p1.len(),
                    //     p2.len()
                    // )));
                }
                for (a, b) in p1.iter().zip(p2.iter()) {
                    self.unify(*a, *b)?;
                }
                self.unify(r1, r2)?;
            }
            (TypeKind::Struct { fields: f1 },
                TypeKind::Struct { fields: f2 }) => {
                if f1.len() != f2.len() {
                    return Err(panic!("struct field count mismatch: {} vs {}",
                                      f1.len(),
                                      f2.len()
                    ));
                }
                for (a, b) in f1.iter().zip(f2.iter()) {
                    self.unify(*a, *b)?;
                }
            }
            _ => {
                panic!("type mismatch: incompatible type shapes");
            }
        }
        Ok(())
    }
    fn bind_var(&mut self, var: TyId, ty: TyId) -> Result<(), DiagMsg> {
        if self.occurs(var, ty) {
            panic!("infinite type: variable occurs inside its own binding");
        }
        self.bindings[var] = Some(ty);
        Ok(())
    }
    fn occurs(&self, var: TyId, ty: TyId) -> bool {
        let ty = self.resolve(ty);
        if ty == var {
            return true;
        }
        let kind = &self.type_pool[ty].kind;
        match kind {
            TypeKind::Builtin(_) | TypeKind::Var(_) => false,
            TypeKind::Fun { param_tys, return_ty } => {
                param_tys.iter().any(|&p| self.occurs(var, p))
                    || self.occurs(var, *return_ty)
            }
            TypeKind::Struct { fields } => {
                fields.iter().any(|&f| self.occurs(var, f))
            }
            TypeKind::Tuple(tys) => {
                tys.iter().any(|&t| self.occurs(var, t))
            }
        }
    }
}