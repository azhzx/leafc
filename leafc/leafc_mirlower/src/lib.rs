use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::{
    HirBinOp, HirCrate, HirDeclId, HirDeclKind, HirExpr, HirExprId, HirExprKind,
    HirLit, HirName, HirTypeName, HirUnaryOp,
};
use leafc_coreapi::mir::{
    BasicBlock, BasicBlockId, BinOp, Const, FnSig, LocalDecl, LocalId,
    MirCrate, MirFun, MirStmt, MirStmtKind, Place, Rvalue,
    TerminatorKind, UnOp,
};
use leafc_coreapi::mir_lower::MirLowerApi;
use leafc_coreapi::type_checker::TypeCheckerResult;
use leafc_coreapi::type_system::{TyId, TypeNode, TypeNodeKind};
use std::collections::HashMap;

struct FnBuilder {
    name: String,
    signature: FnSig,
    locals: Vec<LocalDecl>,
    basic_blocks: Vec<BasicBlock>,
    current_block: BasicBlockId,
    expr_types: HashMap<HirExprId, TyId>,
    let_types: HashMap<HirExprId, TyId>,
    type_pool: Vec<TypeNode>,
    next_local: LocalId,
}

impl FnBuilder {
    fn new(
        name: String,
        return_ty: TyId,
        type_pool: Vec<TypeNode>,
        expr_types: HashMap<HirExprId, TyId>,
        let_types: HashMap<HirExprId, TyId>,
    ) -> Self {
        let mut builder = FnBuilder {
            name,
            signature: FnSig {
                params: Vec::new(),
                return_ty,
            },
            locals: Vec::new(),
            basic_blocks: Vec::new(),
            current_block: 0,
            expr_types,
            let_types,
            type_pool,
            next_local: 0,
        };
        let entry = builder.new_block();
        builder.current_block = entry;
        builder
    }

    fn new_block(&mut self) -> BasicBlockId {
        let id = self.basic_blocks.len();
        self.basic_blocks.push(BasicBlock {
            statements: vec![],
            terminator: TerminatorKind::Unreachable,
        });
        id
    }

    fn alloc_local(&mut self, ty: TyId, mutable: bool, name: Option<String>) -> LocalId {
        let id = self.next_local;
        self.next_local += 1;
        self.locals.push(LocalDecl { ty, mutable, name });
        id
    }

    fn place_for_local(&self, id: LocalId) -> Place {
        Place::Local(id)
    }

    fn push_stmt(&mut self, stmt: MirStmt) {
        self.basic_blocks[self.current_block].statements.push(stmt);
    }

    fn set_terminator(&mut self, term: TerminatorKind) {
        self.basic_blocks[self.current_block].terminator = term;
    }

    fn const_from_lit(&self, lit: &HirLit) -> Const {
        match lit {
            HirLit::Int(s) => Const::Int32(s.parse().unwrap_or(0)),
            HirLit::Decimal(s) => Const::Float64(s.parse().unwrap_or(0.0_f64).to_bits()),
            HirLit::Str(s) => Const::Str(s.clone()),
            HirLit::Bool(b) => Const::Bool(*b),
        }
    }

    fn ty_of_expr(&self, expr_id: HirExprId) -> TyId {
        self.expr_types
            .get(&expr_id)
            .copied()
            .unwrap_or_else(|| panic!("missing type for expr {}", expr_id))
    }

    fn compile_expr(&mut self, expr_id: HirExprId, hir_pool: &[HirExpr]) -> Place {
        let expr = &hir_pool[expr_id];
        match &expr.kind {
            HirExprKind::Lit(lit) => {
                let con = self.const_from_lit(lit);
                let ty = self.ty_of_expr(expr_id);
                let tmp = self.alloc_local(ty, false, None);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: self.place_for_local(tmp),
                        rvalue: Rvalue::Constant(con),
                    },
                });
                self.place_for_local(tmp)
            }

            HirExprKind::Ident(_) => todo!("identifier lowering"),

            HirExprKind::Binary { left, right, op } => {
                let lhs = self.compile_expr(*left, hir_pool);
                let rhs = self.compile_expr(*right, hir_pool);
                let ty = self.ty_of_expr(expr_id);
                let tmp = self.alloc_local(ty, false, None);
                let binop = match op {
                    HirBinOp::Add => BinOp::Add,
                    HirBinOp::Sub => BinOp::Sub,
                    HirBinOp::Mul => BinOp::Mul,
                    HirBinOp::Div => BinOp::Div,
                    HirBinOp::Mod => BinOp::Rem,
                    HirBinOp::And => BinOp::BitAnd,
                    HirBinOp::Or  => BinOp::BitOr,
                    HirBinOp::Eq  => BinOp::Eq,
                    HirBinOp::Neq => BinOp::Ne,
                    HirBinOp::Lt  => BinOp::Lt,
                    HirBinOp::Gt  => BinOp::Gt,
                    HirBinOp::Le  => BinOp::Le,
                    HirBinOp::Ge  => BinOp::Ge,
                };
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: self.place_for_local(tmp),
                        rvalue: Rvalue::BinaryOp {
                            op: binop,
                            left: Box::new(self.place_to_rvalue(lhs)),
                            right: Box::new(self.place_to_rvalue(rhs)),
                        },
                    },
                });
                self.place_for_local(tmp)
            }

            HirExprKind::Unary { op, right } => {
                let rhs = self.compile_expr(*right, hir_pool);
                let ty = self.ty_of_expr(expr_id);
                let tmp = self.alloc_local(ty, false, None);
                let unop = match op {
                    HirUnaryOp::Neg => UnOp::Neg,
                    HirUnaryOp::Not => UnOp::Not,
                };
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: self.place_for_local(tmp),
                        rvalue: Rvalue::UnaryOp {
                            op: unop,
                            right: Box::new(self.place_to_rvalue(rhs)),
                        },
                    },
                });
                self.place_for_local(tmp)
            }

            HirExprKind::Let { name, init, .. } => {
                let init_place = self.compile_expr(*init, hir_pool);
                // fixme: 优先用 let_type_map，回退到 expr_type_map
                let ty = self
                    .let_types
                    .get(&expr_id)
                    .or_else(|| self.expr_types.get(init))
                    .copied()
                    .expect("let binding missing type");
                let local = self.alloc_local(ty, false, Some(name.name.clone()));
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: self.place_for_local(local),
                        rvalue: self.place_to_rvalue(init_place),
                    },
                });
                let unit_ty = self
                    .type_pool
                    .iter()
                    .position(|n| n.kind == TypeNodeKind::Unit)
                    .unwrap();
                let tmp = self.alloc_local(unit_ty, false, None);
                self.push_stmt(MirStmt {
                    kind: MirStmtKind::Assign {
                        place: self.place_for_local(tmp),
                        rvalue: Rvalue::Constant(Const::Bool(false)), // 占位
                    },
                });
                self.place_for_local(tmp)
            }

            HirExprKind::Block { stmts } => {
                if stmts.is_empty() {
                    let unit_ty = self
                        .type_pool
                        .iter()
                        .position(|n| n.kind == TypeNodeKind::Unit)
                        .unwrap();
                    let tmp = self.alloc_local(unit_ty, false, None);
                    return self.place_for_local(tmp);
                }
                let len = stmts.len();
                for (i, &stmt_id) in stmts.iter().enumerate() {
                    if i == len - 1 {
                        return self.compile_expr(stmt_id, hir_pool);
                    } else {
                        self.compile_expr(stmt_id, hir_pool);
                    }
                }
                unreachable!()
            }

            HirExprKind::If { .. } => todo!("if lowering"),

            HirExprKind::Return { expr } => {
                if let Some(e) = expr {
                    self.compile_expr(*e, hir_pool);
                }
                self.set_terminator(TerminatorKind::Return);
                let never_ty = self
                    .type_pool
                    .iter()
                    .position(|n| n.kind == TypeNodeKind::Never)
                    .unwrap();
                let tmp = self.alloc_local(never_ty, false, None);
                self.place_for_local(tmp)
            }

            HirExprKind::Tuple { elements } => {
                // let mut values = Vec::new();
                // for &elem in elements {
                //     let value = self.place_to_rvalue(self
                //         .compile_expr(elem, hir_pool));
                //     values.push(value);
                // }
                // let ty = self.ty_of_expr(expr_id);
                // let tmp = self.alloc_local(ty, false, None);
                // self.push_stmt(MirStmt {
                //     kind: MirStmtKind::Assign {
                //         place: self.place_for_local(tmp),
                //         rvalue: Rvalue::Tuple(values),
                //     },
                // });
                // self.place_for_local(tmp)
                todo!("tuple lowering")
            }

            _ => todo!("MIR lowering for {:?}", expr.kind),
        }
    }

    fn place_to_rvalue(&self, place: Place) -> Rvalue {
        match place {
            Place::Local(id) => Rvalue::Copy(Place::Local(id)),
            other => Rvalue::Copy(other),
        }
    }

    fn finish(mut self) -> (MirFun, Vec<BasicBlock>) {
        let last = self.basic_blocks.last_mut().unwrap();
        let ret_is_unit = self
            .type_pool
            .get(self.signature.return_ty)
            .map_or(false, |n| n.kind == TypeNodeKind::Unit);
        if matches!(last.terminator, TerminatorKind::Unreachable) {
            last.terminator = TerminatorKind::Return;
        }
        let local_ids: Vec<BasicBlockId> = (0..self.basic_blocks.len()).collect();
        let fun = MirFun {
            name: self.name,
            signature: self.signature,
            local_decls: self.locals,
            blocks: local_ids,
        };
        (fun, self.basic_blocks)
    }
}

pub struct MirLower {
    result: TypeCheckerResult,
}

impl MirLowerApi for MirLower {
    fn new(result: TypeCheckerResult) -> Self {
        MirLower { result }
    }

    fn lower(self) -> Result<MirCrate, DiagMsg> {
        let mut global_blocks: Vec<BasicBlock> = vec![];
        let mut functions: Vec<MirFun> = vec![];

        let type_pool = &self.result.hir.type_pool;

        for (decl_id, decl) in self.result.hir.hir_decl_pool.iter().enumerate() {
            if let HirDeclKind::Fun {
                params,
                return_type,
                body,
                ..
            } = &decl.kind
            {
                let scheme = self.result.decl_type_map.get(&decl_id).unwrap();
                let fun_ty_id = scheme.body;
                let ret_ty = if let TypeNodeKind::Fun { return_ty, .. } = &type_pool[fun_ty_id].kind {
                    *return_ty
                } else {
                    type_pool
                        .iter()
                        .position(|n| n.kind == TypeNodeKind::Unit)
                        .unwrap() // 后备
                };

                let mut builder = FnBuilder::new(
                    decl.ident.clone(),
                    ret_ty,
                    type_pool.clone(),
                    self.result.expr_type_map.clone(),
                    self.result.let_type_map.clone(),
                );

                // param
                if let TypeNodeKind::Fun { param_tys, .. } = &type_pool[fun_ty_id].kind {
                    for (i, p) in params.iter().enumerate() {
                        let ty = param_tys[i];
                        builder.alloc_local(ty, false, Some(p.name.name.clone()));
                        // TODO: 记录参数名→local 映射，以便支持 Ident
                    }
                }

                if body.is_empty() {
                    builder.set_terminator(TerminatorKind::Return);
                } else {
                    let last_idx = body.len().saturating_sub(1);
                    for (i, &expr_id) in body.iter().enumerate() {
                        builder.compile_expr(expr_id, &self.result.hir.hir_expr_pool);
                    }
                }

                let (mut fun, local_blocks) = builder.finish();
                // 将本地块合并到全局块池，并重映射索引
                let base = global_blocks.len();
                let new_blocks: Vec<BasicBlockId> =
                    fun.blocks.iter().map(|&local| base + local).collect();
                global_blocks.extend(local_blocks);
                fun.blocks = new_blocks;
                functions.push(fun);
            } else if let HirDeclKind::External { .. } = &decl.kind {
                todo!()
            }
        }

        Ok(MirCrate {
            functions,
            blocks: global_blocks,
        })
    }
}