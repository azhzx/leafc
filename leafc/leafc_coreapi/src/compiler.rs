use std::path::PathBuf;
use crate::ast::FileAst;
use crate::codegen::CodegenApi;
use crate::diagnostic::{DiagMsg, DiagnosticianApi};

pub struct CompilerConfig {

}

pub trait CompilerApi<'a> {
    type Output;

    fn get_version() -> &'static str;
    fn compile_to_ast(
        &mut self,
        file_path: PathBuf,
        diag: &mut impl DiagnosticianApi
    ) -> Result<Vec<FileAst>, DiagMsg>;
    fn compile_a_module(&mut self, dir_path: &str) -> Option<Self::Output>;
}