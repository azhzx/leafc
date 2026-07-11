use std::fs;
use std::path::{Path, PathBuf};
use leafc_coreapi::codegen::CodegenApi;
use leafc_coreapi::compiler::{CompilerApi, CompilerConfig};
use leafc_coreapi::diagnostic::{DiagMsg, DiagTextColor, DiagnosticianApi};
use leafc_coreapi::hir_lower::HirLowerApi;
use leafc_coreapi::lexer::{LexerApi, TokenStream};
use leafc_coreapi::name_pass::{NamePassApi, NamePassResult};
use leafc_coreapi::parser::{ParserApi};
use leafc_coreapi::source::{Source, SourceId, SourcePool};
use leafc_coreapi::tokens_pass::TokenPassApi;
use leafc_diag::Diagnostician;
use leafc_parser::Parser;
use leafc_namepass::NamePass;

const COMPILER_VERSION: &'static str = env!("CARGO_PKG_VERSION");

const DIAG_COLORS: DiagTextColor = DiagTextColor {
    diag_title: "\x1b[31m",
    diag_message: "\x1b[34m",
    diag_bar: "\x1b[35m",
    diag_source_name: "\x1b[35m",
    diag_reset: "\x1b[0m",
};

pub struct NativeCompiler {
    source_pool: SourcePool,
}

impl CompilerApi for NativeCompiler {
    type Output = ();

    fn new() -> Self {
        Self {
            source_pool: SourcePool(Vec::new())
        }
    }

    fn get_version() -> &'static str {
        COMPILER_VERSION
    }


    fn compile_a_crate(&mut self, dir_path: &str) -> Option<Self::Output> {

        let dir_path_buf = fs::canonicalize(PathBuf::from(dir_path)).unwrap();


        // 解析
        let mut parser = Parser::new(dir_path_buf, &mut self.source_pool);
        let ast = match parser.parse() {
            Ok( ast ) => {
                println!("=== ast ===");
                println!("{:#?}", ast);
                println!("=== === ===");
                ast
            },
            Err(e) => {
                let diag = Diagnostician::new(&self.source_pool, DIAG_COLORS);
                print!("{}", diag.report(e));
                return None;
            }
        };

        let mut name_pass = NamePass::new(ast);
        let name_pass_result = match name_pass.pass() {
            Ok(res @ NamePassResult {
                do_scope_map,
                fun_scope_map,
                tree
            })  => {
                println!("=== scope tree ===");
                println!("{:#?}", tree);
                println!("=== === ===");

                println!("=== fun-scope map ===");
                println!("{:#?}", fun_scope_map);
                println!("=== === ===");

                println!("=== do-scope map ===");
                println!("{:#?}", do_scope_map);
                println!("=== === ===");
                res
            },
            Err(e) => {
                let diag = Diagnostician::new(&self.source_pool, DIAG_COLORS);
                println!("{}", diag.report(e));
                return None;
            }
        };

        Some(())
    }
}