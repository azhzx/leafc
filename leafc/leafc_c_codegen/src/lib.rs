use leafc_coreapi::codegen::CodegenApi;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lang_items::BuiltinType;
use leafc_coreapi::mir::*;
use leafc_coreapi::type_system::{get_type_root, TyId, TypeNodeKind};
use leafc_coreapi::type_system::{TypeCtx, TypeDefKind};
use std::collections::HashMap;

pub struct CCodeGen {
    mono_mir: MirCrate,
    type_checker_result: TypeCtx,
}

impl CCodeGen {
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
                BuiltinType::Ptr => "void*".into(),
                _ => "unknown".into(),
            },
            TypeNodeKind::Ref(inner) | TypeNodeKind::MutRef(inner)=> {
                let inner_c = self.ty_to_c(*inner);
                if inner_c.contains("%s") {
                    inner_c.replacen("%s", "*%s", 1)
                } else {
                    format!("{}*", inner_c)
                }
            }
            TypeNodeKind::Share(inner) => {
                todo!()
            }
            TypeNodeKind::Tuple(elems) if elems.is_empty() => "void".into(),
            TypeNodeKind::Tuple(elems) => {
                let inner: Vec<String> = elems.iter().map(|&t| self.ty_to_c(t)).collect();
                format!("Tuple_{}", inner.join("_"))
            }
            TypeNodeKind::Fun { param_tys, return_ty } => {
                let ret = self.ty_to_c(*return_ty);
                let params: Vec<String> = param_tys.iter()
                    .map(|&t| self.ty_to_c(t))
                    .collect();
                format!("{} (*%s)({})", ret, params.join(", "))
            }
            TypeNodeKind::Struct { decl_id, subst, .. }
            | TypeNodeKind::ADT { decl_id, subst, .. } => {
                let key = (*decl_id, subst.clone());
                if let Some(def) = self.type_checker_result.concrete_type_defs.get(&key) {
                    def.name.clone()
                } else {
                    // fallback
                    if let Some(def) = self.type_checker_result.generic_type_defs.get(decl_id) {
                        def.name.clone()
                    } else {
                        format!("/* unknown_type_{} */", decl_id)
                    }
                }
            }
            TypeNodeKind::Never => "void*".into(),
            _ => format!("/* ty{} */", ty_id),
        }
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
                        self.type_checker_result.type_pool.iter().position(|n| {
                            matches!(&n.kind, TypeNodeKind::Tuple(e) if e.is_empty())
                        }).unwrap()
                    })
                } else {
                    unreachable!()
                }
            }
            _ => unreachable!(),
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
        }
    }

    fn mangle(&self, name: &str, param_tys: &[TyId]) -> String {
        if name == "main" && param_tys.is_empty() {
            return "main".into();
        }
        let ty_names: Vec<String> = param_tys.iter().map(|&t| self.ty_to_c(t)).collect();
        format!("{}_{}", name, ty_names.join("_"))
    }

    fn place_to_c(&self, place: &Place, var_names: &HashMap<LocalId, String>) -> String {
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

    fn static_name(&self, sid: StaticId) -> String {
        self.mono_mir.statics[sid].name.clone()
    }

    fn rvalue_to_c(&self, rv: &Rvalue, var_names: &HashMap<LocalId, String>, fun: &MirFun) -> String {
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
            Rvalue::TempRef(place) | Rvalue::TempRefMut(place) => {
                format!("&({})", self.place_to_c(place, var_names))
            }

            _ => unreachable!("should be handled by rvalue_to_c_with_ty")
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
        var_names: &HashMap<LocalId, String>,
        fun: &MirFun,
    ) -> String {
        match rv {
            Rvalue::BuildStruct(vals) => {
                let type_name = self.ty_to_c(ty);
                let field_vals: Vec<String> = vals.iter()
                    .map(|v| self.rvalue_to_c(v, var_names, fun))
                    .collect();
                let init_list = if field_vals.is_empty() {
                    "{0}".to_string()
                } else {
                    let fields_str = field_vals.iter().enumerate()
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
                        variants[*tag as usize].is_some()
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
                    let fields_str = elements.iter().enumerate()
                        .map(|(i, e)| format!(".f{} = {}", i, self.rvalue_to_c(
                            e, var_names, fun)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({}){{ {} }}", type_name, fields_str)
                }
            }
            Rvalue::TempRef(place) | Rvalue::TempRefMut(place) => {
                format!("&({})", self.place_to_c(place, var_names))
            }
            Rvalue::GetFunPtr(fun_id) => {
                let callee = &self.mono_mir.functions[*fun_id];
                let mangled = if callee.blocks.is_empty() {
                    callee.name.clone()
                } else {
                    self.mangle(&callee.name, &callee.signature.params)
                };
                mangled
            }

            _ => self.rvalue_to_c(rv, var_names, fun),
        }
    }

    fn gen_function(&self, fun: &MirFun, fun_id: FunId) -> String {
        let blocks = &self.mono_mir.blocks;

        let mangled = self.mangle(&fun.name, &fun.signature.params);

        let is_main = fun.name == "main" && fun.signature.params.is_empty();

        let ret_ty_id = fun.signature.return_ty;
        let ret_root = get_type_root(&self.type_checker_result.type_pool, ret_ty_id);

        let (ret_str, inner_fun_params, inner_fun_ret) =
            if let TypeNodeKind::Fun { param_tys, return_ty } = &self.type_checker_result.type_pool[ret_root].kind {
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
                let ret = if is_main && rty == "void" {
                    "int".to_string()
                } else {
                    rty
                };
                (ret, None, None)
            };

        let original_ret_ty = self.ty_to_c(ret_ty_id);
        let returns_value = ret_str != "void";

        let mut var_names: HashMap<LocalId, String> = HashMap::new();
        var_names.insert(0, "_ret".into());
        let mut var_counter = 0u32;
        for (i, decl) in fun.local_decls.iter().enumerate() {
            if i == 0 { continue; }
            let mut name = decl.name.clone().unwrap_or_else(|| format!("v{}", i));
            if name == "return" {
                name = "_ret".to_string();
            }
            let unique = format!("{}_{}", name, var_counter);
            var_counter += 1;
            var_names.insert(i, unique);
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

        // function sig
        if let TypeNodeKind::Fun { .. } = &self.type_checker_result.type_pool[ret_root].kind {
            code += &format!("{} {{\n", ret_str);
        } else {
            code += &format!("{} {}({}) {{\n", ret_str, mangled, param_str);
        }

        for (i, decl) in fun.local_decls.iter().enumerate() {
            if i == 0 && is_main { continue; }
            if i == 0 && !returns_value { continue; }
            if i == 0 && returns_value { continue; }
            if i >= 1 && i <= param_count { continue; }
            let ty = self.ty_to_c(decl.ty);
            if ty == "void" { continue; }
            let name = &var_names[&i];
            let decl_str = if ty.contains("%s") {
                ty.replace("%s", name)
            } else {
                format!("{} {}", ty, name)
            };
            code += &format!("    {};\n", decl_str);
        }

        if returns_value && !is_main {
            let decl_str = if let Some((ref fun_params, fun_ret)) = inner_fun_params.zip(inner_fun_ret) {
                let ret_c = self.ty_to_c(fun_ret);
                let params_c = fun_params.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", ");
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
            let block = &blocks[bid];
            code += &format!("  block_{}:\n", bid);

            for stmt in &block.statements {
                match &stmt.kind {
                    MirStmtKind::Let { local, rvalue } => {
                        let ty = fun.local_decls[*local].ty;
                        if self.ty_to_c(ty) == "void" { continue; }
                        if is_main && *local == 0 { continue; }
                        let lhs = var_names[local].clone();
                        let rhs = self.rvalue_to_c_with_ty(rvalue, ty, &var_names, fun);
                        code += &format!("    {} = {};\n", lhs, rhs);
                    }
                    MirStmtKind::Store { place, rvalue } => {
                        let ty = self.place_ty(place, fun);
                        if self.ty_to_c(ty) == "void" { continue; }
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

            code += &self.gen_terminator(block, bid, fun, &var_names, blocks, is_main, original_ret_ty.clone());
        }

        code += "}\n\n";
        code
    }

    fn gen_terminator(
        &self,
        block: &BasicBlock,
        block_id: BasicBlockId,
        fun: &MirFun,
        var_names: &HashMap<LocalId, String>,
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
                    if self.ty_to_c(param_ty) == "void" { continue; }
                    let lhs = var_names[param_id].clone();
                    let rhs = self.rvalue_to_c(arg, var_names, fun);
                    code += &format!("    {} = {};\n", lhs, rhs);
                }
                code += &format!("    goto block_{};\n", target);
            }
            TerminatorKind::SwitchInt { discriminant, targets, default } => {
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
            TerminatorKind::Call { func, args, dest, target } => {
                let callee = &self.mono_mir.functions[*func];
                let callee_mangled = if callee.blocks.is_empty() {
                    callee.name.clone()
                } else {
                    self.mangle(&callee.name, &callee.signature.params)
                };
                let arg_str: Vec<String> = args.iter()
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
                if is_main && original_ret_ty == "void" {
                    code += "    return 0;\n";
                } else if self.ty_to_c(fun.signature.return_ty) != "void" {
                    code += "    return _ret;\n";
                } else {
                    code += "    return;\n";
                }
            }
            TerminatorKind::CallByPtr { func, args, dest, target } => {
                let func_expr = self.rvalue_to_c(func, var_names, fun);
                let arg_str: Vec<String> = args.iter()
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
            _ => {
                code += &format!("    /* terminator {:?} */;\n", block.terminator);
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
            let params: Vec<String> = ext.signature.params.iter()
                .enumerate()
                .map(|(i, ty)| format!("{} a{}", self.ty_to_c(*ty), i))
                .collect();
            code += &format!("extern {} {}({});\n", ret_ty, ext.name, params.join(", "));
        }
        code
    }

    fn def_type(
        &self,
        ty_id: TyId,
        out: &mut String,
        defined: &mut std::collections::HashSet<String>,
    ) {
        let c_name = self.ty_to_c(ty_id);
        if c_name == "void" || defined.contains(&c_name) {
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
                defined.insert(c_name);
            }
            TypeNodeKind::Struct { decl_id, subst, field_tys } => {
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
                    defined.insert(def.name.clone());
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
                                union_fields += &format!(
                                    "        {} v{}; /* {} */\n",
                                    payload_ty, i, v.name
                                );
                            }
                        }
                        if !has_payload {
                            union_fields += "        int _dummy;\n";
                        }
                        *out += &format!(
                            "typedef struct {{\n    int tag;\n    union {{\n{}    }} data;\n}} {};\n\n",
                            union_fields, def.name
                        );
                        defined.insert(def.name.clone());
                    }
                }
            }
            _ => {}
        }
    }

    fn gen_type_definitions(&self) -> String {
        let mut out = String::new();
        let mut defined = std::collections::HashSet::new();

        let mut used_types = std::collections::HashSet::new();

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
}

impl CodegenApi for CCodeGen {
    type Output = String;

    fn new(mono_mir: MirCrate, type_checker_result: TypeCtx) -> Self {
        Self { mono_mir, type_checker_result }
    }

    fn emit(self) -> Result<Self::Output, DiagMsg> {
        let mut out = String::new();
        out += "#include \"runtime.h\"\n";

        out += &self.gen_type_definitions();
        out += "\n";

        out += &self.gen_externs();
        out += "\n";

        for fun in &self.mono_mir.functions {
            let mangled = if fun.blocks.is_empty() {
                fun.name.clone()
            } else {
                self.mangle(&fun.name, &fun.signature.params)
            };

            let ret_ty_id = fun.signature.return_ty;
            let ret_root = get_type_root(&self.type_checker_result.type_pool, ret_ty_id);
            let is_main = fun.name == "main" && fun.signature.params.is_empty();

            let ret_str = if let TypeNodeKind::Fun { param_tys: fun_params, return_ty: fun_ret } = &self.type_checker_result.type_pool[ret_root].kind {
                let ret_c = self.ty_to_c(*fun_ret);
                let params_c: Vec<String> = fun_params.iter().map(|&t| self.ty_to_c(t)).collect();
                if is_main { "int".to_string() } else { format!("{} (*{}())({})", ret_c, mangled, params_c.join(", ")) }
            } else {
                let mut rty = self.ty_to_c(ret_ty_id);
                if rty.contains("%s") { rty = rty.replace("%s", ""); }
                if is_main && rty == "void" { "int".to_string() } else { rty }
            };

            let param_tys: Vec<String> = fun.signature.params.iter().map(|t| self.ty_to_c(*t)).collect();
            if let TypeNodeKind::Fun { .. } = &self.type_checker_result.type_pool[ret_root].kind {
                out += &format!("{};\n", ret_str);
            } else {
                out += &format!("{} {}({});\n", ret_str, mangled, param_tys.join(", "));
            }        }

        out += "\n";

        out += &self.gen_globals();
        out += "\n";

        for (i, fun) in self.mono_mir.functions.iter().enumerate() {
            if fun.blocks.is_empty() {
                continue;
            }
            out += &self.gen_function(fun, i);
        }

        Ok(out)
    }
}