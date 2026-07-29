use std::collections::{HashMap, HashSet, VecDeque};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::mir::*;
use leafc_coreapi::mir_lifetime_checker::MirLifetimeCheckerApi;
use leafc_coreapi::type_system::TypeCtx;
use leafc_coreapi::source::Span;

pub struct MirLifetimeChecker {
    mir: MirCrate,
    type_ctx: TypeCtx,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LocalState {
    Valid,
    Moved,
}

type BlockState = HashMap<LocalId, LocalState>;

impl MirLifetimeChecker {
    fn build_cfg(blocks: &[BasicBlock]) -> HashMap<BasicBlockId, Vec<BasicBlockId>> {
        let mut cfg: HashMap<BasicBlockId, Vec<BasicBlockId>> = HashMap::new();
        for (bid, block) in blocks.iter().enumerate() {
            let mut succs = Vec::new();
            match &block.terminator {
                TerminatorKind::Goto { target, .. } => succs.push(*target),
                TerminatorKind::SwitchInt { targets, default, .. } => {
                    for (_, t) in targets {
                        succs.push(*t);
                    }
                    succs.push(*default);
                }
                TerminatorKind::Call { target, .. } | TerminatorKind::CallByPtr { target, .. } => {
                    if let Some(t) = target {
                        succs.push(*t);
                    }
                }
                TerminatorKind::Return | TerminatorKind::Unreachable => {}
                _ => {}
            }
            cfg.insert(bid, succs);
        }
        cfg
    }

    fn check_rvalue(
        &self,
        rv: &Rvalue,
        span: &Span,
        state: &mut BlockState,
        errors: &mut Vec<DiagMsg>,
    ) {
        match rv {
            Rvalue::Move(place) | Rvalue::Copy(place) => {
                if let Some(local) = self.get_local(place) {
                    let entry = state.entry(local).or_insert(LocalState::Valid);
                    if *entry == LocalState::Moved {
                        errors.push(DiagMsg {
                            title: "lifetime error".into(),
                            msg: format!("use of moved value: local {}", local),
                            span: span.clone(),
                        });
                    }
                    if let Rvalue::Move(_) = rv {
                        *entry = LocalState::Moved;
                    }
                }
            }
            Rvalue::BinaryOp { left, right, .. } => {
                self.check_rvalue(left, span, state, errors);
                self.check_rvalue(right, span, state, errors);
            }
            Rvalue::UnaryOp { right, .. } => {
                self.check_rvalue(right, span, state, errors);
            }
            Rvalue::Cast(place, _) => {
                if let Some(local) = self.get_local(place) {
                    let entry = state.entry(local).or_insert(LocalState::Valid);
                    if *entry == LocalState::Moved {
                        errors.push(DiagMsg {
                            title: "lifetime error".into(),
                            msg: format!("use of moved value: local {}", local),
                            span: span.clone(),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn transfer_block(
        &self,
        block: &BasicBlock,
        fun: &MirFun,
        in_state: &BlockState,
    ) -> Result<BlockState, Vec<DiagMsg>> {
        let mut state = in_state.clone();
        let mut errors = Vec::new();

        for stmt in &block.statements {
            match &stmt.kind {
                MirStmtKind::Let { local, rvalue } => {
                    self.check_rvalue(rvalue, &stmt.span, &mut state, &mut errors);
                    state.insert(*local, LocalState::Valid);
                }
                MirStmtKind::Store { place, rvalue } => {
                    self.check_rvalue(rvalue, &stmt.span, &mut state, &mut errors);
                    if let Place::Local(dst) = place {
                        state.insert(*dst, LocalState::Valid);
                    }
                }
                MirStmtKind::Nop => {}
            }
        }

        match &block.terminator {
            TerminatorKind::Goto { block_args, .. } => {
                for arg in block_args {
                    self.check_rvalue(arg, &block.span, &mut state, &mut errors);
                }
            }
            TerminatorKind::SwitchInt { discriminant, .. } => {
                self.check_rvalue(discriminant, &block.span, &mut state, &mut errors);
            }
            TerminatorKind::Call { args, dest, .. }
            | TerminatorKind::CallByPtr { args, dest, .. } => {
                for arg in args {
                    self.check_rvalue(arg, &block.span, &mut state, &mut errors);
                }
                if let Place::Local(local) = dest {
                    state.insert(*local, LocalState::Valid);
                }
            }
            TerminatorKind::Return => {}
            TerminatorKind::Unreachable => {}
            _ => {}
        }

        if errors.is_empty() {
            Ok(state)
        } else {
            Err(errors)
        }
    }

    fn get_local(&self, place: &Place) -> Option<LocalId> {
        match place {
            Place::Local(id) => Some(*id),
            Place::Field { base, .. } | Place::Index { place: base, .. } | Place::EnumItem { place: base, .. } | Place::Deref(base) => {
                self.get_local(base)
            }
            Place::Static(_) => None,
        }
    }

    fn check_escape_in_block(&self, block: &BasicBlock, fun: &MirFun) -> Result<(), DiagMsg> {
        for stmt in &block.statements {
            match &stmt.kind {
                MirStmtKind::Let { rvalue, .. } | MirStmtKind::Store { rvalue, .. } => {
                    self.check_rvalue_escape(rvalue, fun, &stmt.span)?;
                }
                MirStmtKind::Nop => {}
            }
        }
        match &block.terminator {
            TerminatorKind::Call { args, .. } | TerminatorKind::CallByPtr { args, .. } => {
                for arg in args {
                    self.check_rvalue_escape(arg, fun, &block.span)?;
                }
            }
            TerminatorKind::Goto { block_args, .. } => {
                for arg in block_args {
                    self.check_rvalue_escape(arg, fun, &block.span)?;
                }
            }
            TerminatorKind::SwitchInt { discriminant, .. } => {
                self.check_rvalue_escape(discriminant, fun, &block.span)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn check_rvalue_escape(&self, rv: &Rvalue, fun: &MirFun, span: &Span) -> Result<(), DiagMsg> {
        match rv {
            Rvalue::TempRef(_) | Rvalue::TempRefMut(_) => {
                Err(DiagMsg {
                    title: "lifetime error".into(),
                    msg: "reference may escape".into(),
                    span: span.clone(),
                })
            }
            Rvalue::BinaryOp { left, right, .. } => {
                self.check_rvalue_escape(left, fun, span)?;
                self.check_rvalue_escape(right, fun, span)?;
                Ok(())
            }
            Rvalue::UnaryOp { right, .. } => {
                self.check_rvalue_escape(right, fun, span)
            }
            _ => Ok(()),
        }
    }

    fn analyze_function(&self, fun: &MirFun) -> Result<(), Vec<DiagMsg>> {
        let blocks = &self.mir.blocks;
        let mut errors = Vec::new();

        // check ref/ref mut
        for &bid in &fun.blocks {
            if let Err(e) = self.check_escape_in_block(&blocks[bid], fun) {
                errors.push(e);
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }

        // check move
        let cfg = Self::build_cfg(blocks);
        let mut in_states: HashMap<BasicBlockId, BlockState> = HashMap::new();
        let entry_bid = fun.blocks.first().copied().unwrap();
        let mut initial_state = BlockState::new();
        for (local_id, decl) in fun.local_decls.iter().enumerate() {
            initial_state.insert(local_id, LocalState::Valid);
        }
        in_states.insert(entry_bid, initial_state);

        let mut worklist: VecDeque<BasicBlockId> = fun.blocks.clone().into_iter().collect();
        let mut out_states: HashMap<BasicBlockId, BlockState> = HashMap::new();

        while let Some(bid) = worklist.pop_front() {
            let block = &blocks[bid];
            let in_state = in_states.entry(bid).or_default().clone();

            let out_state = match self.transfer_block(block, fun, &in_state) {
                Ok(state) => state,
                Err(e) => {
                    errors.extend(e);
                    continue;
                }
            };

            out_states.insert(bid, out_state.clone());

            if let Some(succs) = cfg.get(&bid) {
                for &succ in succs {
                    let mut changed = false;
                    let succ_in = in_states.entry(succ).or_default();
                    for (&local, s) in &out_state {
                        let entry = succ_in.entry(local).or_insert(LocalState::Valid);
                        if *s == LocalState::Moved && *entry == LocalState::Valid {
                            *entry = LocalState::Moved;
                            changed = true;
                        }
                    }
                    if changed {
                        worklist.push_back(succ);
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl MirLifetimeCheckerApi for MirLifetimeChecker {
    fn new(mir: MirCrate, type_ctx: TypeCtx) -> Self {
        Self { mir, type_ctx }
    }

    fn check(self) -> Result<(MirCrate, TypeCtx), DiagMsg> {
        let mut errors = Vec::new();
        for fun in &self.mir.functions {
            if fun.blocks.is_empty() {
                continue;
            }
            if let Err(fun_errors) = self.analyze_function(fun) {
                errors.extend(fun_errors);
            }
        }
        if errors.is_empty() {
            Ok((self.mir, self.type_ctx))
        } else {
            Err(errors.into_iter().next().unwrap())
        }
    }
}