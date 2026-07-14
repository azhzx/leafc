use crate::diagnostic::DiagMsg;
use crate::lexer::TokenStream;
use crate::source::SourceId;

#[derive(Debug)]
pub enum TokenPassError {
    InvalidPreprocessorParameterDeclare,
    InvalidPreprocessorArgumentList,
    UserPreprocessorPanic,
    InvalidIdentToString,
    InvalidIdentConcat
}

pub trait TokenPassApi<'a> {
    fn new(tokens: &'a TokenStream, source: SourceId) -> Self;
    fn pass(&mut self) -> Result<TokenStream, DiagMsg>;
}