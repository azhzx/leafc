use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::{HirBinOp, HirCrate, HirDeclId, HirDeclKind, HirExprId, HirExprKind, HirLit, HirUnaryOp};
use leafc_coreapi::mir::{BasicBlock, BasicBlockId, Const, ExternDecl, FnSig, LocalDecl, LocalId, MirBinOp, MirCrate, MirFun, MirStmt, MirStmtKind, MirUnOp, Place, Rvalue, StaticDecl, StaticId, TagId, TerminatorKind};
use leafc_coreapi::mir_lower::MirLowerApi;
use leafc_coreapi::scope::SymId;
use leafc_coreapi::type_checker::TypeCheckerResult;
use leafc_coreapi::type_system::{get_type_root, TyId, TypeNodeKind};
use std::collections::HashMap;


struct FnBuilder {
    pub name: String,
    pub locals_map: HashMap<SymId, LocalId>,
    pub generic_params: Vec<TyId>,
    pub signature: FnSig,
    pub local_decls: Vec<LocalDecl>,
    pub blocks: Vec<BasicBlockId>,
}

pub struct MirLower {
    crate_name: String,
    functions: Vec<MirFun>,
    extern_decls: Vec<ExternDecl>,
    statics: Vec<StaticDecl>,
    blocks: Vec<BasicBlock>,

    type_checker_result: TypeCheckerResult,
    hir: HirCrate,

    fun: Option<FnBuilder>,
    current_block: BasicBlockId,
    current_stmts: Vec<MirStmt>,

    decl_to_static: HashMap<HirDeclId, StaticId>,

    struct_field_map: HashMap<(HirDeclId, String), usize>,
    adt_variant_map: HashMap<(HirDeclId, String), TagId>,
}

impl MirLower {

    fn new_block(&mut self) -> BasicBlockId {
        let id = self.blocks.len();
        self.blocks.push(BasicBlock {
            block_params: vec![],
            statements: vec![],
            terminator: TerminatorKind::Unreachable,
        });
        id
    }

    ///
    fn start_block(&mut self, block_id: BasicBlockId) {
        self.current_block = block_id;
        self.current_stmts.clear();
    }

    ///
    fn finish_block(&mut self, terminator: TerminatorKind) {
        let block = &mut self.blocks[self.current_block];
        block.statements = std::mem::take(&mut self.current_stmts);
        block.terminator = terminator;
    }

    ///
    fn push_stmt(&mut self, kind: MirStmtKind) {
        self.current_stmts.push(MirStmt { kind });
    }

    ///
    fn set_terminator(&mut self, terminator: TerminatorKind) {
        self.finish_block(terminator);
    }

    ///
    fn switch_to_new_block(&mut self) -> BasicBlockId {
        let next_block = self.new_block();
        self.set_terminator(TerminatorKind::Goto {
            target: next_block,
            block_args: vec![],
        });
        self.start_block(next_block);
        next_block
    }

    fn lower_function(&mut self, decl_id: HirDeclId) -> MirFun {
        let decl = self.hir.hir_decl_pool[decl_id].clone();

        let (params, return_type_ann, body) = match &decl.kind {
            HirDeclKind::Fun { params, return_type, body, .. } => {
                (params.clone(), return_type.clone(), body.clone())
            }
            _ => unreachable!(),
        };

        let ty_scheme = self.type_checker_result.decl_type_map.get(&decl_id).unwrap();
        let (param_tys, return_ty) = self.get_fn_sig_from_ty(ty_scheme.body);

        let mut fun = FnBuilder {
            name: decl.ident.clone(),
            locals_map: HashMap::new(),
            generic_params: vec![],
            signature: FnSig { params: param_tys.clone(), return_ty },
            local_decls: vec![],
            blocks: vec![],
        };

        self.fun = Some(fun);

        let ret_local = self.new_local(return_ty, true, Some("return".to_string()));

        for (param, ty) in params.iter().zip(param_tys.iter()) {
            let local = self.new_local(*ty, false, Some(param.name.name.clone()));
            self.bind_local(param.name.sym_id, local);
        }

        let entry_block = self.new_block();
        self.start_block(entry_block);

        let start_block_idx = self.blocks.len() - 1;

        let mut last_place = None;
        for stmt_expr in body {
            last_place = self.compile_expr(stmt_expr);
        }

        let need_terminator = matches!(
            self.blocks[self.current_block].terminator,
            TerminatorKind::Unreachable
        );
        if need_terminator {
            if let Some(place) = last_place {
                self.push_stmt(MirStmtKind::Store {
                    place: Place::Local(ret_local),
                    rvalue: Rvalue::Move(place),
                });
            }
            self.set_terminator(TerminatorKind::Return);
        }

        let mut fun = self.fun.take().unwrap();
        fun.blocks = (entry_block..self.blocks.len()).collect();

        MirFun {
            name: fun.name,
            generic_params: fun.generic_params,
            signature: fun.signature,
            local_decls: fun.local_decls,
            blocks: fun.blocks,
        }
    }

    fn compile_expr(&mut self, expr_id: HirExprId) -> Option<Place> {
        let expr = self.hir.hir_expr_pool[expr_id].clone();
        match &expr.kind {
            HirExprKind::Lit(lit) => {
                let mir_const = match lit {
                    HirLit::Decimal(s) => Const::Float64(s.parse().unwrap_or(0.0 as u64)),
                    HirLit::Int(s) => Const::Int32(s.parse().unwrap_or(0)),
                    HirLit::Str(s) => Const::Str(s.clone()),
                    HirLit::Bool(b) => Const::Bool(*b),
                };
                let ty = self.expr_ty(expr_id);
                let temp = self.new_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::Constant(mir_const),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::Ident(name) => {
                self.lookup_place(name.sym_id)
            }

            HirExprKind::Binary { left, right, op } => {
                let l_place = self.compile_expr(*left)?;
                let r_place = self.compile_expr(*right)?;

                let mir_op = match op {
                    HirBinOp::Add => MirBinOp::Add,
                    HirBinOp::Sub => MirBinOp::Sub,
                    HirBinOp::Mul => MirBinOp::Mul,
                    HirBinOp::Div => MirBinOp::Div,
                    HirBinOp::Mod => MirBinOp::Rem,
                    HirBinOp::And => MirBinOp::BitAnd,
                    HirBinOp::Or => MirBinOp::BitOr,
                    HirBinOp::Eq => MirBinOp::Eq,
                    HirBinOp::Neq => MirBinOp::Ne,
                    HirBinOp::Lt => MirBinOp::Lt,
                    HirBinOp::Gt => MirBinOp::Gt,
                    HirBinOp::Le => MirBinOp::Le,
                    HirBinOp::Ge => MirBinOp::Ge,
                };

                let ty = self.expr_ty(expr_id);
                let temp = self.new_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::BinaryOp {
                        op: mir_op,
                        left: Box::new(Rvalue::Copy(l_place)),
                        right: Box::new(Rvalue::Copy(r_place)),
                    },
                });
                Some(Place::Local(temp))
            }

            HirExprKind::Unary { op, right } => {
                let r_place = self.compile_expr(*right)?;
                let mir_op = match op {
                    HirUnaryOp::Neg => MirUnOp::Neg,
                    HirUnaryOp::Not => MirUnOp::Not,
                };

                let ty = self.expr_ty(expr_id);
                let temp = self.new_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::UnaryOp {
                        op: mir_op,
                        right: Box::new(Rvalue::Copy(r_place)),
                    },
                });
                Some(Place::Local(temp))
            }

            HirExprKind::Let { name, init, mutable, .. } => {
                if let Some(init_place) = self.compile_expr(*init) {
                    let ty = self.expr_ty(*init);
                    let local_id = self.new_local(ty, *mutable, Some(name.name.clone()));
                    self.bind_local(name.sym_id, local_id);

                    self.push_stmt(MirStmtKind::Let {
                        local: local_id,
                        rvalue: Rvalue::Move(init_place),
                    });
                }
                None
            }

            HirExprKind::Block { stmts } => {
                let mut last_place = None;
                for stmt in stmts {
                    last_place = self.compile_expr(*stmt);
                }
                last_place
            }

            HirExprKind::Return { expr } => {
                if let Some(ret_expr_id) = expr {
                    if let Some(place) = self.compile_expr(*ret_expr_id) {
                        self.push_stmt(MirStmtKind::Store {
                            place: Place::Local(0),
                            rvalue: Rvalue::Move(place),
                        });
                    }
                }
                self.set_terminator(TerminatorKind::Return);
                let block = self.new_block();
                self.start_block(block);
                None
            }

            HirExprKind::Call { callee, args } => {
                let callee_place = self.compile_expr(*callee)?;

                let mut mir_args = Vec::new();
                for arg_expr in args {
                    if let Some(arg_place) = self.compile_expr(*arg_expr) {
                        mir_args.push(Rvalue::Move(arg_place));
                    }
                }

                let ty = self.expr_ty(expr_id);
                let result_temp = self.new_temp(ty);
                let next_block = self.new_block();

                self.set_terminator(TerminatorKind::CallByPtr {
                    func: Rvalue::Copy(callee_place),
                    args: mir_args,
                    dest: Place::Local(result_temp),
                    target: Some(next_block),
                });

                self.start_block(next_block);
                Some(Place::Local(result_temp))
            }

            HirExprKind::UnsafeExternalCall { callee, args } => {
                let callee_place = self.compile_expr(*callee)?;
                let mut mir_args = Vec::new();
                for arg in args {
                    if let Some(arg_place) = self.compile_expr(*arg) {
                        mir_args.push(Rvalue::Move(arg_place));
                    }
                }

                let ty = self.expr_ty(expr_id);
                let result_temp = self.new_temp(ty);
                let next_block = self.new_block();

                self.set_terminator(TerminatorKind::CallByPtr {
                    func: Rvalue::Copy(callee_place),
                    args: mir_args,
                    dest: Place::Local(result_temp),
                    target: Some(next_block),
                });

                self.start_block(next_block);
                Some(Place::Local(result_temp))
            }

            HirExprKind::Tuple { elements } => {
                let mut mir_elements = Vec::new();
                for elem in elements {
                    if let Some(place) = self.compile_expr(*elem) {
                        mir_elements.push(Rvalue::Move(place));
                    }
                }

                let ty = self.expr_ty(expr_id);
                let temp = self.new_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::Tuple(mir_elements),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::Move { target } => {
                let place = self.compile_expr(*target)?;
                let ty = self.expr_ty(expr_id);
                let temp = self.new_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::Move(place),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::Copy { target } => {
                let place = self.compile_expr(*target)?;
                let ty = self.expr_ty(expr_id);
                let temp = self.new_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::Copy(place),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::Ref { target } => {
                let place = self.compile_expr(*target)?;
                let ty = self.expr_ty(expr_id);
                let temp = self.new_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::TempRef(place),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::MutRef { target } => {
                let place = self.compile_expr(*target)?;
                let ty = self.expr_ty(expr_id);
                let temp = self.new_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::TempRefMut(place),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::Share { target } => {
                let place = self.compile_expr(*target)?;
                let ty = self.expr_ty(expr_id);
                let temp = self.new_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::GcObjectRef(Box::new(Rvalue::Move(place))),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::TypeCast { expr: cast_expr, type_ann: _ } => {
                let place = self.compile_expr(*cast_expr)?;
                let dest_ty = self.expr_ty(expr_id);
                let temp = self.new_temp(dest_ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::Cast(place, dest_ty),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::FieldAccess { obj, field } => {
                let obj_place = self.compile_expr(*obj)?;
                let obj_ty = self.expr_ty(*obj);
                let obj_root_ty = get_type_root(&self.type_checker_result.type_pool, obj_ty);

                let decl_id = match &self.type_checker_result.type_pool[obj_root_ty].kind {
                    TypeNodeKind::Struct { decl_id, .. } => *decl_id,
                    _ => unreachable!(),
                };

                let field_idx = *self.struct_field_map.get(&(decl_id, field.clone())).expect("Field not found in map");

                Some(Place::Field {
                    base: Box::new(obj_place),
                    field: field_idx,
                })
            }

            HirExprKind::MakeStruct { path: _, fields } => {
                let mut mir_fields = Vec::new();
                for (_, field_expr) in fields {
                    if let Some(place) = self.compile_expr(*field_expr) {
                        mir_fields.push(Rvalue::Move(place));
                    }
                }

                let ty = self.expr_ty(expr_id);
                let temp = self.new_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::BuildStruct(mir_fields),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::BuildVariant { variant_name, target } => {
                let ty = self.expr_ty(expr_id);
                let root_ty = get_type_root(&self.type_checker_result.type_pool, ty);

                let decl_id = match &self.type_checker_result.type_pool[root_ty].kind {
                    TypeNodeKind::ADT { decl_id, .. } => *decl_id,
                    _ => unreachable!()
                };

                let tag = *self.adt_variant_map.get(&(decl_id, variant_name.name.clone())).unwrap();
                let inner_rvalue = self.compile_expr(*target)
                    .map(|p| Box::new(Rvalue::Move(p)))
                    .unwrap_or_else(|| Box::new(Rvalue::Tuple(vec![])));

                let temp = self.new_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::Variant(tag, inner_rvalue),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::If { cond, then, elifs, else_opt } => {
                let result_ty = self.expr_ty(expr_id);
                let result_temp = self.new_temp(result_ty);
                let merge_block = self.new_block();

                // 编译第一个条件
                let cond_place = self.compile_expr(*cond).unwrap();
                let then_block = self.new_block();
                let else_block = self.new_block();

                self.set_terminator(TerminatorKind::SwitchInt {
                    discriminant: Rvalue::Copy(cond_place),
                    targets: vec![(Const::Bool(true), then_block)],
                    default: else_block,
                });

                // Then
                self.start_block(then_block);
                let then_place = self.compile_expr(*then);
                if let Some(place) = then_place {
                    self.push_stmt(MirStmtKind::Store {
                        place: Place::Local(result_temp),
                        rvalue: Rvalue::Move(place),
                    });
                }
                self.set_terminator(TerminatorKind::Goto { target: merge_block, block_args: vec![] });

                let mut current_else = else_block;
                for (elif_cond, elif_body) in elifs {
                    self.start_block(current_else);
                    let cond_place = self.compile_expr(*elif_cond).unwrap();
                    let elif_then = self.new_block();
                    let next_else = self.new_block();

                    self.set_terminator(TerminatorKind::SwitchInt {
                        discriminant: Rvalue::Copy(cond_place),
                        targets: vec![(Const::Bool(true), elif_then)],
                        default: next_else,
                    });

                    self.start_block(elif_then);
                    let body_place = self.compile_expr(*elif_body);
                    if let Some(place) = body_place {
                        self.push_stmt(MirStmtKind::Store {
                            place: Place::Local(result_temp),
                            rvalue: Rvalue::Move(place),
                        });
                    }
                    self.set_terminator(TerminatorKind::Goto { target: merge_block, block_args: vec![] });

                    current_else = next_else;
                }

                self.start_block(current_else);
                if let Some(else_expr) = else_opt {
                    let else_place = self.compile_expr(*else_expr);
                    if let Some(place) = else_place {
                        self.push_stmt(MirStmtKind::Store {
                            place: Place::Local(result_temp),
                            rvalue: Rvalue::Move(place),
                        });
                    }
                } else {
                    self.push_stmt(MirStmtKind::Store {
                        place: Place::Local(result_temp),
                        rvalue: Rvalue::Tuple(vec![]),
                    });
                }
                self.set_terminator(TerminatorKind::Goto { target: merge_block, block_args: vec![] });

                self.start_block(merge_block);
                Some(Place::Local(result_temp))
            }

            HirExprKind::Ellipsis => {
                self.set_terminator(TerminatorKind::Unreachable);
                let block = self.new_block();
                self.start_block(block);
                None
            }

            HirExprKind::Match { scrutinee, arms } => {
                todo!("Lowering match expressions")
            }

            HirExprKind::Is { expr, pattern } => {
                todo!("Lowering is expressions")
            }

            HirExprKind::Raise { control_name, args } => {
                todo!("Lowering raise expressions for Algebraic Effects")
            }

            HirExprKind::With { handler: _, clauses } => {
                todo!("Lowering with expressions for Algebraic Effects")
            }

            HirExprKind::Resume { expr } => {
                todo!("Lowering resume expressions")
            }
        }
    }

    fn finish(mut self) -> (MirFun, Vec<BasicBlock>) {
        let fun = self.fun.take().expect("No function is being built");
        (MirFun {
            name: fun.name,
            generic_params: fun.generic_params,
            signature: fun.signature,
            local_decls: fun.local_decls,
            blocks: fun.blocks,
        }, self.blocks)
    }

    fn expr_ty(&mut self, expr_id: HirExprId) -> TyId {
        self.type_checker_result.expr_type_map[&expr_id]
    }

    fn get_static_id(&self, decl_id: HirDeclId) -> Option<StaticId> {
        self.decl_to_static.get(&decl_id).copied()
    }

    fn get_fn_sig_from_ty(&self, ty: TyId) -> (Vec<TyId>, TyId) {
        let root = get_type_root(&*self.type_checker_result.type_pool, ty);
        match &self.type_checker_result.type_pool[root].kind {
            TypeNodeKind::Fun { param_tys, return_ty } => (param_tys.clone(), *return_ty),
            _ => unreachable!(),
        }
    }

    fn lower_decls(&mut self) {
        let decls = self.hir.hir_decl_pool.clone();
        for (decl_id, decl) in decls.iter().enumerate() {
            match &decl.kind {
                HirDeclKind::External { sym_name, .. } => {
                    let scheme = self.type_checker_result.decl_type_map.get(&decl_id)
                        .expect("external decl type not found");
                    let (param_tys, return_ty) = self.get_fn_sig_from_ty(scheme.body);
                    self.extern_decls.push(ExternDecl {
                        name: sym_name.clone(),
                        signature: FnSig { params: param_tys, return_ty },
                    });
                }
                HirDeclKind::Global { .. } | HirDeclKind::Const { .. } => {
                    let scheme = self.type_checker_result.decl_type_map.get(&decl_id)
                        .expect("global/const type not found");
                    let ty = scheme.body;

                    let static_id = self.statics.len();
                    self.decl_to_static.insert(decl_id, static_id);

                    self.statics.push(StaticDecl {
                        name: decl.ident.clone(),
                        ty,
                        mutable: matches!(&decl.kind, HirDeclKind::Global { .. }),
                        init: todo!("convert const expr"),
                    });
                }
                HirDeclKind::Fun { .. } => {
                    let mir_fun = self.lower_function(decl_id);
                    self.functions.push(mir_fun);
                }
                HirDeclKind::Struct { fields, .. } => {
                    for (idx, f) in fields.iter().enumerate() {
                        self.struct_field_map.insert((decl_id, f.name.name.clone()), idx);
                    }
                }
                HirDeclKind::ADT { ctors, .. } => {
                    for (tag, ctor) in ctors.iter().enumerate() {
                        self.adt_variant_map.insert((decl_id, ctor.name.name.clone()), tag);
                    }
                }
                _ => {}
            }
        }
    }

    ///
    fn new_local(&mut self, ty: TyId, mutable: bool, name: Option<String>) -> LocalId {
        let fun = self.fun.as_mut().unwrap();
        let id = fun.local_decls.len();
        fun.local_decls.push(LocalDecl {
            ty,
            mutable,
            name,
        });
        id
    }

    ///
    fn new_temp(&mut self, ty: TyId) -> LocalId {
        self.new_local(ty, false, None)
    }

    ///
    fn bind_local(&mut self, sym: SymId, local: LocalId) {
        let fun = self.fun.as_mut().expect("no function being built");
        fun.locals_map.insert(sym, local);
    }

    ///
    fn lookup_place(&self, sym: SymId) -> Option<Place> {
        if let Some(fun) = &self.fun {
            if let Some(&local) = fun.locals_map.get(&sym) {
                return Some(Place::Local(local));
            }
        }
        if let Some(&decl_id) = self.type_checker_result.sym_to_decl.get(&sym) {
            if let Some(&static_id) = self.decl_to_static.get(&decl_id) {
                return Some(Place::Static(static_id));
            }
        }

        None
    }
}

impl MirLowerApi for MirLower {
    fn new(ty_ck_result: TypeCheckerResult, hir_crate: HirCrate) -> Self {
        MirLower {
            crate_name: hir_crate.name.clone(),
            functions: vec![],
            extern_decls: vec![],
            statics: vec![],
            blocks: vec![],
            type_checker_result: ty_ck_result,
            hir: hir_crate,
            fun: None,
            current_block: 0,
            current_stmts: vec![],
            decl_to_static: HashMap::new(),
            struct_field_map: HashMap::new(),
            adt_variant_map: HashMap::new(),
        }
    }

    fn lower(mut self) -> Result<MirCrate, DiagMsg> {
        self.lower_decls();
        Ok(MirCrate {
            name: self.hir.name,
            functions: self.functions,
            extern_decls: self.extern_decls,
            statics: self.statics,
            blocks: self.blocks,
        })
    }
}