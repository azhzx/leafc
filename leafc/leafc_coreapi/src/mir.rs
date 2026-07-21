use crate::type_system::TyId;

pub type LocalId = usize;
pub type BasicBlockId = usize;

pub type FunId = usize;
pub type StaticId = usize;

pub type TagId = usize;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirCrate {
    pub functions: Vec<MirFun>,
    pub blocks: Vec<BasicBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirFun {
    pub name: String,
    pub signature: FnSig,
    pub local_decls: Vec<LocalDecl>,
    pub blocks: Vec<BasicBlockId>, // MirCrate.blocks的下标
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalDecl {
    pub ty: TyId,
    pub mutable: bool,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FnSig {
    pub params: Vec<TyId>,
    pub return_ty: TyId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BasicBlock {
    pub statements: Vec<MirStmt>,
    pub terminator: TerminatorKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirStmt {
    pub kind: MirStmtKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MirStmtKind {
    Assign {
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
    EnumItem {
        place: Box<Place>,
        variant: TagId
    }
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Rvalue {
    TempRef(Place),
    TempRefMut(Place),
    BinaryOp  {
        op: BinOp,
        left: Box<Rvalue>,
        right: Box<Rvalue>,
    },
    UnaryOp  {
        op: UnOp,
        right: Box<Rvalue>,
    },
    Index {
        place: Box<Place>,
        item_index: usize
    },
    GetFunPtr(FunId),
    Tuple(Vec<Rvalue>),
    Variant(TagId, Box<Rvalue>),
    Len(Place),
    Tag(Place),
    Copy(Place),
    Move(Place),
    Constant(Const),

    GcNew(Box<Rvalue>),
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminatorKind {
    Goto {
        target: BasicBlockId
    },
    SwitchInt {
        for_switch: Rvalue,
        targets: Vec<(Rvalue, BasicBlockId)>,
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
pub enum BinOp {
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
pub enum UnOp {
    Neg,
    Not,
}