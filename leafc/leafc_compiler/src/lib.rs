use std::collections::HashMap;
use leafc_coreapi::compiler::{CompilerApi, IncrementalCompiler};
use leafc_coreapi::diagnostic::{DiagTextColor, DiagnosticianApi};
use leafc_coreapi::hir_lower::HirLowerApi;
use leafc_coreapi::name_pass::{NamePassApi, NamePassResult};
use leafc_coreapi::parser::ParserApi;
use leafc_coreapi::source::{AbsPathSourceMap, SourcePool};
use leafc_diag::Diagnostician;
use leafc_hirlower::HirLower;
use leafc_namepass::NamePass;
use leafc_parser::Parser;

use std::fs;
use std::path::PathBuf;
use leafc_coreapi::type_checker::TypeCheckerApi;
use leafc_typechecker::TypeChecker;
use realworld_io_api::RealWorldIOApi;

const COMPILER_VERSION: &'static str = env!("CARGO_PKG_VERSION");

const DIAG_COLORS: DiagTextColor = DiagTextColor {
    diag_title: "\x1b[31m",
    diag_message: "\x1b[34m",
    diag_bar: "\x1b[35m",
    diag_source_name: "\x1b[35m",
    diag_reset: "\x1b[0m",
};

pub struct RealWorld {}
impl RealWorldIOApi for RealWorld {
    fn println(text: &String) {
        println!("{}", text);
    }
    fn print(text: &String) {
        print!("{}", text);
    }
    fn read_file(path: &PathBuf) -> std::io::Result<String> {
        fs::read_to_string(&path)
    }
}

pub struct NativeCompiler {
    crate_path: PathBuf,
    source_pool: SourcePool,
    abs_path_source_map: AbsPathSourceMap
}

impl NativeCompiler {
    fn collect_sources(&mut self, dir: &PathBuf) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                self.collect_sources(&path)?;
            } else if path.is_file() {
                if path.extension().and_then(|s| s.to_str()) == Some("leaf") {
                    let abs_path = fs::canonicalize(&path)?
                        .to_string_lossy()
                        .to_string();

                    let content = RealWorld::read_file(&path)?;
                    let source_id = self.source_pool.add_source(abs_path.clone(), content);

                    self.abs_path_source_map.insert(abs_path, source_id);
                }
            }
        }
        Ok(())
    }
}

impl CompilerApi for NativeCompiler {
    type Output = ();

    fn new() -> Self {
        Self {
            crate_path: PathBuf::new(),
            source_pool: SourcePool(Vec::new()),
            abs_path_source_map: HashMap::new(),
        }
    }

    fn get_version() -> &'static str {
        COMPILER_VERSION
    }

    fn set_crate_path(&mut self, dir_path: &str) -> Option<&mut Self> {
        let dir_path_buf = fs::canonicalize(PathBuf::from(dir_path)).ok()?;

        if let Err(e) = self.collect_sources(&dir_path_buf) {
            eprintln!("Failed to collect sources: {}", e);
            return None
        }
        return Some(self)
    }


    fn compile(&mut self) -> Option<Self::Output> {

        let diag = Diagnostician::new(&self.source_pool, DIAG_COLORS);

        // 解析
        let mut parser = Parser::new(
            self.crate_path.clone(),
            &self.source_pool,
            &self.abs_path_source_map,
        );
        let ast = match parser.parse() {
            Ok( ast ) => {
                println!("=== ast ===");
                println!("{:#?}", ast);
                println!("=== === ===");
                ast
            },
            Err(e) => {
                print!("{}", diag.report(e));
                return None;
            }
        };

        let mut name_pass = NamePass::new(&ast);
        let name_pass_result = match name_pass.pass() {
            Ok(res @ NamePassResult { .. })  => {
                println!("=== scope tree ===");
                println!("{:#?}", res.pool);
                println!("=== === ===");

                println!("=== fun-scope map ===");
                println!("{:#?}", res.fun_scope_map);
                println!("=== === ===");

                println!("=== do-scope map ===");
                println!("{:#?}", res.do_scope_map);
                println!("=== === ===");
                res
            },
            Err(e) => {
                println!("{}", diag.report(e));
                return None;
            }
        };

        let crate_name = self.crate_path.file_stem().unwrap().to_str().unwrap().to_string();
        let hir_lower = HirLower::new(&ast, name_pass_result, crate_name);

        let hir = match hir_lower.lower() {
            Ok(hir) => {
                println!("=== hir ===");
                println!("{:#?}", hir);
                println!("=== === ===");
                hir
            }
            Err(e) => {
                println!("{}", diag.report(e));
                return None;
            }
        };

        let type_checker = TypeChecker::new(hir);
        let ty_map = match type_checker.check() {
            Ok(hir) => {
                println!("=== ty decl map ===");
                println!("{:#?}", hir.decl_type_map);
                println!("=== === ===");

                println!("=== ty expr map ===");
                println!("{:#?}", hir.expr_type_map);
                println!("=== === ===");
                hir
            }
            Err(e) => {
                println!("{}", diag.report(e));
                return None;
            }
        };

        Some(())
    }
}

impl IncrementalCompiler for NativeCompiler {
    fn edit_append(&mut self, abs_path: String, line: &str, start_offset: usize) -> &mut Self {
        todo!()
    }

    fn edit_remove(&mut self, abs_path: String, start_offset: usize) -> &mut Self {
        todo!()
    }
}