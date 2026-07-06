use crate::ast::FileAst;
use crate::diagnostic::DiagMsg;
use crate::lexer::{Token, TokenStream};
use crate::source::{Source, SourceId, Span};

#[derive(Debug)]
pub enum ParserError {
    TokenExpect,
    InvalidTopDeclaration,
    InvalidImportList,
    InvalidOnlyList,
    InvalidUseDeclaration,
    FunctionDeclarationMissingParameterList,
    InvalidGenericList,
    InvalidFunctionParameterList,
    InvalidFunctionBody,
    InvalidGenericParameterList,
    InvalidWhereBody,
    WhereBodyGenericMissingMatchGenericParameterList,
    InvalidTypeDeclaration,
    InvalidTupleLiteral,
    InvalidExpression,
    InvalidOperator,
    InvalidCallArgumentList,
}

#[derive(Debug, Clone)]
pub struct Require {
    pub path: Vec<String>,
    pub is_external_module: bool,
    pub only: Vec<String>,
    pub is_open: bool,
}

#[derive(Debug)]
pub struct ParserResult {
    pub ast: FileAst,
    pub requires: Vec<Require>,
}

pub trait ParserApi<'a> {
    fn new(source: SourceId, tokens: &'a TokenStream) -> Self;
    fn parse(&mut self)
        -> Result<ParserResult, DiagMsg>;
}