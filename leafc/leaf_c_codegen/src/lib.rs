use leaf_coreapi::diagnose::DiagCtx;
use leaf_coreapi::lang_items::BuiltinType;
use leaf_coreapi::mir::*;
use leaf_coreapi::type_ctx::{GenericParamDef, TypeCtx, TypeDef, TypeDefKind};
use leaf_coreapi::type_ctx::{TyId, TypeNodeKind, get_type_root};
use std::collections::{HashMap, HashSet};

pub struct CCodeGen<'a> {
    pub diag: &'a mut DiagCtx,
    mono_mir: MirCrate,
    type_checker_result: TypeCtx,
}

impl CCodeGen {
    fn terminator_successors(terminator_kind: TerminatorKind) -> Vec<MirBasicBlockId> {
        match terminator_kind {
            TerminatorKind::Goto { target, .. } => vec![target],
            TerminatorKind::Call { target, .. } | TerminatorKind::CallByPtr { target, .. } => {
                target.iter().cloned().collect()
            }
            TerminatorKind::SwitchInt {
                targets, default, ..
            } => {
                let mut v: Vec<_> = targets.iter().map(|(_, t)| *t).collect();
                v.push(default);
                v
            }
            TerminatorKind::Resume { target, .. } => vec![target],
            _ => vec![],
        }
    }

    fn ty_to_mangle(&self, ty_id: TyId) -> String {
        let pool = &self.type_checker_result.type_pool;
        let root = get_type_root(pool, ty_id);
        match &pool[root].kind {
            TypeNodeKind::Builtin(b) => match b {
                BuiltinType::I8 => "I8".into(),
                BuiltinType::I16 => "I16".into(),
                BuiltinType::I32 => "I32".into(),
                BuiltinType::I64 => "I64".into(),
                BuiltinType::U8 => "U8".into(),
                BuiltinType::U16 => "U16".into(),
                BuiltinType::U32 => "U32".into(),
                BuiltinType::U64 => "U64".into(),
                BuiltinType::F32 => "F32".into(),
                BuiltinType::F64 => "F64".into(),
                BuiltinType::Bool => "Bool".into(),
                BuiltinType::CVoidPtr => "VoidPtr".into(),
                BuiltinType::CChar => "Char".into(),
                _ => "Unknown".into(),
            },
            TypeNodeKind::Ref(inner) => format!("Ref_{}", self.ty_to_mangle(*inner)),
            TypeNodeKind::MutRef(inner) => format!("MutRef_{}", self.ty_to_mangle(*inner)),
            TypeNodeKind::Share(inner) => format!("Share_{}", self.ty_to_mangle(*inner)),
            TypeNodeKind::RawPtr(inner) => format!("RawPtr_{}", self.ty_to_mangle(*inner)),
            TypeNodeKind::Tuple(elems) if elems.is_empty() => "Void".into(),
            TypeNodeKind::Tuple(elems) => {
                let inner: Vec<String> = elems.iter().map(|&t| self.ty_to_mangle(t)).collect();
                format!("Tuple_{}", inner.join("_"))
            }
            TypeNodeKind::Fun {
                param_tys,
                return_ty,
            } => {
                let ret = self.ty_to_mangle(*return_ty);
                let params: Vec<String> = param_tys.iter().map(|&t| self.ty_to_mangle(t)).collect();
                format!("Fn_{}_{}", ret, params.join("_"))
            }
            TypeNodeKind::Struct { decl_id, subst, .. }
            | TypeNodeKind::ADT { decl_id, subst, .. } => {
                let key = (*decl_id, subst.clone());
                if let Some(def) = self.type_checker_result.concrete_type_defs.get(&key) {
                    return def.name.clone();
                }
                if let Some(def) = self.type_checker_result.generic_type_defs.get(decl_id) {
                    let suffix: Vec<String> = subst.iter().map(|&s| self.ty_to_mangle(s)).collect();
                    if suffix.is_empty() {
                        def.name.clone()
                    } else {
                        format!("{}_{}", def.name, suffix.join("_"))
                    }
                } else {
                    format!("unknown_decl_{}", decl_id)
                }
            }
            TypeNodeKind::Never => "Never".into(),
            TypeNodeKind::Var => format!("var_{}", ty_id),
            _ => format!("ty_{}", ty_id),
        }
    }

    fn ty_to_c(&self, ty_id: TyId) -> String {
        let pool = &self.type_checker_result.type_pool;
        let root = get_type_root(pool, ty_id);
        match &pool[root].kind {
            TypeNodeKind::Builtin(b) => match b {
                BuiltinType::I8 => "int8_t".into(),
                BuiltinType::I16 => "int16_t".into(),
                BuiltinType::I32 => "int32_t".into(),
                BuiltinType::I64 => "int64_t".into(),
                BuiltinType::U8 => "uint8_t".into(),
                BuiltinType::U16 => "uint16_t".into(),
                BuiltinType::U32 => "uint32_t".into(),
                BuiltinType::U64 => "uint64_t".into(),
                BuiltinType::F32 => "float".into(),
                BuiltinType::F64 => "double".into(),
                BuiltinType::Bool => "bool".into(),
                BuiltinType::CVoidPtr => "void*".into(),
                BuiltinType::CChar => "char".into(),
                _ => "unknown".into(),
            },
            TypeNodeKind::Ref(inner) | TypeNodeKind::MutRef(inner) => {
                let inner_c = self.ty_to_c(*inner);
                if inner_c.contains("%s") {
                    inner_c.replacen("%s", "*%s", 1)
                } else {
                    format!("{}*", inner_c)
                }
            }
            TypeNodeKind::Share(inner) => todo!(),
            TypeNodeKind::Tuple(elems) if elems.is_empty() => "void".into(),
            TypeNodeKind::Tuple(elems) => {
                let inner: Vec<String> = elems.iter().map(|&t| self.ty_to_c(t)).collect();
                format!("Tuple_{}", inner.join("_"))
            }
            TypeNodeKind::Fun {
                param_tys,
                return_ty,
            } => {
                let ret = self.ty_to_c(*return_ty);
                let params: Vec<String> = param_tys.iter().map(|&t| self.ty_to_c(t)).collect();
                format!("{} (*%s)({})", ret, params.join(", "))
            }
            TypeNodeKind::Struct { decl_id, subst, .. }
            | TypeNodeKind::ADT { decl_id, subst, .. } => {
                let key = (*decl_id, subst.clone());
                if let Some(def) = self.type_checker_result.concrete_type_defs.get(&key) {
                    return def.name.clone();
                }
                if let Some(def) = self.type_checker_result.generic_type_defs.get(decl_id) {
                    let suffix: Vec<String> = subst.iter().map(|&s| self.ty_to_c(s)).collect();
                    if suffix.is_empty() {
                        def.name.clone()
                    } else {
                        format!("{}_{}", def.name, suffix.join("_"))
                    }
                } else {
                    format!("unknown_decl_{}", decl_id)
                }
            }
            TypeNodeKind::RawPtr(inner) => {
                let inner_c = self.ty_to_c(*inner);
                if inner_c.contains("%s") {
                    inner_c.replacen("%s", "const *%s", 1)
                } else {
                    format!("const {}*", inner_c)
                }
            }
            TypeNodeKind::MutRawPtr(inner) => {
                let inner_c = self.ty_to_c(*inner);
                if inner_c.contains("%s") {
                    inner_c.replacen("%s", "*%s", 1)
                } else {
                    format!("{}*", inner_c)
                }
            }
            TypeNodeKind::Never => "void*".into(),
            TypeNodeKind::Var => format!("unknown_ty_var_{}", ty_id),
            _ => format!("unknown_ty_{}", ty_id),
        }
    }

    fn mangle(&self, name: &str, param_tys: &[TyId], ret_ty: TyId) -> String {
        if name == "main" && param_tys.is_empty() {
            return "main".into();
        }
        let mut parts: Vec<String> = vec![name.to_string()];
        parts.extend(param_tys.iter().map(|&t| self.ty_to_mangle(t)));
        parts.push(self.ty_to_mangle(ret_ty));
        parts.join("_")
    }

    fn place_ty(&self, place: &Place, fun: &MirFun) -> TyId {
        match place {
            Place::Local(id) => fun.local_decls[*id].ty,
            Place::Static(sid) => self.mono_mir.statics[*sid].ty,
            Place::Field { base, field } => {
                let base_ty = self.place_ty(base, fun);
                let pool = &self.type_checker_result.type_pool;
                let root = get_type_root(pool, base_ty);
                match &pool[root].kind {
                    TypeNodeKind::Struct { field_tys, .. } => field_tys[*field],
                    TypeNodeKind::Tuple(elements) => elements[*field],
                    _ => unreachable!(),
                }
            }
            Place::Deref(p) => {
                let inner_place_ty = self.place_ty(p, fun);
                let pool = &self.type_checker_result.type_pool;
                let root = get_type_root(pool, inner_place_ty);
                match &pool[root].kind {
                    TypeNodeKind::Ref(inner) | TypeNodeKind::MutRef(inner) => *inner,
                    _ => unreachable!("Deref on non-reference type"),
                }
            }
            Place::EnumItem { place, variant } => {
                let adt_ty = self.place_ty(place, fun);
                let pool = &self.type_checker_result.type_pool;
                let root = get_type_root(pool, adt_ty);
                if let TypeNodeKind::ADT { variants, .. } = &pool[root].kind {
                    variants[*variant].unwrap_or_else(|| {
                        self.type_checker_result
                            .type_pool
                            .iter()
                            .position(|n| matches!(&n.kind, TypeNodeKind::Tuple(e) if e.is_empty()))
                            .unwrap()
                    })
                } else {
                    unreachable!()
                }
            }
            _ => unreachable!(),
        }
    }

    fn const_to_c_with_ty(&self, c: &Const, ty: TyId) -> String {
        match c {
            Const::Unit => "/* void */".into(),
            Const::Tuple(elems) => {
                let type_name = self.ty_to_c(ty);
                if elems.is_empty() {
                    "/* void */".into()
                } else {
                    let pool = &self.type_checker_result.type_pool;
                    let root = get_type_root(pool, ty);
                    let elem_tys = if let TypeNodeKind::Tuple(elem_tys) = &pool[root].kind {
                        elem_tys.clone()
                    } else {
                        vec![]
                    };
                    let fields_str = elems
                        .iter()
                        .enumerate()
                        .map(|(i, e)| {
                            let elem_ty = elem_tys.get(i).copied().unwrap_or(0);
                            format!(".f{} = {}", i, self.const_to_c_with_ty(e, elem_ty))
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({}){{ {} }}", type_name, fields_str)
                }
            }
            Const::Struct(fields) => {
                let type_name = self.ty_to_c(ty);
                if fields.is_empty() {
                    format!("({}){{0}}", type_name)
                } else {
                    let pool = &self.type_checker_result.type_pool;
                    let root = get_type_root(pool, ty);
                    let field_tys = if let TypeNodeKind::Struct { field_tys, .. } = &pool[root].kind
                    {
                        field_tys.clone()
                    } else {
                        vec![]
                    };
                    let fields_str = fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            let f_ty = field_tys.get(i).copied().unwrap_or(0);
                            format!(".f{} = {}", i, self.const_to_c_with_ty(f, f_ty))
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({}){{ {} }}", type_name, fields_str)
                }
            }
            Const::Enum(tag, data) => {
                let type_name = self.ty_to_c(ty);
                let has_payload = {
                    let pool = &self.type_checker_result.type_pool;
                    let root = get_type_root(pool, ty);
                    if let TypeNodeKind::ADT { variants, .. } = &pool[root].kind {
                        variants[tag.0].is_some()
                    } else {
                        false
                    }
                };
                if has_payload {
                    let payload_ty = {
                        let pool = &self.type_checker_result.type_pool;
                        let root = get_type_root(pool, ty);
                        if let TypeNodeKind::ADT { variants, .. } = &pool[root].kind {
                            variants[tag.0].unwrap()
                        } else {
                            0
                        }
                    };
                    let inner_str = self.const_to_c_with_ty(data, payload_ty);
                    format!(
                        "({}){{ .tag = {}, .data.v{} = {} }}",
                        type_name, tag, tag, inner_str
                    )
                } else {
                    format!("({}){{ .tag = {} }}", type_name, tag)
                }
            }
            other => Self::const_to_c(other),
        }
    }

    fn const_to_c(c: &Const) -> String {
        match c {
            Const::Int8(v) => v.to_string(),
            Const::Int16(v) => v.to_string(),
            Const::Int32(v) => v.to_string(),
            Const::Int64(v) => format!("{}LL", v),
            Const::UInt8(v) => v.to_string(),
            Const::UInt16(v) => v.to_string(),
            Const::UInt32(v) => format!("{}U", v),
            Const::UInt64(v) => format!("{}ULL", v),
            Const::Float32(bits) => {
                let f = f32::from_bits(*bits as u32);
                format!("{:?}f", f)
            }
            Const::Float64(bits) => {
                let f = f64::from_bits(*bits);
                format!("{:?}", f)
            }
            Const::Bool(true) => "true".into(),
            Const::Bool(false) => "false".into(),
            Const::Char(v) => {
                if let Some(c) = char::from_u32(*v as u32) {
                    format!("'{}'", c.escape_default())
                } else {
                    format!("0x{:x}", v)
                }
            }
            Const::Str(s) => format!("\"{}\"", s.escape_default()),
            _ => todo!(),
        }
    }

    fn place_to_c(&self, place: &Place, var_names: &HashMap<MirLocalId, String>) -> String {
        match place {
            Place::Local(id) => var_names[id].clone(),
            Place::Static(sid) => self.static_name(*sid),
            Place::Field { base, field } => {
                format!("{}.f{}", self.place_to_c(base, var_names), field)
            }
            Place::Index { place, item_index } => {
                format!("{}[{}]", self.place_to_c(place, var_names), item_index)
            }
            Place::Deref(p) => {
                format!("(*{})", self.place_to_c(p, var_names))
            }
            Place::EnumItem { place, variant } => {
                let base = self.place_to_c(place, var_names);
                format!("{}.data.v{}", base, variant)
            }
        }
    }

    fn static_name(&self, sid: MirStaticId) -> String {
        self.mono_mir.statics[sid].name.clone()
    }

    fn rvalue_to_c(
        &self,
        rv: &Rvalue,
        var_names: &HashMap<MirLocalId, String>,
        fun: &MirFun,
    ) -> String {
        match rv {
            Rvalue::Constant(c) => Self::const_to_c(c),
            Rvalue::Move(place) | Rvalue::Copy(place) => self.place_to_c(place, var_names),
            Rvalue::BinaryOp { op, left, right } => {
                let l = self.rvalue_to_c(left, var_names, fun);
                let r = self.rvalue_to_c(right, var_names, fun);
                let op_str = match op {
                    MirBinOp::Add => "+",
                    MirBinOp::Sub => "-",
                    MirBinOp::Mul => "*",
                    MirBinOp::Div => "/",
                    MirBinOp::Rem => "%",
                    MirBinOp::BitAnd => "&",
                    MirBinOp::BitOr => "|",
                    MirBinOp::BitXor => "^",
                    MirBinOp::Shl => "<<",
                    MirBinOp::Shr => ">>",
                    MirBinOp::Eq => "==",
                    MirBinOp::Ne => "!=",
                    MirBinOp::Lt => "<",
                    MirBinOp::Le => "<=",
                    MirBinOp::Gt => ">",
                    MirBinOp::Ge => ">=",
                };
                format!("({} {} {})", l, op_str, r)
            }
            Rvalue::UnaryOp { op, right } => {
                let r = self.rvalue_to_c(right, var_names, fun);
                let op_str = match op {
                    MirUnOp::Neg => "-",
                    MirUnOp::Not => "!",
                };
                format!("({}{})", op_str, r)
            }
            Rvalue::Cast(place, target_ty) => {
                let p = self.place_to_c(place, var_names);
                let src_ty = self.place_ty(place, fun);
                if self.is_adt_type(src_ty) {
                    format!("({})", format!("{}.tag", p))
                } else {
                    let ty = self.ty_to_c(*target_ty);
                    format!("(({}){})", ty, p)
                }
            }
            Rvalue::Ref(place) | Rvalue::RefMut(place) => {
                format!("&({})", self.place_to_c(place, var_names))
            }

            _ => unreachable!("should be handled by rvalue_to_c_with_ty"),
        }
    }

    fn is_adt_type(&self, ty: TyId) -> bool {
        let pool = &self.type_checker_result.type_pool;
        let root = get_type_root(pool, ty);
        matches!(&pool[root].kind, TypeNodeKind::ADT { .. })
    }

    fn is_struct_type(&self, ty: TyId) -> bool {
        let pool = &self.type_checker_result.type_pool;
        let root = get_type_root(pool, ty);
        matches!(&pool[root].kind, TypeNodeKind::Struct { .. })
    }

    fn is_tuple_type(&self, ty: TyId) -> bool {
        let pool = &self.type_checker_result.type_pool;
        let root = get_type_root(pool, ty);
        matches!(&pool[root].kind, TypeNodeKind::Tuple { .. })
    }

    fn rvalue_to_c_with_ty(
        &self,
        rv: &Rvalue,
        ty: TyId,
        var_names: &HashMap<MirLocalId, String>,
        fun: &MirFun,
    ) -> String {
        match rv {
            Rvalue::Constant(c) => self.const_to_c_with_ty(c, ty),
            Rvalue::BuildStruct(vals) => {
                let type_name = self.ty_to_c(ty);
                let field_vals: Vec<String> = vals
                    .iter()
                    .map(|v| self.rvalue_to_c(v, var_names, fun))
                    .collect();
                let init_list = if field_vals.is_empty() {
                    "{0}".to_string()
                } else {
                    let fields_str = field_vals
                        .iter()
                        .enumerate()
                        .map(|(i, val)| format!(".f{} = {}", i, val))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{{ {} }}", fields_str)
                };
                format!("({}){}", type_name, init_list)
            }
            Rvalue::Variant(tag, inner) => {
                let type_name = self.ty_to_c(ty);
                let has_payload = {
                    let pool = &self.type_checker_result.type_pool;
                    let root = get_type_root(pool, ty);
                    if let TypeNodeKind::ADT { variants, .. } = &pool[root].kind {
                        variants[tag.0].is_some()
                    } else {
                        false
                    }
                };
                if has_payload {
                    let inner_str = self.rvalue_to_c(inner, var_names, fun);
                    format!(
                        "({}){{ .tag = {}, .data.v{} = {} }}",
                        type_name, tag, tag, inner_str
                    )
                } else {
                    format!("({}){{ .tag = {} }}", type_name, tag)
                }
            }
            Rvalue::Tuple(elements) => {
                let type_name = self.ty_to_c(ty);
                if elements.is_empty() {
                    "/* void */".into()
                } else {
                    let fields_str = elements
                        .iter()
                        .enumerate()
                        .map(|(i, e)| format!(".f{} = {}", i, self.rvalue_to_c(e, var_names, fun)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({}){{ {} }}", type_name, fields_str)
                }
            }
            Rvalue::Ref(place) | Rvalue::RefMut(place) => {
                format!("&({})", self.place_to_c(place, var_names))
            }
            Rvalue::GetFunPtr(fun_id) => {
                let callee = &self.mono_mir.functions[*fun_id];
                let mangled = if callee.blocks.is_empty() {
                    callee.name.clone()
                } else {
                    self.mangle(
                        &callee.name,
                        &callee.signature.params,
                        callee.signature.return_ty,
                    )
                };
                mangled
            }

            _ => self.rvalue_to_c(rv, var_names, fun),
        }
    }

    fn compute_merge_block(blocks: &[BasicBlock], handler_block: MirBasicBlockId) -> MirBasicBlockId {
        let mut visited = HashSet::new();
        let mut stack = vec![handler_block];
        while let Some(bid) = stack.pop() {
            if !visited.insert(bid) {
                continue;
            }
            match &blocks[bid].terminator {
                TerminatorKind::Resume { target, .. } => return *target,
                TerminatorKind::Goto { target, .. } => stack.push(*target),
                _ => {}
            }
        }
        unreachable!("Handler block must eventually reach a Resume")
    }

    fn compute_merge_block_from_body(
        &self,
        blocks: &[BasicBlock],
        next: MirBasicBlockId,
        handler_block: MirBasicBlockId,
    ) -> MirBasicBlockId {
        let mut body_set = HashSet::new();
        let mut stack = vec![next];
        while let Some(bid) = stack.pop() {
            if bid == handler_block || !body_set.insert(bid) {
                continue;
            }
            let block = &blocks[bid];
            for succ in Self::terminator_successors(block.terminator.clone()) {
                if succ != handler_block {
                    stack.push(succ)
                }
            }
        }

        let mut merge_candidates = HashSet::new();
        for &bid in &body_set {
            let block = &blocks[bid];
            for succ in Self::terminator_successors(block.terminator.clone()) {
                if succ != handler_block && !body_set.contains(&succ) {
                    merge_candidates.insert(succ);
                }
            }
        }

        if merge_candidates.len() == 1 {
            *merge_candidates.iter().next().unwrap()
        } else if merge_candidates.is_empty() {
            blocks.len() - 1
        } else {
            *merge_candidates.iter().min().unwrap()
        }
    }

    fn gen_function(&self, fun: &MirFun, fun_id: MirFunId) -> String {
        let blocks = &self.mono_mir.blocks;

        let mangled = self.mangle(&fun.name, &fun.signature.params, fun.signature.return_ty);
        let is_main = fun.name == "main";

        let ret_ty_id = fun.signature.return_ty;
        let ret_root = get_type_root(&self.type_checker_result.type_pool, ret_ty_id);

        let (ret_str, inner_fun_params, inner_fun_ret) = if let TypeNodeKind::Fun {
            param_tys,
            return_ty,
        } =
            &self.type_checker_result.type_pool[ret_root].kind
        {
            let ret_c = self.ty_to_c(*return_ty);
            let params_c: Vec<String> = param_tys.iter().map(|&t| self.ty_to_c(t)).collect();
            let ret = if is_main {
                "int".to_string()
            } else {
                format!("{} (*{}())({})", ret_c, mangled, params_c.join(", "))
            };
            (ret, Some(params_c), Some(*return_ty))
        } else {
            let mut rty = self.ty_to_c(ret_ty_id);
            if rty.contains("%s") {
                rty = rty.replace("%s", "");
            }
            let ret = if is_main { "int".to_string() } else { rty };
            (ret, None, None)
        };

        let original_ret_ty = self.ty_to_c(ret_ty_id);
        let returns_value = ret_str != "void";

        let mut var_names: HashMap<MirLocalId, String> = HashMap::new();
        var_names.insert(MirLocalId(0), "_ret".into());
        let mut var_counter = 0u32;
        for (i, decl) in fun.local_decls.iter().enumerate() {
            if i == 0 {
                continue;
            }
            let mut name = decl.name.clone().unwrap_or_else(|| format!("v{}", i));
            if name == "return" {
                name = "_ret".to_string();
            }
            let unique = format!("{}_{}", name, var_counter);
            var_counter += 1;
            var_names.insert(MirLocalId(i), unique);
        }

        let mut body_functions = String::new();
        let mut body_info: HashMap<
            MirBasicBlockId,
            (String, MirBasicBlockId, MirBasicBlockId, MirControlId, Vec<MirLocalId>),
        > = HashMap::new();
        let mut body_fun_count = 0;
        let mut skip_blocks: HashSet<MirBasicBlockId> = HashSet::new();

        for &bid in &fun.blocks {
            if let TerminatorKind::InstallHandler {
                handler_block,
                next,
                args_dest,
                control_id,
            } = &blocks[bid].terminator
            {
                let merge_block = self.compute_merge_block_from_body(blocks, *next, *handler_block);
                let body_fun_name = format!("{}_with_body_{}", mangled, body_fun_count);
                body_info.insert(
                    bid,
                    (
                        body_fun_name.clone(),
                        merge_block,
                        *next,
                        *control_id,
                        args_dest.clone(),
                    ),
                );

                body_functions += &self.extract_with_body(
                    fun,
                    blocks,
                    bid,
                    *next,
                    merge_block,
                    MirBasicBlockId(0),
                    *control_id,
                    args_dest,
                    &body_fun_name,
                );

                let mut body_set = HashSet::new();
                let mut stack = vec![*next];
                while let Some(bid2) = stack.pop() {
                    if bid2 == merge_block || bid2.0 == 0 || !body_set.insert(bid2) {
                        continue;
                    }
                    match &blocks[bid2].terminator {
                        TerminatorKind::Goto { target, .. } => stack.push(*target),
                        TerminatorKind::Call { target, .. }
                        | TerminatorKind::CallByPtr { target, .. } => {
                            if let Some(t) = target {
                                stack.push(*t);
                            }
                        }
                        TerminatorKind::Resume { target, .. } => stack.push(*target),
                        _ => {}
                    }
                }
                skip_blocks.extend(body_set);
                body_fun_count += 1;
            }
        }

        loop {
            let mut changed = false;
            for &bid in &fun.blocks {
                if skip_blocks.contains(&bid) {
                    continue;
                }
                let term = &blocks[bid].terminator;
                let targets: Vec<MirBasicBlockId> = match term {
                    TerminatorKind::Goto { target, .. } => vec![*target],
                    TerminatorKind::Call { target, .. }
                    | TerminatorKind::CallByPtr { target, .. } => target.iter().cloned().collect(),
                    TerminatorKind::SwitchInt {
                        targets, default, ..
                    } => {
                        let mut v: Vec<_> = targets.iter().map(|(_, target)| *target).collect();
                        v.push(*default);
                        v
                    }
                    _ => vec![],
                };
                for t in targets {
                    if skip_blocks.contains(&t) {
                        skip_blocks.insert(bid);
                        changed = true;
                        break;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        let param_count = fun.signature.params.len();
        let params: Vec<String> = (0..param_count)
            .map(|i| {
                let local_id = 1 + i;
                let ty = self.ty_to_c(fun.local_decls[local_id].ty);
                let name = var_names[&local_id].clone();
                if ty.contains("%s") {
                    ty.replace("%s", &name)
                } else {
                    format!("{} {}", ty, name)
                }
            })
            .collect();
        let param_str = params.join(", ");

        let mut code = String::new();

        // function signature
        if let TypeNodeKind::Fun { .. } = &self.type_checker_result.type_pool[ret_root].kind {
            code += &format!("{} {{\n", ret_str);
        } else {
            code += &format!("{} {}({}) {{\n", ret_str, mangled, param_str);
        }

        // local variable declarations
        for (i, decl) in fun.local_decls.iter().enumerate() {
            if i == 0 && is_main {
                continue;
            }
            if i == 0 && !returns_value {
                continue;
            }
            if i == 0 && returns_value {
                continue;
            }
            if i >= 1 && i <= param_count {
                continue;
            }
            let ty = self.ty_to_c(decl.ty);
            if ty == "void" {
                continue;
            }
            let name = &var_names[&i];
            let decl_str = if ty.contains("%s") {
                ty.replace("%s", name)
            } else {
                format!("{} {}", ty, name)
            };
            code += &format!("    {};\n", decl_str);
        }

        code += "    leaf_fiber_t _main_fiber;\n";
        code += "    leaf_fiber_t _body_fiber = NULL;\n";
        if is_main {
            code += "    main_fiber = leaf_convert_thread_to_fiber(NULL);\n";
        }

        // _ret
        if returns_value && !is_main {
            let decl_str =
                if let Some((ref fun_params, fun_ret)) = inner_fun_params.zip(inner_fun_ret) {
                    let ret_c = self.ty_to_c(fun_ret);
                    let params_c = fun_params
                        .iter()
                        .map(|t| t.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{} (*_ret)({})", ret_c, params_c)
                } else {
                    let mut rty = self.ty_to_c(ret_ty_id);
                    if rty.contains("%s") {
                        rty.replace("%s", "_ret")
                    } else {
                        format!("{} _ret", rty)
                    }
                };
            code += &format!("    {};\n", decl_str);
        }

        for &bid in &fun.blocks {
            if skip_blocks.contains(&bid) {
                continue;
            }

            if body_info.values().any(|(_, mb, _, _, _)| *mb == bid) {
                code += &format!("  block_{}:\n", bid.0);
                let ret_ty = fun.signature.return_ty;
                let ret_c_ty = self.ty_to_c(ret_ty);
                if ret_c_ty != "void" {
                    code += &format!("    _ret = ({})_raise_resume_val;\n", ret_c_ty);
                    code += "    return _ret;\n";
                } else {
                    code += "    return;\n";
                }
                continue;
            }

            let block = &blocks[bid];
            code += &format!("  block_{}:\n", bid.0);

            if let Some((body_fun_name, merge_block, _next, control_id, args_dest)) =
                body_info.get(&bid)
            {
                let handler_block = match &block.terminator {
                    TerminatorKind::InstallHandler { handler_block, .. } => *handler_block,
                    _ => unreachable!(),
                };
                let args_count = std::cmp::max(1, args_dest.len());
                code += &format!("    intptr_t _handler_args_{}[{}];\n", bid.0, args_count);
                let args_dest_array = if !args_dest.is_empty() {
                    let names: Vec<String> = (0..args_dest.len())
                        .map(|i| format!("&_handler_args_{}[{}]", bid.0, i))
                        .collect();
                    format!("(void*[]){{{}}}", names.join(", "))
                } else {
                    "(void*[]){0}".to_string()
                };

                code += &format!(
                    "    _body_fiber = leaf_create_fiber({}, NULL);\n",
                    body_fun_name
                );
                code += &format!(
                    "    leaf_push_handler({}, {}, {}, _body_fiber, GetCurrentFiber());\n",
                    control_id,
                    args_dest_array,
                    args_dest.len()
                );
                code += "    leaf_switch_to_fiber(_body_fiber);\n";
                code += "    if (_effect_raised) {\n";
                code += "        _effect_raised = 0;\n";

                for (i, &local_id) in args_dest.iter().enumerate() {
                    let ty_c = self.ty_to_c(fun.local_decls[local_id].ty);
                    code += &format!(
                        "        {} = ({})_handler_args_{}[{}];\n",
                        var_names[&local_id], ty_c, bid.0, i
                    );
                }

                code += &format!("        goto block_{};\n", handler_block.0);
                code += "    } else {\n";
                code += "        leaf_pop_handler();\n";
                code += &format!("        goto block_{};\n", merge_block);
                code += "    }\n";

                code += &format!(
                    "    _body_fiber = leaf_create_fiber({}, NULL);\n",
                    body_fun_name
                );
                code += &format!(
                    "    leaf_push_handler({}, {}, {}, _body_fiber, GetCurrentFiber());\n",
                    control_id,
                    args_dest_array,
                    args_dest.len()
                );
                code += "    leaf_switch_to_fiber(_body_fiber);\n";
                code += "    if (_effect_raised) {\n";
                code += "        _effect_raised = 0;\n";
                code += &format!("        goto block_{};\n", handler_block.0);
                code += "    } else {\n";
                code += &format!("        goto block_{};\n", merge_block);
                code += "    }\n";
                continue;
            }

            for stmt in &block.statements {
                match &stmt.kind {
                    MirStmtKind::Let { local, rvalue } => {
                        let ty = fun.local_decls[*local].ty;
                        if self.ty_to_c(ty) == "void" {
                            continue;
                        }
                        if is_main && *local == 0 {
                            continue;
                        }
                        let lhs = var_names[local].clone();
                        let rhs = match rvalue {
                            Rvalue::HandlerArg(idx) => {
                                let param_local = block.block_params[*idx];
                                var_names[&param_local].clone()
                            }
                            _ => self.rvalue_to_c_with_ty(rvalue, ty, &var_names, fun),
                        };
                        code += &format!("    {} = {};\n", lhs, rhs);
                    }
                    MirStmtKind::Store { place, rvalue } => {
                        let ty = self.place_ty(place, fun);
                        if self.ty_to_c(ty) == "void" {
                            continue;
                        }
                        if is_main && matches!(place, Place::Local(id) if *id == 0) {
                            continue;
                        }
                        let lhs = self.place_to_c(place, &var_names);
                        let rhs = self.rvalue_to_c_with_ty(rvalue, ty, &var_names, fun);
                        code += &format!("    {} = {};\n", lhs, rhs);
                    }
                    MirStmtKind::Nop => {
                        code += "    ;\n";
                    }
                }
            }

            code += &self.gen_terminator(
                block,
                bid,
                fun,
                &var_names,
                blocks,
                is_main,
                original_ret_ty.clone(),
            );
        }

        code += "}\n\n";

        body_functions + &code
    }
    fn gen_terminator(
        &self,
        block: &BasicBlock,
        block_id: MirBasicBlockId,
        fun: &MirFun,
        var_names: &HashMap<MirLocalId, String>,
        blocks: &[BasicBlock],
        is_main: bool,
        original_ret_ty: String,
    ) -> String {
        let mut code = String::new();
        match &block.terminator {
            TerminatorKind::Goto { target, block_args } => {
                let target_params = &blocks[*target].block_params;
                for (param_id, arg) in target_params.iter().zip(block_args.iter()) {
                    let param_ty = fun.local_decls[*param_id].ty;
                    if self.ty_to_c(param_ty) == "void" {
                        continue;
                    }
                    let lhs = var_names[param_id].clone();
                    let rhs = self.rvalue_to_c_with_ty(arg, param_ty, var_names, fun);
                    code += &format!("    {} = {};\n", lhs, rhs);
                }
                code += &format!("    goto block_{};\n", target);
            }
            TerminatorKind::SwitchInt {
                discriminant,
                targets,
                default,
            } => {
                let disc_str = match discriminant {
                    Rvalue::Copy(place) | Rvalue::Move(place) => {
                        let place_ty = self.place_ty(place, fun);
                        if self.is_adt_type(place_ty) {
                            format!("{}.tag", self.place_to_c(place, var_names))
                        } else {
                            self.rvalue_to_c(discriminant, var_names, fun)
                        }
                    }
                    _ => self.rvalue_to_c(discriminant, var_names, fun),
                };
                code += &format!("    switch ({}) {{\n", disc_str);
                for (val, target) in targets {
                    let case_val = Self::const_to_c(val);
                    code += &format!("        case {}: goto block_{};\n", case_val, target);
                }
                code += &format!("        default: goto block_{};\n", default);
                code += "    }\n";
            }
            TerminatorKind::Call {
                func,
                args,
                dest,
                target,
            } => {
                let callee = &self.mono_mir.functions[*func];
                let callee_mangled = if callee.blocks.is_empty() {
                    callee.name.clone()
                } else {
                    self.mangle(
                        &callee.name,
                        &callee.signature.params,
                        callee.signature.return_ty,
                    )
                };
                let arg_str: Vec<String> = args
                    .iter()
                    .map(|a| self.rvalue_to_c(a, var_names, fun))
                    .collect();
                let call_expr = format!("{}({})", callee_mangled, arg_str.join(", "));
                if self.ty_to_c(callee.signature.return_ty) != "void" {
                    let lhs = self.place_to_c(dest, var_names);
                    code += &format!("    {} = {};\n", lhs, call_expr);
                } else {
                    code += &format!("    {};\n", call_expr);
                }
                if let Some(t) = target {
                    code += &format!("    goto block_{};\n", t);
                }
            }
            TerminatorKind::Return => {
                if is_main {
                    code += "    return 0;\n";
                } else if self.ty_to_c(fun.signature.return_ty) != "void" {
                    code += "    return _ret;\n";
                } else {
                    code += "    return;\n";
                }
            }
            TerminatorKind::CallByPtr {
                func,
                args,
                dest,
                target,
            } => {
                let func_expr = self.rvalue_to_c(func, var_names, fun);
                let arg_str: Vec<String> = args
                    .iter()
                    .map(|a| self.rvalue_to_c(a, var_names, fun))
                    .collect();
                let call_expr = format!("{}({})", func_expr, arg_str.join(", "));
                let dest_ty = self.place_ty(dest, fun);
                if self.ty_to_c(dest_ty) != "void" {
                    let lhs = self.place_to_c(dest, var_names);
                    code += &format!("    {} = {};\n", lhs, call_expr);
                } else {
                    code += &format!("    {};\n", call_expr);
                }
                if let Some(t) = target {
                    code += &format!("    goto block_{};\n", t);
                }
            }
            TerminatorKind::Unreachable => {
                code += "    /* unreachable */ __builtin_unreachable();\n";
            }

            TerminatorKind::Raise {
                control_name,
                args,
                dest,
            } => {
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| format!("(intptr_t)({})", self.rvalue_to_c(a, var_names, fun)))
                    .collect();
                let arg_list = arg_strs.join(", ");
                code += &format!("    leaf_raise({}, {});\n", control_name, arg_list);
                let dest_str = self.place_to_c(dest, var_names);
                let dest_ty = self.place_ty(dest, fun);
                if self.ty_to_c(dest_ty) != "void" {
                    code += &format!("    {} = _raise_resume_val;\n", dest_str);
                }
                let resume_block = block_id + 1;
                code += &format!("    goto block_{};\n", resume_block);
            }
            TerminatorKind::Resume { place, target } => {
                let val = self.place_to_c(place, var_names);
                code += &format!("    leaf_resume((intptr_t){});\n", val);
                code += &format!("    goto block_{};\n", target);
            }
            TerminatorKind::InstallHandler { .. } => {
                unreachable!("InstallHandler should be handled in gen_function directly");
            }
            _ => {
                code += &format!("    /* unsupported terminator: {:?} */\n", block.terminator);
            }
        }
        code
    }

    fn extract_with_body(
        &self,
        fun: &MirFun,
        blocks: &[BasicBlock],
        install_block: MirBasicBlockId,
        next_block: MirBasicBlockId,
        merge_block: MirBasicBlockId,
        handler_block: MirBasicBlockId,
        control_id: MirControlId,
        args_dest: &[MirLocalId],
        body_fun_name: &str,
    ) -> String {
        let mut body_blocks = Vec::new();
        let mut stack = vec![next_block];
        let mut visited = HashSet::new();
        while let Some(bid) = stack.pop() {
            if bid == merge_block || bid == handler_block || !visited.insert(bid) {
                continue;
            }
            body_blocks.push(bid);
            match &blocks[bid].terminator {
                TerminatorKind::Goto { target, .. } => stack.push(*target),
                TerminatorKind::Call { target, .. } | TerminatorKind::CallByPtr { target, .. } => {
                    if let Some(t) = target {
                        stack.push(*t);
                    }
                }
                TerminatorKind::Resume { target, .. } => stack.push(*target),
                _ => {}
            }
        }
        let mut code = format!("void {}() {{\n", body_fun_name);
        let mut var_names = HashMap::new();
        var_names.insert(MirLocalId(0), "_ret".into());
        let mut var_counter = 0u32;
        for (i, decl) in fun.local_decls.iter().enumerate() {
            let mut name = decl.name.clone().unwrap_or_else(|| format!("v{}", i));
            if name == "return" {
                name = "_ret".to_string();
            }
            let unique = format!("{}_{}", name, var_counter);
            var_counter += 1;
            var_names.insert(MirLocalId(i), unique);
        }
        for (i, decl) in fun.local_decls.iter().enumerate() {
            let ty = self.ty_to_c(decl.ty);
            if ty == "void" {
                continue;
            }
            let name = &var_names[&i];
            code += &format!("    {} {};\n", ty, name);
        }
        for &bid in &body_blocks {
            let block = &blocks[bid];
            code += &format!("  block_{}:\n", bid.0);
            for stmt in &block.statements {
                match &stmt.kind {
                    MirStmtKind::Let { local, rvalue } => {
                        let ty = fun.local_decls[*local].ty;
                        if self.ty_to_c(ty) == "void" {
                            continue;
                        }
                        let lhs = var_names[local].clone();
                        let rhs = match rvalue {
                            Rvalue::HandlerArg(idx) => {
                                let param_local = block.block_params[*idx];
                                var_names[&param_local].clone()
                            }
                            _ => self.rvalue_to_c_with_ty(rvalue, ty, &var_names, fun),
                        };
                        code += &format!("    {} = {};\n", lhs, rhs);
                    }
                    MirStmtKind::Store { place, rvalue } => {
                        let ty = self.place_ty(place, fun);
                        if self.ty_to_c(ty) == "void" {
                            continue;
                        }
                        let lhs = self.place_to_c(place, &var_names);
                        let rhs = self.rvalue_to_c_with_ty(rvalue, ty, &var_names, fun);
                        code += &format!("    {} = {};\n", lhs, rhs);
                    }
                    MirStmtKind::Nop => {
                        code += "    ;\n";
                    }
                }
            }
            code += &self.gen_terminator_for_body(block, bid, fun, &var_names, blocks, merge_block);
        }
        code += "}\n\n";
        code
    }

    fn gen_terminator_for_body(
        &self,
        block: &BasicBlock,
        block_id: MirBasicBlockId,
        fun: &MirFun,
        var_names: &HashMap<MirLocalId, String>,
        blocks: &[BasicBlock],
        merge_block: MirBasicBlockId,
    ) -> String {
        let mut code = String::new();
        match &block.terminator {
            TerminatorKind::Goto { target, block_args } => {
                if *target == merge_block {
                    let resume_val = if block_args.len() == 1 {
                        let arg = &block_args[0];
                        let target_ty = blocks[*target]
                            .block_params
                            .first()
                            .map(|id| fun.local_decls[*id].ty);

                        match arg {
                            Rvalue::Move(Place::Local(id)) | Rvalue::Copy(Place::Local(id)) => {
                                let ty = fun.local_decls[*id].ty;
                                if self.ty_to_c(ty) == "void" {
                                    "0".to_string()
                                } else {
                                    self.rvalue_to_c(arg, var_names, fun)
                                }
                            }
                            Rvalue::Tuple(elems) if elems.is_empty() => "0".to_string(),
                            _ => {
                                if let Some(ty) = target_ty {
                                    self.rvalue_to_c_with_ty(arg, ty, var_names, fun)
                                } else {
                                    self.rvalue_to_c(arg, var_names, fun)
                                }
                            }
                        }
                    } else if block_args.is_empty() {
                        "0".to_string()
                    } else {
                        unreachable!("multiple return values in with body not supported")
                    };

                    code += "    _effect_raised = 0;\n";
                    code += &format!("    _raise_resume_val = (intptr_t){};\n", resume_val);
                    code +=
                        "    leaf_switch_to_fiber(handler_stack[handler_sp - 1]->caller_fiber);\n";
                    code += "    __builtin_unreachable();\n";
                } else {
                    let target_params = &blocks[*target].block_params;
                    for (param_id, arg) in target_params.iter().zip(block_args.iter()) {
                        let param_ty = fun.local_decls[*param_id].ty;
                        if self.ty_to_c(param_ty) == "void" {
                            continue;
                        }
                        let lhs = var_names[param_id].clone();

                        let rhs = self.rvalue_to_c_with_ty(arg, param_ty, var_names, fun);

                        code += &format!("    {} = {};\n", lhs, rhs);
                    }
                    code += &format!("    goto block_{};\n", target);
                }
            }
            TerminatorKind::SwitchInt {
                discriminant,
                targets,
                default,
            } => {
                let disc_str = match discriminant {
                    Rvalue::Copy(place) | Rvalue::Move(place) => {
                        let place_ty = self.place_ty(place, fun);
                        if self.is_adt_type(place_ty) {
                            format!("{}.tag", self.place_to_c(place, var_names))
                        } else {
                            self.rvalue_to_c(discriminant, var_names, fun)
                        }
                    }
                    _ => self.rvalue_to_c(discriminant, var_names, fun),
                };
                code += &format!("    switch ({}) {{\n", disc_str);
                for (val, target) in targets {
                    let case_val = Self::const_to_c(val);
                    code += &format!("        case {}: goto block_{};\n", case_val, target);
                }
                code += &format!("        default: goto block_{};\n", default);
                code += "    }\n";
            }
            TerminatorKind::Call {
                func,
                args,
                dest,
                target,
            } => {
                let callee = &self.mono_mir.functions[*func];
                let callee_mangled = if callee.blocks.is_empty() {
                    callee.name.clone()
                } else {
                    self.mangle(
                        &callee.name,
                        &callee.signature.params,
                        callee.signature.return_ty,
                    )
                };
                let arg_str: Vec<String> = args
                    .iter()
                    .map(|a| self.rvalue_to_c(a, var_names, fun))
                    .collect();
                let call_expr = format!("{}({})", callee_mangled, arg_str.join(", "));
                if self.ty_to_c(callee.signature.return_ty) != "void" {
                    let lhs = self.place_to_c(dest, var_names);
                    code += &format!("    {} = {};\n", lhs, call_expr);
                } else {
                    code += &format!("    {};\n", call_expr);
                }
                if let Some(t) = target {
                    code += &format!("    goto block_{};\n", t);
                }
            }
            TerminatorKind::CallByPtr {
                func,
                args,
                dest,
                target,
            } => {
                let func_expr = self.rvalue_to_c(func, var_names, fun);
                let arg_str: Vec<String> = args
                    .iter()
                    .map(|a| self.rvalue_to_c(a, var_names, fun))
                    .collect();
                let call_expr = format!("{}({})", func_expr, arg_str.join(", "));
                let dest_ty = self.place_ty(dest, fun);
                if self.ty_to_c(dest_ty) != "void" {
                    let lhs = self.place_to_c(dest, var_names);
                    code += &format!("    {} = {};\n", lhs, call_expr);
                } else {
                    code += &format!("    {};\n", call_expr);
                }
                if let Some(t) = target {
                    code += &format!("    goto block_{};\n", t);
                }
            }
            TerminatorKind::Raise {
                control_name,
                args,
                dest,
            } => {
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| format!("(intptr_t)({})", self.rvalue_to_c(a, var_names, fun)))
                    .collect();
                let arg_list = arg_strs.join(", ");
                code += &format!("    leaf_raise({}, {});\n", control_name, arg_list);
                let dest_str = self.place_to_c(dest, var_names);
                code += &format!("    {} = _raise_resume_val;\n", dest_str);
                let resume_block = block_id + 1;
                code += &format!("    goto block_{};\n", resume_block);
            }
            TerminatorKind::Resume { place, target: _ } => {
                let val = self.place_to_c(place, var_names);
                code += &format!("    leaf_resume((intptr_t){});\n", val);
                code += "    __builtin_unreachable();\n";
            }
            TerminatorKind::Return => {
                code += "    _effect_raised = 0;\n";
                code += "    _raise_resume_val = 0;\n";
                code += "    leaf_switch_to_fiber(handler_stack[handler_sp - 1]->caller_fiber);\n";
                code += "    __builtin_unreachable();\n";
            }
            TerminatorKind::Unreachable => {
                code += "    __builtin_unreachable();\n";
            }
            _ => {
                unreachable!()
            }
        }
        code
    }
    fn gen_globals(&self) -> String {
        let mut code = String::new();
        for stat in &self.mono_mir.statics {
            let ty = self.ty_to_c(stat.ty);
            let name = &stat.name;
            let init = if self.is_adt_type(stat.ty) {
                format!("{{ .tag = 0 }}")
            } else if self.is_struct_type(stat.ty) || self.is_tuple_type(stat.ty) {
                "{0}".to_string()
            } else {
                Self::const_to_c(&stat.init)
            };
            code += &format!("{} {} = {};\n", ty, name, init);
        }
        code
    }

    fn gen_externs(&self) -> String {
        let mut code = String::new();
        for ext in &self.mono_mir.extern_decls {
            let ret_ty = self.ty_to_c(ext.signature.return_ty);
            let params: Vec<String> = ext
                .signature
                .params
                .iter()
                .enumerate()
                .map(|(i, ty)| format!("{} a{}", self.ty_to_c(*ty), i))
                .collect();
            let variadic = if ext.is_variadic { ", ..." } else { "" };
            code += &format!(
                "extern {} {}({}{});\n",
                ret_ty,
                ext.name,
                params.join(", "),
                variadic
            );
        }
        code
    }

    fn def_type(&self, ty_id: TyId, out: &mut String, defined: &mut HashSet<String>) {
        let c_name = self.ty_to_c(ty_id);
        if c_name == "void" || !defined.insert(c_name.clone()) {
            return;
        }

        let pool = &self.type_checker_result.type_pool;
        let root = get_type_root(pool, ty_id);
        let node = &pool[root];
        match &node.kind {
            TypeNodeKind::Tuple(elements) => {
                for &elem_ty in elements {
                    self.def_type(elem_ty, out, defined);
                }
                let mut fields_code = String::new();
                for (i, &elem_ty) in elements.iter().enumerate() {
                    fields_code += &format!("    {} f{};\n", self.ty_to_c(elem_ty), i);
                }
                *out += &format!("typedef struct {{\n{}}} {};\n\n", fields_code, c_name);
            }
            TypeNodeKind::Struct {
                decl_id,
                subst,
                field_tys,
            } => {
                let key = (*decl_id, subst.clone());
                if let Some(def) = self.type_checker_result.concrete_type_defs.get(&key) {
                    for &f_ty in field_tys {
                        self.def_type(f_ty, out, defined);
                    }
                    let mut fields_code = String::new();
                    for (i, &f_ty) in field_tys.iter().enumerate() {
                        fields_code += &format!("    {} f{};\n", self.ty_to_c(f_ty), i);
                    }
                    *out += &format!("typedef struct {{\n{}}} {};\n\n", fields_code, def.name);
                }
            }
            TypeNodeKind::ADT { decl_id, subst, .. } => {
                let key = (*decl_id, subst.clone());
                if let Some(def) = self.type_checker_result.concrete_type_defs.get(&key) {
                    if let TypeDefKind::Enum { variants } = &def.kind {
                        for v in variants {
                            if v.fields.len() == 1 {
                                self.def_type(v.fields[0].ty, out, defined);
                            }
                        }
                        let mut union_fields = String::new();
                        let mut has_payload = false;
                        for (i, v) in variants.iter().enumerate() {
                            if v.fields.len() == 1 {
                                has_payload = true;
                                let payload_ty = self.ty_to_c(v.fields[0].ty);
                                union_fields +=
                                    &format!("        {} v{}; /* {} */\n", payload_ty, i, v.name);
                            }
                        }
                        if !has_payload {
                            union_fields += "        int _dummy;\n";
                        }
                        *out += &format!(
                            "typedef struct {{\n    int tag;\n    union {{\n{}    }} data;\n}} {};\n\n",
                            union_fields, def.name
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn gen_type_definitions(&self) -> String {
        let mut out = String::new();
        let mut defined = HashSet::new();
        let mut used_types = HashSet::new();

        for fun in &self.mono_mir.functions {
            for &ty in &fun.signature.params {
                used_types.insert(ty);
            }
            used_types.insert(fun.signature.return_ty);
            for local in &fun.local_decls {
                used_types.insert(local.ty);
            }
        }
        for stat in &self.mono_mir.statics {
            used_types.insert(stat.ty);
        }

        let mut ty_ids: Vec<TyId> = used_types.into_iter().collect();
        ty_ids.sort();

        for ty_id in ty_ids {
            self.def_type(ty_id, &mut out, &mut defined);
        }

        out
    }
    fn new(mono_mir: MirCrate, type_checker_result: TypeCtx) -> Self {
        Self {
            diag,
            mono_mir,
            type_checker_result,
        }
    }
    fn emit(self) -> Result<String, ()> {
        let mut out = String::new();
        out += "#include \"runtime.h\"\n";
        out += &self.gen_type_definitions();
        out += "\n";
        out += &self.gen_externs();
        out += "\n";
        for fun in &self.mono_mir.functions {
            if fun.blocks.is_empty() {
                continue;
            }
            let mangled = self.mangle(&fun.name, &fun.signature.params, fun.signature.return_ty);
            let ret_ty_id = fun.signature.return_ty;
            let ret_root = get_type_root(&self.type_checker_result.type_pool, ret_ty_id);
            let ret_str = if let TypeNodeKind::Fun {
                param_tys: fp,
                return_ty: fr,
            } = &self.type_checker_result.type_pool[ret_root].kind
            {
                let ret_c = self.ty_to_c(*fr);
                let params_c: Vec<String> = fp.iter().map(|&t| self.ty_to_c(t)).collect();
                if fun.name == "main" {
                    "int".to_string()
                } else {
                    format!("{} (*{}())({})", ret_c, mangled, params_c.join(", "))
                }
            } else {
                let mut rty = self.ty_to_c(ret_ty_id);
                if rty.contains("%s") {
                    rty = rty.replace("%s", "");
                }
                if fun.name == "main" {
                    "int".to_string()
                } else {
                    rty
                }
            };
            let param_tys: Vec<String> = fun
                .signature
                .params
                .iter()
                .map(|&t| self.ty_to_c(t))
                .collect();
            if let TypeNodeKind::Fun { .. } = &self.type_checker_result.type_pool[ret_root].kind {
                out += &format!("{};\n", ret_str);
            } else {
                out += &format!("{} {}({});\n", ret_str, mangled, param_tys.join(", "));
            }
        }
        out += "\n";
        out += &self.gen_globals();
        out += "\n";
        for (i, fun) in self.mono_mir.functions.iter().enumerate() {
            if fun.blocks.is_empty() {
                continue;
            }
            out += &self.gen_function(fun, MirFunId(i));
        }
        Ok(out)
    }
}
