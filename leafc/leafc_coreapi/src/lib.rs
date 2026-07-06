extern crate core;

use crate::codegen::CodegenApi;
use crate::hir_lower::HirLowerApi;
use crate::hir_pass::HirPassApi;
use crate::lexer::LexerApi;
use crate::mir_lower::MirLowerApi;
use crate::name_pass::NamePassApi;
use crate::parser::ParserApi;
use crate::type_checker::TypeCheckerApi;

pub mod lexer;
pub mod source;
pub mod parser;
pub mod symbol;
pub mod symbol_name;
pub mod name_pass;

pub mod type_checker;
pub mod ast;
pub mod codegen;
pub mod mir;
pub mod hir_lower;
pub mod hir_pass;
pub mod mir_lower;
pub mod hir;

pub mod tokens_pass;
pub mod diagnostic;

pub struct CompilerConfig {
    
}

pub trait CompilerApi {
    type Output;
    fn get_version() -> &'static str;
    fn compile<'a>(
        &self,
        code: &str,
        config: &CompilerConfig,
        
        lexer: impl LexerApi,
        parser: impl ParserApi<'a>,
        name_pass: impl NamePassApi<'a>,
        type_checker: impl TypeCheckerApi,
        hir_lower: impl HirLowerApi,
        hir_pass: impl HirPassApi,
        mir_lower: impl MirLowerApi,
        codegen: impl CodegenApi,
    ) -> Self::Output;
}
