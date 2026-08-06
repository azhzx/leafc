use std::collections::HashMap;
use crate::crate_meta::OperatorDef;
use crate::diagnostic::DiagMsg;
use crate::error_items::DiagCtx;
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


pub trait LexerApi<'a> {
    fn new(
        source: SourceId,
        text: &str,
        user_operators: &'a HashMap<String, OperatorDef>
    ) -> Self;
    fn tokenize(&mut self, diag: &mut DiagCtx) -> TokenStream;
    fn get_document_strings(&self) -> &Document;
}