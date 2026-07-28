use crate::source::Span;
use crate::type_system::TyId;

pub type LocalId = usize;
pub type BasicBlockId = usize;

pub type FunId = usize;
pub type StaticId = usize;

pub type TagId = usize;

pub type ControlId = usize;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirCrate {
    pub name: String,
    pub functions: Vec<MirFun>,
    pub extern_decls: Vec<ExternDecl>,
    pub statics: Vec<StaticDecl>,
    pub blocks: Vec<BasicBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirFun {
    pub name: String,
    pub generic_params: Vec<TyId>,
    pub signature: FnSig,
    pub local_decls: Vec<LocalDecl>,
    pub blocks: Vec<BasicBlockId>,
    pub span: Span
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternDecl {
    pub name: String,
    pub signature: FnSig,
    pub span: Span
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StaticDecl {
    pub name: String,
    pub ty: TyId,
    pub mutable: bool,
    pub init: Const, // todo : 暂时const
    pub span: Span
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalDecl {
    pub ty: TyId,
    pub mutable: bool,
    pub name: Option<String>,
    pub span: Span
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FnSig {
    pub params: Vec<TyId>,
    pub return_ty: TyId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BasicBlock {
    pub block_params: Vec<LocalId>,
    pub statements: Vec<MirStmt>,
    pub terminator: TerminatorKind,
    pub span: Span
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirStmt {
    pub kind: MirStmtKind,
    pub span: Span
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MirStmtKind {
    Let {
        local: LocalId,
        rvalue: Rvalue,
    },
    Store {
        place: Place,
        rvalue: Rvalue,
    },
    Nop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Place {
    Local(LocalId),
    Static(StaticId),
    Deref(Box<Place>),
    Index {
        place: Box<Place>,
        item_index: usize
    },
    Field {
        base: Box<Place>,
        field: usize,
    },
    EnumItem {
        place: Box<Place>,
        variant: TagId,
    },
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Rvalue {
    BinaryOp  {
        op: MirBinOp,
        left: Box<Rvalue>,
        right: Box<Rvalue>,
    },
    UnaryOp  {
        op: MirUnOp,
        right: Box<Rvalue>,
    },
    Index {
        place: Box<Place>,
        item_index: usize
    },
    Field {
        place: Box<Place>,
        item_index: usize
    },
    TempRef(Place),
    TempRefMut(Place),
    GetFunPtr(FunId),
    BuildStruct(Vec<Rvalue>),
    Tuple(Vec<Rvalue>),
    Variant(TagId, Box<Rvalue>),
    Len(Place),
    Tag(Place),
    Copy(Place),
    Move(Place),
    Constant(Const),
    Cast(Place, TyId),

    // share
    GcNewObject(Box<Rvalue>),
    GcObjectRef(Box<Rvalue>),
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminatorKind {
    Goto {
        target: BasicBlockId,
        block_args: Vec<Rvalue>,
    },
    SwitchInt {
        discriminant: Rvalue,
        targets: Vec<(Const, BasicBlockId)>,
        default: BasicBlockId,
    },
    Call {
        func: FunId,
        args: Vec<Rvalue>,
        dest: Place,
        target: Option<BasicBlockId>,
    },
    CallByPtr {
        func: Rvalue,
        args: Vec<Rvalue>,
        dest: Place,
        target: Option<BasicBlockId>,
    },
    Raise {
        control_name: ControlId,
        args: Vec<Rvalue>,
    },
    InstallHandler {
        handler_block: BasicBlockId,
        next: BasicBlockId,
    },
    Resume {
        place: Place,
        target: BasicBlockId,
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