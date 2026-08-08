use crate::crate_meta::OperatorDef;
use crate::diagnose::DiagCollector;
use crate::id::FileId;
use crate::token::TokenStream;
use std::collections::HashMap;

pub trait PassNode {}

pub trait LexerApi: PassNode {
    fn pass(
        source: FileId,
        text: &str,
        user_operators: &HashMap<String, OperatorDef>,
    ) -> (TokenStream, DiagCollector);
}
