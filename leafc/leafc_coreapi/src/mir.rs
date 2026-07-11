pub type LocalId = usize;
pub type FieldId = usize;
pub type BasicBlockId = usize;
pub type StaticId = usize;

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


#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Const {
    Int(i64),
    Uint(u64),
    Bool(bool),
    Char(char),
    Str(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefKind {
    Shared,
    Mut,
    Fake,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Place {
    Local(LocalId),
    Static(StaticId),
    Deref(Box<Place>),
    Index(Box<Place>, usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operand {
    Copy(Place),
    Move(Place),
    Constant(Const),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Rvalue {
    Use(Operand),
    Ref(RefKind, Place),
    BinaryOp(BinOp, Operand, Operand),
    UnaryOp(UnOp, Operand),
    Cast( usize, Operand, NativeType),
    Tuple(Vec<Operand>),
    Len(Place),
    Discriminant(Place),
    Repeat(Operand, usize),
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminatorKind {
    Goto {
        target: BasicBlockId
    },
    SwitchInt {
        for_switch: Operand,
        targets: Vec<(Rvalue, BasicBlockId)>,
        otherwise: BasicBlockId,
    },
    Call {
        func: Operand,
        args: Vec<Operand>,
        dest: Place,
        target: Option<BasicBlockId>,
        unwind: UnwindAction,
    },
    Return,
    Resume,
    Unreachable,
    Drop {
        place: Place,
        target: BasicBlockId,
        unwind: UnwindAction,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnwindAction {
    Continue,
    Unreachable,
    Terminate,
    Cleanup,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BasicBlockData {
    pub statements: Vec<Statement>,
    pub terminator: TerminatorKind,
    pub is_cleanup: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Statement {
    pub kind: StatementKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StatementKind {
    Assign(Place, Rvalue),
    StorageLive(LocalId),
    StorageDead(LocalId),
    Nop,
    DeInit(Place),
    Validate {
        place: Place,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrateMirBody {
    pub blocks: Vec<BasicBlockData>,
}