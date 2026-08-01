use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::hir::HirDeclId;
use leafc_coreapi::lang_items::BuiltinType;
use leafc_coreapi::mir::*;
use leafc_coreapi::mir_mono::MirMonoApi;
use leafc_coreapi::type_system::{get_type_root, TyId, TypeNode};
use leafc_coreapi::type_system::{GenericParamDef, TypeCtx, TypeDef, TypeDefKind, TypeNodeKind};
use std::collections::{HashMap, HashSet};

pub struct MirMono {
    mir: MirCrate,
    type_checker_result: TypeCtx,
}

impl MirMono {
    fn intern_type(&mut self, kind: TypeNodeKind) -> TyId {
        self.type_checker_result.intern_type(kind)
    }
    fn get_type_pool(&self) -> &[TypeNode] {
        &self.type_checker_result.type_pool
    }

    fn subst_ty(&mut self, ty: TyId, mapping: &HashMap<TyId, TyId>) -> TyId {
        let root = get_type_root(self.get_type_pool(), ty);
        let kind = &self.type_checker_result.type_pool[root].kind.clone();
        if let Some(&replacement) = mapping.get(&root) {
            return replacement;
        }
        let new_kind = match kind {
            TypeNodeKind::Var => unreachable!(),
            TypeNodeKind::RigidVar => return root,
            TypeNodeKind::Builtin(_) | TypeNodeKind::Never => return ty,
            TypeNodeKind::Fun { param_tys, return_ty } => {
                let new_params: Vec<TyId> = param_tys.iter()
                    .map(|&t| self.subst_ty(t, mapping))
                    .collect();
                let new_ret = self.subst_ty(*return_ty, mapping);
                TypeNodeKind::Fun { param_tys: new_params, return_ty: new_ret }
            }
            TypeNodeKind::Tuple(elems) => {
                let new_elems: Vec<TyId> = elems.iter()
                    .map(|&e| self.subst_ty(e, mapping))
                    .collect();
                TypeNodeKind::Tuple(new_elems)
            }
            TypeNodeKind::Struct { decl_id, subst, field_tys } => {
                let new_subst: Vec<TyId> = subst.iter()
                    .map(|&s| self.subst_ty(s, mapping))
                    .collect();
                let new_fields: Vec<TyId> = field_tys.iter()
                    .map(|&f| self.subst_ty(f, mapping))
                    .collect();
                TypeNodeKind::Struct { decl_id: *decl_id, subst: new_subst, field_tys: new_fields }
            }
            TypeNodeKind::ADT { decl_id, subst, variants } => {
                let new_subst: Vec<TyId> = subst.iter()
                    .map(|&s| self.subst_ty(s, mapping))
                    .collect();
                let new_variants: Vec<Option<TyId>> = variants.iter()
                    .map(|opt| opt.map(|t| self.subst_ty(t, mapping)))
                    .collect();
                TypeNodeKind::ADT { decl_id: *decl_id, subst: new_subst, variants: new_variants }
            }
            TypeNodeKind::Ref(inner) => TypeNodeKind::Ref(self.subst_ty(*inner, mapping)),
            TypeNodeKind::MutRef(inner) => TypeNodeKind::MutRef(self.subst_ty(*inner, mapping)),
            TypeNodeKind::Share(inner) => TypeNodeKind::Share(self.subst_ty(*inner, mapping)),
            TypeNodeKind::RawPtr(inner) => TypeNodeKind::RawPtr(self.subst_ty(*inner, mapping)),
        };

        self.intern_type(new_kind)
    }

    fn match_ty(&self, pattern: TyId, concrete: TyId, mapping: &mut HashMap<TyId, TyId>) -> bool {
        let pool = self.get_type_pool();
        let p_root = get_type_root(pool, pattern);
        let c_root = get_type_root(pool, concrete);
        if let Some(&mapped) = mapping.get(&p_root) {
            return mapped == c_root;
        }
        match &pool[p_root].kind {
            TypeNodeKind::Var => {
                mapping.insert(p_root, c_root);
                true
            }
            _ => {
                let c_kind = &pool[c_root].kind;
                match (&pool[p_root].kind, c_kind) {
                    (TypeNodeKind::Builtin(a), TypeNodeKind::Builtin(b)) => a == b,
                    (TypeNodeKind::Never, TypeNodeKind::Never) => true,
                    (TypeNodeKind::Tuple(pe), TypeNodeKind::Tuple(ce)) => {
                        pe.len() == ce.len() && pe.iter().zip(ce).all(|(p, c)| self.match_ty(*p, *c, mapping))
                    }
                    (TypeNodeKind::Struct { decl_id: pd, subst: ps, .. }, TypeNodeKind::Struct { decl_id: cd, subst: cs, .. }) if pd == cd => {
                        ps.len() == cs.len() && ps.iter().zip(cs).all(|(p, c)| self.match_ty(*p, *c, mapping))
                    }
                    (TypeNodeKind::ADT { decl_id: pd, subst: ps, .. }, TypeNodeKind::ADT { decl_id: cd, subst: cs, .. }) if pd == cd => {
                        ps.len() == cs.len() && ps.iter().zip(cs).all(|(p, c)| self.match_ty(*p, *c, mapping))
                    }
                    (TypeNodeKind::Ref(a), TypeNodeKind::Ref(b)) |
                    (TypeNodeKind::MutRef(a), TypeNodeKind::MutRef(b)) |
                    (TypeNodeKind::Share(a), TypeNodeKind::Share(b)) |
                    (TypeNodeKind::RawPtr(a), TypeNodeKind::RawPtr(b)) => self.match_ty(*a, *b, mapping),
                    _ => false,
                }
            }
        }
    }

    fn map_rvalue_ty(&mut self, rvalue: &mut Rvalue, mapping: &HashMap<TyId, TyId>) {
        match rvalue {
            Rvalue::Cast(_, target_ty) => {
                *target_ty = self.subst_ty(*target_ty, mapping);
            }
            Rvalue::BinaryOp { left, right, .. } => {
                self.map_rvalue_ty(left, mapping);
                self.map_rvalue_ty(right, mapping);
            }
            Rvalue::UnaryOp { right, .. } => {
                self.map_rvalue_ty(right, mapping);
            }
            Rvalue::Index { .. } | Rvalue::Field { .. } => {}
            Rvalue::TempRef(_) | Rvalue::TempRefMut(_) => {}
            Rvalue::GetFunPtr(_) => {}
            Rvalue::BuildStruct(fields) => {
                for field in fields {
                    self.map_rvalue_ty(field, mapping);
                }
            }
            Rvalue::Tuple(elements) => {
                for elem in elements {
                    self.map_rvalue_ty(elem, mapping);
                }
            }
            Rvalue::Variant(_, inner) => {
                self.map_rvalue_ty(inner, mapping);
            }
            Rvalue::Len(_) | Rvalue::Tag(_) => {}
            Rvalue::Copy(_) | Rvalue::Move(_) => {}
            Rvalue::Constant(_) => {}
            Rvalue::GcNewObject(inner) => self.map_rvalue_ty(inner, mapping),
            Rvalue::GcObjectRef(inner) => self.map_rvalue_ty(inner, mapping),
        }
    }

    fn map_terminator_ty(&mut self, terminator: &mut TerminatorKind, mapping: &HashMap<TyId, TyId>) {
        match terminator {
            TerminatorKind::Goto { block_args, .. } => {
                for arg in block_args {
                    self.map_rvalue_ty(arg, mapping);
                }
            }
            TerminatorKind::SwitchInt { discriminant, .. } => {
                self.map_rvalue_ty(discriminant, mapping);
            }
            TerminatorKind::Call { args, .. } | TerminatorKind::CallByPtr { args, .. } => {
                for arg in args {
                    self.map_rvalue_ty(arg, mapping);
                }
            }
            TerminatorKind::Raise { args, .. } => {
                for arg in args {
                    self.map_rvalue_ty(arg, mapping);
                }
            }
            _ => {}
        }
    }

    fn rvalue_ty(&self, rv: &Rvalue, fun: &MirFun) -> TyId {
        match rv {
            Rvalue::Move(Place::Local(id)) | Rvalue::Copy(Place::Local(id)) => {
                let raw_ty = fun.local_decls[*id].ty;
                get_type_root(self.get_type_pool(), raw_ty)
            }
            _ => panic!("unsupported rvalue in call arg"),
        }
    }

    fn clone_fun_with_blocks(
        fun: &MirFun,
        old_blocks: &[BasicBlock],
        new_blocks: &mut Vec<BasicBlock>,
        old_to_new_block: &mut HashMap<BasicBlockId, BasicBlockId>,
        global_map: &mut HashMap<BasicBlockId, BasicBlockId>,
    ) -> MirFun {
        let mut new_fun = fun.clone();
        for &old_bid in &fun.blocks {
            let new_bid = new_blocks.len();
            let mut new_block = old_blocks[old_bid].clone();
            new_blocks.push(new_block);
            old_to_new_block.insert(old_bid, new_bid);
            global_map.insert(old_bid, new_bid); // 记录全局映射
        }
        for &old_bid in &fun.blocks {
            let new_bid = old_to_new_block[&old_bid];
            let block = &mut new_blocks[new_bid];
            Self::remap_block_terminator(block, old_to_new_block);
        }
        new_fun.blocks = fun.blocks.iter()
            .map(|old_bid| old_to_new_block[old_bid])
            .collect();
        new_fun
    }

    fn remap_block_terminator(
        block: &mut BasicBlock,
        old_to_new: &HashMap<BasicBlockId, BasicBlockId>,
    ) {
        let map = |id: &mut BasicBlockId| { *id = old_to_new[id]; };
        match &mut block.terminator {
            TerminatorKind::Goto { target, .. } => map(target),
            TerminatorKind::SwitchInt { targets, default, .. } => {
                for (_, target) in targets { map(target); }
                map(default);
            }
            TerminatorKind::Call { target, .. } => { if let Some(t) = target { map(t); } }
            TerminatorKind::CallByPtr { target, .. } => { if let Some(t) = target { map(t); } }
            TerminatorKind::Resume { target, .. } => map(target),
            TerminatorKind::InstallHandler { handler_block, next, .. } => {
                map(handler_block);
                map(next);
            }
            _ => {}
        }
    }

    fn subst_ty_in_fun(&mut self, fun: &mut MirFun, mapping: &HashMap<TyId, TyId>) {
        for param in &mut fun.signature.params {
            *param = self.subst_ty(*param, mapping);
        }
        fun.signature.return_ty = self.subst_ty(fun.signature.return_ty, mapping);
        for decl in &mut fun.local_decls {
            decl.ty = self.subst_ty(decl.ty, mapping);
        }
    }

    fn subst_ty_in_block(&mut self, block: &mut BasicBlock, mapping: &HashMap<TyId, TyId>) {
        for stmt in &mut block.statements {
            match &mut stmt.kind {
                MirStmtKind::Let { rvalue, .. } => self.map_rvalue_ty(rvalue, mapping),
                MirStmtKind::Store { rvalue, .. } => self.map_rvalue_ty(rvalue, mapping),
                MirStmtKind::Nop => {}
            }
        }
        self.map_terminator_ty(&mut block.terminator, mapping);
    }

    fn is_concrete_ty(&self, ty: TyId) -> bool {
        let pool = self.get_type_pool();
        let root = get_type_root(pool, ty);
        match &pool[root].kind {
            TypeNodeKind::Var => false,
            TypeNodeKind::Struct { subst, .. } | TypeNodeKind::ADT { subst, .. } => {
                subst.iter().all(|&s| self.is_concrete_ty(s))
            }
            TypeNodeKind::Ref(inner) | TypeNodeKind::MutRef(inner) | TypeNodeKind::Share(inner) => {
                self.is_concrete_ty(*inner)
            }
            TypeNodeKind::Fun { param_tys, return_ty } => {
                param_tys.iter().all(|&p| self.is_concrete_ty(p)) && self.is_concrete_ty(*return_ty)
            }
            TypeNodeKind::Tuple(elems) => elems.iter().all(|&e| self.is_concrete_ty(e)),
            _ => true,
        }
    }

    fn collect_concrete_adt_tys(
        &self,
        functions: &[MirFun],
        blocks: &[BasicBlock],
        statics: &[StaticDecl],
    ) -> HashSet<(HirDeclId, Vec<TyId>)> {
        let pool = self.get_type_pool();
        let mut result = HashSet::new();

        let mut process_ty = |ty: TyId| {
            let root = get_type_root(pool, ty);
            match &pool[root].kind {
                TypeNodeKind::Struct { decl_id, subst, .. }
                | TypeNodeKind::ADT { decl_id, subst, .. } => {
                    if subst.iter().all(|&s| self.is_concrete_ty(s)) {
                        result.insert((*decl_id, subst.clone()));
                    }
                }
                _ => {}
            }
        };

        for fun in functions {
            for &pty in &fun.signature.params { process_ty(pty); }
            process_ty(fun.signature.return_ty);
            for decl in &fun.local_decls { process_ty(decl.ty); }
        }

        for stat in statics { process_ty(stat.ty); }

        for block in blocks {
            for stmt in &block.statements {
                match &stmt.kind {
                    MirStmtKind::Let { rvalue, .. } | MirStmtKind::Store { rvalue, .. } => {
                        self.collect_tys_in_rvalue(rvalue, &mut process_ty);
                    }
                    _ => {}
                }
            }
            self.collect_tys_in_terminator(&block.terminator, &mut process_ty);
        }

        result
    }

    fn collect_tys_in_rvalue(&self, rv: &Rvalue, f: &mut impl FnMut(TyId)) {
        match rv {
            Rvalue::Cast(_, target_ty) => f(*target_ty),
            Rvalue::BinaryOp { left, right, .. } => {
                self.collect_tys_in_rvalue(left, f);
                self.collect_tys_in_rvalue(right, f);
            }
            Rvalue::UnaryOp { right, .. } => self.collect_tys_in_rvalue(right, f),
            Rvalue::BuildStruct(fields) | Rvalue::Tuple(fields) => {
                for field in fields { self.collect_tys_in_rvalue(field, f); }
            }
            Rvalue::Variant(_, inner) => self.collect_tys_in_rvalue(inner, f),
            _ => {}
        }
    }

    fn collect_tys_in_terminator(&self, term: &TerminatorKind, f: &mut impl FnMut(TyId)) {
        match term {
            TerminatorKind::Goto { block_args, .. } => {
                for arg in block_args { self.collect_tys_in_rvalue(arg, f); }
            }
            TerminatorKind::SwitchInt { discriminant, .. } => {
                self.collect_tys_in_rvalue(discriminant, f);
            }
            TerminatorKind::Call { args, .. } | TerminatorKind::CallByPtr { args, .. } => {
                for arg in args { self.collect_tys_in_rvalue(arg, f); }
            }
            TerminatorKind::Raise { args, .. } => {
                for arg in args { self.collect_tys_in_rvalue(arg, f); }
            }
            _ => {}
        }
    }

    fn subst_type_def(&self, generic_def: &TypeDef, subst_map: &HashMap<TyId, TyId>) -> TypeDef {
        let pool = self.get_type_pool();
        let subst_ty = |ty: &TyId| -> TyId {
            let root = get_type_root(pool, *ty);
            subst_map.get(&root).copied().unwrap_or(*ty)
        };

        let mut new_def = generic_def.clone();
        match &mut new_def.kind {
            TypeDefKind::Struct { fields } => {
                for f in fields { f.ty = subst_ty(&f.ty); }
            }
            TypeDefKind::Enum { variants } => {
                for v in variants {
                    for f in &mut v.fields { f.ty = subst_ty(&f.ty); }
                }
            }
        }
        new_def.name = self.mangled_type_name(&generic_def.name, &generic_def.generics, subst_map);
        new_def.generics.clear();
        new_def
    }

    fn mangled_type_name(&self, base: &str, generics: &[GenericParamDef], subst: &HashMap<TyId, TyId>) -> String {
        let suffix: Vec<String> = generics.iter()
            .map(|gp| {
                let concrete = subst.get(&gp.def_id).copied().unwrap_or(gp.def_id);
                self.ty_to_string(concrete)
            })
            .collect();
        if suffix.is_empty() { base.to_string() } else { format!("{}_{}", base, suffix.join("_")) }
    }

    fn ty_to_string(&self, ty: TyId) -> String {
        let pool = self.get_type_pool();
        let root = get_type_root(pool, ty);
        match &pool[root].kind {
            TypeNodeKind::Builtin(b) => match b {
                BuiltinType::I8 => "int8_t",
                BuiltinType::I16 => "int16_t",
                BuiltinType::I32 => "int32_t",
                BuiltinType::I64 => "int64_t",
                BuiltinType::U8 => "uint8_t",
                BuiltinType::U16 => "uint16_t",
                BuiltinType::U32 => "uint32_t",
                BuiltinType::U64 => "uint64_t",
                BuiltinType::F32 => "float",
                BuiltinType::F64 => "double",
                BuiltinType::Bool => "bool",
                _ => "unknown",
            }.to_string(),
            TypeNodeKind::Struct { decl_id, subst, .. } | TypeNodeKind::ADT { decl_id, subst, .. } => {
                if let Some(def) = self.type_checker_result.generic_type_defs.get(decl_id) {
                    let suffix = subst.iter().map(|&s| self.ty_to_string(s)).collect::<Vec<_>>().join("_");
                    if suffix.is_empty() { def.name.clone() } else { format!("{}_{}", def.name, suffix) }
                } else {
                    format!("unknown_adt_{}", decl_id)
                }
            }
            _ => format!("ty{}", root),
        }
    }
}

impl MirMonoApi for MirMono {
    fn new(mir: MirCrate, type_checker_result: TypeCtx) -> Self {
        Self { mir, type_checker_result }
    }

    fn mono(mut self) -> Result<(MirCrate, TypeCtx), DiagMsg> {
        let functions = std::mem::take(&mut self.mir.functions);
        let old_blocks = std::mem::take(&mut self.mir.blocks);
        let mut block_to_fun: HashMap<BasicBlockId, &MirFun> = HashMap::new();
        for fun in &functions {
            for &bid in &fun.blocks { block_to_fun.insert(bid, fun); }
        }

        let mut generic_calls: Vec<(BasicBlockId, FunId, Vec<TyId>)> = Vec::new();
        let mut inst_map: HashMap<FunId, Vec<Vec<TyId>>> = HashMap::new();

        for (bid, block) in old_blocks.iter().enumerate() {
            if let TerminatorKind::Call { func, args, dest, .. } = &block.terminator {
                let callee = &functions[*func];
                if !callee.generic_params.is_empty() {
                    let caller_fun = block_to_fun[&bid];
                    let mut mapping: HashMap<TyId, TyId> = HashMap::new();
                    for (rv, &param_ty) in args.iter().zip(callee.signature.params.iter()) {
                        let arg_ty = self.rvalue_ty(rv, caller_fun);
                        if !self.match_ty(param_ty, arg_ty, &mut mapping) {
                            return Err(DiagMsg {
                                title: "monomorphization error".into(),
                                msg: format!("argument type mismatch in call to {}", callee.name),
                                span: block.span.clone(),
                            });
                        }
                    }
                    if let Place::Local(dest_id) = dest {
                        let ret_ty = get_type_root(self.get_type_pool(), caller_fun.local_decls[*dest_id].ty);
                        if !self.match_ty(callee.signature.return_ty, ret_ty, &mut mapping) {
                            return Err(DiagMsg {
                                title: "monomorphization error".into(),
                                msg: format!("return type mismatch in call to {}", callee.name),
                                span: block.span.clone(),
                            });
                        }
                    }
                    let concrete_tys: Vec<TyId> = callee.generic_params.iter()
                        .map(|gp| {
                            let root = get_type_root(self.get_type_pool(), *gp);
                            mapping.get(&root).copied().unwrap_or(root)
                        })
                        .collect();

                    generic_calls.push((bid, *func, concrete_tys.clone()));
                    inst_map.entry(*func).or_default().push(concrete_tys);
                }
            }
        }

        let mut new_functions: Vec<MirFun> = Vec::new();
        let mut new_blocks: Vec<BasicBlock> = Vec::new();
        let mut old_to_new_fun: HashMap<FunId, FunId> = HashMap::new();
        let mut old_to_new_instances: HashMap<FunId, HashMap<Vec<TyId>, FunId>> = HashMap::new();
        let mut global_block_map: HashMap<BasicBlockId, BasicBlockId> = HashMap::new();

        for (old_fid, fun) in functions.iter().enumerate() {
            if !fun.generic_params.is_empty() { continue; }
            let mut local_map = HashMap::new();
            let new_fun = Self::clone_fun_with_blocks(fun, &old_blocks, &mut new_blocks, &mut local_map, &mut global_block_map);
            let new_fid = new_functions.len();
            new_functions.push(new_fun);
            old_to_new_fun.insert(old_fid, new_fid);
        }

        for (old_fid, fun) in functions.iter().enumerate() {
            if fun.generic_params.is_empty() { continue; }
            let instances = inst_map.get(&old_fid).cloned().unwrap_or_default();
            let mut instance_map: HashMap<Vec<TyId>, FunId> = HashMap::new();
            for concrete_tys in &instances {
                let mut ty_subst: HashMap<TyId, TyId> = HashMap::new();
                for (gp, ct) in fun.generic_params.iter().zip(concrete_tys) {
                    ty_subst.insert(*gp, *ct);
                }
                let mut local_map = HashMap::new();
                let mut new_fun = Self::clone_fun_with_blocks(fun, &old_blocks, &mut new_blocks, &mut local_map, &mut global_block_map);
                new_fun.generic_params.clear();
                self.subst_ty_in_fun(&mut new_fun, &ty_subst);
                for &new_bid in &new_fun.blocks {
                    let block = &mut new_blocks[new_bid];
                    self.subst_ty_in_block(block, &ty_subst);
                }
                let new_fid = new_functions.len();
                new_functions.push(new_fun);
                instance_map.insert(concrete_tys.clone(), new_fid);
            }
            old_to_new_instances.insert(old_fid, instance_map);
        }

        for (old_bid, fun_id, concrete_tys) in &generic_calls {
            let new_bid = global_block_map[old_bid];
            if let Some(instance_map) = old_to_new_instances.get(fun_id) {
                let new_func = instance_map.get(concrete_tys).expect("monomorphization instance not found");
                if let TerminatorKind::Call { func, .. } = &mut new_blocks[new_bid].terminator {
                    *func = *new_func;
                }
            } else if let Some(&new_fid) = old_to_new_fun.get(fun_id) {
                if let TerminatorKind::Call { func, .. } = &mut new_blocks[new_bid].terminator {
                    *func = new_fid;
                }
            }
        }

        self.mir.functions = new_functions;
        self.mir.blocks = new_blocks;

        let concrete_adts = self.collect_concrete_adt_tys(&self.mir.functions, &self.mir.blocks, &self.mir.statics);
        for (decl_id, subst) in &concrete_adts {
            if let Some(generic_def) = self.type_checker_result.generic_type_defs.get(decl_id).cloned() {
                let mut subst_map = HashMap::new();
                for (gp, &ct) in generic_def.generics.iter().zip(subst.iter()) {
                    subst_map.insert(gp.def_id, ct);
                }
                let concrete_def = self.subst_type_def(&generic_def, &subst_map);
                self.type_checker_result.concrete_type_defs.insert((*decl_id, subst.clone()), concrete_def);
            }
        }

        Ok((self.mir, self.type_checker_result))
    }}