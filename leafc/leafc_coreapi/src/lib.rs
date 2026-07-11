extern crate core;

pub mod lexer;
pub mod source;
pub mod parser;
pub mod name_pass;

pub mod type_checker;
pub mod ast;
pub mod codegen;
pub mod mir;
pub mod hir_lower;
pub mod mir_lower;
pub mod hir;

pub mod tokens_pass;
pub mod diagnostic;
pub mod scope;
pub mod compiler;
pub mod mir_mono;
pub mod mir_borrow_checker;