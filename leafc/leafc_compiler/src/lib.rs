use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use leafc_coreapi::ast::{AstModule, FileAst};
use leafc_coreapi::codegen::CodegenApi;
use leafc_coreapi::compiler::{CompilerApi, CompilerConfig};
use leafc_coreapi::diagnostic::{DiagMsg, DiagTextColor, DiagnosticianApi};
use leafc_coreapi::hir_lower::HirLowerApi;
use leafc_coreapi::lexer::LexerApi;
use leafc_coreapi::mir_lower::MirLowerApi;
use leafc_coreapi::name_pass::{NamePassApi, NamePassResult};
use leafc_coreapi::parser::{ParserApi, ParserResult};
use leafc_coreapi::scope::ScopeId;
use leafc_coreapi::source::{SourceId, SourcePool};
use leafc_coreapi::tokens_pass::TokenPassApi;
use leafc_coreapi::type_checker::TypeCheckerApi;
use leafc_diag::Diagnostician;
use leafc_lexer::Lexer;
use leafc_parser::Parser;
use leafc_tokenpass::Preprocessor;
use leafc_namepass::NamePass;

const COMPILER_VERSION: &'static str = env!("CARGO_PKG_VERSION");

const MAIN_LEAF_FILE: &'static str = "main.leaf";

pub struct NativeCompiler {
    processed_abs_paths: HashSet<PathBuf>,
}

impl NativeCompiler {
    pub fn new() -> Self {
        Self {
            processed_abs_paths: HashSet::new()
        }
    }
}

impl CompilerApi<'_> for NativeCompiler {
    type Output = ();

    fn get_version() -> &'static str {
        COMPILER_VERSION
    }

    fn compile_to_ast(
        &mut self,
        file_path: PathBuf,
        diag: &mut impl DiagnosticianApi
    ) -> Result<Vec<FileAst>, DiagMsg> {

        let file_path = match fs::canonicalize(&file_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to canonicalize path {:?} because of {}", file_path, e);
                std::process::exit(9);
            }
        };

        if self.processed_abs_paths.contains(&file_path) {
            return Ok(vec![]);
        }
        self.processed_abs_paths.insert(file_path.clone());

        let code = match fs::read_to_string(&file_path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("Cannot to read {:?} because of {}", file_path, e);
                std::process::exit(9);
            }
        };

        let source_id = diag.add_source(
            file_path.to_str().unwrap().to_string(), code.clone());

        let mut lex = Lexer::new(source_id, &code);
        let tokens = match lex.tokenize() {
            Ok(tokens) => {
                println!("Tokens: {:#?}", tokens);
                tokens
            },
            Err(e) => {
                return Err(e);
            }
        };

        // let new_tokens = match Preprocessor::new(&tokens, source_id)
        //     .pre_definitions(
        //         vec![
        //             if cfg!(target_os = "windows") {
        //                 "__Windows".to_string()
        //             } else if cfg!(target_os = "macos") {
        //                 "__Mac".to_string()
        //             } else if cfg!(target_os = "linux") {
        //                 "__Linux".to_string()
        //             } else {
        //                 "__Unknown".to_string()
        //             }
        //         ]
        //     )
        //     .pass() {
        //     Ok(new_tokens) => {
        //         println!("\n\n== token pass ==\n\n");
        //
        //         for token in &new_tokens.data {
        //             println!("{:?}", token);
        //         }
        //         println!("== === ==");
        //         new_tokens
        //     }
        //     Err(e) => {
        //         return Err(e);
        //     }
        // };


        let ast = match Parser::new(source_id, &tokens).parse() {
            Ok(ParserResult {ast})  => {
                println!("=== ast ===");

                println!("{:#?}", ast);
                println!("=== === ===");

                println!("=== requires ==");
                println!("{:?}", ast.requires);
                println!("=== === ===");

                ast
            },
            Err(e) => {
                return Err(e);
            }
        };

        let requires = ast.requires.clone();
        let mut asts = vec![ast];

        for require in requires{
            if require.is_external_module {
                unimplemented!()
            }
            let dir = file_path.parent().unwrap_or(Path::new("."));
            let mut dep_path = PathBuf::new();
            for item in require.path {
                dep_path.push(item);
            }
            let full_path = dir.join(dep_path).with_extension("leaf");

            let dep_asts = self.compile_to_ast(full_path, diag)?;
            asts.extend(dep_asts);
        }

        Ok(asts)

    }

    fn compile_a_module(&mut self, dir_path: &str) -> Option<Self::Output> {

        let dir_path_buf = PathBuf::from(dir_path);
        let main_file_path = dir_path_buf.join(MAIN_LEAF_FILE);

        let main_file_abs = match fs::canonicalize(&main_file_path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to canonicalize path {:?} because of {}", main_file_path, e);
                return None;
            }
        };


        let source_pool = SourcePool::new();
        let mut diag = Diagnostician::new(source_pool, DiagTextColor {
            diag_title: "\x1b[31m",
            diag_message: "\x1b[34m",
            diag_bar: "\x1b[35m",
            diag_source_name: "\x1b[35m",
            diag_reset: "\x1b[0m",
        });

        let asts = match self.compile_to_ast(main_file_abs, &mut diag) {
            Ok(asts) => {asts},
            Err(e) => {
                print!("{}", diag.report(e));
                return None;
            }
        };

        let ast_module = AstModule { asts };

        let mut name_pass = NamePass::new(&ast_module);
        let name_pass_result = match name_pass.pass() {
            Ok(res @ NamePassResult {
                top_scope_ids,
                scope_pool,
                do_scope_map,
                fun_scope_map
            })  => {
                println!("=== scope ===");
                println!("{:#?}", top_scope_ids);
                println!("=== === ===");

                println!("=== scope ===");
                println!("{:#?}", scope_pool);
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
                println!("{}", diag.report(e));
                return None;
            }
        };

        None
    }
}