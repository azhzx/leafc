use std::cell::RefCell;
use leaf_coreapi::diagnose::{make_error, CompileTimeErrorKind, DiagCollector, ErrorKind, LocalizedMessage, MirLowerErrorKind, MsgKind};
use leaf_coreapi::hir::{HirBinOp, HirCatchParam, HirCrate, HirDeclKind, HirExprKind, HirLit, HirMatchArm, HirName, HirPattern, HirTypeName, HirUnaryOp};
use leaf_coreapi::mir::{BasicBlock, MirBasicBlockId, Const, MirControlId, ExternDecl, FnSig, MirCrate, MirFunId, LocalDecl, MirLocalId, MirBinOp, MirFun, MirStmt, MirStmtKind, MirUnOp, Place, Rvalue, StaticDecl, MirStaticId, MirTagId, TerminatorKind};
use leaf_coreapi::id::{SymId, HirDeclId, HirExprId};
use leaf_coreapi::type_ctx::TypeCtx;
use leaf_coreapi::type_ctx::{get_type_root, TyId, TypeNodeKind};
use std::collections::{HashMap, HashSet};
use std::ptr::addr_of_mut;
use leaf_coreapi::lang_items::BuiltinType;
use leaf_coreapi::source::Span;

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
    merge_block: MirBasicBlockId,
    dag_cache: HashMap<String, MirBasicBlockId>,
    is_result_local: Option<MirLocalId>,
    span: Span,
}

impl<'a> DecisionTreeBuilder<'a> {
    fn compute_matrix_signature(&self, _matrix: &[MatrixRow], _occurrences: &[Place]) -> String {
        String::new()
    }

    fn select_best_column(&self, matrix: &[MatrixRow], cols: usize) -> usize {
        if cols == 0 {
            return 0;
        }

        let mut best_col = 0;
        let mut best_score = (1, usize::MAX, usize::MAX);

        for c in 0..cols {
            let first_pat = matrix.first().and_then(|r| r.patterns.get(c));
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
                if let Some(pat) = row.patterns.get(c) {
                    distinct_kinds.insert(Self::get_pattern_kind(pat));
                    match pat {
                        HirPattern::Constructor { args, .. } => max_arity = max_arity.max(args.len()),
                        HirPattern::Tuple { elements, .. } => max_arity = max_arity.max(elements.len()),
                        HirPattern::Struct { fields, .. } => max_arity = max_arity.max(fields.len()),
                        _ => {}
                    }
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
                _ => PatternKindKey::Constructor(SymId(0)),
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
    ) -> Option<Vec<Place>> {
        match pattern {
            HirPattern::Literal(_)
            | HirPattern::Wildcard
            | HirPattern::Binding(_)
            | HirPattern::Rest => Some(vec![]),
            HirPattern::Constructor { args, .. } => {
                if args.is_empty() {
                    Some(vec![])
                } else {
                    let discr_ty = self.mir.place_ty(base, self.span.clone())?;
                    let root_ty = get_type_root(&self.mir.type_checker_result.type_pool, discr_ty);
                    if let TypeNodeKind::ADT { decl_id, .. } = &self.mir.type_checker_result.type_pool[root_ty].kind {
                        let sym_id = match key {
                            PatternKindKey::Constructor(sym) => *sym,
                            _ => {
                                self.mir.emit_mir_err(MirLowerErrorKind::GenericInternal("expected constructor key".into()), self.span.clone());
                                return None;
                            }
                        };
                        let tag_id =
                            self.mir.adt_variant_map.get(&(*decl_id, sym_id)).copied();
                        let tag = self.mir.require(
                            tag_id,
                            format!("variant for sym {} not found", sym_id.0),
                            self.span.clone(),
                        )?;
                        let enum_place = Place::EnumItem {
                            place: Box::new(base.clone()),
                            variant: tag,
                        };
                        Some(vec![enum_place])
                    } else {
                        self.mir.emit_mir_err(MirLowerErrorKind::ExpectedADTType, self.span.clone());
                        None
                    }
                }
            }
            HirPattern::Tuple { elements, .. } => {
                Some((0..elements.len())
                    .map(|i| Place::Field {
                        base: Box::new(base.clone()),
                        field: i,
                    })
                    .collect())
            }
            HirPattern::Struct { fields, .. } => {
                let base_ty = self.mir.place_ty(base, self.span.clone())?;
                let root_ty = get_type_root(&self.mir.type_checker_result.type_pool, base_ty);
                let decl_id = match &self.mir.type_checker_result.type_pool[root_ty].kind {
                    TypeNodeKind::Struct { decl_id, .. } => *decl_id,
                    _ => {
                        self.mir.emit_mir_err(MirLowerErrorKind::ExpectedStructType, self.span.clone());
                        return None;
                    }
                };
                let mut places = Vec::new();
                for sf in fields {
                    let field_id =
                        self.mir.struct_field_map.get(&(decl_id, sf.field_name.name.clone())).copied();
                    let idx = self.mir.require(
                        field_id,
                        format!("field {} not found in struct", sf.field_name.name),
                        self.span.clone(),
                    )?;
                    places.push(Place::Field {
                        base: Box::new(base.clone()),
                        field: idx,
                    });
                }
                Some(places)
            }
            HirPattern::Alias { pattern, .. } => {
                self.generate_sub_occurrences(base, key, pattern)
            }
            HirPattern::Or { .. } => {
                self.mir.emit_mir_err(MirLowerErrorKind::OrPatternNotExpanded, self.span.clone());
                None
            }
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
            (HirPattern::Wildcard | HirPattern::Binding(_) | HirPattern::Rest, PatternKindKey::Literal(_)) => Some(vec![]),
            (HirPattern::Wildcard | HirPattern::Binding(_) | HirPattern::Rest, _) => {
                Some(vec![pattern])
            }
            _ => None,
        }
    }

    fn bind_pattern_variables(&mut self, pattern: &HirPattern, place: &Place) -> Option<()> {
        match pattern {
            HirPattern::Binding(name) => {
                let ty = self.mir.place_ty(place, self.span.clone())?;
                let local = if let Some(&existing) = self.mir.fun.as_ref().unwrap().locals_map.get(&name.sym_id) {
                    existing
                } else {
                    let new_local = self.mir.new_local(ty, true, Some(name.name.clone()), self.span.clone());
                    self.mir.bind_local(name.sym_id, new_local);
                    new_local
                };
                self.mir.push_stmt(
                    MirStmtKind::Let {
                        local,
                        rvalue: Rvalue::Copy(place.clone()),
                    },
                    self.span.clone(),
                );
            }
            HirPattern::Alias { pattern, name, .. } => {
                self.bind_pattern_variables(pattern, place)?;
                let ty = self.mir.place_ty(place, self.span.clone())?;
                let local = if let Some(&existing) = self.mir.fun.as_ref().unwrap().locals_map.get(&name.sym_id) {
                    existing
                } else {
                    let new_local = self.mir.new_local(ty, true, Some(name.name.clone()), self.span.clone());
                    self.mir.bind_local(name.sym_id, new_local);
                    new_local
                };
                self.mir.push_stmt(
                    MirStmtKind::Let {
                        local,
                        rvalue: Rvalue::Copy(place.clone()),
                    },
                    self.span.clone(),
                );
            }
            HirPattern::Tuple { elements, .. } => {
                for (idx, elem) in elements.iter().enumerate() {
                    let field_place = Place::Field {
                        base: Box::new(place.clone()),
                        field: idx,
                    };
                    self.bind_pattern_variables(elem, &field_place)?;
                }
            }
            HirPattern::Constructor { args, .. } => {
                if let Some(arg) = args.first() {
                    self.bind_pattern_variables(arg, place)?;
                }
            }
            HirPattern::Struct { fields, .. } => {
                let base_ty = self.mir.place_ty(place, self.span.clone())?;
                let root_ty = get_type_root(&self.mir.type_checker_result.type_pool, base_ty);
                let decl_id = match &self.mir.type_checker_result.type_pool[root_ty].kind {
                    TypeNodeKind::Struct { decl_id, .. } => *decl_id,
                    _ => {
                        self.mir.emit_mir_err(MirLowerErrorKind::ExpectedStructType, self.span.clone());
                        return None;
                    }
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
                        self.bind_pattern_variables(&sf.pattern, &field_place)?;
                    }
                }
            }
            HirPattern::Wildcard | HirPattern::Literal(_) | HirPattern::Rest => {}
            HirPattern::Or { .. } => {}
        }
        Some(())
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
    ) -> Option<MirBasicBlockId> {
        let saved_block = self.mir.current_block;
        let saved_stmts = std::mem::take(&mut self.mir.current_stmts);

        if matrix.is_empty() {
            let fail_block = self.mir.new_block(self.span.clone());
            self.mir.start_block(fail_block);
            self.mir.set_terminator(TerminatorKind::Unreachable);
            self.mir.current_block = saved_block;
            self.mir.current_stmts = saved_stmts;
            return Some(fail_block);
        }

        let mut expanded_matrix = Vec::new();
        for row in matrix {
            expanded_matrix.extend(self.expand_patterns(&row));
        }

        let current_block = self.mir.new_block(self.span.clone());

        let first_row = &expanded_matrix[0];
        let is_all_wildcards = Self::is_all_wildcards(&first_row.patterns);

        if is_all_wildcards {
            self.mir.start_block(current_block);

            let arm_idx = first_row.arm_idx;
            let arm = &self.arms[arm_idx];

            if let Some(result_local) = self.is_result_local {
                for (pat, occ) in first_row.patterns.iter().zip(occurrences.iter()) {
                    self.bind_pattern_variables(pat, occ)?;
                }
                self.mir.push_stmt(
                    MirStmtKind::Let {
                        local: result_local,
                        rvalue: Rvalue::Constant(Const::Bool(true)),
                    },
                    self.span.clone(),
                );
                self.mir.set_terminator(TerminatorKind::Goto {
                    target: self.merge_block,
                    block_args: vec![Rvalue::Copy(Place::Local(result_local))],
                });
                self.mir.current_block = saved_block;
                self.mir.current_stmts = saved_stmts;
                return Some(current_block);
            }

            for (pat, occ) in first_row.patterns.iter().zip(occurrences.iter()) {
                self.bind_pattern_variables(pat, occ)?;
            }

            if let Some(guard_expr) = arm.guard {
                let value = self.mir.compile_expr(guard_expr)?;
                let guard_place = self.mir.require(
                    value,
                    "guard expression produced no place",
                    self.span.clone(),
                )?;
                let guard_success_block = self.mir.new_block(self.span.clone());
                let guard_fail_block = self.mir.new_block(self.span.clone());

                self.mir.set_terminator(TerminatorKind::SwitchInt {
                    discriminant: Rvalue::Copy(guard_place),
                    targets: vec![(Const::Bool(true), guard_success_block)],
                    default: guard_fail_block,
                });

                // guard 成功
                self.mir.start_block(guard_success_block);
                let body_place = self.mir.compile_expr(arm.body)?;
                let ret_val = body_place
                    .map(Rvalue::Move)
                    .unwrap_or(Rvalue::Tuple(vec![]));
                self.mir.set_terminator(TerminatorKind::Goto {
                    target: self.merge_block,
                    block_args: vec![ret_val],
                });

                // guard 失败
                let remaining_matrix: Vec<MatrixRow> = expanded_matrix
                    .iter()
                    .filter(|r| r.arm_idx != first_row.arm_idx)
                    .cloned()
                    .collect();
                let fallback_block = self.compile_matrix(remaining_matrix, occurrences)?;
                self.mir.start_block(guard_fail_block);
                self.mir.set_terminator(TerminatorKind::Goto {
                    target: fallback_block,
                    block_args: vec![],
                });

                self.mir.current_block = saved_block;
                self.mir.current_stmts = saved_stmts;
                return Some(current_block);
            } else {
                let body_place = self.mir.compile_expr(arm.body)?;
                let ret_val = body_place
                    .map(Rvalue::Move)
                    .unwrap_or(Rvalue::Tuple(vec![]));
                self.mir.set_terminator(TerminatorKind::Goto {
                    target: self.merge_block,
                    block_args: vec![ret_val],
                });

                self.mir.current_block = saved_block;
                self.mir.current_stmts = saved_stmts;
                return Some(current_block);
            }
        }
        // 不可反驳列处理
        let col_idx = self.select_best_column(&expanded_matrix, occurrences.len());
        let place_to_test = occurrences[col_idx].clone();
        let discr_ty = self.mir.place_ty(&place_to_test, self.span.clone())?;

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

        // 默认矩阵
        let mut default_matrix = Vec::new();
        for row in &expanded_matrix {
            let pat = row.patterns[col_idx];
            if Self::is_wildcard_or_binding(pat) {
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
            let fail_b = self.mir.new_block(self.span.clone());
            self.mir.start_block(fail_b);

            if let Some(result_local) = self.is_result_local {
                self.mir.push_stmt(
                    MirStmtKind::Let {
                        local: result_local,
                        rvalue: Rvalue::Constant(Const::Bool(false)),
                    },
                    self.span.clone(),
                );
                self.mir.set_terminator(TerminatorKind::Goto {
                    target: self.merge_block,
                    block_args: vec![Rvalue::Copy(Place::Local(result_local))],
                });
            } else {
                self.mir.set_terminator(TerminatorKind::Unreachable);
            }
            fail_b
        } else {
            self.compile_matrix(default_matrix, default_occurrences)?
        };

        self.mir.start_block(current_block);

        if has_testable {
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

                        for row in &expanded_matrix {
                            let pat = row.patterns[col_idx];
                            if let HirPattern::Alias { .. } = pat {
                                if self.specialize_pattern(pat, ctor_key).is_some() {
                                    self.bind_pattern_variables(pat, &place_to_test)?;
                                }
                            }
                        }

                        let first_matching_pat = self.mir.require(
                            expanded_matrix
                                .iter()
                                .find_map(|r| {
                                    let pat = r.patterns[col_idx];
                                    let inner = match pat {
                                        HirPattern::Alias { pattern, .. } => pattern.as_ref(),
                                        other => other,
                                    };
                                    self.specialize_pattern(inner, ctor_key)
                                        .map(|_| pat)
                                }),
                            "no matching pattern for literal key",
                            self.span.clone(),
                        )?;

                        let inner_first = match first_matching_pat {
                            HirPattern::Alias { pattern, .. } => pattern.as_ref(),
                            other => other,
                        };
                        let sub_occurrences = self.generate_sub_occurrences(
                            &place_to_test,
                            ctor_key,
                            inner_first,
                        )?;

                        let mut spec_occurrences = occurrences[..col_idx].to_vec();
                        spec_occurrences.extend(sub_occurrences);
                        spec_occurrences.extend_from_slice(&occurrences[col_idx + 1..]);

                        let mut spec_matrix = Vec::new();
                        for row in &expanded_matrix {
                            let pat = row.patterns[col_idx];
                            let inner = match pat {
                                HirPattern::Alias { pattern, .. } => pattern.as_ref(),
                                other => other,
                            };
                            if let Some(sub_pats) = self.specialize_pattern(inner, ctor_key) {
                                let mut new_patterns = row.patterns.clone();
                                new_patterns.remove(col_idx);
                                spec_matrix.push(MatrixRow {
                                    patterns: sub_pats.into_iter().chain(new_patterns).collect(),
                                    arm_idx: row.arm_idx,
                                });
                            }
                        }

                        let target_block = self.compile_matrix(spec_matrix, spec_occurrences)?;
                        switch_targets.push((mir_const, target_block));
                    }
                    PatternKindKey::Constructor(sym_id) => {
                        let root_ty =
                            get_type_root(&self.mir.type_checker_result.type_pool, discr_ty);
                        let decl_id = match &self.mir.type_checker_result.type_pool[root_ty].kind {
                            TypeNodeKind::ADT { decl_id, .. } => *decl_id,
                            _ => {
                                self.mir.emit_mir_err(MirLowerErrorKind::ExpectedADTType, self.span.clone());
                                return None;
                            }
                        };
                        let tag = *self.mir.require(
                            self.mir.adt_variant_map.get(&(decl_id, *sym_id)),
                            format!("variant {:?} not found", sym_id),
                            self.span.clone(),
                        )?;
                        let const_val = Const::Int32(tag.0 as i32);

                        for row in &expanded_matrix {
                            let pat = row.patterns[col_idx];
                            if let HirPattern::Alias { .. } = pat {
                                if self.specialize_pattern(pat, ctor_key).is_some() {
                                    self.bind_pattern_variables(pat, &place_to_test)?;
                                }
                            }
                        }

                        let first_matching_pat = self.mir.require(
                            expanded_matrix
                                .iter()
                                .find_map(|r| {
                                    let pat = r.patterns[col_idx];
                                    let inner = match pat {
                                        HirPattern::Alias { pattern, .. } => pattern.as_ref(),
                                        other => other,
                                    };
                                    self.specialize_pattern(inner, ctor_key)
                                        .map(|_| pat)
                                }),
                            "no matching pattern for constructor key",
                            self.span.clone(),
                        )?;

                        let inner_first = match first_matching_pat {
                            HirPattern::Alias { pattern, .. } => pattern.as_ref(),
                            other => other,
                        };
                        let sub_occurrences = self.generate_sub_occurrences(
                            &place_to_test,
                            ctor_key,
                            inner_first,
                        )?;

                        let mut spec_occurrences = occurrences[..col_idx].to_vec();
                        spec_occurrences.extend(sub_occurrences);
                        spec_occurrences.extend_from_slice(&occurrences[col_idx + 1..]);

                        let mut spec_matrix = Vec::new();
                        for row in &expanded_matrix {
                            let pat = row.patterns[col_idx];
                            let inner = match pat {
                                HirPattern::Alias { pattern, .. } => pattern.as_ref(),
                                other => other,
                            };
                            if let Some(sub_pats) = self.specialize_pattern(inner, ctor_key) {
                                let mut new_patterns = row.patterns.clone();
                                new_patterns.remove(col_idx);
                                spec_matrix.push(MatrixRow {
                                    patterns: sub_pats.into_iter().chain(new_patterns).collect(),
                                    arm_idx: row.arm_idx,
                                });
                            }
                        }

                        let target_block = self.compile_matrix(spec_matrix, spec_occurrences)?;
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
            let chosen_key = constructors_seen
                .iter()
                .find(|k| !matches!(k, PatternKindKey::WildcardOrBinding))
                .unwrap_or(&PatternKindKey::WildcardOrBinding);

            // 不可反驳列
            for row in &expanded_matrix {
                let pat = row.patterns[col_idx];
                if let HirPattern::Alias { .. } = pat {
                    self.bind_pattern_variables(pat, &place_to_test)?;
                }
            }

            let first_inner_pat = self.mir.require(
                expanded_matrix
                    .iter()
                    .find_map(|r| {
                        let pat = r.patterns[col_idx];
                        let inner = match pat {
                            HirPattern::Alias { pattern, .. } => pattern.as_ref(),
                            other => other,
                        };
                        self.specialize_pattern(inner, chosen_key).map(|_| inner)
                    }),
                "no matching pattern for irrefutable column",
                self.span.clone(),
            )?;

            let sub_occurrences = self.generate_sub_occurrences(
                &place_to_test,
                chosen_key,
                first_inner_pat,
            )?;

            let mut spec_occurrences = occurrences[..col_idx].to_vec();
            spec_occurrences.extend(sub_occurrences);
            spec_occurrences.extend_from_slice(&occurrences[col_idx + 1..]);

            let mut spec_matrix = Vec::new();
            for row in &expanded_matrix {
                let pat = row.patterns[col_idx];
                let inner = match pat {
                    HirPattern::Alias { pattern, .. } => pattern.as_ref(),
                    other => other,
                };
                if let Some(sub_pats) = self.specialize_pattern(inner, chosen_key) {
                    let mut new_patterns = row.patterns.clone();
                    new_patterns.remove(col_idx);
                    spec_matrix.push(MatrixRow {
                        patterns: sub_pats.into_iter().chain(new_patterns).collect(),
                        arm_idx: row.arm_idx,
                    });
                }
            }

            if spec_occurrences.is_empty() {
                let success_block = self.mir.new_block(self.span.clone());
                self.mir.start_block(success_block);
                if let Some(result_local) = self.is_result_local {
                    self.mir.push_stmt(
                        MirStmtKind::Let {
                            local: result_local,
                            rvalue: Rvalue::Constant(Const::Bool(true)),
                        },
                        self.span.clone(),
                    );
                    self.mir.set_terminator(TerminatorKind::Goto {
                        target: self.merge_block,
                        block_args: vec![Rvalue::Copy(Place::Local(result_local))],
                    });
                } else {
                    let arm = &self.arms[first_row.arm_idx];
                    let body_place = self.mir.compile_expr(arm.body)?;
                    let ret_val = body_place
                        .map(Rvalue::Move)
                        .unwrap_or(Rvalue::Tuple(vec![]));
                    self.mir.set_terminator(TerminatorKind::Goto {
                        target: self.merge_block,
                        block_args: vec![ret_val],
                    });
                }
                self.mir.current_block = saved_block;
                self.mir.current_stmts = saved_stmts;
                return Some(current_block);
            }

            let target_block = self.compile_matrix(spec_matrix, spec_occurrences)?;
            self.mir.set_terminator(TerminatorKind::Goto {
                target: target_block,
                block_args: vec![],
            });
        }

        self.mir.current_block = saved_block;
        self.mir.current_stmts = saved_stmts;
        Some(current_block)
    }
}

struct FnBuilder {
    pub name: String,
    pub locals_map: HashMap<SymId, MirLocalId>,
    pub generic_params: Vec<TyId>,
    pub signature: FnSig,
    pub local_decls: Vec<LocalDecl>,
    pub blocks: Vec<MirBasicBlockId>,
    pub return_local: MirLocalId,
}

pub struct MirLower {
    functions: Vec<MirFun>,
    extern_decls: Vec<ExternDecl>,
    statics: Vec<StaticDecl>,
    blocks: Vec<BasicBlock>,

    type_checker_result: TypeCtx,
    hir: HirCrate,

    fun: Option<FnBuilder>,
    current_block: MirBasicBlockId,
    current_stmts: Vec<MirStmt>,

    decl_to_static: HashMap<HirDeclId, MirStaticId>,

    struct_field_map: HashMap<(HirDeclId, String), usize>,
    adt_variant_map: HashMap<(HirDeclId, SymId), MirTagId>,
    decl_to_fun: HashMap<HirDeclId, MirFunId>,

    control_map: HashMap<SymId, MirControlId>,
    resume_target: Option<MirBasicBlockId>,
    pub_fun_ids: Vec<MirFunId>,

    bool_ty: TyId,
    uint8_ty: TyId,
    unit_ty: TyId,

    const_eval_counter: usize,
    current_blocks: Option<Vec<MirBasicBlockId>>,

    diag_collector: RefCell<DiagCollector>
}

impl MirLower {
    fn emit_mir_err(&self, err: MirLowerErrorKind, span: Span) {
        let compile_err = CompileTimeErrorKind::MirLowerError(err.clone());
        let error_kind = ErrorKind::CompileTimeError(compile_err);

        let diag_err = make_error(error_kind, span, Self::mir_lower_error_args_iter(err));

        self.diag_collector.borrow_mut().push_diag_error(diag_err);
    }

    fn new_block(&mut self, span: Span) -> MirBasicBlockId {
        let id = self.blocks.len();
        self.blocks.push(BasicBlock {
            block_params: vec![],
            statements: vec![],
            terminator: TerminatorKind::Unreachable,
            span,
        });
        if let Some(ref mut current) = self.current_blocks {
            current.push(MirBasicBlockId(id));
        }
        MirBasicBlockId(id)
    }

    fn start_block(&mut self, block_id: MirBasicBlockId) {
        self.current_block = block_id;
        self.current_stmts.clear();
    }

    fn finish_block(&mut self, terminator: TerminatorKind) {
        let block = &mut self.blocks[self.current_block.0];
        block.statements = std::mem::take(&mut self.current_stmts);
        block.terminator = terminator;
    }

    fn push_stmt(&mut self, kind: MirStmtKind, span: Span) {
        self.current_stmts.push(MirStmt { kind, span });
    }

    fn set_terminator(&mut self, terminator: TerminatorKind) {
        self.finish_block(terminator);
    }

    fn switch_to_new_block(&mut self, span: Span) -> MirBasicBlockId {
        let next_block = self.new_block(span);
        self.set_terminator(TerminatorKind::Goto {
            target: next_block,
            block_args: vec![],
        });
        self.start_block(next_block);
        next_block
    }

    fn make_const_eval_fun(&mut self, inner_expr: HirExprId, span: Span) -> Option<MirFunId> {
        let inner_ty = self.expr_ty(inner_expr)?;
        let counter = self.const_eval_counter;
        self.const_eval_counter += 1;
        let fun_name = format!("__const_eval_{}", counter);

        let sig = FnSig { params: vec![], return_ty: inner_ty };

        let saved_fun = self.fun.take();
        let saved_current_block = self.current_block;
        let saved_stmts = std::mem::take(&mut self.current_stmts);
        let saved_current_blocks = self.current_blocks.take();   // 保存外层跟踪

        self.current_blocks = Some(Vec::new());

        let mut builder = FnBuilder {
            name: fun_name.clone(),
            locals_map: HashMap::new(),
            generic_params: vec![],
            signature: sig.clone(),
            local_decls: vec![],
            blocks: vec![],
            return_local: MirLocalId(0),
        };

        self.fun = Some(builder);
        let ret_local = self.new_local(
            inner_ty, true, Some("return_val".to_string()), span.clone());
        self.fun.as_mut().unwrap().return_local = ret_local;

        let entry_block = self.new_block(span.clone());
        self.start_block(entry_block);

        let maybe_place = self.compile_expr(inner_expr)?;
        if let Some(place) = maybe_place {
            let ret_id = self.fun.as_ref().unwrap().return_local;
            self.push_stmt(MirStmtKind::Store {
                place: Place::Local(ret_id),
                rvalue: Rvalue::Move(place),
            }, span.clone());
        }
        self.set_terminator(TerminatorKind::Return);

        let mut finished = self.fun.take().unwrap();
        let fun_blocks = self.current_blocks.take().unwrap();    // 匿名函数的块列表
        finished.blocks = fun_blocks;

        self.fun = saved_fun;
        self.current_block = saved_current_block;
        self.current_stmts = saved_stmts;
        self.current_blocks = saved_current_blocks;

        let fun_id = self.functions.len();
        self.functions.push(MirFun {
            name: finished.name,
            generic_params: finished.generic_params,
            signature: finished.signature,
            local_decls: finished.local_decls,
            blocks: finished.blocks,
            is_consteval: true,
            span,
        });
        Some(MirFunId(fun_id))
    }

    fn build_call_by_ptr(
        &mut self,
        callee: HirExprId,
        args: Vec<Rvalue>,
        dest: MirLocalId,
        target: MirBasicBlockId,
        span: Span,
    ) -> Option<TerminatorKind> {

        let value = self.compile_expr(callee)?;
        let mut func_place = self.require(
            value,
            "callee expression produced no place",
            span.clone(),
        )?;

        loop {
            let ty = self.place_ty(&func_place, span.clone())?;
            let root = get_type_root(&self.type_checker_result.type_pool, ty);
            match &self.type_checker_result.type_pool[root].kind {
                TypeNodeKind::Ref(_) | TypeNodeKind::MutRef(_) | TypeNodeKind::Share(_) => {
                    func_place = Place::Deref(Box::new(func_place));
                }
                _ => break,
            }
        }

        Some(TerminatorKind::CallByPtr {
            func: Rvalue::Move(func_place),
            args,
            dest: Place::Local(dest),
            target: Some(target),
        })
    }

    fn expr_ty(&mut self, expr_id: HirExprId) -> Option<TyId> {
        self.require(
            self.type_checker_result.expr_type_map.get(&expr_id).copied(),
            format!("type not found for expression {}", expr_id.0),
            self.hir.hir_expr_pool[expr_id.0].span.clone(),
        )
    }

    fn get_static_id(&self, decl_id: HirDeclId) -> Option<MirStaticId> {
        self.decl_to_static.get(&decl_id).copied()
    }

    fn get_fn_sig_from_ty(&self, ty: TyId, span: Span) -> Option<(Vec<TyId>, TyId)> {
        let root = get_type_root(&self.type_checker_result.type_pool, ty);
        match &self.type_checker_result.type_pool[root].kind {
            TypeNodeKind::Fun { param_tys, return_ty } => Some((param_tys.clone(), *return_ty)),
            _ => {
                self.emit_mir_err(MirLowerErrorKind::ExpectedFnType, span);
                None
            }
        }
    }

    fn new_local(&mut self, ty: TyId, mutable: bool, name: Option<String>, span: Span) -> MirLocalId {
        let fun = self.fun.as_mut().unwrap();
        let id = fun.local_decls.len();
        fun.local_decls.push(LocalDecl {
            ty,
            mutable,
            name,
            span,
        });
        MirLocalId(id)
    }

    fn new_mutable_temp(&mut self, ty: TyId, span: Span) -> MirLocalId {
        self.new_local(ty, true, None, span)
    }

    fn new_immutable_temp(&mut self, ty: TyId, span: Span) -> MirLocalId {
        self.new_local(ty, false, None, span)
    }

    fn require<T>(&self, value: Option<T>, msg: impl Into<String>, span: Span) -> Option<T> {
        match value {
            Some(v) => Some(v),
            None => {
                self.emit_mir_err(MirLowerErrorKind::GenericInternal(msg.into()), span);
                None
            }
        }
    }

    fn bind_local(&mut self, sym: SymId, local: MirLocalId) {
        let fun = self.fun.as_mut().expect("no function being built");
        fun.locals_map.insert(sym, local);
    }

    fn resolve_constructor_tag(&self, type_name: &HirTypeName, ty: TyId) -> Option<MirTagId> {
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

    fn get_constructor_field_ty(&self, _decl_id: HirDeclId, _tag: MirTagId) -> TyId {
        self.uint8_ty
    }

    fn lower_decls(&mut self) -> Option<()> {
        let decls = self.hir.hir_decl_pool.clone();
        for (decl_usize, decl) in decls.iter().enumerate() {
            let decl_id = HirDeclId(decl_usize);
            match &decl.kind {
                HirDeclKind::External { sym_name, is_variadic ,.. } => {
                    let scheme = self.require(
                        self.type_checker_result.decl_type_map.get(&decl_id),
                        "external decl type not found",
                        decl.span.clone(),
                    )?;
                    let (param_tys, return_ty) = self.get_fn_sig_from_ty(scheme.body, decl.span.clone())?;
                    self.extern_decls.push(ExternDecl {
                        name: sym_name.clone(),
                        signature: FnSig { params: param_tys.clone(), return_ty },
                        is_variadic: *is_variadic,
                        span: decl.span.clone(),
                    });

                    let fun_usize = self.functions.len();
                    let fun_id = MirFunId(fun_usize);
                    self.decl_to_fun.insert(decl_id, fun_id);
                    self.functions.push(MirFun {
                        name: sym_name.clone(),
                        generic_params: vec![],
                        signature: FnSig { params: param_tys, return_ty },
                        local_decls: vec![],
                        blocks: vec![],
                        is_consteval: false,
                        span: decl.span.clone(),
                    });
                }
                HirDeclKind::Global { .. } | HirDeclKind::Const { .. } => {
                    let scheme = self.require(
                        self.type_checker_result.decl_type_map.get(&decl_id),
                        "global/const type not found",
                        decl.span.clone(),
                    )?;
                    let ty = scheme.body;
                    let static_usize = self.statics.len();
                    let static_id = MirStaticId(static_usize);
                    self.decl_to_static.insert(decl_id, static_id);
                    self.statics.push(StaticDecl {
                        name: decl.ident.clone(),
                        ty,
                        mutable: matches!(&decl.kind, HirDeclKind::Global { .. }),
                        init: todo!("convert const expr"),
                        span: decl.span.clone(),
                    });
                }
                HirDeclKind::Fun { is_consteval, .. } => {
                    let fun_usize = self.functions.len();
                    let fun_id = MirFunId(fun_usize);
                    self.decl_to_fun.insert(decl_id, fun_id);
                    let mir_fun = self.lower_function(decl_id, *is_consteval)?;
                    self.functions.push(mir_fun);
                    if decl.is_pub_external {
                        self.pub_fun_ids.push(fun_id);
                    }
                }
                HirDeclKind::Struct { fields, .. } => {
                    for (idx, f) in fields.iter().enumerate() {
                        self.struct_field_map.insert((decl_id, f.name.name.clone()), idx);
                    }
                }
                HirDeclKind::ADT { ctors, .. } => {
                    for (tag_usize, ctor) in ctors.iter().enumerate() {
                        let tag = MirTagId(tag_usize);
                        self.adt_variant_map.insert((decl_id, ctor.name.sym_id), tag);
                    }
                }
                HirDeclKind::Effect { controls } => {
                    for (name, _, _) in controls {
                        let ctrl_id = MirControlId(self.control_map.len());
                        self.control_map.insert(name.sym_id, ctrl_id);
                    }
                }
                _ => {}
            }
        }
        Some(())
    }

    fn lower_function(&mut self, decl_id: HirDeclId, is_consteval: bool) -> Option<MirFun> {
        let decl = self.hir.hir_decl_pool[decl_id.0].clone();
        let (params, return_type_ann, body) = match &decl.kind {
            HirDeclKind::Fun { params, return_type, body, .. } => {
                (params.clone(), return_type.clone(), body.clone())
            }
            _ => {
                self.emit_mir_err(MirLowerErrorKind::GenericInternal("expected function declaration".into()), decl.span.clone());
                return None;
            }
        };

        let ty_scheme = self.require(
            self.type_checker_result.decl_type_map.get(&decl_id),
            "function type scheme not found",
            decl.span.clone(),
        )?;
        let (param_tys, return_ty) = self.get_fn_sig_from_ty(ty_scheme.body, decl.span.clone())?;
        let generic_params = ty_scheme.quantified.clone();

        let mut fun = FnBuilder {
            name: decl.ident.clone(),
            locals_map: HashMap::new(),
            generic_params,
            signature: FnSig { params: param_tys.clone(), return_ty },
            local_decls: vec![],
            blocks: vec![],
            return_local: MirLocalId(0),
        };

        self.fun = Some(fun);

        let ret_local = self.new_local(return_ty, true, Some("return_val".to_string()), decl.span.clone());
        self.fun.as_mut().unwrap().return_local = ret_local;

        for (param, ty) in params.iter().zip(param_tys.iter()) {
            let param_span = param.span.clone();
            let local = self.new_local(*ty, false, Some(param.name.name.clone()), param_span);
            self.bind_local(param.name.sym_id, local);
        }

        self.current_blocks = Some(Vec::new());

        let entry_block = self.new_block(decl.span.clone());
        self.start_block(entry_block);

        let mut last_place = None;
        for stmt_expr in body {
            last_place = self.compile_expr(stmt_expr)?;
        }

        let need_terminator = matches!(
            self.blocks[self.current_block.0].terminator,
            TerminatorKind::Unreachable
        );
        let is_never = self.type_checker_result.type_pool[return_ty].kind == TypeNodeKind::Never;

        if need_terminator && !is_never {
            if let Some(place) = last_place {
                let ret_id = self.fun.as_ref().unwrap().return_local;
                self.push_stmt(
                    MirStmtKind::Store {
                        place: Place::Local(ret_id),
                        rvalue: Rvalue::Move(place),
                    },
                    decl.span.clone(),
                );
            }
            self.set_terminator(TerminatorKind::Return);
        }

        let fun_blocks = self.current_blocks.take().unwrap();
        let mut fun = self.fun.take().unwrap();
        fun.blocks = fun_blocks;

        Some(MirFun {
            name: fun.name,
            generic_params: fun.generic_params,
            signature: fun.signature,
            local_decls: fun.local_decls,
            blocks: fun.blocks,
            is_consteval,
            span: decl.span,
        })
    }

    pub fn place_ty(&self, place: &Place, span: Span) -> Option<TyId> {
        match place {
            Place::Local(id) => Some(self.fun.as_ref().unwrap().local_decls[id.0].ty),
            Place::Field { base, field } => {
                let base_ty = self.place_ty(base, span.clone())?;
                let root = get_type_root(&self.type_checker_result.type_pool, base_ty);
                match &self.type_checker_result.type_pool[root].kind {
                    TypeNodeKind::Struct { field_tys, .. } => Some(field_tys[*field]),
                    TypeNodeKind::Tuple(elements) => Some(elements[*field]),
                    _ => {
                        self.emit_mir_err(MirLowerErrorKind::FieldAccessNonStruct { field: field.to_string() }, span);
                        None
                    }
                }
            }
            Place::EnumItem { place, variant } => {
                let adt_ty = self.place_ty(place, span.clone())?;
                let root = get_type_root(&self.type_checker_result.type_pool, adt_ty);
                if let TypeNodeKind::ADT { variants, .. } = &self.type_checker_result.type_pool[root].kind {
                    Some(variants[variant.0].unwrap_or(self.unit_ty))
                } else {
                    self.emit_mir_err(MirLowerErrorKind::ExpectedADTType, span);
                    None
                }
            }
            Place::Deref(p) => {
                let inner_ty = self.place_ty(p, span.clone())?;
                let root = get_type_root(&self.type_checker_result.type_pool, inner_ty);
                match &self.type_checker_result.type_pool[root].kind {
                    TypeNodeKind::Ref(inner)
                    | TypeNodeKind::MutRef(inner)
                    | TypeNodeKind::Share(inner) => Some(*inner),
                    _ => {
                        self.emit_mir_err(MirLowerErrorKind::GenericInternal("Deref on non-reference type".into()), span);
                        None
                    }
                }
            }
            _ => {
                self.emit_mir_err(MirLowerErrorKind::GenericInternal("unsupported place kind".into()), span);
                None
            }
        }
    }

    fn compile_expr(&mut self, expr_id: HirExprId) -> Option<Option<Place>> {
        let expr = self.hir.hir_expr_pool[expr_id.0].clone();
        let span = expr.span.clone();

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
                let ty = self.expr_ty(expr_id)?;
                let temp = self.new_mutable_temp(ty, span.clone());
                self.push_stmt(
                    MirStmtKind::Let {
                        local: temp,
                        rvalue: Rvalue::Constant(mir_const),
                    },
                    span,
                );
                Some(Some(Place::Local(temp)))
            }

            HirExprKind::Ident(name) => {
                if let Some(place) = self.lookup_place(name.sym_id) {
                    return Some(Some(place));
                }

                if let Some(&decl_id) = self.type_checker_result.sym_to_decl.get(&name.sym_id) {
                    if let Some(&fun_id) = self.decl_to_fun.get(&decl_id) {
                        let ty = self.expr_ty(expr_id)?;
                        let temp = self.new_mutable_temp(ty, span.clone());
                        self.push_stmt(
                            MirStmtKind::Let {
                                local: temp,
                                rvalue: Rvalue::GetFunPtr(fun_id),
                            },
                            span,
                        );
                        return Some(Some(Place::Local(temp)));
                    }
                }

                self.emit_mir_err(MirLowerErrorKind::IdentNoValueFound(name.name.clone()), span);
                None
            }

            HirExprKind::Binary { left, right, op } => {
                let value = self.compile_expr(*left)?;
                let l_place = self.require(
                    value,
                    "left operand has no place",
                    span.clone(),
                )?;
                let value = self.compile_expr(*right)?;
                let r_place = self.require(
                    value,
                    "right operand has no place",
                    span.clone(),
                )?;

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

                let ty = self.expr_ty(expr_id)?;
                let temp = self.new_mutable_temp(ty, span.clone());
                self.push_stmt(
                    MirStmtKind::Let {
                        local: temp,
                        rvalue: Rvalue::BinaryOp {
                            op: mir_op,
                            left: Box::new(Rvalue::Copy(l_place)),
                            right: Box::new(Rvalue::Copy(r_place)),
                        },
                    },
                    span,
                );
                Some(Some(Place::Local(temp)))
            }

            HirExprKind::Unary { op, right } => {
                let value = self.compile_expr(*right)?;
                let r_place = self.require(
                    value,
                    "unary operand has no place",
                    span.clone(),
                )?;
                let mir_op = match op {
                    HirUnaryOp::Neg => MirUnOp::Neg,
                    HirUnaryOp::Not => MirUnOp::Not,
                };

                let ty = self.expr_ty(expr_id)?;
                let temp = self.new_mutable_temp(ty, span.clone());
                self.push_stmt(
                    MirStmtKind::Let {
                        local: temp,
                        rvalue: Rvalue::UnaryOp {
                            op: mir_op,
                            right: Box::new(Rvalue::Copy(r_place)),
                        },
                    },
                    span,
                );
                Some(Some(Place::Local(temp)))
            }

            HirExprKind::Let { name, init, mutable, .. } => {
                if let Some(init_place) = self.compile_expr(*init)? {
                    let ty = self.expr_ty(*init)?;
                    let local_id = self.new_local(ty, *mutable, Some(name.name.clone()), span.clone());
                    self.bind_local(name.sym_id, local_id);
                    self.push_stmt(
                        MirStmtKind::Let {
                            local: local_id,
                            rvalue: Rvalue::Move(init_place),
                        },
                        span,
                    );
                }
                Some(None)
            }

            HirExprKind::Block { stmts } => {
                let mut last_place = None;
                for stmt in stmts {
                    last_place = self.compile_expr(*stmt)?;
                }
                Some(last_place)
            }

            HirExprKind::Return { expr } => {
                if let Some(ret_expr_id) = expr {
                    if let Some(place) = self.compile_expr(*ret_expr_id)? {
                        let ret_id = self.fun.as_ref().unwrap().return_local;
                        self.push_stmt(
                            MirStmtKind::Store {
                                place: Place::Local(ret_id),
                                rvalue: Rvalue::Move(place),
                            },
                            span.clone(),
                        );
                    }
                }
                self.set_terminator(TerminatorKind::Return);
                let block = self.new_block(span);
                self.start_block(block);
                Some(None)
            }

            HirExprKind::Call { callee, args }
            | HirExprKind::UnsafeExternalCall { callee, args } => {
                let mut mir_args = Vec::new();
                for arg_expr in args {
                    if let Some(arg_place) = self.compile_expr(*arg_expr)? {
                        mir_args.push(Rvalue::Move(arg_place));
                    }
                }

                let ty = self.expr_ty(expr_id)?;
                let result_temp = self.new_mutable_temp(ty, span.clone());
                let next_block = self.new_block(span.clone());

                let callee_expr = &self.hir.hir_expr_pool[callee.0];
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
                            self.build_call_by_ptr(*callee, mir_args, result_temp, next_block, span.clone())?
                        }
                    } else {
                        self.build_call_by_ptr(*callee, mir_args, result_temp, next_block, span.clone())?
                    }
                } else {
                    self.build_call_by_ptr(*callee, mir_args, result_temp, next_block, span.clone())?
                };

                self.set_terminator(terminator);
                self.start_block(next_block);
                Some(Some(Place::Local(result_temp)))
            }

            HirExprKind::Tuple { elements } => {
                let mut mir_elements = Vec::new();
                for elem in elements {
                    if let Some(place) = self.compile_expr(*elem)? {
                        mir_elements.push(Rvalue::Move(place));
                    }
                }
                let ty = self.expr_ty(expr_id)?;
                let temp = self.new_mutable_temp(ty, span.clone());
                self.push_stmt(
                    MirStmtKind::Let {
                        local: temp,
                        rvalue: Rvalue::Tuple(mir_elements),
                    },
                    span,
                );
                Some(Some(Place::Local(temp)))
            }

            HirExprKind::Move { target } => {
                let value = self.compile_expr(*target)?;
                let place = self.require(
                    value,
                    "move source has no place",
                    span.clone(),
                )?;
                let ty = self.expr_ty(expr_id)?;
                let temp = self.new_mutable_temp(ty, span.clone());
                self.push_stmt(
                    MirStmtKind::Let {
                        local: temp,
                        rvalue: Rvalue::Move(place),
                    },
                    span,
                );
                Some(Some(Place::Local(temp)))
            }

            HirExprKind::Copy { target } => {
                let value = self.compile_expr(*target)?;
                let place = self.require(
                    value,
                    "copy source has no place",
                    span.clone(),
                )?;
                let ty = self.expr_ty(expr_id)?;
                let temp = self.new_mutable_temp(ty, span.clone());
                self.push_stmt(
                    MirStmtKind::Let {
                        local: temp,
                        rvalue: Rvalue::Copy(place),
                    },
                    span,
                );
                Some(Some(Place::Local(temp)))
            }

            HirExprKind::Ref { target } => {
                let value = self.compile_expr(*target)?;
                let place = self.require(
                    value,
                    "ref target has no place",
                    span.clone(),
                )?;
                let ty = self.expr_ty(expr_id)?;
                let temp = self.new_mutable_temp(ty, span.clone());
                self.push_stmt(
                    MirStmtKind::Let {
                        local: temp,
                        rvalue: Rvalue::Ref(place),
                    },
                    span,
                );
                Some(Some(Place::Local(temp)))
            }

            HirExprKind::MutRef { target } => {
                let value = self.compile_expr(*target)?;

                let place = self.require(
                    value,
                    "mut ref target has no place",
                    span.clone(),
                )?;
                let ty = self.expr_ty(expr_id)?;
                let temp = self.new_mutable_temp(ty, span.clone());
                self.push_stmt(
                    MirStmtKind::Let {
                        local: temp,
                        rvalue: Rvalue::RefMut(place),
                    },
                    span,
                );
                Some(Some(Place::Local(temp)))
            }

            HirExprKind::Share { target } => {
                let value = self.compile_expr(*target)?;

                let place = self.require(
                    value,
                    "share target has no place",
                    span.clone(),
                )?;
                let ty = self.expr_ty(expr_id)?;
                let temp = self.new_mutable_temp(ty, span.clone());
                self.push_stmt(
                    MirStmtKind::Let {
                        local: temp,
                        rvalue: Rvalue::GcObjectRef(Box::new(Rvalue::Move(place))),
                    },
                    span,
                );
                Some(Some(Place::Local(temp)))
            }

            HirExprKind::TypeCast { expr: cast_expr, type_ann: _ } => {
                let value = self.compile_expr(*cast_expr)?;
                let place = self.require(
                    value,
                    "cast source has no place",
                    span.clone(),
                )?;
                let dest_ty = self.expr_ty(expr_id)?;
                let temp = self.new_mutable_temp(dest_ty, span.clone());
                self.push_stmt(
                    MirStmtKind::Let {
                        local: temp,
                        rvalue: Rvalue::Cast(place, dest_ty),
                    },
                    span,
                );
                Some(Some(Place::Local(temp)))
            }

            HirExprKind::FieldAccess { obj, field } => {
                let obj_place = match self.compile_expr(*obj)? {
                    Some(p) => p,
                    None => {
                        self.emit_mir_err(MirLowerErrorKind::GenericInternal("field access object has no place".into()), span.clone());
                        return None;
                    }
                };
                let mut obj_place = obj_place;
                let mut obj_ty = self.expr_ty(*obj)?;
                loop {
                    let root = get_type_root(&self.type_checker_result.type_pool, obj_ty);
                    match &self.type_checker_result.type_pool[root].kind {
                        TypeNodeKind::Struct { .. } => break,
                        TypeNodeKind::Ref(inner)
                        | TypeNodeKind::MutRef(inner)
                        | TypeNodeKind::Share(inner) => {
                            obj_place = Place::Deref(Box::new(obj_place));
                            obj_ty = *inner;
                        }
                        _ => {
                            self.emit_mir_err(MirLowerErrorKind::FieldAccessNonStruct { field: field.clone() }, span.clone());
                            return None;
                        }
                    }
                }
                let obj_root_ty = get_type_root(&self.type_checker_result.type_pool, obj_ty);
                let decl_id = match &self.type_checker_result.type_pool[obj_root_ty].kind {
                    TypeNodeKind::Struct { decl_id, .. } => *decl_id,
                    _ => unreachable!(),
                };
                let field_idx = match self.struct_field_map.get(&(decl_id, field.clone())) {
                    Some(&idx) => idx,
                    None => {
                        self.emit_mir_err(MirLowerErrorKind::FieldMissingInStruct(field.clone()), span.clone());
                        return None;
                    }
                };
                Some(Some(Place::Field {
                    base: Box::new(obj_place),
                    field: field_idx,
                }))
            }

            HirExprKind::TupleIndex { expr, index } => {
                let obj_place = match self.compile_expr(*expr)? {
                    Some(p) => p,
                    None => {
                        self.emit_mir_err(MirLowerErrorKind::GenericInternal("tuple index expression has no place".into()), span.clone());
                        return None;
                    }
                };
                Some(Some(Place::Field {
                    base: Box::new(obj_place),
                    field: *index,
                }))
            }

            HirExprKind::MakeStruct { path: _, fields } => {
                let mut mir_fields = Vec::new();
                for (_, field_expr) in fields {
                    if let Some(place) = self.compile_expr(*field_expr)? {
                        mir_fields.push(Rvalue::Move(place));
                    }
                }
                let ty = self.expr_ty(expr_id)?;
                let temp = self.new_mutable_temp(ty, span.clone());
                self.push_stmt(
                    MirStmtKind::Let {
                        local: temp,
                        rvalue: Rvalue::BuildStruct(mir_fields),
                    },
                    span,
                );
                Some(Some(Place::Local(temp)))
            }

            HirExprKind::BuildVariant { variant_name, target } => {
                let ty = self.expr_ty(expr_id)?;
                let root_ty = get_type_root(&self.type_checker_result.type_pool, ty);
                let decl_id = match &self.type_checker_result.type_pool[root_ty].kind {
                    TypeNodeKind::ADT { decl_id, .. } => *decl_id,
                    _ => {
                        self.emit_mir_err(MirLowerErrorKind::VariantBuildNonADT, span.clone());
                        return None;
                    }
                };
                let tag = match self.adt_variant_map.get(&(decl_id, variant_name.sym_id)) {
                    Some(&t) => t,
                    None => {
                        self.emit_mir_err(MirLowerErrorKind::VariantMissingInADTMap(variant_name.name.clone()), span.clone());
                        return None;
                    }
                };

                let inner_rvalue = if let Some(payload_place) = self.compile_expr(*target)? {
                    Box::new(Rvalue::Move(payload_place))
                } else {
                    Box::new(Rvalue::Tuple(vec![]))
                };

                let temp = self.new_mutable_temp(ty, span.clone());
                self.push_stmt(
                    MirStmtKind::Let {
                        local: temp,
                        rvalue: Rvalue::Variant(tag, inner_rvalue),
                    },
                    span,
                );
                Some(Some(Place::Local(temp)))
            }

            HirExprKind::If { cond, then, elifs, else_opt } => {
                let result_ty = self.expr_ty(expr_id)?;
                let merge_block = self.new_block(span.clone());

                let cond_place = match self.compile_expr(*cond)? {
                    Some(p) => p,
                    None => {
                        self.emit_mir_err(MirLowerErrorKind::IfCondNoPlace, span.clone());
                        return None;
                    }
                };
                let then_block = self.new_block(span.clone());
                let else_block = self.new_block(span.clone());

                self.set_terminator(TerminatorKind::SwitchInt {
                    discriminant: Rvalue::Copy(cond_place),
                    targets: vec![(Const::Bool(true), then_block)],
                    default: else_block,
                });

                self.start_block(then_block);
                let then_place = self.compile_expr(*then)?;
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
                    let cond_place = match self.compile_expr(*elif_cond)? {
                        Some(p) => p,
                        None => {
                            self.emit_mir_err(MirLowerErrorKind::ElifCondNoPlace, span.clone());
                            return None;
                        }
                    };
                    let elif_then = self.new_block(span.clone());
                    let next_else = self.new_block(span.clone());

                    self.set_terminator(TerminatorKind::SwitchInt {
                        discriminant: Rvalue::Copy(cond_place),
                        targets: vec![(Const::Bool(true), elif_then)],
                        default: next_else,
                    });

                    self.start_block(elif_then);
                    let elif_place = self.compile_expr(*elif_body)?;
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
                    if let Some(place) = self.compile_expr(*else_expr)? {
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

                let result_local = self.new_mutable_temp(result_ty, span.clone());
                self.blocks[merge_block.0].block_params = vec![result_local];
                self.start_block(merge_block);

                Some(Some(Place::Local(result_local)))
            }

            HirExprKind::Match { scrutinee, arms } => {
                let result_ty = self.expr_ty(expr_id)?;
                let merge_block = self.new_block(span.clone());
                let result_local = self.new_mutable_temp(result_ty, span.clone());
                self.blocks[merge_block.0].block_params = vec![result_local];

                let scrutinee_place = match self.compile_expr(*scrutinee)? {
                    Some(p) => p,
                    None => {
                        self.emit_mir_err(MirLowerErrorKind::MatchScrutineeNoPlace, span.clone());
                        return None;
                    }
                };

                let mut expanded_arms: Vec<HirMatchArm> = Vec::new();
                for arm in arms {
                    let mut to_expand = vec![arm.pattern.clone()];
                    while let Some(pat) = to_expand.pop() {
                        match pat {
                            HirPattern::Or { left, right, .. } => {
                                to_expand.push(*left);
                                to_expand.push(*right);
                            }
                            other => {
                                expanded_arms.push(HirMatchArm {
                                    pattern: other,
                                    guard: arm.guard.clone(),
                                    body: arm.body,
                                    span: arm.span.clone(),
                                });
                            }
                        }
                    }
                }

                let mut initial_matrix = Vec::new();
                for (idx, arm) in expanded_arms.iter().enumerate() {
                    initial_matrix.push(MatrixRow {
                        patterns: vec![&arm.pattern],
                        arm_idx: idx,
                    });
                }

                let initial_occurrences = vec![scrutinee_place];

                let mut builder = DecisionTreeBuilder {
                    mir: self,
                    arms: &expanded_arms,
                    merge_block,
                    dag_cache: HashMap::new(),
                    is_result_local: None,
                    span,
                };

                let start_block = builder.compile_matrix(initial_matrix, initial_occurrences)?;

                self.set_terminator(TerminatorKind::Goto {
                    target: start_block,
                    block_args: vec![],
                });

                self.start_block(merge_block);
                Some(Some(Place::Local(result_local)))
            }

            HirExprKind::Is { expr, pattern } => {
                let result_ty = self.expr_ty(expr_id)?;
                let merge_block = self.new_block(span.clone());
                let result_local = self.new_local(result_ty, true, Some("is_result".into()), span.clone());
                self.blocks[merge_block.0].block_params = vec![result_local];

                let scrutinee_place = match self.compile_expr(*expr)? {
                    Some(p) => p,
                    None => {
                        self.emit_mir_err(MirLowerErrorKind::IsExprNoPlace, span.clone());
                        return None;
                    }
                };

                let dummy_arm = HirMatchArm {
                    pattern: pattern.clone(),
                    guard: None,
                    body: HirExprId(0),
                    span: span.clone(),
                };
                let arms = vec![dummy_arm];

                let mut builder = DecisionTreeBuilder {
                    mir: self,
                    arms: &arms,
                    merge_block,
                    dag_cache: HashMap::new(),
                    is_result_local: Some(result_local),
                    span,
                };

                let initial_matrix = vec![MatrixRow {
                    patterns: vec![pattern],
                    arm_idx: 0,
                }];
                let initial_occurrences = vec![scrutinee_place];

                let start_block = builder.compile_matrix(initial_matrix, initial_occurrences)?;

                self.set_terminator(TerminatorKind::Goto {
                    target: start_block,
                    block_args: vec![],
                });

                self.start_block(merge_block);
                Some(Some(Place::Local(result_local)))
            }

            HirExprKind::With { handler, clauses } => {
                let result_ty = self.expr_ty(expr_id)?;

                let merge_block = self.new_block(span.clone());
                let result_local = self.new_local(result_ty, true, Some("with_res".to_string()), span.clone());
                self.blocks[merge_block.0].block_params = vec![result_local];

                let entry_block = self.current_block;

                let body_block = self.new_block(span.clone());
                let body_entry = self.new_block(span.clone());

                self.start_block(body_block);
                self.set_terminator(TerminatorKind::Goto {
                    target: body_entry,
                    block_args: vec![],
                });

                self.start_block(body_entry);
                let body_place = self.compile_expr(*handler)?;
                let body_value = match body_place {
                    Some(p) => Rvalue::Move(p),
                    None => Rvalue::Tuple(vec![]),
                };
                self.set_terminator(TerminatorKind::Goto {
                    target: merge_block,
                    block_args: vec![body_value],
                });

                let mut next_block = body_block;
                for clause in clauses.iter().rev() {
                    let handler_block = self.new_block(span.clone());

                    let mut param_locals = Vec::new();
                    let mut bound_syms = Vec::new();
                    for param in &clause.params {
                        match param {
                            HirCatchParam::Binding(name) => {
                                let pty = self.get_control_param_ty(
                                    &clause.control_path,
                                    param_locals.len(),
                                    span.clone(),
                                )?;
                                let local = self.new_local(pty, false, Some(name.name.clone()), span.clone());
                                param_locals.push(local);
                                bound_syms.push((name.sym_id, local));
                            }
                            HirCatchParam::Rest => {}
                        }
                    }

                    self.blocks[handler_block.0].block_params = param_locals.clone();

                    self.start_block(handler_block);
                    for (i, &(sym_id, local)) in bound_syms.iter().enumerate() {
                        self.bind_local(sym_id, local);
                        self.push_stmt(
                            MirStmtKind::Let {
                                local,
                                rvalue: Rvalue::HandlerArg(i),
                            },
                            span.clone(),
                        );
                    }

                    let handler_body_entry = self.new_block(span.clone());
                    self.set_terminator(TerminatorKind::Goto {
                        target: handler_body_entry,
                        block_args: vec![],
                    });
                    self.start_block(handler_body_entry);

                    let old_resume_target = self.resume_target;
                    self.resume_target = Some(merge_block);

                    let _body_place = self.compile_expr(clause.body)?;
                    let need_terminator = matches!(
                        self.blocks[self.current_block.0].terminator,
                        TerminatorKind::Unreachable
                    );
                    if need_terminator {
                        self.set_terminator(TerminatorKind::Goto {
                            target: merge_block,
                            block_args: vec![Rvalue::Tuple(vec![])],
                        });
                    }

                    self.resume_target = old_resume_target;

                    let control_id = match self.control_map.get(&clause.control_path.sym_id) {
                        Some(&id) => id,
                        None => {
                            self.emit_mir_err(MirLowerErrorKind::ControlNotFound(clause.control_path.name.clone()), span.clone());
                            return None;
                        }
                    };

                    let install_block = self.new_block(span.clone());
                    self.start_block(install_block);
                    self.set_terminator(TerminatorKind::InstallHandler {
                        handler_block,
                        next: next_block,
                        args_dest: param_locals,
                        control_id,
                    });
                    next_block = install_block;
                }

                self.start_block(entry_block);
                self.set_terminator(TerminatorKind::Goto {
                    target: next_block,
                    block_args: vec![],
                });

                self.start_block(merge_block);
                Some(Some(Place::Local(result_local)))
            }

            HirExprKind::Raise { control_name, args } => {
                let control_id = match self.control_map.get(&control_name.sym_id) {
                    Some(&id) => id,
                    None => {
                        self.emit_mir_err(MirLowerErrorKind::ControlNotFound(control_name.name.clone()), span.clone());
                        return None;
                    }
                };
                let mut mir_args = Vec::new();
                for arg in args {
                    if let Some(place) = self.compile_expr(*arg)? {
                        mir_args.push(Rvalue::Move(place));
                    }
                }
                let ty = self.expr_ty(expr_id)?;
                let dest_temp = self.new_mutable_temp(ty, span.clone());
                self.set_terminator(TerminatorKind::Raise {
                    control_name: control_id,
                    args: mir_args,
                    dest: Place::Local(dest_temp),
                });
                let block = self.new_block(span);
                self.start_block(block);
                Some(Some(Place::Local(dest_temp)))
            }

            HirExprKind::Resume { expr } => {
                let target = match self.resume_target {
                    Some(t) => t,
                    None => {
                        self.emit_mir_err(MirLowerErrorKind::ResumeOutsideHandler, span.clone());
                        return None;
                    }
                };
                let place = self.compile_expr(*expr)?;
                let expr_ty = self.expr_ty(*expr)?;
                let temp = self.new_mutable_temp(expr_ty, span.clone());

                if let Some(p) = place {
                    self.push_stmt(
                        MirStmtKind::Let {
                            local: temp,
                            rvalue: Rvalue::Move(p),
                        },
                        span.clone(),
                    );
                } else {
                    self.push_stmt(
                        MirStmtKind::Let {
                            local: temp,
                            rvalue: Rvalue::Tuple(vec![]),
                        },
                        span.clone(),
                    );
                }

                self.set_terminator(TerminatorKind::Resume {
                    place: Place::Local(temp),
                    target,
                });
                Some(None)
            }

            HirExprKind::Ellipsis => todo!(),

            HirExprKind::ConstEval { expr } => {
                let fun_id = self.make_const_eval_fun(*expr, span.clone())?;
                let ty = self.expr_ty(expr_id)?;
                let result_temp = self.new_mutable_temp(ty, span.clone());
                let next_block = self.new_block(span.clone());

                self.set_terminator(TerminatorKind::Call {
                    func: fun_id,
                    args: vec![],
                    dest: Place::Local(result_temp),
                    target: Some(next_block),
                });
                self.start_block(next_block);
                Some(Some(Place::Local(result_temp)))
            }
        }
    }

    fn get_control_param_ty(
        &mut self,
        control_name: &HirName,
        index: usize,
        span: Span,
    ) -> Option<TyId> {
        let scheme = self.type_checker_result.name_type_map
            .get(&control_name.sym_id)?;
        let ty = scheme.body;
        let root = get_type_root(&self.type_checker_result.type_pool, ty);
        match &self.type_checker_result.type_pool[root].kind {
            TypeNodeKind::Fun { param_tys, .. } => {
                param_tys.get(index).copied().or_else(|| {
                    self.emit_mir_err(
                        MirLowerErrorKind::ControlParamIndexOOB {
                            ctrl_name: control_name.name.clone(),
                            param_cnt: param_tys.len(),
                            idx: index,
                        },
                        span.clone(),
                    );
                    None
                })
            }
            _ => {
                self.emit_mir_err(MirLowerErrorKind::ControlNotFunctionType(control_name.name.clone()), span);
                None
            }
        }
    }

    fn new(ty_ck_result: TypeCtx, hir_crate: HirCrate) -> Self {
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
            functions: vec![],
            extern_decls: vec![],
            statics: vec![],
            blocks: vec![],
            type_checker_result: ty_ck_result,
            hir: hir_crate,
            fun: None,
            current_block: MirBasicBlockId(0),
            current_stmts: vec![],
            decl_to_static: HashMap::new(),
            struct_field_map: HashMap::new(),
            adt_variant_map: HashMap::new(),
            decl_to_fun: HashMap::new(),
            control_map: HashMap::new(),
            resume_target: None,
            pub_fun_ids: vec![],
            bool_ty,
            uint8_ty,
            unit_ty,
            const_eval_counter: 0,
            current_blocks: None,
            diag_collector: Default::default(),
        }
    }

    fn lower(mut self) -> ((MirCrate, TypeCtx), DiagCollector) {
        self.lower_decls();
        (
            (MirCrate {
                id: self.hir.id,
                functions: self.functions,
                extern_decls: self.extern_decls,
                pub_decl_ids: self.pub_fun_ids,
                statics: self.statics,
                blocks: self.blocks,
            },
             self.type_checker_result
            ),
            self.diag_collector.into_inner()
        )
    }
    pub fn mir_lower_error_args_iter(err: MirLowerErrorKind) -> impl IntoIterator<Item=String> {
        use MirLowerErrorKind::*;
        let mut args = Vec::new();
        match err {
            FieldMissingInStruct(s) => args.push(s.clone()),
            VariantMissingInADTMap(s) => args.push(s.clone()),
            CtorNoMatchingPattern(s) => args.push(format!("{:?}", s)),
            ExprTypeMissing => args.push("".to_string()),
            IdentNoValueFound(s) => args.push(s.clone()),
            ControlNotFound(s) => args.push(s.clone()),
            ControlNotFunctionType(s) => args.push(s.clone()),
            ControlParamIndexOOB { ctrl_name, param_cnt, idx } => {
                args.push(ctrl_name.clone());
                args.push(param_cnt.to_string());
                args.push(idx.to_string());
            }
            FieldAccessNonStruct { field } => args.push(field.clone()),
            GenericInternal(s) => args.push(s.clone()),
            _ => {}
        }
        args
    }
}