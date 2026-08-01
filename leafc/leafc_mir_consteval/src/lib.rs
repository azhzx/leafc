use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lang_items::BuiltinType;
use leafc_coreapi::mir::{BasicBlockId, Const, FunId, MirBinOp, MirCrate, MirStmt, MirStmtKind, MirUnOp, Place, Rvalue, StaticId, TagId, TerminatorKind};
use leafc_coreapi::mir_consteval::MirConstEvalApi;
use leafc_coreapi::source::Span;
use leafc_coreapi::type_system::{get_type_root, TyId, TypeCtx, TypeNode, TypeNodeKind};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Unit,
    Int(i64),
    Float(u64),
    Bool(bool),
    Str(String),
    Tuple(Vec<Value>),
    Struct(Vec<Value>),
    Enum(TagId, Box<Value>),
    Never,
}

struct Frame {
    locals: Vec<Value>,
    return_value: Option<Value>,
}

pub struct MirConstEval {
    mir: MirCrate,
    type_ctx: TypeCtx,
    const_cache: HashMap<FunId, Const>,
    static_cache: HashMap<StaticId, Const>,
}

impl MirConstEval {
    fn const_to_value(constant: &Const) -> Value {
        match constant {
            Const::Int8(v) => Value::Int(*v as i64),
            Const::Int16(v) => Value::Int(*v as i64),
            Const::Int32(v) => Value::Int(*v as i64),
            Const::Int64(v) => Value::Int(*v),
            Const::UInt8(v) => Value::Int(*v as i64),
            Const::UInt16(v) => Value::Int(*v as i64),
            Const::UInt32(v) => Value::Int(*v as i64),
            Const::UInt64(v) => Value::Int(*v as i64),
            Const::Float32(v) => Value::Float(*v),
            Const::Float64(v) => Value::Float(*v),
            Const::Char(v) => Value::Int(*v as i64),
            Const::Str(s) => Value::Str(s.clone()),
            Const::Bool(b) => Value::Bool(*b),
            Const::Unit => Value::Unit,
            Const::Tuple(elems) => Value::Tuple(elems.iter().map(Self::const_to_value).collect()),
            Const::Struct(fields) => Value::Struct(fields.iter().map(Self::const_to_value).collect()),
            Const::Enum(tag, data) => Value::Enum(*tag, Box::new(Self::const_to_value(data))),
        }
    }

    fn value_to_const(value: &Value, ty: TyId, type_pool: &[TypeNode], span: Span) -> Result<Const, DiagMsg> {
        let root = get_type_root(type_pool, ty);
        let kind = &type_pool[root].kind;
        match (value, kind) {
            (Value::Int(v), TypeNodeKind::Builtin(b)) => {
                use BuiltinType::*;
                Ok(match b {
                    I8 => Const::Int8(*v as i8),
                    I16 => Const::Int16(*v as i16),
                    I32 => Const::Int32(*v as i32),
                    I64 => Const::Int64(*v),
                    U8 => Const::UInt8(*v as u8),
                    U16 => Const::UInt16(*v as u16),
                    U32 => Const::UInt32(*v as u32),
                    U64 => Const::UInt64(*v as u64),
                    _ => return Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: "unsupported integer target type".into(),
                        span,
                    }),
                })
            }
            (Value::Float(bits), TypeNodeKind::Builtin(BuiltinType::F32)) => Ok(Const::Float32(*bits)),
            (Value::Float(bits), TypeNodeKind::Builtin(BuiltinType::F64)) => Ok(Const::Float64(*bits)),
            (Value::Bool(b), TypeNodeKind::Builtin(BuiltinType::Bool)) => Ok(Const::Bool(*b)),
            (Value::Str(s), _) => Ok(Const::Str(s.clone())),
            (Value::Unit, TypeNodeKind::Tuple(elems)) if elems.is_empty() => Ok(Const::Unit),
            (Value::Tuple(elems), TypeNodeKind::Tuple(tys)) => {
                if elems.len() != tys.len() {
                    return Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: format!("tuple arity mismatch: expected {}, got {}", tys.len(), elems.len()),
                        span,
                    });
                }
                let consts: Result<Vec<_>, _> = elems.iter().zip(tys)
                    .map(|(v, &ty)| Self::value_to_const(v, ty, type_pool, span.clone()))
                    .collect();
                Ok(Const::Tuple(consts?))
            }
            (Value::Struct(elems), TypeNodeKind::Struct { field_tys, .. }) => {
                if elems.len() != field_tys.len() {
                    return Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: format!("struct field count mismatch: expected {}, got {}", field_tys.len(), elems.len()),
                        span,
                    });
                }
                let consts: Result<Vec<_>, _> = elems.iter().zip(field_tys)
                    .map(|(v, &ty)| Self::value_to_const(v, ty, type_pool, span.clone()))
                    .collect();
                Ok(Const::Struct(consts?))
            }
            (Value::Enum(tag, data), TypeNodeKind::ADT { variants, .. }) => {
                let payload_ty = variants.get(*tag).copied().flatten().ok_or_else(|| DiagMsg {
                    title: "const eval error".into(),
                    msg: format!("invalid enum variant tag {}", tag),
                    span: span.clone(),
                })?;
                let payload_const = Self::value_to_const(data, payload_ty, type_pool, span)?;
                Ok(Const::Enum(*tag, Box::new(payload_const)))
            }
            _ => Err(DiagMsg {
                title: "const eval error".into(),
                msg: "unsupported constant type".into(),
                span,
            }),
        }
    }

    fn eval_rvalue(&self, rvalue: &Rvalue, frame: &Frame, span: Span) -> Result<Value, DiagMsg> {
        match rvalue {
            Rvalue::Constant(c) => Ok(Self::const_to_value(c)),
            Rvalue::Copy(place) | Rvalue::Move(place) => self.eval_place_to_value(place, frame, span),
            Rvalue::BinaryOp { op, left, right } => {
                let l = self.eval_rvalue(left, frame, span.clone())?;
                let r = self.eval_rvalue(right, frame, span.clone())?;
                self.eval_binary(op, l, r, span)
            }
            Rvalue::UnaryOp { op, right } => {
                let v = self.eval_rvalue(right, frame, span.clone())?;
                self.eval_unary(op, v, span)
            }
            Rvalue::Tuple(elems) => {
                let vals: Result<Vec<_>, _> = elems.iter()
                    .map(|e| self.eval_rvalue(e, frame, span.clone()))
                    .collect();
                Ok(Value::Tuple(vals?))
            }
            Rvalue::BuildStruct(fields) => {
                let vals: Result<Vec<_>, _> = fields.iter()
                    .map(|e| self.eval_rvalue(e, frame, span.clone()))
                    .collect();
                Ok(Value::Struct(vals?))
            }
            Rvalue::Variant(tag, inner) => {
                let v = self.eval_rvalue(inner, frame, span.clone())?;
                Ok(Value::Enum(*tag, Box::new(v)))
            }
            Rvalue::Ref(_) | Rvalue::RefMut(_) | Rvalue::GcNewObject(_) | Rvalue::GcObjectRef(_) => {
                Err(DiagMsg {
                    title: "const eval error".into(),
                    msg: "reference or GC operations not allowed in const context".into(),
                    span,
                })
            }
            Rvalue::GetFunPtr(_) => Err(DiagMsg {
                title: "const eval error".into(),
                msg: "function pointers not allowed in const context".into(),
                span,
            }),
            Rvalue::Index { place, item_index } => {
                let base_val = self.eval_place_to_value(place, frame, span.clone())?;
                match base_val {
                    Value::Tuple(elems) | Value::Struct(elems) => {
                        elems.get(*item_index).cloned().ok_or_else(|| DiagMsg {
                            title: "const eval error".into(),
                            msg: "index out of bounds".into(),
                            span,
                        })
                    }
                    _ => Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: "index access on non-compound value".into(),
                        span,
                    }),
                }
            }
            Rvalue::Field { place, item_index } => {
                self.eval_rvalue(&Rvalue::Index { place: place.clone(), item_index: *item_index }, frame, span)
            }
            Rvalue::Len(place) => {
                let val = self.eval_place_to_value(place, frame, span.clone())?;
                match val {
                    Value::Tuple(elems) | Value::Struct(elems) => Ok(Value::Int(elems.len() as i64)),
                    Value::Str(s) => Ok(Value::Int(s.len() as i64)),
                    _ => Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: "len not supported for this type".into(),
                        span,
                    }),
                }
            }
            Rvalue::Tag(place) => {
                let val = self.eval_place_to_value(place, frame, span.clone())?;
                if let Value::Enum(tag, _) = val {
                    Ok(Value::Int(tag as i64))
                } else {
                    Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: "tag access on non-enum value".into(),
                        span,
                    })
                }
            }
            Rvalue::Cast(place, target_ty) => {
                let val = self.eval_place_to_value(place, frame, span.clone())?;
                self.cast_value(val, *target_ty, span)
            }
            Rvalue::HandlerArg(_) => todo!()
        }
    }

    fn cast_value(&self, value: Value, target_ty: TyId, span: Span) -> Result<Value, DiagMsg> {
        let root = get_type_root(&self.type_ctx.type_pool, target_ty);
        let kind = &self.type_ctx.type_pool[root].kind;
        match (value, kind) {
            (Value::Int(i), TypeNodeKind::Builtin(b)) => {
                use BuiltinType::*;
                match b {
                    I8 | I16 | I32 | I64 | U8 | U16 | U32 | U64 => Ok(Value::Int(i)),
                    F32 => Ok(Value::Float((i as f32).to_bits() as u64)),
                    F64 => Ok(Value::Float((i as f64).to_bits())),
                    _ => Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: "unsupported cast target".into(),
                        span,
                    }),
                }
            }
            (Value::Float(bits), TypeNodeKind::Builtin(b)) => match b {
                BuiltinType::I32 => Ok(Value::Int(f64::from_bits(bits) as i64)),
                BuiltinType::F64 => Ok(Value::Float(bits)),
                _ => Err(DiagMsg {
                    title: "const eval error".into(),
                    msg: "unsupported cast from float".into(),
                    span,
                }),
            },
            _ => Err(DiagMsg {
                title: "const eval error".into(),
                msg: "cast not supported for these types".into(),
                span,
            }),
        }
    }

    fn eval_place_to_value(&self, place: &Place, frame: &Frame, span: Span) -> Result<Value, DiagMsg> {
        match place {
            Place::Local(id) => frame.locals.get(*id).cloned().ok_or_else(|| DiagMsg {
                title: "const eval error".into(),
                msg: format!("local {} not initialized", id),
                span,
            }),
            Place::Static(sid) => {
                self.static_cache.get(sid)
                    .map(|c| Self::const_to_value(c))
                    .ok_or_else(|| DiagMsg {
                        title: "const eval error".into(),
                        msg: format!("static {} not evaluated", sid),
                        span,
                    })
            }
            Place::Field { base, field } => {
                let base_val = self.eval_place_to_value(base, frame, span.clone())?;
                match base_val {
                    Value::Tuple(elems) | Value::Struct(elems) => {
                        elems.get(*field).cloned().ok_or_else(|| DiagMsg {
                            title: "const eval error".into(),
                            msg: format!("field index {} out of bounds", field),
                            span,
                        })
                    }
                    _ => Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: "field access on non-compound value".into(),
                        span,
                    }),
                }
            }
            Place::EnumItem { place: inner, variant } => {
                let val = self.eval_place_to_value(inner, frame, span.clone())?;
                match val {
                    Value::Enum(tag, data) if tag == *variant => Ok(*data),
                    _ => Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: "enum variant mismatch".into(),
                        span,
                    }),
                }
            }
            Place::Deref(_) => Err(DiagMsg {
                title: "const eval error".into(),
                msg: "dereference not allowed in const context".into(),
                span,
            }),
            Place::Index { .. } => Err(DiagMsg {
                title: "const eval error".into(),
                msg: "index place not supported".into(),
                span,
            }),
        }
    }

    fn eval_binary(&self, op: &MirBinOp, l: Value, r: Value, span: Span) -> Result<Value, DiagMsg> {
        use MirBinOp::*;
        match op {
            Add => Ok(Value::Int(l.as_int(span.clone())? + r.as_int(span.clone())?)),
            Sub => Ok(Value::Int(l.as_int(span.clone())? - r.as_int(span.clone())?)),
            Mul => Ok(Value::Int(l.as_int(span.clone())? * r.as_int(span.clone())?)),
            Div => {
                let rv = r.as_int(span.clone())?;
                if rv == 0 {
                    return Err(DiagMsg { title: "const eval error".into(), msg: "division by zero".into(), span });
                }
                Ok(Value::Int(l.as_int(span.clone())? / rv))
            }
            Rem => {
                let rv = r.as_int(span.clone())?;
                if rv == 0 {
                    return Err(DiagMsg { title: "const eval error".into(), msg: "modulo by zero".into(), span });
                }
                Ok(Value::Int(l.as_int(span.clone())? % rv))
            }
            Eq => Ok(Value::Bool(l == r)),
            Ne => Ok(Value::Bool(l != r)),
            Lt => {
                match (&l, &r) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f64::from_bits(*a) < f64::from_bits(*b))),
                    _ => Err(DiagMsg { title: "const eval error".into(), msg: "comparison requires numbers".into(), span }),
                }
            }
            Le => {
                match (&l, &r) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f64::from_bits(*a) <= f64::from_bits(*b))),
                    _ => Err(DiagMsg { title: "const eval error".into(), msg: "comparison requires numbers".into(), span }),
                }
            }
            Gt => {
                match (&l, &r) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f64::from_bits(*a) > f64::from_bits(*b))),
                    _ => Err(DiagMsg { title: "const eval error".into(), msg: "comparison requires numbers".into(), span }),
                }
            }
            Ge => {
                match (&l, &r) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
                    (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f64::from_bits(*a) >= f64::from_bits(*b))),
                    _ => Err(DiagMsg { title: "const eval error".into(), msg: "comparison requires numbers".into(), span }),
                }
            }
            BitAnd | BitOr | BitXor | Shl | Shr => Err(DiagMsg {
                title: "const eval error".into(),
                msg: "bitwise operations not yet supported".into(),
                span,
            }),
        }
    }

    fn eval_unary(&self, op: &MirUnOp, v: Value, span: Span) -> Result<Value, DiagMsg> {
        match op {
            MirUnOp::Neg => match v {
                Value::Int(i) => Ok(Value::Int(-i)),
                Value::Float(bits) => Ok(Value::Float((-f64::from_bits(bits)).to_bits())),
                _ => Err(DiagMsg { title: "const eval error".into(), msg: "negation on non-numeric".into(), span }),
            },
            MirUnOp::Not => match v {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Err(DiagMsg { title: "const eval error".into(), msg: "logical not on non-bool".into(), span }),
            },
        }
    }

    fn eval_function(&mut self, fun_id: FunId, args: Vec<Value>) -> Result<Value, DiagMsg> {
        // 缓存命中
        if let Some(c) = self.const_cache.get(&fun_id) {
            return Ok(Self::const_to_value(c));
        }

        let fun = self.mir.functions[fun_id].clone();
        if !fun.is_consteval {
            return Err(DiagMsg {
                title: "const eval error".into(),
                msg: format!("function '{}' is not consteval", fun.name),
                span: fun.span.clone(),
            });
        }

        let return_local = fun.local_decls
            .iter()
            .position(|d| d.name.as_deref() == Some("return_val"))
            .unwrap_or(0);

        let mut locals = vec![Value::Unit; fun.local_decls.len()];
        for (i, arg) in args.iter().enumerate() {
            let param_local = return_local + 1 + i;
            if param_local < fun.local_decls.len() {
                locals[param_local] = arg.clone();
            } else {
                return Err(DiagMsg {
                    title: "const eval error".into(),
                    msg: format!("parameter index out of bounds: {} >= {}", param_local, fun.local_decls.len()),
                    span: fun.span.clone(),
                });
            }
        }

        let mut frame = Frame { locals, return_value: None };

        let mut current_block = fun.blocks[0];
        loop {
            let block_span;
            let terminator;
            {
                let block = &self.mir.blocks[current_block];
                block_span = block.span.clone();

                for stmt in &block.statements {
                    let stmt_span = stmt.span.clone();
                    match &stmt.kind {
                        MirStmtKind::Let { local, rvalue } => {
                            let val = self.eval_rvalue(rvalue, &frame, stmt_span)?;
                            frame.locals[*local] = val;
                        }
                        MirStmtKind::Store { place, rvalue } => {
                            let val = self.eval_rvalue(rvalue, &frame, stmt_span.clone())?;
                            match place {
                                Place::Local(id) => frame.locals[*id] = val,
                                Place::Field { base, field } => {
                                    let mut base_val = self.eval_place_to_value(
                                        base, &frame, stmt_span.clone()
                                    )?;
                                    match &mut base_val {
                                        Value::Tuple(elems) | Value::Struct(elems) => {
                                            if *field < elems.len() {
                                                elems[*field] = val;
                                            } else {
                                                return Err(DiagMsg {
                                                    title: "const eval error".into(),
                                                    msg: format!("field index {} out of bounds", field),
                                                    span: stmt_span,
                                                });
                                            }
                                            if let Place::Local(id) = base.as_ref() {
                                                frame.locals[*id] = base_val;
                                            } else {
                                                return Err(DiagMsg {
                                                    title: "const eval error".into(),
                                                    msg: "store to non-local compound place not supported".into(),
                                                    span: stmt_span,
                                                });
                                            }
                                        }
                                        _ => return Err(DiagMsg {
                                            title: "const eval error".into(),
                                            msg: "store field on non-compound value".into(),
                                            span: stmt_span,
                                        }),
                                    }
                                }
                                _ => return Err(DiagMsg {
                                    title: "const eval error".into(),
                                    msg: "store to non-local place not supported".into(),
                                    span: stmt_span,
                                }),
                            }
                        }
                        MirStmtKind::Nop => {}
                    }
                }

                terminator = block.terminator.clone();
            }

            match &terminator {
                TerminatorKind::Return => {
                    // 从 return_local 读取真正的返回值
                    let ret = frame.locals.get(return_local).cloned().unwrap_or(Value::Unit);
                    let const_ret = Self::value_to_const(&ret, fun.signature.return_ty, &self.type_ctx.type_pool, block_span)?;
                    self.const_cache.insert(fun_id, const_ret);
                    return Ok(ret);
                }
                TerminatorKind::Goto { target, block_args } => {
                    if !block_args.is_empty() {
                        let target_block = &self.mir.blocks[*target];
                        if let Some(&first_param) = target_block.block_params.first() {
                            let val = self.eval_rvalue(&block_args[0], &frame, block_span.clone())?;
                            frame.locals[first_param] = val;
                        }
                    }
                    current_block = *target;
                }
                TerminatorKind::SwitchInt { discriminant, targets, default } => {
                    let disc_val = self.eval_rvalue(discriminant, &frame, block_span.clone())?;
                    let mut next = *default;
                    for (c, target) in targets {
                        let const_val = Self::const_to_value(c);
                        if disc_val == const_val {
                            next = *target;
                            break;
                        }
                    }
                    current_block = next;
                }
                TerminatorKind::Call { func, args, dest, target } => {
                    let mut call_args = Vec::new();
                    for a in args {
                        call_args.push(self.eval_rvalue(a, &frame, block_span.clone())?);
                    }
                    let ret_val = self.eval_function(*func, call_args)?;

                    match dest {
                        Place::Local(id) => frame.locals[*id] = ret_val.clone(),
                        _ => return Err(DiagMsg { title: "const eval error".into(), msg: "unsupported call dest".into(), span: block_span }),
                    }

                    if let Some(t) = target {
                        current_block = *t;
                    } else {
                        let const_ret = Self::value_to_const(&ret_val, fun.signature.return_ty, &self.type_ctx.type_pool, block_span)?;
                        self.const_cache.insert(fun_id, const_ret);
                        return Ok(ret_val);
                    }
                }
                TerminatorKind::CallByPtr { .. } => {
                    return Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: "call by pointer not allowed in const context".into(),
                        span: block_span,
                    });
                }
                TerminatorKind::Raise { .. } => {
                    return Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: "raise not allowed in const context".into(),
                        span: block_span,
                    });
                }
                TerminatorKind::InstallHandler { .. } | TerminatorKind::Resume { .. } => {
                    return Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: "effect handlers not allowed in const context".into(),
                        span: block_span,
                    });
                }
                TerminatorKind::Unreachable => {
                    return Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: "reached unreachable code".into(),
                        span: block_span,
                    });
                }
            }
        }
    }
}

impl Value {
    fn as_int(&self, span: Span) -> Result<i64, DiagMsg> {
        match self {
            Value::Int(v) => Ok(*v),
            _ => Err(DiagMsg {
                title: "const eval error".into(),
                msg: "expected integer value".into(),
                span,
            }),
        }
    }
}

impl MirConstEvalApi for MirConstEval {
    fn new(mir: MirCrate, type_ctx: TypeCtx) -> Self {
        MirConstEval {
            mir,
            type_ctx,
            const_cache: HashMap::new(),
            static_cache: HashMap::new(),
        }
    }

    fn eval(mut self) -> Result<(MirCrate, TypeCtx), DiagMsg> {
        // 初始化 static cache
        for (sid, s) in self.mir.statics.iter().enumerate() {
            self.static_cache.insert(sid, s.init.clone());
        }

        // 第一阶段：收集所有可常量折叠的 const fn 调用（支持局部常量传播）
        let mut const_calls: Vec<(FunId, BasicBlockId, Place, Option<BasicBlockId>, Vec<Value>)> = Vec::new();
        for fid in 0..self.mir.functions.len() {
            let fun = &self.mir.functions[fid];
            for &block_id in &fun.blocks {
                let block = &self.mir.blocks[block_id];
                if let TerminatorKind::Call { func, args, dest, target } = &block.terminator {
                    if self.mir.functions[*func].is_consteval {
                        let mut const_args = Vec::new();
                        let mut all_const = true;
                        for a in args {
                            match a {
                                Rvalue::Constant(c) => {
                                    const_args.push(Self::const_to_value(c));
                                }
                                Rvalue::Move(Place::Local(local_id))
                                | Rvalue::Copy(Place::Local(local_id)) => {
                                    // 在该基本块的语句中寻找对应的常量定义
                                    let mut found = false;
                                    for stmt in &block.statements {
                                        if let MirStmtKind::Let { local, rvalue } = &stmt.kind {
                                            if *local == *local_id {
                                                if let Rvalue::Constant(c) = rvalue {
                                                    const_args.push(Self::const_to_value(c));
                                                    found = true;
                                                }
                                                break;
                                            }
                                        }
                                    }
                                    if !found {
                                        all_const = false;
                                        break;
                                    }
                                }
                                _ => {
                                    all_const = false;
                                    break;
                                }
                            }
                        }
                        if all_const {
                            const_calls.push((*func, block_id, dest.clone(), *target, const_args));
                        }
                    }
                }
            }
        }

        // 第二阶段：求值并生成替换
        let mut replacements: Vec<(BasicBlockId, Place, Const, Option<BasicBlockId>)> = Vec::new();
        for (func_id, block_id, dest, target, const_args) in const_calls {
            let result = self.eval_function(func_id, const_args)?;
            let ret_ty = self.mir.functions[func_id].signature.return_ty;
            let span = self.mir.blocks[block_id].span.clone();
            let const_result = Self::value_to_const(&result, ret_ty, &self.type_ctx.type_pool, span)?;
            replacements.push((block_id, dest, const_result, target));
        }

        // 第三阶段：应用替换
        for (block_id, dest, const_val, target) in replacements {
            let block = &mut self.mir.blocks[block_id];
            if let Place::Local(local) = dest {
                block.statements.push(MirStmt {
                    kind: MirStmtKind::Let {
                        local,
                        rvalue: Rvalue::Constant(const_val),
                    },
                    span: block.span.clone(),
                });
            }
            block.terminator = if let Some(t) = target {
                TerminatorKind::Goto {
                    target: t,
                    block_args: vec![],
                }
            } else {
                TerminatorKind::Unreachable
            };
        }

        // 写回 static
        for (sid, const_val) in &self.static_cache {
            self.mir.statics[*sid].init = const_val.clone();
        }

        Ok((self.mir, self.type_ctx))
    }
}