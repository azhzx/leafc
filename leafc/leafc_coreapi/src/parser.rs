use crate::ast::FileAst;
use crate::diagnostic::DiagMsg;
use crate::lexer::{Token, TokenStream};
use crate::source::{Source, SourceId, Span};

#[derive(Debug)]
pub enum ParserError {
    TokenExpect
}

pub enum Require {
    AbsPath(Vec<String>),
    SubPath(Vec<String>),
}

pub struct ParserResult {
    ast: FileAst,
    requires: Vec<Require>,
}

pub trait ParserApi<'a> {
    fn new(source: SourceId, tokens: &'a TokenStream) -> Self;
    fn parse(&self)
        -> Result<ParserResult, DiagMsg>;
}