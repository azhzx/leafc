use crate::id::CrateId;
use crate::source::Span;
use crate::type_ctx::TyId;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct MirLocalId(pub usize);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, Ord, PartialOrd)]
pub struct MirBasicBlockId(pub usize);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct MirFunId(pub usize);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct MirStaticId(pub usize);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct MirTagId(pub usize);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct MirControlId(pub usize);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirCrate {
    pub id: CrateId,
    pub functions: Vec<MirFun>,
    pub extern_decls: Vec<ExternDecl>,
    pub pub_decl_ids: Vec<MirFunId>,
    pub statics: Vec<StaticDecl>,
    pub blocks: Vec<BasicBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirFun {
    pub name: String,
    pub generic_params: Vec<TyId>,
    pub signature: FnSig,
    pub local_decls: Vec<LocalDecl>,
    pub blocks: Vec<MirBasicBlockId>,
    pub is_consteval: bool,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternDecl {
    pub name: String,
    pub signature: FnSig,
    pub is_variadic: bool,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StaticDecl {
    pub name: String,
    pub ty: TyId,
    pub mutable: bool,
    pub init: Const, // todo : 暂时const
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalDecl {
    pub ty: TyId,
    pub mutable: bool,
    pub name: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FnSig {
    pub params: Vec<TyId>,
    pub return_ty: TyId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BasicBlock {
    pub block_params: Vec<MirLocalId>,
    pub statements: Vec<MirStmt>,
    pub terminator: TerminatorKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirStmt {
    pub kind: MirStmtKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MirStmtKind {
    Let { local: MirLocalId, rvalue: Rvalue },
    Store { place: Place, rvalue: Rvalue },
    Nop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Place {
    Local(MirLocalId),
    Static(MirStaticId),
    Deref(Box<Place>),
    Index {
        place: Box<Place>,
        item_index: usize,
    },
    Field {
        base: Box<Place>,
        field: usize,
    },
    EnumItem {
        place: Box<Place>,
        variant: MirTagId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Rvalue {
    BinaryOp {
        op: MirBinOp,
        left: Box<Rvalue>,
        right: Box<Rvalue>,
    },
    UnaryOp {
        op: MirUnOp,
        right: Box<Rvalue>,
    },
    Index {
        place: Box<Place>,
        item_index: usize,
    },
    Field {
        place: Box<Place>,
        item_index: usize,
    },
    Ref(Place),
    RefMut(Place),
    GetFunPtr(MirFunId),
    BuildStruct(Vec<Rvalue>),
    Tuple(Vec<Rvalue>),
    Variant(MirTagId, Box<Rvalue>),
    Len(Place),
    Tag(Place),
    Copy(Place),
    Move(Place),
    Constant(Const),
    Cast(Place, TyId),
    HandlerArg(usize),

    // share
    GcNewObject(Box<Rvalue>),
    GcObjectRef(Box<Rvalue>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminatorKind {
    Goto {
        target: MirBasicBlockId,
        block_args: Vec<Rvalue>,
    },
    SwitchInt {
        discriminant: Rvalue,
        targets: Vec<(Const, MirBasicBlockId)>,
        default: MirBasicBlockId,
    },
    Call {
        func: MirFunId,
        args: Vec<Rvalue>,
        dest: Place,
        target: Option<MirBasicBlockId>,
    },
    CallByPtr {
        func: Rvalue,
        args: Vec<Rvalue>,
        dest: Place,
        target: Option<MirBasicBlockId>,
    },
    Raise {
        control_name: MirControlId,
        args: Vec<Rvalue>,
        dest: Place,
    },
    InstallHandler {
        handler_block: MirBasicBlockId,
        next: MirBasicBlockId,
        args_dest: Vec<MirLocalId>,
        control_id: MirControlId,
    },
    Resume {
        place: Place,
        target: MirBasicBlockId,
    },
    Return,
    Unreachable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeType {
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Const {
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Float32(u64),
    Float64(u64),
    Char(u64),
    Str(String),
    Bool(bool),
    Tuple(Vec<Const>),
    Struct(Vec<Const>),
    Enum(MirTagId, Box<Const>),
    Unit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MirBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MirUnOp {
    Neg,
    Not,
}
