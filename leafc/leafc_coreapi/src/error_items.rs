use thiserror::Error;
use crate::source::Span;
use crate::token::LiteToken;

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub span: Span,
    pub replacement: Vec<LiteToken>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum Context {
    FunctionSignature {
        name: String,
        params: Vec<(String, String)>,
        return_ty: String,
    },
    EnumDefinition {
        name: String,
        variants: Vec<String>,
    },
}
const MESSAGE_TEMPLATES: &[&str] = &[
    // Lexer
    "unexpected end of file",
    "Unexpected end of input in escape sequence",
    "Invalid escape sequence {}",
    "Unclosed string literal",
    "invalid character '{ch}'",
    "invalid indent",

    // Preprocessor
    "cannot parse integer: {}",
    "cannot eval expression",
    "invalid preprocessor syntax",
    "unexpected end of tokens in conditional",
    "unexpected end of tokens after macro name",
    "unmatched parentheses in macro arguments",
    "macro {} expects at least {} arguments, got {}",
    "macro {} expects {} arguments, got {}",
    "invalid macro argument list",
    "user panic {}",
    "ident to string expects exactly one identifier argument",
    "repeat count must be non-negative",

    // Parser

    // Name Pass
    "undefined name `{name}`",
    "duplicate definition of `{name}`",
    "undefined module `{name}`",
    "cannot access member `{member}` of `{base}`",
    "invalid ADT constructor `{name}`",

    // HirLower
    "empty path",
    "path `{path}` not found",
    "module scope for `{module}` not found",
    "struct `{struct_name}` has no field `{field}`",
    "ADT `{adt}` has no constructor `{ctor}`",
    "effect `{effect}` has no control `{control}`",
    "invalid path `{path}`",
    "generic parameter `{name}` not found",
    "binding `{name}` not found",
    "parameter `{name}` not found",
    "method `{name}` not found",
    "constructor `{name}` not found",
    "name `{name}` not found",
    "let variable `{name}` not found",

    // Type Checker
    "duplicate type `{name}`",
    "infinite type `{ty}`",
    "type mismatch: expected `{expected}`, found `{found}`",
    "generic `{name}` arity mismatch: expected {expected} arguments, got {got}",
    "arity mismatch: expected {expected}, got {got}",
    "undefined variable `{name}`",
    "type of `{name}` not yet checked",
    "internal error: {message}",
    "struct `{struct_name}` has no field `{field_name}`",
    "missing type annotation for `{name}`",
    "undefined type `{name}`",
];