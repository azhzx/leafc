use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::{fs, process};

use intervaltree::{Element, IntervalTree};
use serde::Serialize;

use leafc_coreapi::ast::{CrateAst, FileRedUnit, GreenDecl};
use leafc_coreapi::codegen::CodegenApi;
use leafc_coreapi::compiler::{CompilerApi, IncrementalCompiler};
use leafc_coreapi::crate_meta::{
    CrateManifest, OperatorDef, OperatorKind, PriorityRelation,
};
pub use leafc_coreapi::error_items::{
    DiagCtx, DiagColorConfig, DiagError, ErrorKind, LocalizedMessage, MsgKind, SourceMap,
    TokenCache, Localizer, TomlLocalizer, DEFAULT_EN_TOML,
};
use leafc_coreapi::hir::HirCrate;
use leafc_coreapi::hir_lower::HirLowerApi;
use leafc_coreapi::lexer::{LexerApi, TokenStream};
use leafc_coreapi::mir::MirCrate;
use leafc_coreapi::mir_consteval::MirConstEvalApi;
use leafc_coreapi::mir_lifetime_checker::MirLifetimeCheckerApi;
use leafc_coreapi::mir_lower::MirLowerApi;
use leafc_coreapi::mir_mono::MirMonoApi;
use leafc_coreapi::name_pass::{NamePassApi, NamePassResult};
use leafc_coreapi::parser::ParserApi;
use leafc_coreapi::source::{AbsPathSourceMap, SourceId, SourcePool, Span};
use leafc_coreapi::type_checker::TypeCheckerApi;
use leafc_coreapi::type_system::TypeCtx;
use leafc_c_codegen::CCodeGen;
use leafc_hirlower::HirLower;
use leafc_lexer::Lexer;
use leafc_mir_consteval::MirConstEval;
use leafc_mir_lifetime_checker::MirLifetimeChecker;
use leafc_mirlower::MirLower;
use leafc_mirmono::MirMono;
use leafc_namepass::NamePass;
use leafc_parser::Parser;
use leafc_typechecker::TypeChecker;
use realworld_io_api::RealWorldIOApi;

const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
struct ExportedSignature {
    name: String,
    params: Vec<String>,
    return_type: String,
}

pub struct RealWorld;
impl RealWorldIOApi for RealWorld {
    fn println(text: &String) {
        println!("{}", text);
    }
    fn print(text: &String) {
        print!("{}", text);
    }
    fn read_file(path: &PathBuf) -> std::io::Result<String> {
        fs::read_to_string(path)
    }
}


pub struct Session {
    pub crate_path: PathBuf,
    pub diag: DiagCtx,
    pub abs_path_source_map: AbsPathSourceMap,
    pub ast_cache: CrateAst,
    pub file_decl_trees: HashMap<SourceId, IntervalTree<usize, Arc<GreenDecl>>>,

    /// operator
    pub manifest_operators: HashMap<String, OperatorDef>,
    pub user_op_info: HashMap<String, (usize, OperatorKind)>,
}

impl Session {
    pub fn new(crate_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let leaf_toml_path = crate_path.join("LeafCrate.toml");
        let content = fs::read_to_string(&leaf_toml_path)?;
        let manifest = CrateManifest::from_str(&content)?;
        let manifest_operators = manifest.operator;

        let mut user_op_info = HashMap::new();
        for def in manifest_operators.values() {
            let base_prio = match def.priority_relation() {
                PriorityRelation::HigherThan(op) => {
                    Parser::builtin_priority(op) + Parser::PRIORITY_OFFSET
                }
                PriorityRelation::LowerThan(op) => {
                    Parser::builtin_priority(op) - Parser::PRIORITY_OFFSET
                }
            };
            user_op_info.insert(def.text.clone(), (base_prio, def.kind));
        }

        let mut pool = SourcePool(Vec::new());
        let mut abs_map: AbsPathSourceMap = HashMap::new();
        Self::collect_sources(&crate_path, &mut pool, &mut abs_map)?;

        let source_map = SourceMap::new(pool);

        let localizer = TomlLocalizer::new(DEFAULT_EN_TOML, DEFAULT_EN_TOML)
            .map_err(|e| format!("Failed to initialize localizer: {}", e))?;

        let diag = DiagCtx::new(
            source_map,
            Box::new(localizer),
            DiagColorConfig::default(),
        );

        Ok(Self {
            crate_path,
            diag,
            abs_path_source_map: abs_map,
            ast_cache: CrateAst {
                external_requires: vec![],
                file_units: vec![],
            },
            file_decl_trees: HashMap::new(),
            manifest_operators,
            user_op_info,
        })
    }

    fn collect_sources(
        dir: &PathBuf,
        pool: &mut SourcePool,
        abs_map: &mut AbsPathSourceMap,
    ) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                Self::collect_sources(&path, pool, abs_map)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("leaf") {
                let abs_path = fs::canonicalize(&path)?.to_string_lossy().to_string();
                let content = fs::read_to_string(&path)?;
                let source_id = pool.add_source(abs_path.clone(), content);
                abs_map.insert(abs_path, source_id);
            }
        }
        Ok(())
    }

    fn build_decl_tree(file_unit: &FileRedUnit) -> IntervalTree<usize, Arc<GreenDecl>> {
        let elements: Vec<Element<usize, Arc<GreenDecl>>> = file_unit
            .green
            .top_decls
            .iter()
            .map(|child| {
                let start = file_unit.span.start_off + child.relative_start;
                let end = start + child.node.text_len;
                Element::from((start..end, Arc::clone(&child.node)))
            })
            .collect();
        IntervalTree::from_iter(elements)
    }

    pub fn build_decl_trees(&mut self, ast: &CrateAst) {
        self.file_decl_trees.clear();
        for file_unit in &ast.file_units {
            let tree = Self::build_decl_tree(file_unit);
            self.file_decl_trees.insert(file_unit.span.source_id, tree);
        }
    }

    pub fn source_map(&self) -> &SourceMap {
        &self.diag.source_map
    }
}


pub struct NativeCompiler {
    pub session: Session,
}

impl NativeCompiler {
    pub fn new(crate_root: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let session = Session::new(crate_root)?;
        Ok(Self { session })
    }

    pub fn set_crate_path(&mut self, dir_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let abs_path = fs::canonicalize(dir_path)?;
        self.session = Session::new(abs_path)?;
        Ok(())
    }

    pub fn compile(&mut self) -> Result<String, Vec<DiagError>> {
        let session = &mut self.session;


        // Lexer
        let mut token_streams: Vec<TokenStream> = Vec::new();

        let files: Vec<(usize, String)> = {
            let pool = session.source_map().pool();
            (0..pool.0.len())
                .map(|id| (id, pool.0[id].file_content.clone()))
                .collect()
        };

        for (source_id, content) in files {
            let mut lexer = Lexer::new(source_id, &content, &session.manifest_operators);
            let ts = lexer.tokenize(&mut session.diag);
            token_streams.push(ts);
        }

        if session.diag.has_errors() {
            return Err(session.diag.collector.errors.clone());
        }


        /*
        // TODO: 待 Parser 适配新的 DiagCtx 后启用
        let parser = Parser::new(
            session.crate_path.clone(),
            &session.source_map().pool(),
            &session.abs_path_source_map,
            &session.manifest_operators,
            &session.user_op_info,
        );
        let ast = parser.parse().map_err(|old_diag| {
            // 转换旧的 DiagMsg 到新的 DiagError
            vec![]
        })?;

        println!("=== ast ===");
        println!("{:#?}", ast);
        println!("=== === ===");
        */
        let ast: CrateAst = todo!("Parser not yet adapted to new diagnostics");

        // 为每个文件构建声明区间树
        session.build_decl_trees(&ast);

        /*
        let mut name_pass = NamePass::new(&ast);
        let name_pass_result = name_pass.pass().map_err(|old_diag| {
            vec![]
        })?;
        println!("=== scope tree ===");
        println!("{:#?}", name_pass_result.pool);
        println!("=== === ===");
        */
        let name_pass_result: NamePassResult = todo!("NamePass not yet adapted");


        /*
        let crate_name = session.crate_path.file_stem().unwrap().to_str().unwrap().to_string();
        let hir_lower = HirLower::new(&ast, name_pass_result, crate_name);
        let hir = hir_lower.lower().map_err(|old_diag| vec![])?;
        println!("=== hir ===");
        println!("{:#?}", hir);
        println!("=== === ===");
        */
        let hir: HirCrate = todo!("HirLower not yet adapted");


        /*
        let type_checker = TypeChecker::new(hir);
        let (ty_map, hir) = type_checker.check().map_err(|old_diag| vec![])?;
        println!("=== type map ===");
        println!("{:#?}", ty_map.decl_type_map);
        println!("=== === ===");
        */
        let (ty_map, hir): (TypeCtx, HirCrate) = todo!("TypeChecker not yet adapted");


        /*
        let mir_lower = MirLower::new(ty_map, hir);
        let (mir, ty_map) = mir_lower.lower().map_err(|old_diag| vec![])?;
        println!("=== mir ===");
        println!("{:#?}", mir);
        println!("=== === ===");
        */
        let (mir, ty_map): (MirCrate, TypeCtx) = todo!("MirLower not yet adapted");

        // 导出公共函数签名
        if !mir.pub_decl_ids.is_empty() {
            let mut exports = Vec::new();
            for &fun_id in &mir.pub_decl_ids {
                if let Some(fun) = mir.functions.get(fun_id) {
                    exports.push(ExportedSignature {
                        name: fun.name.clone(),
                        params: fun
                            .signature
                            .params
                            .iter()
                            .map(|ty| format!("{:?}", ty_map.type_pool[*ty].kind))
                            .collect(),
                        return_type: format!(
                            "{:?}",
                            ty_map.type_pool[fun.signature.return_ty].kind
                        ),
                    });
                }
            }
            let meta_path = session.crate_path.join("build").join("crate_exports.json");
            let json = serde_json::to_string_pretty(&exports).expect("serialize failed");
            fs::write(meta_path, json).expect("write failed");
        }


        /*
        let mir_consteval = MirConstEval::new(mir, ty_map);
        let (mir, ty_map) = mir_consteval.eval().map_err(|old_diag| vec![])?;
        */
        let (mir, ty_map) = (mir, ty_map);
        let _: (MirCrate, TypeCtx) = todo!("MirConstEval not yet adapted");

        /*
        let lifetime_checker = MirLifetimeChecker::new(mir, ty_map);
        let (mir, ty_map) = lifetime_checker.check().map_err(|old_diag| vec![])?;
        */
        let _: (MirCrate, TypeCtx) = todo!("MirLifetimeChecker not yet adapted");


        /*
        let mir_mono = MirMono::new(mir, ty_map);
        let (mono_mir, ty_map) = mir_mono.mono().map_err(|old_diag| vec![])?;
        println!("=== mono mir ===");
        println!("{:#?}", mono_mir);
        println!("=== === ===");
        */
        let (mono_mir, ty_map): (MirCrate, TypeCtx) = todo!("MirMono not yet adapted");


        /*
        let codegen = CCodeGen::new(mono_mir, ty_map);
        let c_code = codegen.emit().map_err(|old_diag| vec![])?;
        */
        let c_code: String = todo!("Codegen not yet adapted");

        Ok(c_code)
    }


    pub fn write_to_path(&self, src: &str) -> std::io::Result<PathBuf> {
        let out_path = self.session.crate_path.join("build/out.c");
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&out_path, src)?;
        Ok(out_path)
    }

    pub fn run_by_gcc(&self) -> std::io::Result<()> {
        println!("start to call gcc");
        let build_dir = self.session.crate_path.join("build");
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
        let exe_name = if cfg!(target_os = "windows") { "out.exe" } else { "out" };
        let exe_file = build_dir.join(exe_name);
        let exe_file_str = clean_path(&exe_file);

        let output = process::Command::new("gcc")
            .arg(&c_file_str)
            .arg("-o")
            .arg(&exe_file_str)
            .arg("-std=c11")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("gcc failed:\n{}", stderr),
            ));
        }

        println!("start to run");
        let mut child = process::Command::new(&exe_file).spawn()?;
        let status = child.wait()?;
        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("executable exited with status {}", status),
            ));
        }
        Ok(())
    }
}


impl IncrementalCompiler for NativeCompiler {
    fn edit_append(
        &mut self,
        _abs_path: String,
        _line: &str,
        _start_offset: usize,
    ) -> &mut Self {
        todo!("Incremental compilation not yet supported with new diagnostics");
    }

    fn edit_remove(&mut self, _abs_path: String, _start_offset: usize) -> &mut Self {
        todo!("Incremental compilation not yet supported with new diagnostics");
    }
}