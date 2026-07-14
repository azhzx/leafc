use std::path::PathBuf;
use crate::ast::CrateAst;
use crate::diagnostic::DiagMsg;
use crate::lexer::{Token, TokenStream};
use crate::scope::ScopePool;
use crate::source::{AbsPathSourceMap, Source, SourceId, SourcePool, Span};

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


pub trait ParserApi<'a> {
    fn new(
        dir_abs_path: PathBuf,
        source_pool: &'a mut SourcePool,
        abs_path_source_map: &'a AbsPathSourceMap
    ) -> Self;
    fn parse(self)
        -> Result<CrateAst, DiagMsg>;
}