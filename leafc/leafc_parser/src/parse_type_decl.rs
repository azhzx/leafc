use leafc_coreapi::ast::{DeclNode, Param, TypeNameString, Visibility};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Token, TokenType};
use leafc_coreapi::parser::{ParserError, ParserResult};
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_type_decl(&mut self, visibility: Visibility) -> Result<(), DiagMsg> {
        self.skip_token();
        let name_token = self.current_token();
        self.skip_token_if(TokenType::Ident)?;
        todo!()
    }
}