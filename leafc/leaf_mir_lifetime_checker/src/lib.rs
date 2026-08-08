use std::collections::{HashMap, HashSet, VecDeque};
use leaf_coreapi::diagnose::{CompileTimeErrorKind, DiagCtx, ErrorKind, LocalizedMessage, MirLifetimeCheckerErrorKind, MsgKind};
use leaf_coreapi::mir::*;
use leaf_coreapi::mir_lifetime_checker::MirLifetimeCheckerApi;
use leaf_coreapi::type_ctx::TypeCtx;
use leaf_coreapi::source::Span;

pub struct MirLifetimeChecker<'a> {
    pub diag: &'a mut DiagCtx,
    mir: MirCrate,
    type_ctx: TypeCtx,
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
            cfg.insert(bid, succs);
        }
        cfg
    }

    fn check_rvalue(
        rv: &Rvalue,
        span: &Span,
        state: &mut BlockState,
        diag: &mut DiagCtx,
    ) -> bool {
        let mut found_error = false;
        match rv {
            Rvalue::Move(place) | Rvalue::Copy(place) => {
                if let Some(local) = Self::get_local(place) {
                    let entry = state.entry(local).or_insert(LocalState::Valid);
                    if *entry == LocalState::Moved {
                        diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirLifetimeCheckerError(MirLifetimeCheckerErrorKind::Lifetime)),
                            span.clone(),
                            LocalizedMessage::new(MsgKind::MirLifetimeGeneric, [format!("use of moved value: local {}", local)]),
                        );
                        found_error = true;
                    }
                    if let Rvalue::Move(_) = rv {
                        *entry = LocalState::Moved;
                    }
                }
            }
            Rvalue::BinaryOp { left, right, .. } => {
                let l = Self::check_rvalue(left, span, state, diag);
                let r = Self::check_rvalue(right, span, state, diag);
                found_error = l || r;
            }
            Rvalue::UnaryOp { right, .. } => {
                found_error = Self::check_rvalue(right, span, state, diag);
            }
            Rvalue::Cast(place, _) => {
                if let Some(local) = Self::get_local(place) {
                    let entry = state.entry(local).or_insert(LocalState::Valid);
                    if *entry == LocalState::Moved {
                        diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirLifetimeCheckerError(MirLifetimeCheckerErrorKind::Lifetime)),
                            span.clone(),
                            LocalizedMessage::new(MsgKind::MirLifetimeGeneric, [format!("use of moved value: local {}", local)]),
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
        block: &BasicBlock,
        fun: &MirFun,
        in_state: &BlockState,
        diag: &mut DiagCtx,
    ) -> Result<BlockState, ()> {
        let mut state = in_state.clone();
        let mut had_error = false;

        for stmt in &block.statements {
            match &stmt.kind {
                MirStmtKind::Let { local, rvalue } => {
                    if Self::check_rvalue(rvalue, &stmt.span, &mut state, diag) {
                        had_error = true;
                    }
                    state.insert(*local, LocalState::Valid);
                }
                MirStmtKind::Store { place, rvalue } => {
                    if Self::check_rvalue(rvalue, &stmt.span, &mut state, diag) {
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
                    if Self::check_rvalue(arg, &block.span, &mut state, diag) {
                        had_error = true;
                    }
                }
            }
            TerminatorKind::SwitchInt { discriminant, .. } => {
                if Self::check_rvalue(discriminant, &block.span, &mut state, diag) {
                    had_error = true;
                }
            }
            TerminatorKind::Call { args, dest, .. }
            | TerminatorKind::CallByPtr { args, dest, .. } => {
                for arg in args {
                    if Self::check_rvalue(arg, &block.span, &mut state, diag) {
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

    fn check_escape_in_block(block: &BasicBlock, fun: &MirFun, diag: &mut DiagCtx) -> Result<(), ()> {
        for stmt in &block.statements {
            match &stmt.kind {
                MirStmtKind::Let { rvalue, .. } | MirStmtKind::Store { rvalue, .. } => {
                    Self::check_rvalue_escape(rvalue, fun, &stmt.span, diag)?;
                }
                MirStmtKind::Nop => {}
            }
        }
        match &block.terminator {
            TerminatorKind::Call { args, .. } | TerminatorKind::CallByPtr { args, .. } => {
                for arg in args {
                    Self::check_rvalue_escape(arg, fun, &block.span, diag)?;
                }
            }
            TerminatorKind::Goto { block_args, .. } => {
                for arg in block_args {
                    Self::check_rvalue_escape(arg, fun, &block.span, diag)?;
                }
            }
            TerminatorKind::SwitchInt { discriminant, .. } => {
                Self::check_rvalue_escape(discriminant, fun, &block.span, diag)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn check_rvalue_escape(rv: &Rvalue, fun: &MirFun, span: &Span, diag: &mut DiagCtx) -> Result<(), ()> {
        match rv {
            Rvalue::Ref(_) | Rvalue::RefMut(_) => {
                diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirLifetimeCheckerError(MirLifetimeCheckerErrorKind::Lifetime)),
                    span.clone(),
                    LocalizedMessage::new(MsgKind::MirLifetimeGeneric, ["reference may escape"]),
                );
                Err(())
            }
            Rvalue::BinaryOp { left, right, .. } => {
                Self::check_rvalue_escape(left, fun, span, diag)?;
                Self::check_rvalue_escape(right, fun, span, diag)?;
                Ok(())
            }
            Rvalue::UnaryOp { right, .. } => {
                Self::check_rvalue_escape(right, fun, span, diag)
            }
            _ => Ok(()),
        }
    }

    fn analyze_function(&mut self, fun: &MirFun) -> Result<(), ()> {
        let blocks = &self.mir.blocks;

        // check ref/ref mut
        for &bid in &fun.blocks {
            Self::check_escape_in_block(&blocks[bid], fun, &mut self.diag)?;
        }

        // check move
        let cfg = Self::build_cfg(blocks);
        let mut in_states: HashMap<MirBasicBlockId, BlockState> = HashMap::new();
        let entry_bid = fun.blocks.first().copied().unwrap();
        let mut initial_state = BlockState::new();
        for (local_id, decl) in fun.local_decls.iter().enumerate() {
            initial_state.insert(local_id, LocalState::Valid);
        }
        in_states.insert(entry_bid, initial_state);

        let mut worklist: VecDeque<MirBasicBlockId> = fun.blocks.clone().into_iter().collect();
        let mut out_states: HashMap<MirBasicBlockId, BlockState> = HashMap::new();
        let mut had_error = false;

        while let Some(bid) = worklist.pop_front() {
            let block = &blocks[bid];
            let in_state = in_states.entry(bid).or_default().clone();

            let out_state = match Self::transfer_block(block, fun, &in_state, &mut self.diag) {
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
}

impl<'a> MirLifetimeCheckerApi<'a> for MirLifetimeChecker<'a> {
    fn new(mir: MirCrate, type_ctx: TypeCtx, diag: &'a mut DiagCtx) -> Self {
        Self { diag, mir, type_ctx }
    }

    fn check(self) -> Result<(MirCrate, TypeCtx), ()> {
        let mut this = self;
        for fun in &this.mir.functions {
            if fun.blocks.is_empty() {
                continue;
            }
            this.analyze_function(fun)?;
        }
        Ok((this.mir, this.type_ctx))
    }
}