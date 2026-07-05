pub enum MirOp {
    
}

pub struct BasicBlock {
    ops: Vec<MirOp>,
    terminator: Option<MirOp>,
}

pub struct MirModule {
    name: String,
    bbs: Vec<BasicBlock>,
}