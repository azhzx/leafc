pub type LocalId = usize;
pub type FieldId = usize;
pub type BasicBlockId = usize;

pub type FunId = usize;
pub type StaticId = usize;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrateMirBody {
    pub blocks: Vec<BasicBlock>,
    pub functions: Vec<MirFun>
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirFun {
    pub name: String,
    pub blocks: Vec<BasicBlockId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BasicBlock {
    pub statements: Vec<MirStmt>,
    pub terminator: TerminatorKind,
    pub is_cleanup: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirStmt {
    pub kind: MirStmtKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MirStmtKind {
    Assign {
        place: Place,
        rvalue: Rvalue
    },
    Nop,
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Rvalue {
    Use {
        operand: Box<Rvalue>
    },
    Ref(Place),
    RefMut(Place),
    BinaryOp  {
        op: BinOp,
        left: Box<Rvalue>,
        right: Box<Rvalue>,
    },
    UnaryOp  {
        op: BinOp,
        right: Box<Rvalue>,
    },
    GetFunPtr(FunId),
    Tuple(Vec<Rvalue>),
    Len(Place),
    Discriminant(Place),
    Copy(Place),
    Move(Place),
    Constant(Const),
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminatorKind {
    Goto {
        target: BasicBlockId
    },
    SwitchInt {
        for_switch: Rvalue,
        targets: Vec<(Rvalue, BasicBlockId)>,
        otherwise: BasicBlockId,
    },
    Call {
        func: FunId,
        args: Vec<Rvalue>,
        dest: Place,
        target: Option<BasicBlockId>,
    },
    Return,
    Unreachable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Place {
    Local(LocalId),
    Static(StaticId),
    Deref(Box<Place>),
    Index {
        place: Box<Place>,
        idx: usize
    },
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


#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Const {
    Int(i64),
    UInt(u64),
    Bool(bool),
    Char(char),
    Str(String),
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