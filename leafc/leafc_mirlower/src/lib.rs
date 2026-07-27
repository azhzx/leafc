use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::{HirBinOp, HirCrate, HirDeclId, HirDeclKind, HirExprId, HirExprKind, HirLit, HirMatchArm, HirPattern, HirTypeName, HirUnaryOp};
use leafc_coreapi::mir::{BasicBlock, BasicBlockId, Const, ControlId, ExternDecl, FnSig, FunId, LocalDecl, LocalId, MirBinOp, MirCrate, MirFun, MirStmt, MirStmtKind, MirUnOp, Place, Rvalue, StaticDecl, StaticId, TagId, TerminatorKind};
use leafc_coreapi::mir_lower::MirLowerApi;
use leafc_coreapi::scope::SymId;
use leafc_coreapi::type_checker::TypeCheckerResult;
use leafc_coreapi::type_system::{get_type_root, TyId, TypeNodeKind};
use std::collections::{HashMap, HashSet};
use leafc_coreapi::lang_items::BuiltinType;


#[derive(Clone, Debug)]
struct MatrixRow<'a> {
    patterns: Vec<&'a HirPattern>,
    arm_idx: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum PatternKindKey {
    Literal(HirLit),
    Constructor(SymId),
    Tuple,
    Struct,
    WildcardOrBinding,
}

struct DecisionTreeBuilder<'a> {
    mir: &'a mut MirLower,
    arms: &'a Vec<HirMatchArm>,
    merge_block: BasicBlockId,
    dag_cache: HashMap<String, BasicBlockId>,
    is_result_local: Option<LocalId>,
}

impl<'a> DecisionTreeBuilder<'a> {
    // DAG 缓存暂时禁用
    fn compute_matrix_signature(&self, _matrix: &[MatrixRow], _occurrences: &[Place]) -> String {
        // 暂时返回空字符串，禁用缓存
        String::new()
    }

    fn select_best_column(&self, matrix: &[MatrixRow], cols: usize) -> usize {
        let mut best_col = 0;
        let mut best_score = (1, usize::MAX, usize::MAX);

        for c in 0..cols {
            let first_pat = matrix.first().map(|r| r.patterns[c]);
            let q = matches!(
                first_pat,
                Some(HirPattern::Constructor { .. })
                | Some(HirPattern::Literal(_))
                | Some(HirPattern::Struct { .. })
                | Some(HirPattern::Tuple { .. })
            );

            let mut distinct_kinds = HashSet::new();
            let mut max_arity = 0;

            for row in matrix {
                let pat = row.patterns[c];
                distinct_kinds.insert(Self::get_pattern_kind(pat));
                match pat {
                    HirPattern::Constructor { args, .. } => max_arity = max_arity.max(args.len()),
                    HirPattern::Tuple { elements, .. } => max_arity = max_arity.max(elements.len()),
                    HirPattern::Struct { fields, .. } => max_arity = max_arity.max(fields.len()),
                    _ => {}
                }
            }

            let b = distinct_kinds.len();
            let a = max_arity;
            let score = (if q { 0 } else { 1 }, usize::MAX - b, usize::MAX - a);
            if score < best_score {
                best_score = score;
                best_col = c;
            }
        }
        best_col
    }

    fn expand_patterns<'b>(&self, row: &MatrixRow<'b>) -> Vec<MatrixRow<'b>> {
        if row.patterns.is_empty() {
            return vec![row.clone()];
        }

        let mut current_patterns = row.patterns.clone();
        let head = current_patterns.remove(0);

        match head {
            HirPattern::Or { left, right, .. } => {
                let mut left_row = MatrixRow {
                    patterns: std::iter::once(&**left)
                        .chain(current_patterns.iter().copied())
                        .collect(),
                    arm_idx: row.arm_idx,
                };
                let mut right_row = MatrixRow {
                    patterns: std::iter::once(&**right)
                        .chain(current_patterns.iter().copied())
                        .collect(),
                    arm_idx: row.arm_idx,
                };
                let mut res = self.expand_patterns(&left_row);
                res.extend(self.expand_patterns(&right_row));
                res
            }
            _ => vec![row.clone()],
        }
    }

    fn get_pattern_kind(pattern: &HirPattern) -> PatternKindKey {
        match pattern {
            HirPattern::Literal(lit) => PatternKindKey::Literal(lit.clone()),
            HirPattern::Constructor { type_name, .. } => match type_name {
                HirTypeName::Named { path, .. } => PatternKindKey::Constructor(path.sym_id),
                _ => PatternKindKey::Constructor(0),
            },
            HirPattern::Tuple { .. } => PatternKindKey::Tuple,
            HirPattern::Struct { .. } => PatternKindKey::Struct,
            HirPattern::Alias { pattern, .. } => Self::get_pattern_kind(pattern),
            HirPattern::Or { left, .. } => Self::get_pattern_kind(left),
            _ => PatternKindKey::WildcardOrBinding,
        }
    }

    fn generate_sub_occurrences(
        &self,
        base: &Place,
        key: &PatternKindKey,
        pattern: &HirPattern,
    ) -> Vec<Place> {
        match pattern {
            HirPattern::Literal(_)
            | HirPattern::Wildcard
            | HirPattern::Binding(_)
            | HirPattern::Rest => vec![],
            HirPattern::Constructor { args, .. } => {
                if args.is_empty() {
                    vec![]
                } else {
                    let enum_place = match key {
                        PatternKindKey::Constructor(sym_id) => {
                            let discr_ty = self.mir.place_ty(base);
                            let root_ty = get_type_root(&self.mir.type_checker_result.type_pool, discr_ty);
                            if let TypeNodeKind::ADT { decl_id, .. } = &self.mir.type_checker_result.type_pool[root_ty].kind {
                                let tag = *self.mir.adt_variant_map.get(&(*decl_id, *sym_id)).unwrap();
                                Place::EnumItem { place: Box::new(base.clone()), variant: tag }
                            } else { unreachable!() }
                        }
                        _ => unreachable!(),
                    };
                    vec![enum_place]
                }
            }
            HirPattern::Tuple { elements, .. } => (0..elements.len())
                .map(|i| Place::Field {
                    base: Box::new(base.clone()),
                    field: i,
                })
                .collect(),
            HirPattern::Struct { fields, .. } => {
                let base_ty = self.mir.place_ty(base);
                let root_ty = get_type_root(&self.mir.type_checker_result.type_pool, base_ty);
                let decl_id = match &self.mir.type_checker_result.type_pool[root_ty].kind {
                    TypeNodeKind::Struct { decl_id, .. } => *decl_id,
                    _ => panic!("Expected struct type for struct pattern"),
                };
                fields
                    .iter()
                    .map(|sf| {
                        let idx = *self
                            .mir
                            .struct_field_map
                            .get(&(decl_id, sf.field_name.name.clone()))
                            .expect(&format!(
                                "Field {} not found in struct",
                                sf.field_name.name
                            ));
                        Place::Field {
                            base: Box::new(base.clone()),
                            field: idx,
                        }
                    })
                    .collect()
            }
            HirPattern::Alias { pattern, .. } => {
                self.generate_sub_occurrences(base, key, pattern)
            }
            HirPattern::Or { .. } => unreachable!("Or should have been expanded"),
        }
    }

    fn specialize_pattern<'b>(
        &self,
        pattern: &'b HirPattern,
        key: &PatternKindKey,
    ) -> Option<Vec<&'b HirPattern>> {
        match (pattern, key) {
            (HirPattern::Literal(l1), PatternKindKey::Literal(l2)) if l1 == l2 => Some(vec![]),
            (HirPattern::Constructor { type_name, args, .. }, PatternKindKey::Constructor(sym_id)) => {
                let pat_sym = match type_name {
                    HirTypeName::Named { path, .. } => path.sym_id,
                    _ => return None,
                };
                if pat_sym == *sym_id {
                    Some(args.iter().collect())
                } else {
                    None
                }
            }
            (HirPattern::Tuple { elements, .. }, PatternKindKey::Tuple) => {
                Some(elements.iter().collect())
            }
            (HirPattern::Struct { fields, .. }, PatternKindKey::Struct) => {
                Some(fields.iter().map(|f| &f.pattern).collect())
            }
            (HirPattern::Alias { pattern, .. }, _) => self.specialize_pattern(pattern, key),
            (HirPattern::Wildcard | HirPattern::Binding(_) | HirPattern::Rest, _) => Some(vec![]),
            _ => None,
        }
    }

    fn bind_pattern_variables(&mut self, pattern: &HirPattern, place: &Place) {
        match pattern {
            HirPattern::Binding(name) => {
                let ty = self.mir.place_ty(place);
                let local = self.mir.new_local(ty, true, Some(name.name.clone()));
                self.mir.bind_local(name.sym_id, local);
                self.mir.push_stmt(MirStmtKind::Let {
                    local,
                    rvalue: Rvalue::Copy(place.clone()),
                });
            }
            HirPattern::Alias { pattern, name, .. } => {
                self.bind_pattern_variables(pattern, place);
                let ty = self.mir.place_ty(place);
                let local = self.mir.new_local(ty, true, Some(name.name.clone()));
                self.mir.bind_local(name.sym_id, local);
                self.mir.push_stmt(MirStmtKind::Let {
                    local,
                    rvalue: Rvalue::Copy(place.clone()),
                });
            }
            HirPattern::Tuple { elements, .. } => {
                for (idx, elem) in elements.iter().enumerate() {
                    let field_place = Place::Field {
                        base: Box::new(place.clone()),
                        field: idx,
                    };
                    self.bind_pattern_variables(elem, &field_place);
                }
            }
            HirPattern::Constructor { args, .. } => {
                if let Some(arg) = args.first() {
                    self.bind_pattern_variables(arg, place);
                }
            }
            HirPattern::Struct { fields, .. } => {
                let base_ty = self.mir.place_ty(place);
                let root_ty = get_type_root(&self.mir.type_checker_result.type_pool, base_ty);
                let decl_id = match &self.mir.type_checker_result.type_pool[root_ty].kind {
                    TypeNodeKind::Struct { decl_id, .. } => *decl_id,
                    _ => panic!("Expected struct type"),
                };
                for sf in fields {
                    if let Some(&field_idx) = self
                        .mir
                        .struct_field_map
                        .get(&(decl_id, sf.field_name.name.clone()))
                    {
                        let field_place = Place::Field {
                            base: Box::new(place.clone()),
                            field: field_idx,
                        };
                        self.bind_pattern_variables(&sf.pattern, &field_place);
                    }
                }
            }
            HirPattern::Wildcard | HirPattern::Literal(_) | HirPattern::Rest => {}
            HirPattern::Or { .. } => {}
        }
    }

    fn is_all_wildcards(patterns: &[&HirPattern]) -> bool {
        patterns.iter().all(|p| match p {
            HirPattern::Wildcard | HirPattern::Binding(_) | HirPattern::Rest => true,
            HirPattern::Alias { pattern, .. } => Self::is_all_wildcards(&[pattern]),
            _ => false,
        })
    }

    fn is_wildcard_or_binding(pat: &HirPattern) -> bool {
        match pat {
            HirPattern::Wildcard | HirPattern::Binding(_) | HirPattern::Rest => true,
            HirPattern::Alias { pattern, .. } => Self::is_wildcard_or_binding(pattern),
            _ => false,
        }
    }

    fn compile_matrix(
        &mut self,
        matrix: Vec<MatrixRow>,
        occurrences: Vec<Place>,
    ) -> BasicBlockId {
        let saved_block = self.mir.current_block;
        let saved_stmts = std::mem::take(&mut self.mir.current_stmts);

        if matrix.is_empty() {
            let fail_block = self.mir.new_block();
            self.mir.start_block(fail_block);
            self.mir.set_terminator(TerminatorKind::Unreachable);
            self.mir.current_block = saved_block;
            self.mir.current_stmts = saved_stmts;
            return fail_block;
        }

        let mut expanded_matrix = Vec::new();
        for row in matrix {
            expanded_matrix.extend(self.expand_patterns(&row));
        }

        let current_block = self.mir.new_block();

        let first_row = &expanded_matrix[0];
        let is_all_wildcards = Self::is_all_wildcards(&first_row.patterns);

        if is_all_wildcards {
            self.mir.start_block(current_block);
            for (pat, occ) in first_row.patterns.iter().zip(occurrences.iter()) {
                self.bind_pattern_variables(pat, occ);
            }

            // 如果是 is expr 直接返回 true
            if let Some(result_local) = self.is_result_local {
                self.mir.push_stmt(MirStmtKind::Let {
                    local: result_local,
                    rvalue: Rvalue::Constant(Const::Bool(true)),
                });
                self.mir.set_terminator(TerminatorKind::Goto {
                    target: self.merge_block,
                    block_args: vec![Rvalue::Copy(Place::Local(result_local))],
                });
                self.mir.current_block = saved_block;
                self.mir.current_stmts = saved_stmts;
                return current_block;
            }

            let arm = &self.arms[first_row.arm_idx];

            for (pat, occ) in first_row.patterns.iter().zip(occurrences.iter()) {
                self.bind_pattern_variables(pat, occ);
            }

            // 处理 guard
            if let Some(guard_expr) = arm.guard {
                let guard_place = self.mir.compile_expr(guard_expr).unwrap();
                let next_eval_block = self.mir.new_block();
                let fail_eval_block = self.mir.new_block();

                self.mir.set_terminator(TerminatorKind::SwitchInt {
                    discriminant: Rvalue::Copy(guard_place),
                    targets: vec![(Const::Bool(true), next_eval_block)],
                    default: fail_eval_block,
                });

                // guard fail
                let remaining_matrix: Vec<MatrixRow> = expanded_matrix
                    .iter()
                    .filter(|r| r.arm_idx != first_row.arm_idx)
                    .cloned()
                    .collect();
                let fallback_block = self.compile_matrix(remaining_matrix, occurrences);
                self.mir.start_block(fail_eval_block);
                self.mir.set_terminator(TerminatorKind::Goto {
                    target: fallback_block,
                    block_args: vec![],
                });

                self.mir.start_block(next_eval_block);
            }

            let body_block = self.mir.switch_to_new_block();
            let body_place = self.mir.compile_expr(arm.body);

            if self.mir.current_block == body_block &&
                self.mir.blocks[body_block].terminator == TerminatorKind::Unreachable
            {
                let ret_val = body_place.map(Rvalue::Move).unwrap_or(Rvalue::Tuple(vec![]));
                self.mir.set_terminator(TerminatorKind::Goto {
                    target: self.merge_block,
                    block_args: vec![ret_val],
                });
            }

            self.mir.current_block = saved_block;
            self.mir.current_stmts = saved_stmts;
            return current_block;
        }

        let col_idx = self.select_best_column(&expanded_matrix, occurrences.len());
        let place_to_test = occurrences[col_idx].clone();
        let discr_ty = self.mir.place_ty(&place_to_test);

        let mut constructors_seen = Vec::new();
        for row in &expanded_matrix {
            let k = Self::get_pattern_kind(row.patterns[col_idx]);
            if !constructors_seen.contains(&k) {
                constructors_seen.push(k);
            }
        }

        let has_testable = constructors_seen.iter().any(|k| {
            matches!(
            k,
            PatternKindKey::Literal(_) | PatternKindKey::Constructor(_)
        )
        });

        self.mir.start_block(current_block);

        // 生成默认矩阵
        let mut default_matrix = Vec::new();
        for row in &expanded_matrix {
            let pat = row.patterns[col_idx];
            if Self::is_wildcard_or_binding(pat)

        {
                let mut new_patterns = row.patterns.clone();
                new_patterns.remove(col_idx);
                default_matrix.push(MatrixRow {
                    patterns: new_patterns,
                    arm_idx: row.arm_idx,
                });
            }
        }

        let mut default_occurrences = occurrences.clone();
        default_occurrences.remove(col_idx);
        let default_block = if default_matrix.is_empty() {
            let fail_b = self.mir.new_block();
            self.mir.start_block(fail_b);
            self.mir.set_terminator(TerminatorKind::Unreachable);
            fail_b
        } else {
            self.compile_matrix(default_matrix, default_occurrences)
        };

        if has_testable {
            // SwitchInt
            let mut switch_targets = Vec::new();

            for ctor_key in &constructors_seen {
                match ctor_key {
                    PatternKindKey::Literal(lit) => {
                        let mir_const = match lit {
                            HirLit::Decimal(s) => {
                                Const::Float64(s.parse().unwrap_or(0.0_f64).to_bits())
                            }
                            HirLit::Int(s) => Const::Int32(s.parse().unwrap_or(0)),
                            HirLit::Str(s) => Const::Str(s.clone()),
                            HirLit::Bool(b) => Const::Bool(*b),
                        };

                        let sub_occurrences = self.generate_sub_occurrences(
                            &place_to_test,
                            ctor_key,
                            expanded_matrix
                                .iter()
                                .find_map(|r| {
                                    self.specialize_pattern(r.patterns[col_idx], ctor_key)
                                        .map(|_| r.patterns[col_idx])
                                })
                                .unwrap(),
                        );
                        let mut spec_occurrences = occurrences[..col_idx].to_vec();
                        spec_occurrences.extend(sub_occurrences);
                        spec_occurrences.extend_from_slice(&occurrences[col_idx + 1..]);

                        let mut spec_matrix = Vec::new();
                        for row in &expanded_matrix {
                            if let Some(sub_pats) =
                                self.specialize_pattern(row.patterns[col_idx], ctor_key)
                            {
                                let mut new_patterns = row.patterns.clone();
                                new_patterns.remove(col_idx);
                                spec_matrix.push(MatrixRow {
                                    patterns: sub_pats.into_iter().chain(new_patterns).collect(),
                                    arm_idx: row.arm_idx,
                                });
                            }
                        }

                        let target_block = self.compile_matrix(spec_matrix, spec_occurrences);
                        switch_targets.push((mir_const, target_block));
                    }
                    PatternKindKey::Constructor(sym_id) => {
                        let root_ty =
                            get_type_root(&self.mir.type_checker_result.type_pool, discr_ty);
                        let decl_id = match &self.mir.type_checker_result.type_pool[root_ty].kind {
                            TypeNodeKind::ADT { decl_id, .. } => *decl_id,
                            _ => panic!("Constructor pattern on non-ADT type"),
                        };
                        let tag = *self
                            .mir
                            .adt_variant_map
                            .get(&(decl_id, *sym_id))
                            .unwrap_or_else(|| panic!("Variant {:?} not found", sym_id));
                        let const_val = Const::Int32(tag as i32);

                        let sub_occurrences = self.generate_sub_occurrences(
                            &place_to_test,
                            ctor_key,
                            expanded_matrix
                                .iter()
                                .find_map(|r| {
                                    self.specialize_pattern(r.patterns[col_idx], ctor_key)
                                        .map(|_| r.patterns[col_idx])
                                })
                                .unwrap(),
                        );
                        let mut spec_occurrences = occurrences[..col_idx].to_vec();
                        spec_occurrences.extend(sub_occurrences);
                        spec_occurrences.extend_from_slice(&occurrences[col_idx + 1..]);

                        let mut spec_matrix = Vec::new();
                        for row in &expanded_matrix {
                            if let Some(sub_pats) =
                                self.specialize_pattern(row.patterns[col_idx], ctor_key)
                            {
                                let mut new_patterns = row.patterns.clone();
                                new_patterns.remove(col_idx);
                                spec_matrix.push(MatrixRow {
                                    patterns: sub_pats.into_iter().chain(new_patterns).collect(),
                                    arm_idx: row.arm_idx,
                                });
                            }
                        }

                        let target_block = self.compile_matrix(spec_matrix, spec_occurrences);
                        switch_targets.push((const_val, target_block));
                    }
                    _ => {}
                }
            }

            self.mir.set_terminator(TerminatorKind::SwitchInt {
                discriminant: Rvalue::Copy(place_to_test),
                targets: switch_targets,
                default: default_block,
            });
        } else {
            // 不可反驳列
            let chosen_key = constructors_seen
                .iter()
                .find(|k| !matches!(k, PatternKindKey::WildcardOrBinding))
                .unwrap_or(&PatternKindKey::WildcardOrBinding);

            let sub_occurrences = if let Some(first_pat) = expanded_matrix.iter().find_map(|r| {
                self.specialize_pattern(r.patterns[col_idx], chosen_key)
                    .map(|_| r.patterns[col_idx])
            }) {
                self.generate_sub_occurrences(&place_to_test, chosen_key, first_pat)
            } else {
                vec![]
            };

            let mut spec_occurrences = occurrences[..col_idx].to_vec();
            spec_occurrences.extend(sub_occurrences);
            spec_occurrences.extend_from_slice(&occurrences[col_idx + 1..]);

            let mut spec_matrix = Vec::new();
            for row in &expanded_matrix {
                if let Some(sub_pats) =
                    self.specialize_pattern(row.patterns[col_idx], chosen_key)
                {
                    let mut new_patterns = row.patterns.clone();
                    new_patterns.remove(col_idx);
                    spec_matrix.push(MatrixRow {
                        patterns: sub_pats.into_iter().chain(new_patterns).collect(),
                        arm_idx: row.arm_idx,
                    });
                }
            }

            let target_block = self.compile_matrix(spec_matrix, spec_occurrences);
            self.mir.set_terminator(TerminatorKind::Goto {
                target: target_block,
                block_args: vec![],
            });
        }

        self.mir.current_block = saved_block;
        self.mir.current_stmts = saved_stmts;
        current_block
    }
}

struct FnBuilder {
    pub name: String,
    pub locals_map: HashMap<SymId, LocalId>,
    pub generic_params: Vec<TyId>,
    pub signature: FnSig,
    pub local_decls: Vec<LocalDecl>,
    pub blocks: Vec<BasicBlockId>,
    pub return_local: LocalId,
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
    adt_variant_map: HashMap<(HirDeclId, SymId), TagId>,
    decl_to_fun: HashMap<HirDeclId, FunId>,

    control_map: HashMap<SymId, ControlId>,
    resume_target: Option<BasicBlockId>,

    bool_ty: TyId,
    uint8_ty: TyId,
    unit_ty: TyId
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


    fn build_call_by_ptr(&mut self, callee: HirExprId, args: Vec<Rvalue>, dest: LocalId, target: BasicBlockId) -> TerminatorKind {
        let func_place = self.compile_expr(callee).unwrap();
        let func_ty = self.expr_ty(callee);
        TerminatorKind::CallByPtr {
            func: Rvalue::Move(func_place),
            args,
            dest: Place::Local(dest),
            target: Some(target),
        }
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
    fn new_mutable_temp(&mut self, ty: TyId) -> LocalId {
        self.new_local(ty, true, None)
    }

    ///
    fn new_immutable_temp(&mut self, ty: TyId) -> LocalId {
        self.new_local(ty, false, None)
    }

    ///
    fn bind_local(&mut self, sym: SymId, local: LocalId) {
        let fun = self.fun.as_mut().expect("no function being built");
        fun.locals_map.insert(sym, local);
    }

    ///
    fn resolve_constructor_tag(&self, type_name: &HirTypeName, ty: TyId) -> Option<TagId> {
        let root_ty = get_type_root(&self.type_checker_result.type_pool, ty);
        let decl_id = match &self.type_checker_result.type_pool[root_ty].kind {
            TypeNodeKind::ADT { decl_id, .. } => *decl_id,
            _ => return None,
        };

        match type_name {
            HirTypeName::Named { path, .. } => {
                self.adt_variant_map.get(&(decl_id, path.sym_id)).copied()
            }
            _ => None,
        }
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

    fn get_constructor_field_ty(&self, _decl_id: HirDeclId, _tag: TagId) -> TyId {
        self.uint8_ty
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
                    let fun_id = self.functions.len();
                    self.decl_to_fun.insert(decl_id, fun_id);
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
                        self.adt_variant_map.insert((decl_id, ctor.name.sym_id), tag);
                    }
                }
                HirDeclKind::Effect { controls } => {
                    for (name, _, _) in controls {
                        let ctrl_id = self.control_map.len() as ControlId;
                        self.control_map.insert(name.sym_id, ctrl_id);
                    }
                }
                _ => {}
            }
        }
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

        let generic_params = ty_scheme.quantified.clone();

        let mut fun = FnBuilder {
            name: decl.ident.clone(),
            locals_map: HashMap::new(),
            generic_params,
            signature: FnSig { params: param_tys.clone(), return_ty },
            local_decls: vec![],
            blocks: vec![],
            return_local: 0,
        };

        self.fun = Some(fun);

        let ret_local = self.new_local(return_ty, true, Some("return".to_string()));
        self.fun.as_mut().unwrap().return_local = ret_local;

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
        let return_ty = self.fun.as_ref().unwrap().signature.return_ty;
        let is_never = self.type_checker_result.type_pool[return_ty].kind == TypeNodeKind::Never;

        if need_terminator && !is_never {
            if let Some(place) = last_place {
                let ret_id = self.fun.as_ref().unwrap().return_local;
                let ret_ty = self.fun.as_ref().unwrap().signature.return_ty;
                self.push_stmt(MirStmtKind::Store {
                    place: Place::Local(ret_id),
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

    pub fn place_ty(&self, place: &Place) -> TyId {
        match place {
            Place::Local(id) => self.fun.as_ref().unwrap().local_decls[*id].ty,
            Place::Field { base, field } => {
                let base_ty = self.place_ty(base);
                let root = get_type_root(&self.type_checker_result.type_pool, base_ty);
                match &self.type_checker_result.type_pool[root].kind {
                    TypeNodeKind::Struct { field_tys, .. } => field_tys[*field],
                    TypeNodeKind::Tuple(elements) => elements[*field],
                    _ => unreachable!("Field on non-struct/tuple"),
                }
            }
            Place::EnumItem { place, variant } => {
                let adt_ty = self.place_ty(place);
                let root = get_type_root(&self.type_checker_result.type_pool, adt_ty);
                if let TypeNodeKind::ADT { variants, .. } = &self.type_checker_result.type_pool[root].kind {
                    variants[*variant].unwrap_or(self.unit_ty)
                } else {
                    unreachable!("EnumItem on non-ADT")
                }
            }
            _ => unreachable!(),
        }
    }

    fn compile_expr(&mut self, expr_id: HirExprId) -> Option<Place> {
        let expr = self.hir.hir_expr_pool[expr_id].clone();
        match &expr.kind {
            HirExprKind::Lit(lit) => {
                let mir_const = match lit {
                    HirLit::Decimal(s) => {
                        let val: f64 = s.parse().unwrap_or(0.0);
                        Const::Float64(val.to_bits())
                    }
                    HirLit::Int(s) => Const::Int32(s.parse().unwrap_or(0)),
                    HirLit::Str(s) => Const::Str(s.clone()),
                    HirLit::Bool(b) => Const::Bool(*b),
                };
                let ty = self.expr_ty(expr_id);
                let temp = self.new_mutable_temp(ty);
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
                let temp = self.new_mutable_temp(ty);
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
                let temp = self.new_mutable_temp(ty);
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
                        let ret_id = self.fun.as_ref().unwrap().return_local;
                        let ret_ty = self.fun.as_ref().unwrap().signature.return_ty;
                        self.push_stmt(MirStmtKind::Store {
                            place: Place::Local(ret_id),
                            rvalue: Rvalue::Move(place),
                        });
                    }
                }
                self.set_terminator(TerminatorKind::Return);
                let block = self.new_block();
                self.start_block(block);
                None
            }

            HirExprKind::Call { callee, args }
            | HirExprKind::UnsafeExternalCall { callee, args } => {
                let mut mir_args = Vec::new();
                for arg_expr in args {
                    if let Some(arg_place) = self.compile_expr(*arg_expr) {
                        let arg_ty = self.expr_ty(*arg_expr);
                        mir_args.push(Rvalue::Move(arg_place)); // 后续可改为 self.move_or_copy(arg_place, arg_ty)
                    }
                }

                let ty = self.expr_ty(expr_id);
                let result_temp = self.new_mutable_temp(ty);
                let next_block = self.new_block();

                let callee_expr = &self.hir.hir_expr_pool[*callee];
                let terminator = if let HirExprKind::Ident(name) = &callee_expr.kind {
                    if let Some(&decl_id) = self.type_checker_result.sym_to_decl.get(&name.sym_id) {
                        if let Some(&fun_id) = self.decl_to_fun.get(&decl_id) {
                            TerminatorKind::Call {
                                func: fun_id,
                                args: mir_args,
                                dest: Place::Local(result_temp),
                                target: Some(next_block),
                            }
                        } else {
                            self.build_call_by_ptr(*callee, mir_args, result_temp, next_block)
                        }
                    } else {
                        self.build_call_by_ptr(*callee, mir_args, result_temp, next_block)
                    }
                } else {
                    self.build_call_by_ptr(*callee, mir_args, result_temp, next_block)
                };

                self.set_terminator(terminator);
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
                let temp = self.new_mutable_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::Tuple(mir_elements),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::Move { target } => {
                let place = self.compile_expr(*target)?;
                let ty = self.expr_ty(expr_id);
                let temp = self.new_mutable_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::Move(place),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::Copy { target } => {
                let place = self.compile_expr(*target)?;
                let ty = self.expr_ty(expr_id);
                let temp = self.new_mutable_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::Copy(place),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::Ref { target } => {
                let place = self.compile_expr(*target)?;
                let ty = self.expr_ty(expr_id);
                let temp = self.new_mutable_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::TempRef(place),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::MutRef { target } => {
                let place = self.compile_expr(*target)?;
                let ty = self.expr_ty(expr_id);
                let temp = self.new_mutable_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::TempRefMut(place),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::Share { target } => {
                let place = self.compile_expr(*target)?;
                let ty = self.expr_ty(expr_id);
                let temp = self.new_mutable_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::GcObjectRef(Box::new(Rvalue::Move(place))),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::TypeCast { expr: cast_expr, type_ann: _ } => {
                let place = self.compile_expr(*cast_expr)?;
                let dest_ty = self.expr_ty(expr_id);
                let temp = self.new_mutable_temp(dest_ty);
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
                let temp = self.new_mutable_temp(ty);
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

                let tag = *self.adt_variant_map.get(&(decl_id, variant_name.sym_id)).unwrap();

                let inner_rvalue = self.compile_expr(*target)
                    .map(|p| Box::new(Rvalue::Move(p)))
                    .unwrap_or_else(|| Box::new(Rvalue::Tuple(vec![])));

                let temp = self.new_mutable_temp(ty);
                self.push_stmt(MirStmtKind::Let {
                    local: temp,
                    rvalue: Rvalue::Variant(tag, inner_rvalue),
                });
                Some(Place::Local(temp))
            }

            HirExprKind::If { cond, then, elifs, else_opt } => {
                let result_ty = self.expr_ty(expr_id);
                let merge_block = self.new_block(); // 合并点，稍后设置 block_params

                let cond_place = self.compile_expr(*cond).unwrap();
                let then_block = self.new_block();
                let else_block = self.new_block();

                self.set_terminator(TerminatorKind::SwitchInt {
                    discriminant: Rvalue::Copy(cond_place),
                    targets: vec![(Const::Bool(true), then_block)],
                    default: else_block,
                });

                self.start_block(then_block);
                let then_place = self.compile_expr(*then);
                let then_value = if let Some(place) = then_place {
                    Rvalue::Move(place)
                } else {
                    Rvalue::Tuple(vec![])
                };
                self.set_terminator(TerminatorKind::Goto {
                    target: merge_block,
                    block_args: vec![then_value],
                });

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
                    let elif_place = self.compile_expr(*elif_body);
                    let elif_value = if let Some(place) = elif_place {
                        Rvalue::Move(place)
                    } else {
                        Rvalue::Tuple(vec![])
                    };
                    self.set_terminator(TerminatorKind::Goto {
                        target: merge_block,
                        block_args: vec![elif_value],
                    });

                    current_else = next_else;
                }

                self.start_block(current_else);
                let else_value = if let Some(else_expr) = else_opt {
                    if let Some(place) = self.compile_expr(*else_expr) {
                        Rvalue::Move(place)
                    } else {
                        Rvalue::Tuple(vec![])
                    }
                } else {
                    Rvalue::Tuple(vec![])
                };
                self.set_terminator(TerminatorKind::Goto {
                    target: merge_block,
                    block_args: vec![else_value],
                });

                let result_local = self.new_mutable_temp(result_ty);
                self.blocks[merge_block].block_params = vec![result_local];
                self.start_block(merge_block);

                Some(Place::Local(result_local))
            }

            HirExprKind::Ellipsis => {
                todo!()
            }

            HirExprKind::Match { scrutinee, arms } => {
                let result_ty = self.expr_ty(expr_id);
                let merge_block = self.new_block();
                let result_local = self.new_mutable_temp(result_ty);
                self.blocks[merge_block].block_params = vec![result_local];

                let scrutinee_place = self.compile_expr(*scrutinee).unwrap();

                let mut initial_matrix = Vec::new();
                for (idx, arm) in arms.iter().enumerate() {
                    initial_matrix.push(MatrixRow {
                        patterns: vec![&arm.pattern],
                        arm_idx: idx,
                    });
                }

                let initial_occurrences = vec![scrutinee_place];

                let mut builder = DecisionTreeBuilder {
                    mir: self,
                    arms: &arms,
                    merge_block,
                    dag_cache: HashMap::new(),
                    is_result_local: None,
                };

                let start_block = builder.compile_matrix(initial_matrix, initial_occurrences);

                // 将进入 Match 语句的执行流导向决策树 DAG 的根节点
                self.set_terminator(TerminatorKind::Goto {
                    target: start_block,
                    block_args: vec![],
                });

                self.start_block(merge_block);
                Some(Place::Local(result_local))
            }

            HirExprKind::Is { expr, pattern } => {
                let result_ty = self.expr_ty(expr_id);
                let merge_block = self.new_block();
                let result_local = self.new_local(result_ty, true, Some("is_result".into()));
                self.blocks[merge_block].block_params = vec![result_local];

                let scrutinee_place = self.compile_expr(*expr).unwrap();

                let dummy_arm = HirMatchArm {
                    pattern: pattern.clone(),
                    guard: None,
                    body: 0,
                    span: self.hir.hir_expr_pool[expr_id].span.clone(),
                };
                let arms = vec![dummy_arm];

                let mut builder = DecisionTreeBuilder {
                    mir: self,
                    arms: &arms,
                    merge_block,
                    dag_cache: HashMap::new(),
                    is_result_local: Some(result_local),
                };

                let initial_matrix = vec![MatrixRow {
                    patterns: vec![&pattern],
                    arm_idx: 0,
                }];
                let initial_occurrences = vec![scrutinee_place];

                let start_block = builder.compile_matrix(initial_matrix, initial_occurrences);

                self.set_terminator(TerminatorKind::Goto {
                    target: start_block,
                    block_args: vec![],
                });

                self.start_block(merge_block);
                Some(Place::Local(result_local))
            }

            HirExprKind::With { handler: _, clauses } => {
                todo!("Lowering with expressions for Algebraic Effects")
            }

            HirExprKind::Raise { control_name, args } => {
                let control_id = *self.control_map.get(&control_name.sym_id)
                    .expect("Unknown effect control name");
                let mut mir_args = Vec::new();
                for arg in args {
                    if let Some(place) = self.compile_expr(*arg) {
                        mir_args.push(Rvalue::Move(place));
                    }
                }
                self.set_terminator(TerminatorKind::Raise {
                    control_name: control_id,
                    args: mir_args,
                });
                let block = self.new_block();
                self.start_block(block);
                None
            }

            HirExprKind::Resume { expr } => {
                let target = self.resume_target
                    .expect("Resume used outside a With handler");
                let place = self.compile_expr(*expr);
                let resume_value = match place {
                    Some(p) => Rvalue::Move(p),
                    None => Rvalue::Tuple(vec![]),
                };

                let expr_ty = self.expr_ty(*expr);
                let temp = self.new_mutable_temp(expr_ty);
                self.set_terminator(TerminatorKind::Resume {
                    place: Place::Local(temp),
                    target,
                });
                let block = self.new_block();
                self.start_block(block);
                None
            }
        }
    }

}

impl MirLowerApi for MirLower {
    fn new(ty_ck_result: TypeCheckerResult, hir_crate: HirCrate) -> Self {
        let bool_ty = ty_ck_result.type_pool.iter().position(|n| {
            matches!(n.kind, TypeNodeKind::Builtin(BuiltinType::Bool))
        }).unwrap();

        let uint8_ty = ty_ck_result.type_pool.iter().position(|n| {
            matches!(n.kind, TypeNodeKind::Builtin(BuiltinType::U8))
        }).unwrap();
        let unit_ty = ty_ck_result.type_pool.iter().position(|n| {
            matches!(&n.kind, TypeNodeKind::Tuple(elems) if elems.is_empty())
        }).unwrap();
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
            decl_to_fun: HashMap::new(),
            control_map: HashMap::new(),
            resume_target: None,
            bool_ty,
            uint8_ty,
            unit_ty,
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