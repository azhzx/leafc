use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lang_items::BuiltinType;
use leafc_coreapi::mir::{BasicBlockId, Const, ControlId, FunId, LocalId, MirBinOp, MirCrate, MirFun, MirStmt, MirStmtKind, MirUnOp, Place, Rvalue, StaticId, TagId, TerminatorKind};
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

#[derive(Clone, Debug)]
struct Frame {
    locals: Vec<Value>,
    current_block_params: Vec<LocalId>,
}


#[derive(Clone, Debug)]
struct Continuation {
    frames: Vec<Context>,
    resume_target: BasicBlockId,
    dest: Place,
}

#[derive(Clone, Debug)]
struct Context {
    fun_id: FunId,
    frame: Frame,
    current_block: BasicBlockId,
    saved_handler_depth: usize,
    has_returned: bool,

    caller_ctx_idx: Option<usize>,
    ret_dest: Option<Place>,
    ret_target: Option<BasicBlockId>,
}

struct SuspendedRaise {
    continuation: Continuation,
    handler_ctx_idx: usize,
}

struct HandlerEntry {
    control_id: ControlId,
    ctx_idx: usize,
    handler_block: BasicBlockId,
    merge_block: BasicBlockId,
    args_dest: Vec<LocalId>,
}

pub struct MirConstEval {
    mir: MirCrate,
    type_ctx: TypeCtx,
    const_cache: HashMap<FunId, Const>,
    static_cache: HashMap<StaticId, Const>,
    context_stack: Vec<Context>,
    global_handlers: Vec<HandlerEntry>,
    suspended_raise: Option<SuspendedRaise>,
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

    fn value_to_const(
        value: &Value,
        ty: TyId,
        type_pool: &[TypeNode],
        span: Span,
    ) -> Result<Const, DiagMsg> {
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
            (Value::Float(bits), TypeNodeKind::Builtin(BuiltinType::F32)) => {
                Ok(Const::Float32(*bits))
            }
            (Value::Float(bits), TypeNodeKind::Builtin(BuiltinType::F64)) => {
                Ok(Const::Float64(*bits))
            }
            (Value::Bool(b), TypeNodeKind::Builtin(BuiltinType::Bool)) => Ok(Const::Bool(*b)),
            (Value::Str(s), _) => Ok(Const::Str(s.clone())),
            (Value::Unit, TypeNodeKind::Tuple(elems)) if elems.is_empty() => Ok(Const::Unit),
            (Value::Tuple(elems), TypeNodeKind::Tuple(tys)) => {
                if elems.len() != tys.len() {
                    return Err(DiagMsg {
                        title: "const eval error".into(),
                        msg: format!("tuple arity mismatch: expected {}, got {}",
                                     tys.len(), elems.len()),
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
                        msg: format!("struct field count mismatch: expected {}, got {}",
                                     field_tys.len(), elems.len()),
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
            Rvalue::Ref(_) | Rvalue::RefMut(_) | Rvalue::GcNewObject(_) | Rvalue::GcObjectRef(_) =>
                Err(DiagMsg { title: "const eval error".into(), msg: "ref/gc not allowed in const".into(), span }),
            Rvalue::GetFunPtr(_) => Err(DiagMsg { title: "const eval error".into(), msg: "fun ptr not allowed".into(), span }),
            Rvalue::Index { place, item_index } => {
                let base = self.eval_place_to_value(place, frame, span.clone())?;
                match base {
                    Value::Tuple(e) | Value::Struct(e) => e.get(*item_index).cloned().ok_or_else(|| DiagMsg {
                        title: "const eval error".into(), msg: "index out of bounds".into(), span,
                    }),
                    _ => Err(DiagMsg { title: "const eval error".into(), msg: "index on non-compound".into(), span }),
                }
            }
            Rvalue::Field { place, item_index } => self.eval_rvalue(&Rvalue::Index { place: place.clone(), item_index: *item_index }, frame, span),
            Rvalue::Len(place) => {
                let val = self.eval_place_to_value(place, frame, span.clone())?;
                match val {
                    Value::Tuple(e) | Value::Struct(e) => Ok(Value::Int(e.len() as i64)),
                    Value::Str(s) => Ok(Value::Int(s.len() as i64)),
                    _ => Err(DiagMsg { title: "const eval error".into(), msg: "len not supported".into(), span }),
                }
            }
            Rvalue::Tag(place) => {
                let val = self.eval_place_to_value(place, frame, span.clone())?;
                if let Value::Enum(tag, _) = val { Ok(Value::Int(tag as i64)) }
                else { Err(DiagMsg { title: "const eval error".into(), msg: "tag on non-enum".into(), span }) }
            }
            Rvalue::Cast(place, target_ty) => {
                let val = self.eval_place_to_value(place, frame, span.clone())?;
                self.cast_value(val, *target_ty, span)
            }
            Rvalue::HandlerArg(idx) => {
                let local_id = frame.current_block_params.get(*idx).ok_or_else(|| DiagMsg {
                    title: "const eval error".into(), msg: "invalid handler arg index".into(), span: span.clone(),
                })?;
                frame.locals.get(*local_id).cloned().ok_or_else(|| DiagMsg {
                    title: "const eval error".into(), msg: "handler arg not initialized".into(), span,
                })
            }
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
                    _ => Err(DiagMsg { title: "const eval error".into(), msg: "unsupported cast target".into(), span }),
                }
            }
            (Value::Float(bits), TypeNodeKind::Builtin(b)) => match b {
                BuiltinType::I32 => Ok(Value::Int(f64::from_bits(bits) as i64)),
                BuiltinType::F64 => Ok(Value::Float(bits)),
                _ => Err(DiagMsg { title: "const eval error".into(), msg: "unsupported cast from float".into(), span }),
            },
            _ => Err(DiagMsg { title: "const eval error".into(), msg: "cast not supported".into(), span }),
        }
    }

    fn eval_place_to_value(&self, place: &Place, frame: &Frame, span: Span) -> Result<Value, DiagMsg> {
        match place {
            Place::Local(id) => frame.locals.get(*id).cloned().ok_or_else(|| DiagMsg {
                title: "const eval error".into(), msg: format!("local {} not initialized", id), span,
            }),
            Place::Static(sid) => self.static_cache.get(sid).map(|c| Self::const_to_value(c)).ok_or_else(|| DiagMsg {
                title: "const eval error".into(), msg: format!("static {} not evaluated", sid), span,
            }),
            Place::Field { base, field } => {
                let base_val = self.eval_place_to_value(base, frame, span.clone())?;
                match base_val {
                    Value::Tuple(e) | Value::Struct(e) => e.get(*field).cloned().ok_or_else(|| DiagMsg {
                        title: "const eval error".into(), msg: format!("field {} out of bounds", field), span,
                    }),
                    _ => Err(DiagMsg { title: "const eval error".into(), msg: "field on non-compound".into(), span }),
                }
            }
            Place::EnumItem { place: inner, variant } => {
                let val = self.eval_place_to_value(inner, frame, span.clone())?;
                match val {
                    Value::Enum(tag, data) if tag == *variant => Ok(*data),
                    _ => Err(DiagMsg { title: "const eval error".into(), msg: "enum variant mismatch".into(), span }),
                }
            }
            Place::Deref(_) => Err(DiagMsg { title: "const eval error".into(), msg: "deref not allowed".into(), span }),
            Place::Index { .. } => Err(DiagMsg { title: "const eval error".into(), msg: "index place not supported".into(), span }),
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
                if rv == 0 { return Err(DiagMsg { title: "const eval error".into(), msg: "division by zero".into(), span }); }
                Ok(Value::Int(l.as_int(span.clone())? / rv))
            }
            Rem => {
                let rv = r.as_int(span.clone())?;
                if rv == 0 { return Err(DiagMsg { title: "const eval error".into(), msg: "modulo by zero".into(), span }); }
                Ok(Value::Int(l.as_int(span.clone())? % rv))
            }
            Eq => Ok(Value::Bool(l == r)),
            Ne => Ok(Value::Bool(l != r)),
            Lt => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f64::from_bits(*a) < f64::from_bits(*b))),
                _ => Err(DiagMsg { title: "const eval error".into(), msg: "comparison requires numbers".into(), span }),
            },
            Le => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f64::from_bits(*a) <= f64::from_bits(*b))),
                _ => Err(DiagMsg { title: "const eval error".into(), msg: "comparison requires numbers".into(), span }),
            },
            Gt => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f64::from_bits(*a) > f64::from_bits(*b))),
                _ => Err(DiagMsg { title: "const eval error".into(), msg: "comparison requires numbers".into(), span }),
            },
            Ge => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f64::from_bits(*a) >= f64::from_bits(*b))),
                _ => Err(DiagMsg { title: "const eval error".into(), msg: "comparison requires numbers".into(), span }),
            },
            BitAnd | BitOr | BitXor | Shl | Shr => Err(DiagMsg { title: "const eval error".into(), msg: "bitwise ops not yet supported".into(), span }),
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

    fn push_context(&mut self, fun_id: FunId, args: Vec<Value>,
        caller_ctx_idx: Option<usize>,
        ret_dest: Option<Place>,
        ret_target: Option<BasicBlockId>
    ) -> Result<(), DiagMsg> {
        let fun = self.mir.functions[fun_id].clone();
        let return_local = fun.local_decls.iter()
            .position(|d| d.name.as_deref() == Some("return_val")).unwrap_or(0);

        let mut locals = vec![Value::Unit; fun.local_decls.len()];
        for (i, arg) in args.iter().enumerate() {
            let param_local = return_local + 1 + i;
            if param_local < locals.len() {
                locals[param_local] = arg.clone();
            } else {
                return Err(DiagMsg {
                    title: "const eval error".into(),
                    msg: format!("parameter index out of bounds: {} >= {}", param_local, locals.len()),
                    span: fun.span.clone(),
                });
            }
        }

        let start_block = fun.blocks[0];
        let saved_depth = self.global_handlers.len();
        self.context_stack.push(Context {
            fun_id,
            frame: Frame { locals, current_block_params: vec![] },
            current_block: start_block,
            saved_handler_depth: saved_depth,
            has_returned: false,
            caller_ctx_idx,
            ret_dest,
            ret_target,
        });
        Ok(())
    }

    fn cleanup_handlers_for_ctx(&mut self, ctx_idx: usize) {
        self.global_handlers.retain(|h| h.ctx_idx != ctx_idx);
    }

    fn maybe_pop_handler(&mut self, ctx_idx: usize, current_block: BasicBlockId) {
        if let Some(last) = self.global_handlers.last() {
            if last.ctx_idx == ctx_idx && last.merge_block == current_block {
                self.global_handlers.pop();
            }
        }
    }

    fn run_stack(&mut self) -> Result<Value, DiagMsg> {
        loop {
            let ctx_idx = match self.context_stack.len().checked_sub(1) {
                Some(i) => i,
                None => unreachable!(),
            };

            let ctx = &self.context_stack[ctx_idx];
            let block = ctx.current_block;
            self.maybe_pop_handler(ctx_idx, block);

            let fun_id = self.context_stack[ctx_idx].fun_id;
            let fun = self.mir.functions[fun_id].clone();
            let block_span;
            let terminator;

            let ctx = &mut self.context_stack[ctx_idx];
            {
                let block_id = self.context_stack[ctx_idx].current_block;
                let block = &self.mir.blocks[block_id];

                {
                    let ctx = &mut self.context_stack[ctx_idx];
                    ctx.frame.current_block_params = block.block_params.clone();
                }
                block_span = block.span.clone();

                for stmt in &block.statements {
                    let span = stmt.span.clone();
                    match &stmt.kind {
                        MirStmtKind::Let { local, rvalue } => {
                            let frame_snap = {
                                let ctx = &self.context_stack[ctx_idx];
                                ctx.frame.clone()
                            };
                            let val = self.eval_rvalue(rvalue, &frame_snap, span)?;
                            let ctx = &mut self.context_stack[ctx_idx];
                            ctx.frame.locals[*local] = val;
                        }
                        MirStmtKind::Store { place, rvalue } => {
                            let frame_snap = {
                                let ctx = &self.context_stack[ctx_idx];
                                ctx.frame.clone()
                            };
                            let val = self.eval_rvalue(rvalue, &frame_snap, span.clone())?;

                            match place {
                                Place::Local(id) => {
                                    let ctx = &mut self.context_stack[ctx_idx];
                                    ctx.frame.locals[*id] = val;
                                }
                                Place::Field { base, field } => {
                                    let (base_id, mut new_base) = {
                                        let frame_snap = {
                                            let ctx = &self.context_stack[ctx_idx];
                                            ctx.frame.clone()
                                        };
                                        let base_id = if let Place::Local(id) = base.as_ref() {
                                            *id
                                        } else {
                                            return Err(DiagMsg {
                                                title: "const eval error".into(),
                                                msg: "store to non-local compound".into(),
                                                span,
                                            });
                                        };
                                        let mut base_val = self.eval_place_to_value(base, &frame_snap, span.clone())?;
                                        match &mut base_val {
                                            Value::Tuple(e) | Value::Struct(e) => {
                                                if *field < e.len() {
                                                    e[*field] = val;
                                                } else {
                                                    return Err(DiagMsg {
                                                        title: "const eval error".into(),
                                                        msg: "field index out of bounds".into(),
                                                        span,
                                                    });
                                                }
                                            }
                                            _ => return Err(DiagMsg {
                                                title: "const eval error".into(),
                                                msg: "store field on non-compound".into(),
                                                span,
                                            }),
                                        }
                                        (base_id, base_val)
                                    };
                                    // 写回基础值
                                    let ctx = &mut self.context_stack[ctx_idx];
                                    ctx.frame.locals[base_id] = new_base;
                                }
                                _ => return Err(DiagMsg {
                                    title: "const eval error".into(),
                                    msg: "store to non-local".into(),
                                    span,
                                }),
                            }
                        }
                        MirStmtKind::Nop => {}
                    }
                }

                terminator = {
                    let ctx = &self.context_stack[ctx_idx];
                    self.mir.blocks[ctx.current_block].terminator.clone()
                };
            }

            match &terminator {
                TerminatorKind::Return => {
                    let ctx = &self.context_stack[ctx_idx];
                    let return_local = fun.local_decls.iter()
                        .position(|d| d.name.as_deref() == Some("return_val")).unwrap_or(0);
                    let ret = ctx.frame.locals.get(return_local).cloned().unwrap_or(Value::Unit);

                    if ctx.caller_ctx_idx.is_none() {
                        let const_ret = Self::value_to_const(&ret, fun.signature.return_ty,
                                                             &self.type_ctx.type_pool, block_span)?;
                        self.const_cache.insert(fun_id, const_ret);
                        self.cleanup_handlers_for_ctx(ctx_idx);
                        self.context_stack.pop();
                        return Ok(ret);
                    }

                    let caller_idx = ctx.caller_ctx_idx.unwrap();
                    let ret_dest = ctx.ret_dest.clone();
                    let ret_target = ctx.ret_target;

                    self.cleanup_handlers_for_ctx(ctx_idx);
                    self.context_stack.pop();

                    if let Some(dest) = ret_dest {
                        match dest {
                            Place::Local(local) => {
                                self.context_stack[caller_idx].frame.locals[local] = ret;
                            }
                            _ => return Err(DiagMsg { title: "const eval error".into(), msg: "return dest must be local".into(), span: block_span }),
                        }
                    }

                    if let Some(target) = ret_target {
                        self.context_stack[caller_idx].current_block = target;
                    } else {
                        return Err(DiagMsg { title: "const eval error".into(), msg: "return from divergent call".into(), span: block_span });
                    }
                }
                TerminatorKind::Goto { target, block_args } => {
                    if !block_args.is_empty() {
                        let target_block = &self.mir.blocks[*target];
                        if let Some(&first_param) = target_block.block_params.first() {
                            let val = self.eval_rvalue(&block_args[0], &self.context_stack[ctx_idx].frame, block_span.clone())?;
                            self.context_stack[ctx_idx].frame.locals[first_param] = val;
                        }
                    }
                    self.context_stack[ctx_idx].current_block = *target;
                }
                TerminatorKind::SwitchInt { discriminant, targets, default } => {
                    let disc_val = self.eval_rvalue(discriminant, &self.context_stack[ctx_idx].frame, block_span.clone())?;
                    let mut next = *default;
                    for (c, target) in targets {
                        if disc_val == Self::const_to_value(c) {
                            next = *target;
                            break;
                        }
                    }
                    self.context_stack[ctx_idx].current_block = next;
                }
                TerminatorKind::Call { func, args, dest, target } => {
                    let callee_id = *func;
                    let callee = &self.mir.functions[callee_id];
                    if !callee.is_consteval {
                        return Err(DiagMsg { title: "const eval error".into(), msg: "non-consteval call not allowed".into(), span: block_span });
                    }

                    let mut call_args = Vec::new();
                    for a in args {
                        call_args.push(self.eval_rvalue(a, &self.context_stack[ctx_idx].frame, block_span.clone())?);
                    }

                    let caller_ctx_idx = ctx_idx;
                    let ret_dest = dest.clone();
                    let ret_target = *target;
                    self.push_context(callee_id, call_args, Some(caller_ctx_idx), Some(ret_dest), ret_target)?;
                }
                TerminatorKind::InstallHandler { handler_block, next, args_dest, control_id } => {
                    let handler_term = &self.mir.blocks[*handler_block].terminator;
                    let merge_block = match handler_term {
                        TerminatorKind::Goto { target, .. } | TerminatorKind::Resume { target, .. } => *target,
                        _ => return Err(DiagMsg { title: "const eval error".into(), msg: "handler must end with goto/resume".into(), span: block_span }),
                    };

                    self.global_handlers.push(HandlerEntry {
                        control_id: *control_id,
                        ctx_idx,
                        handler_block: *handler_block,
                        merge_block,
                        args_dest: args_dest.clone(),
                    });

                    self.context_stack[ctx_idx].current_block = *next;
                }
                TerminatorKind::Raise { control_name, args, dest } => {
                    self.handle_raise(
                        control_name,
                        args,
                        dest,
                        block_span.clone(),
                        ctx_idx,
                        &fun,
                    )?;
                }
                TerminatorKind::Resume { place, target } => {
                    let susp = self.suspended_raise.take().ok_or_else(|| DiagMsg {
                        title: "const eval error".into(),
                        msg: "resume without matching raise".into(),
                        span: block_span.clone(),
                    })?;
                    let val = self.eval_place_to_value(place, &self.context_stack[ctx_idx].frame, block_span.clone())?;

                    let cont_frames = susp.continuation.frames;
                    let resume_target = susp.continuation.resume_target;
                    let dest = susp.continuation.dest;

                    for ctx in cont_frames {
                        self.context_stack.push(ctx);
                    }

                    let top_idx = self.context_stack.len() - 1;
                    match dest {
                        Place::Local(local) => {
                            self.context_stack[top_idx].frame.locals[local] = val;
                        }
                        _ => return Err(DiagMsg {
                            title: "const eval error".into(),
                            msg: "resume dest must be local".into(),
                            span: block_span,
                        }),
                    }

                    self.context_stack[top_idx].current_block = resume_target;

                    self.context_stack[ctx_idx].current_block = *target;
                }
                TerminatorKind::CallByPtr { .. } => {}
                TerminatorKind::Unreachable => {}
            }
        }
    }

    fn handle_raise(
        &mut self,
        control_name: &ControlId,
        args: &[Rvalue],
        dest: &Place,
        block_span: Span,
        ctx_idx: usize,
        fun: &MirFun,
    ) -> Result<(), DiagMsg> {
        let raised_vals: Vec<Value> = args.iter()
            .map(|a| self.eval_rvalue(a, &self.context_stack[ctx_idx].frame, block_span.clone()))
            .collect::<Result<_, _>>()?;

        let handler_pos = self.global_handlers.iter().rposition(|h| h.control_id == *control_name)
            .ok_or_else(|| DiagMsg { title: "const eval error".into(), msg: "raise with no handler installed".into(), span: block_span.clone() })?;
        let handler_entry = self.global_handlers.remove(handler_pos);

        let raise_block_idx = self.context_stack[ctx_idx].current_block;
        let raise_block_order = fun.blocks.iter().position(|&b| b == raise_block_idx).unwrap();
        let resume_target = if raise_block_order + 1 < fun.blocks.len() {
            fun.blocks[raise_block_order + 1]
        } else {
            return Err(DiagMsg { title: "const eval error".into(), msg: "raise without valid resume target".into(), span: block_span });
        };

        let handler_ctx_idx = handler_entry.ctx_idx;
        let mut captured_frames = Vec::new();
        while self.context_stack.len() > handler_ctx_idx + 1 {
            let ctx = self.context_stack.pop().unwrap();
            self.cleanup_handlers_for_ctx(ctx.fun_id as usize);
            captured_frames.push(ctx);
        }
        captured_frames.reverse();

        self.global_handlers.retain(|h| h.ctx_idx <= handler_ctx_idx);

        let continuation = Continuation {
            frames: captured_frames,
            resume_target,
            dest: dest.clone(),
        };

        let handler_block_obj = &self.mir.blocks[handler_entry.handler_block];
        for (i, &param_local) in handler_block_obj.block_params.iter().enumerate() {
            let val = raised_vals.get(i).cloned().unwrap_or(Value::Unit);
            self.context_stack[handler_ctx_idx].frame.locals[param_local] = val;
        }

        for (i, &local_id) in handler_entry.args_dest.iter().enumerate() {
            let val = raised_vals.get(i).cloned().unwrap_or(Value::Unit);
            self.context_stack[handler_ctx_idx].frame.locals[local_id] = val;
        }

        self.suspended_raise = Some(SuspendedRaise {
            continuation,
            handler_ctx_idx,
        });

        self.context_stack[handler_ctx_idx].current_block = handler_entry.handler_block;

        Ok(())
    }
}


impl MirConstEvalApi for MirConstEval {
    fn new(mir: MirCrate, type_ctx: TypeCtx) -> Self {
        MirConstEval {
            mir,
            type_ctx,
            const_cache: HashMap::new(),
            static_cache: HashMap::new(),
            context_stack: Vec::new(),
            global_handlers: Vec::new(),
            suspended_raise: None,
        }
    }

    fn eval(mut self) -> Result<(MirCrate, TypeCtx), DiagMsg> {
        for (sid, s) in self.mir.statics.iter().enumerate() {
            self.static_cache.insert(sid, s.init.clone());
        }

        let mut const_calls: Vec<(FunId, BasicBlockId, Place, Option<BasicBlockId>, Vec<Value>)> = Vec::new();
        for fid in 0..self.mir.functions.len() {
            let fun = &self.mir.functions[fid];
            for &block_id in &fun.blocks {
                let block = &self.mir.blocks[block_id];
                if let TerminatorKind::Call { func, args, dest, target } = &block.terminator {
                    if self.mir.functions[*func].is_consteval {
                        let callee = &self.mir.functions[*func];
                        let has_raise = callee.blocks.iter().any(|&bid| {
                            matches!(self.mir.blocks[bid].terminator, TerminatorKind::Raise { .. })
                        });
                        if has_raise {
                            continue;
                        }
                        let mut const_args = Vec::new();
                        let mut all_const = true;
                        for a in args {
                            match a {
                                Rvalue::Constant(c) => const_args.push(Self::const_to_value(c)),
                                Rvalue::Move(Place::Local(id)) | Rvalue::Copy(Place::Local(id)) => {
                                    let mut found = false;
                                    for stmt in &block.statements {
                                        if let MirStmtKind::Let { local, rvalue } = &stmt.kind {
                                            if *local == *id {
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

        let mut replacements: Vec<(BasicBlockId, Place, Const, Option<BasicBlockId>)> = Vec::new();
        for (func_id, block_id, dest, target, const_args) in const_calls {
            self.context_stack.clear();
            self.global_handlers.clear();
            self.suspended_raise = None;

            self.push_context(func_id, const_args, None, None, None)?;
            let result = self.run_stack()?;
            let ret_ty = self.mir.functions[func_id].signature.return_ty;
            let span = self.mir.blocks[block_id].span.clone();
            let const_result = Self::value_to_const(&result, ret_ty, &self.type_ctx.type_pool, span)?;
            replacements.push((block_id, dest, const_result, target));
        }

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

        for (sid, const_val) in &self.static_cache {
            self.mir.statics[*sid].init = const_val.clone();
        }

        Ok((self.mir, self.type_ctx))
    }
}