use std::collections::{HashMap, HashSet};
use leafc_coreapi::compiler::{CompilerApi, IncrementalCompiler};
use leafc_coreapi::diagnostic::{DiagMsg, DiagTextColor, DiagnosticianApi};
use leafc_coreapi::hir_lower::HirLowerApi;
use leafc_coreapi::name_pass::{NamePassApi, NamePassResult};
use leafc_coreapi::parser::ParserApi;
use leafc_coreapi::source::{AbsPathSourceMap, SourceId, SourcePool};
use leafc_diag::Diagnostician;
use leafc_hirlower::HirLower;
use leafc_namepass::NamePass;
use leafc_parser::Parser;

use std::{fs, process};
use std::path::PathBuf;
use std::sync::Arc;
use intervaltree::{Element, IntervalTree};
use leafc_coreapi::ast::{CrateAst, GreenDecl};
use leafc_coreapi::codegen::CodegenApi;
use leafc_coreapi::crate_meta::{CrateManifest, OperatorDef, OperatorKind};
use leafc_coreapi::mir::MirCrate;
use leafc_coreapi::mir_lifetime_checker::MirLifetimeCheckerApi;
use leafc_coreapi::mir_lower::MirLowerApi;
use leafc_coreapi::mir_mono::MirMonoApi;
use leafc_coreapi::type_checker::TypeCheckerApi;
use leafc_coreapi::type_system::TypeCtx;
use leafc_c_codegen::CCodeGen;
use leafc_mir_lifetime_checker::MirLifetimeChecker;
use leafc_mirlower::MirLower;
use leafc_mirmono::MirMono;
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
    abs_path_source_map: AbsPathSourceMap,
    ast_cache: CrateAst,
    file_decl_trees: HashMap<SourceId, IntervalTree<usize, Arc<GreenDecl>>>,
    manifest_operators: HashMap<String, OperatorDef>,
    user_op_info: HashMap<String, (usize, OperatorKind)>,
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

    /// 根据文件单元构建声明区间树
    fn build_decl_tree(file_unit: &leafc_coreapi::ast::FileRedUnit) -> IntervalTree<usize, Arc<GreenDecl>> {
        let elements: Vec<Element<usize, Arc<GreenDecl>>> = file_unit.green.top_decls
            .iter()
            .map(|child| {
                let start = file_unit.span.start_off + child.relative_start;
                let end = start + child.node.text_len;
                Element::from((start..end, Arc::clone(&child.node)))
            })
            .collect();
        IntervalTree::from_iter(elements)
    }

    pub fn write_to_path(&self, src: String) -> std::io::Result<PathBuf> {
        let out_path = self.crate_path.join("build/out.c");
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&out_path, src)?;
        Ok(out_path)
    }

    pub fn run_by_gcc(&self) -> std::io::Result<()> {
        println!("start to call gcc");

        let build_dir = self.crate_path.join("build");
        let c_file = build_dir.join("out.c");
        if !c_file.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("C file not found at {}", c_file.display()),
            ));
        }

        let clean_path = |p: &std::path::Path| -> String {
            let s = p.to_str().unwrap_or("");
            if s.starts_with(r"\\?\") {
                s[4..].to_string()
            } else if s.starts_with(r"\\?\UNC\") {
                format!(r"\\{}", &s[7..])
            } else {
                s.to_string()
            }
        };

        let c_file_str = clean_path(&c_file);

        let exe_name = if cfg!(target_os = "windows") {
            "out.exe"
        } else {
            "out"
        };
        let exe_file = build_dir.join(exe_name);
        let exe_file_str = clean_path(&exe_file);

        let output = process::Command::new("gcc")
            .arg(&c_file_str)
            .arg("-o")
            .arg(&exe_file_str)
            .arg("-std=c11")
            .arg("-Wall")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("gcc failed:\n{}", stderr),
            ));
        }

        println!("start to run");
        let mut child = std::process::Command::new(&exe_file).spawn()?;
        let status = child.wait()?;
        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("executable exited with status {}", status),
            ));
        }
        Ok(())
    }}

impl CompilerApi for NativeCompiler {
    type Output = <CCodeGen as CodegenApi>::Output;

    fn new() -> Self {
        Self {
            crate_path: PathBuf::new(),
            source_pool: SourcePool(Vec::new()),
            abs_path_source_map: HashMap::new(),
            ast_cache: CrateAst { external_requires: vec![], file_units: vec![] },
            file_decl_trees: HashMap::new(),
            manifest_operators: HashMap::new(),
            user_op_info: HashMap::new(),
        }
    }

    fn get_version() -> &'static str {
        COMPILER_VERSION
    }

    fn set_crate_path(&mut self, dir_path: &str) -> Option<&mut Self> {
        let dir_path_buf =  PathBuf::from(dir_path);
        let abs_dir_path_buf = fs::canonicalize(&dir_path_buf).ok()?;

        self.crate_path = abs_dir_path_buf;

        if let Err(e) = self.collect_sources(&self.crate_path.clone()) {
            eprintln!("Failed to collect sources: {}", e);
            return None
        }
        Some(self)
    }


    fn compile(&mut self, out: &mut Option<Self::Output>) -> &mut Self {

        let leaf_toml_path = self.crate_path.join("LeafCrate.toml");

        let content = match RealWorld::read_file(&leaf_toml_path) {
            Ok(content) => content,
            Err(err) => {
                eprintln!("fail to read {}: {}", leaf_toml_path.display(), err);
                process::exit(1);
            }
        };

        let manifest = match CrateManifest::from_str(&content) {
            Ok(manifest) => manifest,
            Err(err) => {
                eprintln!("fail to parse LeafCrate.toml: {}", err);
                process::exit(1);
            }
        };

        self.manifest_operators = manifest.operator;
        self.user_op_info = {
            let mut info = HashMap::new();
            for (_op_name, def) in &self.manifest_operators {
                use leafc_coreapi::crate_meta::PriorityRelation;
                let base_prio = match def.priority_relation() {
                    PriorityRelation::HigherThan(op) => Parser::builtin_priority(op) + Parser::PRIORITY_OFFSET,
                    PriorityRelation::LowerThan(op) => Parser::builtin_priority(op) - Parser::PRIORITY_OFFSET,
                };
                info.insert(def.text.clone(), (base_prio, def.kind));
            }
            info
        };

        let diag = Diagnostician::new(&self.source_pool, DIAG_COLORS);

        // 解析
        let parser = Parser::new(
            self.crate_path.clone(),
            &self.source_pool,
            &self.abs_path_source_map,
            &self.manifest_operators,
            &self.user_op_info,
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
                *out = None;
                return self;
            }
        };

        // 为每个文件构建声明区间树
        for file_unit in &ast.file_units {
            let tree = Self::build_decl_tree(file_unit);
            self.file_decl_trees.insert(file_unit.span.source_id, tree);
        }

        let mut name_pass = NamePass::new(&ast);
        let name_pass_result = match name_pass.pass() {
            Ok(res)  => {
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
                *out = None;
                return self;
            }
        };

        let crate_name = self.crate_path.file_stem().unwrap().to_str().unwrap().to_string();
        let hir_lower = HirLower::new(&ast, name_pass_result, crate_name);

        let hir = match hir_lower.lower() {
            Ok(hir) => {
                println!("=== hir crate name ===");
                println!("{:#?}", hir.name);
                println!("=== === ===");

                println!("=== hir main fun ===");
                println!("{:#?}", hir.main_fun);
                println!("=== === ===");

                println!("=== hir pub externals  ===");
                println!("{:#?}", hir.pub_decl_ids);
                println!("=== === ===");

                println!("=== hir exprs  ===");
                println!("{:#?}", hir.hir_expr_pool);
                println!("=== === ===");

                println!("=== hir decls  ===");
                println!("{:#?}", hir.hir_decl_pool);
                println!("=== === ===");

                hir
            }
            Err(e) => {
                println!("{}", diag.report(e));
                *out = None;
                return self;
            }
        };

        let type_checker = TypeChecker::new(hir);
        let (ty_map, hir) = match type_checker.check() {
            Ok((ty_map, hir)) => {
                println!("=== ty decl map ===");
                println!("{:#?}", ty_map.decl_type_map);
                println!("=== === ===");

                println!("=== ty expr map ===");
                println!("{:#?}", ty_map.expr_type_map);
                println!("=== === ===");

                println!("=== ty binding map ===");
                println!("{:#?}", ty_map.local_binding_map);
                println!("=== === ===");

                println!("=== ty pool ===");
                for (index, ty) in ty_map.type_pool.iter().enumerate() {
                    println!("{} => {:?}", index, ty);
                }
                println!("=== === ===");

                (ty_map, hir)
            }
            Err(e) => {
                println!("{}", diag.report(e));
                *out = None;
                return self;
            }
        };

        let mir_lower = MirLower::new(ty_map, hir);
        let (mir, ty_map) = match mir_lower.lower() {
            Ok((mir, ty_map)) => {
                println!("=== mir ===");
                println!("{:#?}", mir);
                println!("=== === ===");
                (mir, ty_map)
            }
            Err(e) => {
                println!("{}", diag.report(e));
                *out = None;
                return self;
            }
        };

        let lifetime_checker = MirLifetimeChecker::new(mir, ty_map);
        let (mir, ty_map) = match lifetime_checker.check() {
            Ok((mir, ty_map)) => {
                (mir, ty_map)
            }
            Err(e) => {
                println!("{}", diag.report(e));
                *out = None;
                return self;
            }
        };

        let mir_mono = MirMono::new(mir, ty_map);
        let (mono_mir, ty_map) = match mir_mono.mono() {
            Ok((mir, ty_map)) => {
                println!("=== mono mir ===");
                println!("{:#?}", mir);
                println!("=== === ===");
                (mir, ty_map)
            }
            Err(e) => {
                println!("{}", diag.report(e));
                *out = None;
                return self;
            }
        };
        
        let codegen = CCodeGen::new(mono_mir, ty_map);
        let c_code = match codegen.emit() {
            Ok(c_code) => c_code,
            Err(e) => {
                println!("{}", diag.report(e));
                *out = None;
                return self;
            }
        };
        
        
        self.ast_cache = ast;
        *out = Some(c_code);
        self
    }
}

impl IncrementalCompiler for NativeCompiler {
    fn edit_append(
        &mut self,
        abs_path: String,
        line: &str,
        start_offset: usize,
    ) -> &mut Self {
        let source_id = self.source_pool.find_source(
            fs::canonicalize(abs_path).unwrap().to_str().unwrap().to_string()
        ).unwrap();
        let source = &mut self.source_pool.0[source_id];
        source.file_content.insert_str(start_offset, line);
        let new_len = line.len();
        let affected_range = start_offset..start_offset + new_len;

        self.apply_edit(source_id, affected_range);

        self
    }

    fn edit_remove(&mut self, abs_path: String, start_offset: usize) -> &mut Self {
        let source_id = self.source_pool.find_source(
            fs::canonicalize(abs_path).unwrap().to_str().unwrap().to_string()
        ).unwrap();
        let source = &mut self.source_pool.0[source_id];
        let end_offset = source.file_content[start_offset..]
            .find('\n')
            .map(|i| start_offset + i)
            .unwrap_or(source.file_content.len());
        source.file_content.replace_range(start_offset..end_offset, "");
        let affected_range = start_offset..end_offset;

        self.apply_edit(source_id, affected_range);

        self
    }
}

impl NativeCompiler {
    /// 应用编辑
    fn apply_edit(
        &mut self,
        source_id: SourceId,
        affected_range: std::ops::Range<usize>
    ) {
        let diag = Diagnostician::new(&self.source_pool, DIAG_COLORS);

        let old_tree = self.file_decl_trees.remove(&source_id)
            .unwrap_or_else(|| IntervalTree::from_iter(std::iter::empty::<Element<usize, Arc<GreenDecl>>>()));

        let old_file_unit = self.ast_cache.file_units
            .iter()
            .find(|fu| fu.span.source_id == source_id)
            .expect("file unit must exist");

        let source = &self.source_pool.0[source_id];
        let content = &source.file_content;

        let token = Parser::lexer(
            source_id,
            content,
            &self.manifest_operators,
        ).expect("lexer error");
        let token = Parser::pp(source_id, &token).expect("preprocessor error");

        let mut parser = Parser {
            dir_abs_path: self.crate_path.clone(),
            tokens: token,
            index: 0,
            source_pool: &self.source_pool,
            abs_path_sources: &self.abs_path_source_map,
            ast: CrateAst { external_requires: vec![], file_units: vec![] },
            user_operators: &self.manifest_operators,
            user_op_info: &self.user_op_info.clone(),
        };

        let new_file_unit = parser.parse_file_incremental(
            old_file_unit.green.name.node.name.clone(),
            &old_file_unit.green,
            &old_tree,
            affected_range,
        ).expect("incremental parse error");

        self.ast_cache.file_units.retain(|fu| fu.span.source_id != source_id);
        self.ast_cache.file_units.push(new_file_unit);

        let updated_file = self.ast_cache.file_units.last().unwrap();
        let new_tree = Self::build_decl_tree(updated_file);

        println!("new tree: {:#?}", new_tree);

        self.file_decl_trees.insert(source_id, new_tree);
    }
}