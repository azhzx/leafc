mod parse_decl;

use leafc_coreapi;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Token, TokenStream, TokenType};
use leafc_coreapi::parser::{ParserApi, ParserError, ParserResult};
use leafc_coreapi::source::SourceId;

struct Parser<'a> {
    tokens: &'a TokenStream,
    index: usize,
    source: SourceId,
}

impl<'a> Parser<'a> {
    fn current_token(&self) -> &Token {
        match self.tokens.data.get(self.index) {
            Some(t) => t,
            None => &self.tokens.data[self.index - 1]
        }
    }
    fn skip_token(&mut self) {
        if self.index >= self.tokens.data.len() {
            return;
        }
        self.index += 1;
    }
    fn skip_token_if(&mut self, expected: TokenType) -> Result<(), DiagMsg> {
        let tok = self.current_token();
        if tok.kind == expected {
            self.skip_token();
            return Ok(());
        }
        
        Err(DiagMsg {
            title: format!("{:?}", ParserError::TokenExpect),
            msg: format!("expected {:?} but got {:?}", expected, tok.kind),
            span: tok.span.clone(),
            source: self.source,
        })

    }
}

impl<'a> ParserApi<'a> for Parser<'a> {
    fn new(source: SourceId, tokens: &'a TokenStream) -> Self {
        Parser {
            tokens,
            index: 0,
            source,
        }
    }

    fn parse(&self) -> Result<ParserResult, DiagMsg> {
        todo!()
    }
}