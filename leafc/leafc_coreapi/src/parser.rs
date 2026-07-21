use std::collections::HashMap;
use std::path::PathBuf;
use crate::ast::CrateAst;
use crate::crate_meta::{OperatorDef, OperatorKind};
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
    InvalidTupleElement,
    InvalidFunctionType,
    InvalidStructInit,
    InvalidPattern,
    InvalidCatch,
}


pub trait ParserApi<'a> {
    fn new(
        dir_abs_path: PathBuf,
        source_pool: &'a SourcePool,
        abs_path_source_map: &'a AbsPathSourceMap,
        user_operators: &'a HashMap<String, OperatorDef>,
        user_op_info: &'a HashMap<String, (usize, OperatorKind)>,
    ) -> Self;
    fn parse(self)
        -> Result<CrateAst, DiagMsg>;
}