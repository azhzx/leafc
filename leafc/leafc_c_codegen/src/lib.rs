use std::collections::HashMap;
use leafc_coreapi::codegen::CodegenApi;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lang_items::BuiltinType;
use leafc_coreapi::mir::*;
use leafc_coreapi::type_system::TypeCtx;
use leafc_coreapi::type_system::{get_type_root, TyId, TypeNodeKind};

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
                _ => "unknown".into(),
            },
            TypeNodeKind::Tuple(elems) if elems.is_empty() => "void".into(),
            TypeNodeKind::Tuple(..) => todo!(),
            TypeNodeKind::Fun { .. } => todo!(),
            TypeNodeKind::Never => "void*".into(),
            _ => format!("/* ty{} */", ty_id),
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
            Place::Deref(p) => format!("(*{})", self.place_to_c(p, var_names)),
            _ => format!("/* unsupported place */"),
        }
    }

    fn static_name(&self, sid: StaticId) -> String {
        self.mono_mir.statics[sid].name.clone()
    }

    fn rvalue_to_c(&self, rv: &Rvalue, var_names: &HashMap<LocalId, String>) -> String {
        match rv {
            Rvalue::Constant(c) => Self::const_to_c(c),
            Rvalue::Move(place) | Rvalue::Copy(place) => self.place_to_c(place, var_names),
            Rvalue::BinaryOp { op, left, right } => {
                let l = self.rvalue_to_c(left, var_names);
                let r = self.rvalue_to_c(right, var_names);
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
                let r = self.rvalue_to_c(right, var_names);
                let op_str = match op {
                    MirUnOp::Neg => "-",
                    MirUnOp::Not => "!",
                };
                format!("({}{})", op_str, r)
            }
            Rvalue::Cast(place, target_ty) => {
                let p = self.place_to_c(place, var_names);
                let ty = self.ty_to_c(*target_ty);
                format!("(({}){})", ty, p)
            }
            _ => format!("/* unsupported rvalue */"),
        }
    }

    fn gen_function(&self, fun: &MirFun, fun_id: FunId) -> String {
        let pool = &self.type_checker_result.type_pool;
        let blocks = &self.mono_mir.blocks;

        let mangled = self.mangle(&fun.name, &fun.signature.params);
        let ret_ty = self.ty_to_c(fun.signature.return_ty);
        let returns_value = ret_ty != "void";

        let mut var_names: HashMap<LocalId, String> = HashMap::new();
        for (i, decl) in fun.local_decls.iter().enumerate() {
            if i == 0 && !returns_value {
                continue;
            }
            let mut name = decl.name.clone().unwrap_or_else(|| format!("v{}", i));
            if name == "return" {
                name = "_ret".to_string();
            }
            var_names.insert(i, name);
        }
        if returns_value {
            var_names.insert(0, "_ret".into());
        }

        let param_count = fun.signature.params.len();
        let params: Vec<String> = (0..param_count)
            .map(|i| {
                let local_id = 1 + i;
                let ty = self.ty_to_c(fun.local_decls[local_id].ty);
                let name = var_names[&local_id].clone();
                format!("{} {}", ty, name)
            })
            .collect();
        let param_str = params.join(", ");

        let mut code = String::new();

        code += &format!("{} {}({}) {{\n", ret_ty, mangled, param_str);

        for (i, decl) in fun.local_decls.iter().enumerate() {
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
            let name = &var_names[&i];
            code += &format!("    {} {};\n", ty, name);
        }
        if returns_value {
            code += &format!("    {} _ret;\n", ret_ty);
        }

        for &bid in &fun.blocks {
            let block = &blocks[bid];
            code += &format!("  block_{}:\n", bid);

            // 语句
            for stmt in &block.statements {
                match &stmt.kind {
                    MirStmtKind::Let { local, rvalue } => {
                        let lhs = var_names[local].clone();
                        let rhs = self.rvalue_to_c(rvalue, &var_names);
                        code += &format!("    {} = {};\n", lhs, rhs);
                    }
                    MirStmtKind::Store { place, rvalue } => {
                        let lhs = self.place_to_c(place, &var_names);
                        let rhs = self.rvalue_to_c(rvalue, &var_names);
                        code += &format!("    {} = {};\n", lhs, rhs);
                    }
                    MirStmtKind::Nop => {
                        code += "    ;\n";
                    }
                }
            }

            // 终结器
            code += &self.gen_terminator(block, bid, fun, &var_names, &blocks);
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
    ) -> String {
        let mut code = String::new();
        match &block.terminator {
            TerminatorKind::Goto { target, block_args } => {
                // 为目标块参数赋值
                let target_params = &blocks[*target].block_params;
                for (param_id, arg) in target_params.iter().zip(block_args.iter()) {
                    let lhs = var_names[param_id].clone();
                    let rhs = self.rvalue_to_c(arg, var_names);
                    code += &format!("    {} = {};\n", lhs, rhs);
                }
                code += &format!("    goto block_{};\n", target);
            }
            TerminatorKind::SwitchInt {
                discriminant,
                targets,
                default,
            } => {
                let disc = self.rvalue_to_c(discriminant, var_names);
                code += &format!("    switch ({}) {{\n", disc);
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
                let callee_mangled = self.mangle(&callee.name, &callee.signature.params);
                let arg_str: Vec<String> = args
                    .iter()
                    .map(|a| self.rvalue_to_c(a, var_names))
                    .collect();
                let call_expr = format!("{}({})", callee_mangled, arg_str.join(", "));

                // 返回值处理
                if self.ty_to_c(callee.signature.return_ty) != "void" {
                    let lhs = self.place_to_c(dest, var_names);
                    code += &format!("    {} = {};\n", lhs, call_expr);
                } else {
                    code += &format!("    {};\n", call_expr);
                }

                // 跳转
                if let Some(t) = target {
                    code += &format!("    goto block_{};\n", t);
                }
            }
            TerminatorKind::Return => {
                if self.ty_to_c(fun.signature.return_ty) != "void" {
                    code += "    return _ret;\n";
                } else {
                    code += "    return;\n";
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
            let init = Self::const_to_c(&stat.init);
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
            code += &format!("extern {} {}({});\n", ret_ty, ext.name, params.join(", "));
        }
        code
    }
}

impl CodegenApi for CCodeGen {
    type Output = String;

    fn new(mono_mir: MirCrate, type_checker_result: TypeCtx) -> Self {
        Self {
            mono_mir,
            type_checker_result,
        }
    }

    fn emit(self) -> Result<Self::Output, DiagMsg> {
        let mut out = String::new();
        out += "#include <stdint.h>\n#include <stdbool.h>\n#include <stddef.h>\n\n";

        out += &self.gen_externs();
        out += "\n";

        for fun in &self.mono_mir.functions {
            let mangled = self.mangle(&fun.name, &fun.signature.params);
            let ret_ty = self.ty_to_c(fun.signature.return_ty);
            let param_tys: Vec<String> = fun
                .signature
                .params
                .iter()
                .map(|t| self.ty_to_c(*t))
                .collect();
            out += &format!("{} {}({});\n", ret_ty, mangled, param_tys.join(", "));
        }
        out += "\n";

        out += &self.gen_globals();
        out += "\n";

        for (i, fun) in self.mono_mir.functions.iter().enumerate() {
            out += &self.gen_function(fun, i);
        }

        Ok(out)
    }
}