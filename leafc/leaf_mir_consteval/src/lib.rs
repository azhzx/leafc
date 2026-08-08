use leaf_coreapi::diagnose::{CompileTimeErrorKind, DiagCtx, ErrorKind, LocalizedMessage, MirConstEvalErrorKind, MsgKind};
use leaf_coreapi::lang_items::BuiltinType;
use leaf_coreapi::mir::{BasicBlock, MirBasicBlockId, Const, MirControlId, MirFunId, MirLocalId, MirBinOp, MirCrate, MirFun, MirStmt, MirStmtKind, MirUnOp, Place, Rvalue, MirStaticId, MirTagId, TerminatorKind};
use leaf_coreapi::mir_consteval::MirConstEvalApi;
use leaf_coreapi::source::Span;
use leaf_coreapi::type_ctx::{get_type_root, TyId, TypeCtx, TypeNode, TypeNodeKind};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Unit,
    Int(i64),
    Float(u64),
    Bool(bool),
    Str(String),
    Tuple(Vec<Value>),
    Struct(Vec<Value>),
    Enum(MirTagId, Box<Value>),
    Never,
}

#[derive(Clone, Debug)]
struct Frame {
    locals: Vec<Value>,
    current_block_params: Vec<MirLocalId>,
}

#[derive(Clone, Debug)]
struct Continuation {
    frames: Vec<Context>,
    resume_target: MirBasicBlockId,
    dest: Place,
}

#[derive(Clone, Debug)]
struct Context {
    fun_id: MirFunId,
    frame: Rc<RefCell<Frame>>,
    current_block: MirBasicBlockId,
    saved_handler_depth: usize,
    has_returned: bool,
    caller_ctx_idx: Option<usize>,
    ret_dest: Option<Place>,
    ret_target: Option<MirBasicBlockId>,
    call_block: Option<MirBasicBlockId>,
}

struct SuspendedRaise {
    continuation: Continuation,
    handler_ctx_idx: usize,
}

struct HandlerEntry {
    control_id: MirControlId,
    ctx_idx: usize,
    handler_block: MirBasicBlockId,
    merge_block: MirBasicBlockId,
    args_dest: Vec<MirLocalId>,
}

pub struct MirConstEval<'a> {
    pub diag: &'a mut DiagCtx,
    mir: MirCrate,
    type_ctx: TypeCtx,
    const_cache: HashMap<MirFunId, Const>,
    static_cache: Rc<RefCell<HashMap<MirStaticId, Const>>>,
    context_stack: Vec<Context>,
    global_handlers: Vec<HandlerEntry>,
    suspended_raise: Option<SuspendedRaise>,
}

impl Value {
    fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            _ => None,
        }
    }
}

impl<'a> MirConstEval<'a> {
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

    fn value_to_const(&mut self, value: &Value, ty: TyId, span: Span) -> Result<Const, ()> {
        let root = get_type_root(&self.type_ctx.type_pool, ty);
        let kind = &self.type_ctx.type_pool[root].kind;
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
                    _ => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span,
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["unsupported integer target type"]),
                        );
                        return Err(());
                    }
                })
            }
            (Value::Float(bits), TypeNodeKind::Builtin(BuiltinType::F32)) => Ok(Const::Float32(*bits)),
            (Value::Float(bits), TypeNodeKind::Builtin(BuiltinType::F64)) => Ok(Const::Float64(*bits)),
            (Value::Bool(b), TypeNodeKind::Builtin(BuiltinType::Bool)) => Ok(Const::Bool(*b)),
            (Value::Str(s), _) => Ok(Const::Str(s.clone())),
            (Value::Unit, TypeNodeKind::Tuple(elems)) if elems.is_empty() => Ok(Const::Unit),
            (Value::Tuple(elems), TypeNodeKind::Tuple(tys)) => {
                if elems.len() != tys.len() {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["tuple arity mismatch"]),
                    );
                    return Err(());
                }
                let consts: Result<Vec<_>, _> = elems.iter().zip(tys).map(|(v, &ty)| self.value_to_const(v, ty, span.clone())).collect();
                Ok(Const::Tuple(consts?))
            }
            (Value::Struct(elems), TypeNodeKind::Struct { field_tys, .. }) => {
                if elems.len() != field_tys.len() {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["struct field count mismatch"]),
                    );
                    return Err(());
                }
                let consts: Result<Vec<_>, _> = elems.iter().zip(field_tys).map(|(v, &ty)| self.value_to_const(v, ty, span.clone())).collect();
                Ok(Const::Struct(consts?))
            }
            (Value::Enum(tag, data), TypeNodeKind::ADT { variants, .. }) => {
                let payload_ty = match variants.get(*tag).copied().flatten() {
                    Some(v) => v,
                    None => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span.clone(),
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, [format!("invalid enum variant tag {}", tag)]),
                        );
                        return Err(());
                    }
                };
                let payload_const = self.value_to_const(data, payload_ty, span)?;
                Ok(Const::Enum(*tag, Box::new(payload_const)))
            }
            _ => {
                self.diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                    span,
                    LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["unsupported constant type"]),
                );
                return Err(());
            }
        }
    }

    fn write_place(&mut self, place: &Place, value: Value, frame: &mut Frame, span: Span) -> Result<(), ()> {
        match place {
            Place::Local(id) => {
                frame.locals[*id] = value;
                Ok(())
            }
            Place::Field { base, field } => {
                let mut base_val = self.eval_place_to_value(base, frame, span.clone())?;
                match &mut base_val {
                    Value::Tuple(elems) | Value::Struct(elems) => {
                        if *field >= elems.len() {
                            self.diag.emit_error(
                                ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                                span,
                                LocalizedMessage::new(MsgKind::MirConstEvalFailed, [format!("field index {} out of bounds", field)]),
                            );
                            return Err(());
                        }
                        elems[*field] = value;
                    }
                    _ => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span,
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["cannot access field on non-compound"]),
                        );
                        return Err(());
                    }
                }
                self.write_place(base, base_val, frame, span)
            }
            Place::Index { place: base, item_index } => {
                let mut base_val = self.eval_place_to_value(base, frame, span.clone())?;
                match &mut base_val {
                    Value::Tuple(elems) | Value::Struct(elems) => {
                        if *item_index >= elems.len() {
                            self.diag.emit_error(
                                ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                                span,
                                LocalizedMessage::new(MsgKind::MirConstEvalFailed, [format!("index {} out of bounds", item_index)]),
                            );
                            return Err(());
                        }
                        elems[*item_index] = value;
                    }
                    _ => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span,
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["cannot index into non-compound"]),
                        );
                        return Err(());
                    }
                }
                self.write_place(base, base_val, frame, span)
            }
            Place::EnumItem { .. } => {
                self.diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                    span,
                    LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["cannot store directly into enum variant"]),
                );
                Err(())
            }
            Place::Static(_) => {
                self.diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                    span,
                    LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["mutation of static not supported"]),
                );
                Err(())
            }
            Place::Deref(_) => {
                self.diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                    span,
                    LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["deref not allowed"]),
                );
                Err(())
            }
        }
    }

    fn eval_place_to_value(&mut self, place: &Place, frame: &Frame, span: Span) -> Result<Value, ()> {
        match place {
            Place::Local(id) => match frame.locals.get(*id).cloned() {
                Some(v) => Ok(v),
                None => {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, [format!("local {} not initialized", id)]),
                    );
                    Err(())
                }
            },
            Place::Static(sid) => {
                let cache = self.static_cache.borrow();
                match cache.get(sid).map(|c| Self::const_to_value(c)) {
                    Some(v) => Ok(v),
                    None => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span,
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, [format!("static {} not evaluated", sid)]),
                        );
                        Err(())
                    }
                }
            }
            Place::Field { base, field } => {
                let base_val = self.eval_place_to_value(base, frame, span.clone())?;
                match base_val {
                    Value::Tuple(e) | Value::Struct(e) => match e.get(*field).cloned() {
                        Some(v) => Ok(v),
                        None => {
                            self.diag.emit_error(
                                ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                                span,
                                LocalizedMessage::new(MsgKind::MirConstEvalFailed, [format!("field {} out of bounds", field)]),
                            );
                            Err(())
                        }
                    },
                    _ => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span,
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["field on non-compound"]),
                        );
                        Err(())
                    }
                }
            }
            Place::EnumItem { place: inner, variant } => {
                let val = self.eval_place_to_value(inner, frame, span.clone())?;
                match val {
                    Value::Enum(tag, data) if tag == *variant => Ok(*data),
                    _ => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span,
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["enum variant mismatch"]),
                        );
                        Err(())
                    }
                }
            }
            Place::Index { place, item_index } => {
                let base_val = self.eval_place_to_value(place, frame, span.clone())?;
                match base_val {
                    Value::Tuple(elems) | Value::Struct(elems) => match elems.get(*item_index).cloned() {
                        Some(v) => Ok(v),
                        None => {
                            self.diag.emit_error(
                                ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                                span,
                                LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["index out of bounds"]),
                            );
                            Err(())
                        }
                    },
                    _ => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span,
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["index on non-compound"]),
                        );
                        Err(())
                    }
                }
            }
            Place::Deref(_) => {
                self.diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                    span,
                    LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["deref not allowed"]),
                );
                Err(())
            }
        }
    }

    fn eval_rvalue(&mut self, rvalue: &Rvalue, frame: &Frame, span: Span) -> Result<Value, ()> {
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
                let vals: Result<Vec<_>, _> = elems.iter().map(|e| self.eval_rvalue(e, frame, span.clone())).collect();
                Ok(Value::Tuple(vals?))
            }
            Rvalue::BuildStruct(fields) => {
                let vals: Result<Vec<_>, _> = fields.iter().map(|e| self.eval_rvalue(e, frame, span.clone())).collect();
                Ok(Value::Struct(vals?))
            }
            Rvalue::Variant(tag, inner) => {
                let v = self.eval_rvalue(inner, frame, span.clone())?;
                Ok(Value::Enum(*tag, Box::new(v)))
            }
            Rvalue::Ref(_) | Rvalue::RefMut(_) | Rvalue::GcNewObject(_) | Rvalue::GcObjectRef(_) => {
                self.diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                    span,
                    LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["ref/gc not allowed in const"]),
                );
                Err(())
            }
            Rvalue::GetFunPtr(_) => {
                self.diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                    span,
                    LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["fun ptr not allowed"]),
                );
                Err(())
            }
            Rvalue::Index { place, item_index } => {
                let base = self.eval_place_to_value(place, frame, span.clone())?;
                match base {
                    Value::Tuple(e) | Value::Struct(e) => match e.get(*item_index).cloned() {
                        Some(v) => Ok(v),
                        None => {
                            self.diag.emit_error(
                                ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                                span,
                                LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["index out of bounds"]),
                            );
                            Err(())
                        }
                    },
                    _ => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span,
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["index on non-compound"]),
                        );
                        Err(())
                    }
                }
            }
            Rvalue::Field { place, item_index } => {
                self.eval_rvalue(&Rvalue::Index { place: place.clone(), item_index: *item_index }, frame, span)
            }
            Rvalue::Len(place) => {
                let val = self.eval_place_to_value(place, frame, span.clone())?;
                match val {
                    Value::Tuple(e) | Value::Struct(e) => Ok(Value::Int(e.len() as i64)),
                    Value::Str(s) => Ok(Value::Int(s.len() as i64)),
                    _ => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span,
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["len not supported"]),
                        );
                        Err(())
                    }
                }
            }
            Rvalue::Tag(place) => {
                let val = self.eval_place_to_value(place, frame, span.clone())?;
                if let Value::Enum(tag, _) = val {
                    Ok(Value::Int(tag as i64))
                } else {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["tag on non-enum"]),
                    );
                    Err(())
                }
            }
            Rvalue::Cast(place, target_ty) => {
                let val = self.eval_place_to_value(place, frame, span.clone())?;
                self.cast_value(val, *target_ty, span)
            }
            Rvalue::HandlerArg(idx) => {
                let local_id = match frame.current_block_params.get(*idx) {
                    Some(v) => v,
                    None => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span.clone(),
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["invalid handler arg index"]),
                        );
                        return Err(());
                    }
                };
                match frame.locals.get(*local_id).cloned() {
                    Some(v) => Ok(v),
                    None => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span,
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["handler arg not initialized"]),
                        );
                        Err(())
                    }
                }
            }
        }
    }

    fn cast_value(&mut self, value: Value, target_ty: TyId, span: Span) -> Result<Value, ()> {
        let root = get_type_root(&self.type_ctx.type_pool, target_ty);
        let kind = &self.type_ctx.type_pool[root].kind;
        match (value, kind) {
            (Value::Int(i), TypeNodeKind::Builtin(b)) => {
                use BuiltinType::*;
                match b {
                    I8 | I16 | I32 | I64 | U8 | U16 | U32 | U64 => Ok(Value::Int(i)),
                    F32 => Ok(Value::Float((i as f32).to_bits() as u64)),
                    F64 => Ok(Value::Float((i as f64).to_bits())),
                    _ => {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            span,
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["unsupported cast target"]),
                        );
                        Err(())
                    }
                }
            }
            (Value::Float(bits), TypeNodeKind::Builtin(b)) => match b {
                BuiltinType::I32 => Ok(Value::Int(f64::from_bits(bits) as i64)),
                BuiltinType::F64 => Ok(Value::Float(bits)),
                _ => {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["unsupported cast from float"]),
                    );
                    Err(())
                }
            },
            _ => {
                self.diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                    span,
                    LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["cast not supported"]),
                );
                Err(())
            }
        }
    }

    fn eval_binary(&mut self, op: &MirBinOp, l: Value, r: Value, span: Span) -> Result<Value, ()> {
        use MirBinOp::*;
        match op {
            Add => {
                let lv = l.as_int().ok_or_else(|| {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span.clone(),
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["expected integer value"]),
                    );
                })?;
                let rv = r.as_int().ok_or_else(|| {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span.clone(),
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["expected integer value"]),
                    );
                })?;
                Ok(Value::Int(lv + rv))
            }
            Sub => {
                let lv = l.as_int().ok_or_else(|| {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span.clone(),
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["expected integer value"]),
                    );
                })?;
                let rv = r.as_int().ok_or_else(|| {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span.clone(),
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["expected integer value"]),
                    );
                })?;
                Ok(Value::Int(lv - rv))
            }
            Mul => {
                let lv = l.as_int().ok_or_else(|| {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span.clone(),
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["expected integer value"]),
                    );
                })?;
                let rv = r.as_int().ok_or_else(|| {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span.clone(),
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["expected integer value"]),
                    );
                })?;
                Ok(Value::Int(lv * rv))
            }
            Div => {
                let lv = l.as_int().ok_or_else(|| {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span.clone(),
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["expected integer value"]),
                    );
                })?;
                let rv = r.as_int().ok_or_else(|| {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span.clone(),
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["expected integer value"]),
                    );
                })?;
                if rv == 0 {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["division by zero"]),
                    );
                    return Err(());
                }
                Ok(Value::Int(lv / rv))
            }
            Rem => {
                let lv = l.as_int().ok_or_else(|| {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span.clone(),
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["expected integer value"]),
                    );
                })?;
                let rv = r.as_int().ok_or_else(|| {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span.clone(),
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["expected integer value"]),
                    );
                })?;
                if rv == 0 {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["modulo by zero"]),
                    );
                    return Err(());
                }
                Ok(Value::Int(lv % rv))
            }
            Eq => Ok(Value::Bool(l == r)),
            Ne => Ok(Value::Bool(l != r)),
            Lt => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f64::from_bits(*a) < f64::from_bits(*b))),
                _ => {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["comparison requires numbers"]),
                    );
                    Err(())
                }
            },
            Le => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f64::from_bits(*a) <= f64::from_bits(*b))),
                _ => {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["comparison requires numbers"]),
                    );
                    Err(())
                }
            },
            Gt => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f64::from_bits(*a) > f64::from_bits(*b))),
                _ => {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["comparison requires numbers"]),
                    );
                    Err(())
                }
            },
            Ge => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f64::from_bits(*a) >= f64::from_bits(*b))),
                _ => {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["comparison requires numbers"]),
                    );
                    Err(())
                }
            },
            BitAnd | BitOr | BitXor | Shl | Shr => {
                self.diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                    span,
                    LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["bitwise ops not yet supported"]),
                );
                Err(())
            }
        }
    }

    fn eval_unary(&mut self, op: &MirUnOp, v: Value, span: Span) -> Result<Value, ()> {
        match op {
            MirUnOp::Neg => match v {
                Value::Int(i) => Ok(Value::Int(-i)),
                Value::Float(bits) => Ok(Value::Float((-f64::from_bits(bits)).to_bits())),
                _ => {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["negation on non-numeric"]),
                    );
                    Err(())
                }
            },
            MirUnOp::Not => match v {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        span,
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["logical not on non-bool"]),
                    );
                    Err(())
                }
            },
        }
    }

    fn push_context(
        &mut self,
        fun_id: MirFunId,
        args: Vec<Value>,
        caller_ctx_idx: Option<usize>,
        ret_dest: Option<Place>,
        ret_target: Option<MirBasicBlockId>,
        call_block: Option<MirBasicBlockId>,
    ) -> Result<(), ()> {
        let fun = self.mir.functions[fun_id].clone();
        let return_local = fun.local_decls.iter().position(|d| d.name.as_deref() == Some("return_val")).unwrap_or(0);
        let mut locals = vec![Value::Unit; fun.local_decls.len()];
        for (i, arg) in args.iter().enumerate() {
            let param_local = return_local + 1 + i;
            if param_local < locals.len() {
                locals[param_local] = arg.clone();
            } else {
                self.diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                    fun.span.clone(),
                    LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["param index out of bounds"]),
                );
                return Err(());
            }
        }
        let start_block = fun.blocks[0];
        let saved_depth = self.global_handlers.len();
        self.context_stack.push(Context {
            fun_id,
            frame: Rc::new(RefCell::new(Frame { locals, current_block_params: vec![] })),
            current_block: start_block,
            saved_handler_depth: saved_depth,
            has_returned: false,
            caller_ctx_idx,
            ret_dest,
            ret_target,
            call_block,
        });
        Ok(())
    }

    fn cleanup_handlers_for_ctx(&mut self, ctx_idx: usize) {
        self.global_handlers.retain(|h| h.ctx_idx != ctx_idx);
    }

    fn maybe_pop_handler(&mut self, ctx_idx: usize, current_block: MirBasicBlockId) {
        if let Some(last) = self.global_handlers.last() {
            if last.ctx_idx == ctx_idx && last.merge_block == current_block {
                self.global_handlers.pop();
            }
        }
    }

    fn handle_raise(
        &mut self,
        control_name: &MirControlId,
        args: &[Rvalue],
        dest: &Place,
        block_span: Span,
        ctx_idx: usize,
        fun: &MirFun,
    ) -> Result<(), ()> {
        let frame_clone = self.context_stack[ctx_idx].frame.clone();
        let frame_ref = frame_clone.borrow();
        let raised_vals: Vec<Value> = args.iter()
            .map(|a| self.eval_rvalue(a, &*frame_ref, block_span.clone()))
            .collect::<Result<_, _>>()?;
        drop(frame_ref);

        let handler_pos = match self.global_handlers.iter().rposition(|h| h.control_id == *control_name) {
            Some(v) => v,
            None => {
                self.diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                    block_span.clone(),
                    LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["raise with no handler installed"]),
                );
                return Err(());
            }
        };
        let handler_entry = self.global_handlers.remove(handler_pos);

        let raise_block_idx = self.context_stack[ctx_idx].current_block;
        let raise_block_order = fun.blocks.iter().position(|&b| b == raise_block_idx).unwrap();
        let resume_target = if raise_block_order + 1 < fun.blocks.len() {
            fun.blocks[raise_block_order + 1]
        } else {
            self.diag.emit_error(
                ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                block_span,
                LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["raise without valid resume target"]),
            );
            return Err(());
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
        {
            let mut handler_frame = self.context_stack[handler_ctx_idx].frame.borrow_mut();
            for (i, &param_local) in handler_block_obj.block_params.iter().enumerate() {
                let val = raised_vals.get(i).cloned().unwrap_or(Value::Unit);
                handler_frame.locals[param_local] = val;
            }
            for (i, &local_id) in handler_entry.args_dest.iter().enumerate() {
                let val = raised_vals.get(i).cloned().unwrap_or(Value::Unit);
                handler_frame.locals[local_id] = val;
            }
        }

        self.suspended_raise = Some(SuspendedRaise {
            continuation,
            handler_ctx_idx,
        });

        self.context_stack[handler_ctx_idx].current_block = handler_entry.handler_block;
        Ok(())
    }

    fn run_stack(&mut self) -> Result<Value, ()> {
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

            {
                let block_id = self.context_stack[ctx_idx].current_block;
                let block = &self.mir.blocks[block_id];

                {
                    let mut frame_mut = self.context_stack[ctx_idx].frame.borrow_mut();
                    frame_mut.current_block_params = block.block_params.clone();
                }
                block_span = block.span.clone();

                for stmt in &block.statements {
                    let span = stmt.span.clone();
                    match &stmt.kind {
                        MirStmtKind::Let { local, rvalue } => {
                            let frame_clone = self.context_stack[ctx_idx].frame.clone();
                            let frame_ref = frame_clone.borrow();
                            let val = self.eval_rvalue(rvalue, &*frame_ref, span)?;
                            drop(frame_ref);
                            let mut frame_mut = self.context_stack[ctx_idx].frame.borrow_mut();
                            frame_mut.locals[*local] = val;
                        }
                        MirStmtKind::Store { place, rvalue } => {
                            let frame_clone = self.context_stack[ctx_idx].frame.clone();
                            let frame_ref = frame_clone.borrow();
                            let val = self.eval_rvalue(rvalue, &*frame_ref, span.clone())?;
                            drop(frame_ref);
                            let mut frame_mut = self.context_stack[ctx_idx].frame.borrow_mut();
                            self.write_place(place, val, &mut *frame_mut, span.clone())?;
                        }
                        MirStmtKind::Nop => {}
                    }
                }

                terminator = {
                    let block_id = self.context_stack[ctx_idx].current_block;
                    self.mir.blocks[block_id].terminator.clone()
                };
            }

            match &terminator {
                TerminatorKind::Return => {
                    let return_local = fun.local_decls.iter()
                        .position(|d| d.name.as_deref() == Some("return_val")).unwrap_or(0);
                    let ret = {
                        let frame_ref = self.context_stack[ctx_idx].frame.borrow();
                        frame_ref.locals.get(return_local).cloned().unwrap_or(Value::Unit)
                    };

                    if self.context_stack[ctx_idx].caller_ctx_idx.is_none() {
                        self.cleanup_handlers_for_ctx(ctx_idx);
                        self.context_stack.pop();
                        return Ok(ret);
                    }

                    let caller_idx = self.context_stack[ctx_idx].caller_ctx_idx.unwrap();
                    let ret_dest = self.context_stack[ctx_idx].ret_dest.clone();
                    let ret_target = self.context_stack[ctx_idx].ret_target;
                    let call_block_id = self.context_stack[ctx_idx].call_block;

                    self.cleanup_handlers_for_ctx(ctx_idx);
                    self.context_stack.pop();

                    if let Some(dest) = &ret_dest {
                        match dest {
                            Place::Local(local) => {
                                let mut caller_frame = self.context_stack[caller_idx].frame.borrow_mut();
                                caller_frame.locals[*local] = ret.clone();
                            }
                            _ => {
                                self.diag.emit_error(
                                    ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                                    block_span.clone(),
                                    LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["return dest must be local"]),
                                );
                                return Err(());
                            }
                        }
                    }

                    if let (Some(call_block), Some(dest), Some(target)) = (call_block_id, &ret_dest, ret_target) {
                        let callee_ret_ty = fun.signature.return_ty;
                        let const_val = self.value_to_const(&ret, callee_ret_ty, block_span.clone())?;
                        if let Place::Local(local) = dest {
                            let block = &mut self.mir.blocks[call_block];
                            block.statements.push(MirStmt {
                                kind: MirStmtKind::Let {
                                    local: *local,
                                    rvalue: Rvalue::Constant(const_val),
                                },
                                span: block.span.clone(),
                            });
                            block.terminator = TerminatorKind::Goto {
                                target,
                                block_args: vec![],
                            };
                        }
                    }

                    if let Some(target) = ret_target {
                        self.context_stack[caller_idx].current_block = target;
                    } else {
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            block_span,
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["return from divergent call"]),
                        );
                        return Err(());
                    }
                }
                TerminatorKind::Goto { target, block_args } => {
                    if !block_args.is_empty() {
                        let target_block = &self.mir.blocks[*target];
                        if let Some(&first_param) = target_block.block_params.first() {
                            let frame_clone = self.context_stack[ctx_idx].frame.clone();
                            let frame_ref = frame_clone.borrow();
                            let val = self.eval_rvalue(&block_args[0], &*frame_ref, block_span.clone())?;
                            drop(frame_ref);
                            let mut frame_mut = self.context_stack[ctx_idx].frame.borrow_mut();
                            frame_mut.locals[first_param] = val;
                        }
                    }
                    self.context_stack[ctx_idx].current_block = *target;
                }
                TerminatorKind::SwitchInt { discriminant, targets, default } => {
                    let frame_clone = self.context_stack[ctx_idx].frame.clone();
                    let frame_ref = frame_clone.borrow();
                    let mut disc_val = self.eval_rvalue(discriminant, &*frame_ref, block_span.clone())?;
                    drop(frame_ref);

                    if let Value::Enum(tag, _) = disc_val {
                        disc_val = Value::Int(tag as i64);
                    }

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
                        self.diag.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                            block_span.clone(),
                            LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["non-consteval call not allowed"]),
                        );
                        return Err(());
                    }

                    let frame_clone = self.context_stack[ctx_idx].frame.clone();
                    let frame_ref = frame_clone.borrow();
                    let mut call_args = Vec::new();
                    for a in args {
                        call_args.push(self.eval_rvalue(a, &*frame_ref, block_span.clone())?);
                    }
                    drop(frame_ref);

                    let caller_ctx_idx = ctx_idx;
                    let ret_dest = dest.clone();
                    let ret_target = *target;
                    let call_block = self.context_stack[ctx_idx].current_block;
                    self.push_context(callee_id, call_args, Some(caller_ctx_idx), Some(ret_dest), ret_target, Some(call_block))?;
                }
                TerminatorKind::InstallHandler { handler_block, next, args_dest, control_id } => {
                    let handler_term = &self.mir.blocks[*handler_block].terminator;
                    let merge_block = match handler_term {
                        TerminatorKind::Goto { target, .. } | TerminatorKind::Resume { target, .. } => *target,
                        _ => {
                            self.diag.emit_error(
                                ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                                block_span.clone(),
                                LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["handler must end with goto/resume"]),
                            );
                            return Err(());
                        }
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
                    self.handle_raise(control_name, args, dest, block_span.clone(), ctx_idx, &fun)?;
                }
                TerminatorKind::Resume { place, target } => {
                    let susp = match self.suspended_raise.take() {
                        Some(v) => v,
                        None => {
                            self.diag.emit_error(
                                ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                                block_span.clone(),
                                LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["resume without matching raise"]),
                            );
                            return Err(());
                        }
                    };
                    let frame_clone = self.context_stack[ctx_idx].frame.clone();
                    let frame_ref = frame_clone.borrow();
                    let val = self.eval_place_to_value(place, &*frame_ref, block_span.clone())?;
                    drop(frame_ref);

                    let cont_frames = susp.continuation.frames;
                    let resume_target = susp.continuation.resume_target;
                    let dest = susp.continuation.dest;

                    for ctx in cont_frames {
                        self.context_stack.push(ctx);
                    }

                    let top_idx = self.context_stack.len() - 1;
                    match dest {
                        Place::Local(local) => {
                            let mut top_frame = self.context_stack[top_idx].frame.borrow_mut();
                            top_frame.locals[local] = val;
                        }
                        _ => {
                            self.diag.emit_error(
                                ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                                block_span,
                                LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["resume dest must be local"]),
                            );
                            return Err(());
                        }
                    }

                    self.context_stack[top_idx].current_block = resume_target;
                    self.context_stack[ctx_idx].current_block = *target;
                }
                TerminatorKind::CallByPtr { .. } => {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        block_span.clone(),
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["call by pointer not supported in const eval"]),
                    );
                    return Err(());
                }
                TerminatorKind::Unreachable => {
                    self.diag.emit_error(
                        ErrorKind::CompileTimeError(CompileTimeErrorKind::MirConstEvalError(MirConstEvalErrorKind::EvalFailed)),
                        block_span.clone(),
                        LocalizedMessage::new(MsgKind::MirConstEvalFailed, ["reached unreachable code"]),
                    );
                    return Err(());
                }
            }
        }
    }

    fn build_local_const_map(fun: &MirFun, blocks: &[BasicBlock]) -> HashMap<MirLocalId, Const> {
        let mut map = HashMap::new();
        for &block_id in &fun.blocks {
            let block = &blocks[block_id];
            for stmt in &block.statements {
                if let MirStmtKind::Let { local, rvalue } = &stmt.kind {
                    if let Rvalue::Constant(c) = rvalue {
                        map.insert(*local, c.clone());
                    } else {
                        map.remove(local);
                    }
                }
            }
        }
        map
    }

    fn try_extract_const(
        rvalue: &Rvalue,
        local_const_map: &HashMap<MirLocalId, Const>,
    ) -> Option<Const> {
        match rvalue {
            Rvalue::Constant(c) => Some(c.clone()),
            Rvalue::Move(Place::Local(id)) | Rvalue::Copy(Place::Local(id)) => {
                local_const_map.get(id).cloned()
            }
            _ => None,
        }
    }

    fn reachable_functions(&self, main_id: MirFunId) -> HashSet<MirFunId> {
        let mut reachable = HashSet::new();
        let mut queue = VecDeque::new();
        reachable.insert(main_id);
        queue.push_back(main_id);

        while let Some(fid) = queue.pop_front() {
            let fun = &self.mir.functions[fid];
            for &block_id in &fun.blocks {
                let block = &self.mir.blocks[block_id];
                match &block.terminator {
                    TerminatorKind::Call { func, .. } => {
                        if !reachable.contains(func) {
                            reachable.insert(*func);
                            queue.push_back(*func);
                        }
                    }
                    _ => {}
                }
            }
        }
        reachable
    }
}
impl<'a> MirConstEvalApi<'a> for MirConstEval<'a> {
    fn new(mir: MirCrate, type_ctx: TypeCtx, diag: &'a mut DiagCtx) -> Self {
        MirConstEval {
            diag,
            mir,
            type_ctx,
            const_cache: HashMap::new(),
            static_cache: Rc::new(RefCell::new(HashMap::new())),
            context_stack: Vec::new(),
            global_handlers: Vec::new(),
            suspended_raise: None,
        }
    }

    fn eval(mut self) -> Result<(MirCrate, TypeCtx), ()> {
        {
            let mut cache = self.static_cache.borrow_mut();
            for (sid, s) in self.mir.statics.iter().enumerate() {
                cache.insert(sid, s.init.clone());
            }
        }

        let main_id = self.mir.functions.iter()
            .position(|f| f.name == "main")
            .unwrap();

        let reachable = self.reachable_functions(main_id);

        let mut pending: Vec<(MirFunId, MirBasicBlockId, MirFunId, Vec<Const>, Place, Option<MirBasicBlockId>)> = Vec::new();

        for &fid in &reachable {
            let fun = &self.mir.functions[fid];
            let local_const_map = Self::build_local_const_map(fun, &self.mir.blocks);

            for &block_id in &fun.blocks {
                let block = &self.mir.blocks[block_id];
                if let TerminatorKind::Call { func, args, dest, target } = &block.terminator {
                    let callee_id = *func;
                    if !self.mir.functions[callee_id].is_consteval {
                        continue;
                    }
                    let mut const_args = Vec::new();
                    let mut all_const = true;
                    for a in args {
                        if let Some(c) = Self::try_extract_const(a, &local_const_map) {
                            const_args.push(c);
                        } else {
                            all_const = false;
                            break;
                        }
                    }
                    if all_const {
                        pending.push((fid, block_id, callee_id, const_args, dest.clone(), *target));
                    }
                }
            }
        }

        for (caller_fun_id, block_id, callee_id, const_args, dest, target) in pending {
            self.context_stack.clear();
            self.global_handlers.clear();
            self.suspended_raise = None;

            let args_as_values: Vec<Value> = const_args.iter().map(|c| Self::const_to_value(c)).collect();
            self.push_context(
                callee_id,
                args_as_values,
                None,
                Some(dest.clone()),
                target,
                Some(block_id),
            )?;

            let result = self.run_stack()?;

            if self.context_stack.is_empty() {
                let ret_ty = self.mir.functions[callee_id].signature.return_ty;
                let const_val = self.value_to_const(
                    &result, ret_ty, self.mir.functions[callee_id].span.clone())?;
                if let Place::Local(local) = dest {
                    let block = &mut self.mir.blocks[block_id];
                    block.statements.push(MirStmt {
                        kind: MirStmtKind::Let {
                            local,
                            rvalue: Rvalue::Constant(const_val),
                        },
                        span: block.span.clone(),
                    });
                    block.terminator = if let Some(t) = target {
                        TerminatorKind::Goto { target: t, block_args: vec![] }
                    } else {
                        TerminatorKind::Unreachable
                    };
                }
            }
        }

        let cache = self.static_cache.borrow();
        for (sid, const_val) in cache.iter() {
            self.mir.statics[*sid].init = const_val.clone();
        }

        Ok((self.mir, self.type_ctx))
    }
}
