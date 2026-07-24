use std::collections::HashMap;
use crate::crate_meta::OperatorDef;
use crate::diagnostic::DiagMsg;
use crate::source::{SourceId, Span};
use crate::token::Token;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenStream {
    pub data: Vec<Token>
}

pub struct DocumentString {
    pub span: Span,
    pub data: String
}

pub struct Document {
    pub data: Vec<DocumentString>
}

#[derive(Debug)]
pub enum LexerError {
    UnexpectedEof,
    InvalidString,
    InvalidIndent,
    InvalidChar,
}

pub trait LexerApi<'a> {
    fn new(
        source: SourceId,
        text: &String,
        user_operators: &'a HashMap<String, OperatorDef>,
    ) -> Self;
    fn tokenize(&mut self)
        -> Result<TokenStream, DiagMsg>;
    fn get_document_strings(&self) -> &Document;
}