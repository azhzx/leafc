use leafc_coreapi::{CompilerApi, CompilerConfig};
use leafc_coreapi::codegen::CodegenApi;
use leafc_coreapi::hir_lower::HirLowerApi;
use leafc_coreapi::hir_pass::HirPassApi;
use leafc_coreapi::lexer::LexerApi;
use leafc_coreapi::mir_lower::MirLowerApi;
use leafc_coreapi::name_pass::NamePassApi;
use leafc_coreapi::parser::ParserApi;
use leafc_coreapi::type_checker::TypeCheckerApi;

const COMPILER_VERSION: &'static str = env!("CARGO_PKG_VERSION");

struct NativeCompiler {}

impl CompilerApi for NativeCompiler {
    type Output = ();

    fn get_version() -> &'static str {
        COMPILER_VERSION
    }

    fn compile<'a>(
        &self, code: &str,
        config: &CompilerConfig,
        lexer: impl LexerApi,
        parser: impl ParserApi<'a>,
        name_pass: impl NamePassApi,
        type_checker: impl TypeCheckerApi,
        hir_lower: impl HirLowerApi,
        hir_pass: impl HirPassApi,
        mir_lower: impl MirLowerApi,
        codegen: impl CodegenApi) -> Self::Output {
        todo!()
    }
}