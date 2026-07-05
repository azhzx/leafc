use leafc_coreapi::ast::ExprNodeId;
use leafc_coreapi::diagnostic::DiagMsg;
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_expr(&mut self) -> Result<ExprNodeId, DiagMsg> {
        todo!()
    }
}