use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use leaf_coreapi::diagnose::{CompileTimeErrorKind, DiagCollector, ErrorKind, LocalizedMessage, MirLifetimeCheckerErrorKind, MsgKind};
use leaf_coreapi::mir::*;
use leaf_coreapi::type_ctx::TypeCtx;
use leaf_coreapi::source::Span;

pub struct MirLifetimeChecker {
    mir: MirCrate,
    type_ctx: TypeCtx,
    diag_collector: RefCell<DiagCollector>
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LocalState {
    Valid,
    Moved,
}

type BlockState = HashMap<MirLocalId, LocalState>;

impl MirLifetimeChecker {
    fn build_cfg(blocks: &[BasicBlock]) -> HashMap<MirBasicBlockId, Vec<MirBasicBlockId>> {
        let mut cfg: HashMap<MirBasicBlockId, Vec<MirBasicBlockId>> = HashMap::new();
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
            cfg.insert(MirBasicBlockId(bid), succs);
        }
        cfg
    }

    fn check_rvalue(
        &self,
        rv: &Rvalue,
        span: &Span,
        state: &mut BlockState,
    ) -> bool {
        let mut found_error = false;
        match rv {
            Rvalue::Move(place) | Rvalue::Copy(place) => {
                if let Some(local) = self.get_local(place) {
                    let entry = state.entry(local).or_insert(LocalState::Valid);
                    if *entry == LocalState::Moved {
                        self.diag_collector.borrow_mut().add_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirLifetimeCheckerError(MirLifetimeCheckerErrorKind::Lifetime)),
                            span.clone(),
                            LocalizedMessage::new(MsgKind::MirLifetimeGeneric, [format!("use of moved value: local {}", local.0)]),
                        );
                        found_error = true;
                    }
                    if let Rvalue::Move(_) = rv {
                        *entry = LocalState::Moved;
                    }
                }
            }
            Rvalue::BinaryOp { left, right, .. } => {
                let l = self.check_rvalue(left, span, state);
                let r = self.check_rvalue(right, span, state);
                found_error = l || r;
            }
            Rvalue::UnaryOp { right, .. } => {
                found_error = self.check_rvalue(right, span, state);
            }
            Rvalue::Cast(place, _) => {
                if let Some(local) = self.get_local(place) {
                    let entry = state.entry(local).or_insert(LocalState::Valid);
                    if *entry == LocalState::Moved {
                        self.diag_collector.borrow_mut().add_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirLifetimeCheckerError(MirLifetimeCheckerErrorKind::Lifetime)),
                            span.clone(),
                            LocalizedMessage::new(MsgKind::MirLifetimeGeneric, [format!("use of moved value: local {}", local.0)]),
                        );
                        found_error = true;
                    }
                }
            }
            _ => {}
        }
        found_error
    }

    fn transfer_block(
        &self,
        bid: MirBasicBlockId,
        fun: &MirFun,
        in_state: &BlockState,
    ) -> Result<BlockState, ()> {
        let block = &self.mir.blocks[bid.0];
        let mut state = in_state.clone();
        let mut had_error = false;

        for stmt in &block.statements {
            match &stmt.kind {
                MirStmtKind::Let { local, rvalue } => {
                    if self.check_rvalue(rvalue, &stmt.span, &mut state) {
                        had_error = true;
                    }
                    state.insert(*local, LocalState::Valid);
                }
                MirStmtKind::Store { place, rvalue } => {
                    if self.check_rvalue(rvalue, &stmt.span, &mut state) {
                        had_error = true;
                    }
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
                    if self.check_rvalue(arg, &block.span, &mut state) {
                        had_error = true;
                    }
                }
            }
            TerminatorKind::SwitchInt { discriminant, .. } => {
                if self.check_rvalue(discriminant, &block.span, &mut state) {
                    had_error = true;
                }
            }
            TerminatorKind::Call { args, dest, .. }
            | TerminatorKind::CallByPtr { args, dest, .. } => {
                for arg in args {
                    if self.check_rvalue(arg, &block.span, &mut state) {
                        had_error = true;
                    }
                }
                if let Place::Local(local) = dest {
                    state.insert(*local, LocalState::Valid);
                }
            }
            TerminatorKind::Return => {}
            TerminatorKind::Unreachable => {}
            _ => {}
        }

        if had_error {
            Err(())
        } else {
            Ok(state)
        }
    }

    fn get_local(&self, place: &Place) -> Option<MirLocalId> {
        match place {
            Place::Local(id) => Some(*id),
            Place::Field { base, .. } | Place::Index { place: base, .. } | Place::EnumItem { place: base, .. } | Place::Deref(base) => {
                self.get_local(base)
            }
            Place::Static(_) => None,
        }
    }

    fn check_escape_in_block(&self, bid: MirBasicBlockId, fun: &MirFun) -> Result<(), ()> {
        let block = &self.mir.blocks[bid.0];
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

    fn check_rvalue_escape(
        &self,
        rv: &Rvalue,
        fun: &MirFun,
        span: &Span
    ) -> Result<(), ()> {
        match rv {
            Rvalue::Ref(_) | Rvalue::RefMut(_) => {
                self.diag_collector.borrow_mut().add_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirLifetimeCheckerError(MirLifetimeCheckerErrorKind::Lifetime)),
                    span.clone(),
                    LocalizedMessage::new(MsgKind::MirLifetimeGeneric, ["reference may escape"]),
                );
                Err(())
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

    fn analyze_function(&self, fun: &MirFun) -> Result<(), ()> {

        // check ref/ref mut
        for &bid in &fun.blocks {
            self.check_escape_in_block(bid, fun)?;
        }

        // check move
        let cfg = Self::build_cfg(&self.mir.blocks);
        let mut in_states: HashMap<MirBasicBlockId, BlockState> = HashMap::new();
        let entry_bid = fun.blocks.first().copied().unwrap();
        let mut initial_state = BlockState::new();
        for (local_id, decl) in fun.local_decls.iter().enumerate() {
            initial_state.insert(MirLocalId(local_id), LocalState::Valid);
        }
        in_states.insert(entry_bid, initial_state);

        let mut worklist: VecDeque<MirBasicBlockId> = fun.blocks.clone().into_iter().collect();
        let mut out_states: HashMap<MirBasicBlockId, BlockState> = HashMap::new();
        let mut had_error = false;

        while let Some(bid) = worklist.pop_front() {
            let in_state = in_states.entry(bid).or_default().clone();

            let out_state = match self.transfer_block(bid, fun, &in_state) {
                Ok(state) => state,
                Err(()) => {
                    had_error = true;
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

        if had_error {
            Err(())
        } else {
            Ok(())
        }
    }

    fn new(mir: MirCrate, type_ctx: TypeCtx) -> Self {
        Self {
            mir,
            type_ctx,
            diag_collector: Default::default()
        }
    }

    fn check(mut self) -> ((MirCrate, TypeCtx), DiagCollector) {
        for fun in &self.mir.functions {
            if fun.blocks.is_empty() {
                continue;
            }
            self.analyze_function(fun);
        }
        ((self.mir, self.type_ctx), self.diag_collector.into_inner())
    }
}