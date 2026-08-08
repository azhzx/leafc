use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::{fs, process};
use std::process::id;
use rayon::prelude::*;
use salsa::{self, Database, Id, Storage, tracked};
use salsa::Setter;
use tracing::{span, Level};

use leaf_coreapi::ast::{CrateAst, FileRedUnit, GreenDecl};
use leaf_coreapi::crate_meta::{
    CrateManifest, Dependency, OperatorDef, OperatorKind, PriorityRelation,
};
pub use leaf_coreapi::diagnose::{
    DiagColorConfig, DiagCtx, DiagError, DiagCollector, LocalizedMessage, Localizer, MsgKind,
    TokenCache, TomlLocalizer, DEFAULT_EN_TOML,
};
use leaf_coreapi::diagnose::{ErrorKind, MiscErrorKind};
use leaf_coreapi::hir::HirCrate;
use leaf_coreapi::id::{CrateId, FileId};
use leaf_coreapi::mir::MirCrate;
use leaf_coreapi::scope::ResolvedNames;
use leaf_coreapi::source::{AbsPathSourceMap, SourcePool};
use leaf_coreapi::token::TokenStream;

use leaf_c_codegen::CCodeGen;
use leaf_coreapi::type_ctx::TypeCtx;
use leaf_hir_lower::HirLower;
use leaf_lexer::Lexer;
use leaf_mir_lower::MirLower;
use leaf_mir_mono::MirMono;
use salsa::plumbing::{FromIdWithDb, ZalsaDatabase};
//use leaf_parser::Parser;

#[salsa::input]
pub struct SourceFile {
    pub file_id: FileId,
    pub content: String,
}

#[salsa::input]
pub struct OperatorRegistry {
    pub id: u32,
    pub user_operators: Arc<HashMap<String, OperatorDef>>,
    pub user_op_info: Arc<HashMap<String, (usize, OperatorKind)>>,
}

#[salsa::input]
pub struct CrateGraph {
    pub id: u32,
    pub files_per_crate: HashMap<CrateId, Vec<FileId>>,
    pub dependencies: HashMap<CrateId, Vec<CrateId>>,
    pub root_crate: CrateId,
}

#[salsa::db]
pub struct LeafDatabase {
    storage: Storage<Self>,
    pub abs_path_map: AbsPathSourceMap,
    file_map: HashMap<FileId, SourceFile>,

    crate_graph_inst: Option<CrateGraph>,
    op_reg_inst: Option<OperatorRegistry>,
}

#[salsa::db]
impl Database for LeafDatabase {}

#[derive(Clone)]
struct DatabaseSnapshot {
    user_operators: Arc<HashMap<String, OperatorDef>>,
    user_op_info: Arc<HashMap<String, (usize, OperatorKind)>>,
    files_per_crate: HashMap<CrateId, Vec<FileId>>,
    dependencies: HashMap<CrateId, Vec<CrateId>>,
    root_crate: CrateId,
    source_entries: Vec<(FileId, String)>,
    abs_path_map: AbsPathSourceMap,
}

#[tracked]
impl LeafDatabase {
    pub fn snapshot(&self) -> DatabaseSnapshot {
        let op_reg = self.op_reg_inst.unwrap();
        let graph = self.crate_graph_inst.unwrap();
        let source_entries: Vec<_> = self.file_map
            .values()
            .map(|sf| (*sf.file_id(self), sf.content(self).clone()))
            .collect();
        DatabaseSnapshot {
            user_operators: op_reg.user_operators(self).clone(),
            user_op_info: op_reg.user_op_info(self).clone(),
            files_per_crate: graph.files_per_crate(self).clone(),
            dependencies: graph.dependencies(self).clone(),
            root_crate: *graph.root_crate(self),
            source_entries,
            abs_path_map: self.abs_path_map.clone(),
        }
    }

    pub fn source_text(&self, file_id: FileId) -> String {
        let source_file = self.file_map.get(&file_id).expect("FileId not registered in salsa db");
        source_file.content(self).clone()
    }

    pub fn token_stream(&self, file_id: FileId) -> (Arc<TokenStream>, Arc<Vec<DiagError>>) {
        let _guard = span!(Level::DEBUG, "token_stream", ?file_id).entered();
        todo!("lexer logic");
    }

    pub fn file_ast(&self, file_id: FileId) -> (Arc<FileRedUnit>, Arc<Vec<DiagError>>) {
        let _guard = span!(Level::DEBUG, "file_ast", ?file_id).entered();
        let (tokens, diags) = self.token_stream(file_id);
        todo!("parser logic");
    }

    pub fn crate_files(&self, crate_id: CrateId) -> Arc<Vec<FileId>> {
        let graph = self.crate_graph_inst.unwrap();
        let list = graph.files_per_crate(self).get(&crate_id).cloned().unwrap_or_default();
        Arc::new(list)
    }

    pub fn crate_ast(&self, crate_id: CrateId) -> (Arc<CrateAst>, Arc<Vec<DiagError>>) {
        let _guard = span!(Level::DEBUG, "crate_ast", ?crate_id).entered();
        let file_ids = self.crate_files(crate_id);
        let mut diag = DiagCollector::new();
        let _op_reg = self.op_reg_inst.unwrap();
        todo!("build crate ast");
    }

    pub fn resolve_names(&self, crate_id: CrateId) -> (Arc<ResolvedNames>, Arc<Vec<DiagError>>) {
        let _guard = span!(Level::DEBUG, "resolve_names", ?crate_id).entered();
        todo!("name resolve");
    }

    pub fn crate_hir(&self, crate_id: CrateId) -> (Arc<HirCrate>, Arc<Vec<DiagError>>) {
        let _guard = span!(Level::DEBUG, "crate_hir", ?crate_id).entered();
        let (ast, ast_diags) = self.crate_ast(crate_id);
        let (names, name_diags) = self.resolve_names(crate_id);
        let mut diag = DiagCollector::new();
        todo!("lower ast to hir");
    }

    pub fn type_check_result(&self, crate_id: CrateId) -> (Arc<(TypeCtx, HirCrate)>, Arc<Vec<DiagError>>) {
        let _guard = span!(Level::DEBUG, "type_check", ?crate_id).entered();
        let (hir, diags) = self.crate_hir(crate_id);
        todo!("type check");
    }

    pub fn crate_mir(&self, crate_id: CrateId) -> (Arc<MirCrate>, Arc<Vec<DiagError>>) {
        let _guard = span!(Level::DEBUG, "crate_mir", ?crate_id).entered();
        let (ty_result, diags) = self.type_check_result(crate_id);
        todo!("lower hir to mir");
    }

    pub fn mir_optimize(&self, crate_id: CrateId) -> (Arc<MirCrate>, Arc<Vec<DiagError>>) {
        let _guard = span!(Level::DEBUG, "mir_optimize", ?crate_id).entered();
        let (mir, diags) = self.crate_mir(crate_id);
        todo!("mir optimize");
    }

    pub fn codegen_c(&self, crate_id: CrateId) -> (Arc<String>, Arc<Vec<DiagError>>) {
        let _guard = span!(Level::DEBUG, "codegen_c", ?crate_id).entered();
        let (mir, diags) = self.mir_optimize(crate_id);
        todo!("c codegen");
    }

    pub fn update_file_content(&mut self, abs_path: &str, new_content: String) -> Result<(), &'static str> {
        let file_id = self.abs_path_map.get(abs_path).ok_or("file not found in abs_path_map")?;
        let source_file = self.file_map.get(file_id).copied().ok_or("FileId missing salsa mapping")?;
        SourceFile::set_content(source_file, self).to(new_content);
        Ok(())
    }

    pub fn root_crate_id(&self) -> CrateId {
        let graph = self.crate_graph_inst.unwrap();
        *graph.root_crate(self)
    }

    pub fn new(
        user_operators: Arc<HashMap<String, OperatorDef>>,
        user_op_info: Arc<HashMap<String, (usize, OperatorKind)>>,
        files_per_crate: HashMap<CrateId, Vec<FileId>>,
        dependencies: HashMap<CrateId, Vec<CrateId>>,
        root_crate: CrateId,
        source_entries: Vec<(FileId, String)>,
        abs_path_map: AbsPathSourceMap,
    ) -> Self {
        let mut db = Self {
            storage: Storage::default(),
            abs_path_map,
            file_map: HashMap::new(),
            crate_graph_inst: None,
            op_reg_inst: None,
        };

        let op_reg_inst = OperatorRegistry::new(&mut db, 0u32, user_operators, user_op_info);
        let crate_graph_inst = CrateGraph::new(&mut db, 1u32, files_per_crate, dependencies, root_crate);

        db.op_reg_inst = Some(op_reg_inst);
        db.crate_graph_inst = Some(crate_graph_inst);

        for (fid, content) in source_entries {
            let sf = SourceFile::new(&mut db, fid, content);
            db.file_map.insert(fid, sf);
        }

        db
    }
}

#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub crate_path: PathBuf,
    pub manifest: CrateManifest,
    pub user_op_info: HashMap<String, (usize, OperatorKind)>,
}

impl BuildConfig {
    pub fn from_dir(crate_path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let manifest = CrateManifest::from_file(crate_path.join("LeafCrate.toml"))?;
        let mut user_op_info = HashMap::new();

        for def in manifest.operator.values() {
            let base_prio = 100;
            user_op_info.insert(def.text.clone(), (base_prio, def.kind));
        }

        Ok(Self {
            crate_path,
            manifest,
            user_op_info,
        })
    }
}

pub struct MainDispatcher {
    db: Arc<RwLock<LeafDatabase>>,
    localizer: Box<dyn Localizer>,
    colors: DiagColorConfig,
    crate_path: PathBuf,
}

impl MainDispatcher {
    pub fn new(crate_root: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let config = BuildConfig::from_dir(crate_root.clone())?;
        let (graph_data, source_entries, abs_map) = Self::build_crate_graph(&config, &crate_root)?;

        let db = LeafDatabase::new(
            Arc::new(config.manifest.operator.clone()),
            Arc::new(config.user_op_info.clone()),
            graph_data.files_per_crate,
            graph_data.dependencies,
            graph_data.root_crate,
            source_entries,
            abs_map,
        );

        Ok(Self {
            db: Arc::new(RwLock::new(db)),
            localizer: Box::new(TomlLocalizer::new(DEFAULT_EN_TOML, "")?),
            colors: DiagColorConfig::default(),
            crate_path: crate_root,
        })
    }

    fn build_crate_graph(
        root_config: &BuildConfig,
        root_path: &Path,
    ) -> Result<(CrateGraphData, Vec<(FileId, String)>, AbsPathSourceMap), Box<dyn std::error::Error>> {
        let mut abs_map = AbsPathSourceMap::new();
        let mut source_entries = Vec::new();
        let mut files_per_crate = HashMap::new();
        let mut dependencies: HashMap<CrateId, Vec<CrateId>> = HashMap::new();

        let mut queue = vec![(root_path.to_path_buf(), None)];
        let mut crate_counter = 0;
        let mut crate_path_to_id = HashMap::new();

        while let Some((crate_dir, parent_cid)) = queue.pop() {
            let manifest = if &crate_dir == root_path {
                root_config.manifest.clone()
            } else {
                CrateManifest::from_file(crate_dir.join("leafrate.toml"))?
            };

            let crate_id = CrateId(crate_counter);
            crate_counter += 1;
            crate_path_to_id.insert(crate_dir.clone(), crate_id);

            let mut file_ids = Vec::new();
            Self::collect_source_files(&crate_dir, &mut abs_map, &mut source_entries, &mut file_ids)?;
            files_per_crate.insert(crate_id, file_ids);

            if let Some(parent) = parent_cid {
                dependencies.entry(parent).or_default().push(crate_id);
            }

            for (_dep_name, dep) in &manifest.dependencies {
                if let Some(dep_path) = dep.path() {
                    let full_dep = crate_dir.join(dep_path).canonicalize()?;
                    if !crate_path_to_id.contains_key(&full_dep) {
                        queue.push((full_dep, Some(crate_id)));
                    }
                }
            }
        }

        let root_cid = crate_path_to_id[root_path];
        Ok((
            CrateGraphData {
                files_per_crate,
                dependencies,
                root_crate: root_cid,
            },
            source_entries,
            abs_map,
        ))
    }

    fn collect_source_files(
        dir: &Path,
        abs_map: &mut AbsPathSourceMap,
        source_entries: &mut Vec<(FileId, String)>,
        file_ids: &mut Vec<FileId>,
    ) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                Self::collect_source_files(&path, abs_map, source_entries, file_ids)?;
            } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("leaf") {
                let abs_path = path.canonicalize()?.to_string_lossy().to_string();
                let content = fs::read_to_string(&path)?;
                let fid = FileId(abs_map.len());
                abs_map.insert(abs_path.clone(), fid);
                file_ids.push(fid);
                source_entries.push((fid, content));
            }
        }
        Ok(())
    }

    pub fn check(&self) -> Result<(), Vec<DiagError>> {
        let db = self.db.read().unwrap();
        let root = db.root_crate_id();
        let (_, diags) = db.type_check_result(root);
        if diags.is_empty() {
            Ok(())
        } else {
            Err(diags.as_ref().clone())
        }
    }

    pub fn build(&self) -> Result<PathBuf, Vec<DiagError>> {
        let db = self.db.read().unwrap();
        let root = db.root_crate_id();
        let (c_code, diags) = db.codegen_c(root);
        if !diags.is_empty() {
            return Err(diags.as_ref().clone());
        }

        let out_path = self.crate_path.join("build/out.c");
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                vec![DiagError::new_without_span(
                    ErrorKind::MiscError(MiscErrorKind::Io),
                    LocalizedMessage::new(MsgKind::MiscIo, [e.to_string()]),
                )]
            })?;
        }
        fs::write(&out_path, c_code.as_str()).map_err(|e| {
            vec![DiagError::new_without_span(
                ErrorKind::MiscError(MiscErrorKind::Io),
                LocalizedMessage::new(MsgKind::MiscIo, [e.to_string()]),
            )]
        })?;
        Ok(out_path)
    }

    pub fn format(&self) -> Result<(), Vec<DiagError>> {
        let db = self.db.read().unwrap();
        let root = db.root_crate_id();
        let (_, diags) = db.crate_ast(root);
        if diags.is_empty() {
            Ok(())
        } else {
            Err(diags.as_ref().clone())
        }
    }

    pub fn run_lsp(self) -> Result<(), Box<dyn std::error::Error>> {
        unimplemented!("LSP server not implemented")
    }

    pub fn edit_file(&mut self, abs_path: &str, new_content: String) -> Result<(), &'static str> {
        let mut db = self.db.write().unwrap();
        db.update_file_content(abs_path, new_content)
    }

    pub fn compile_all(&self) -> Result<HashMap<CrateId, String>, Vec<DiagError>> {
        let snapshot = {
            let db = self.db.read().unwrap();
            db.snapshot()
        };

        let all_crates: Vec<CrateId> = snapshot.files_per_crate.keys().cloned().collect();

        let results: Vec<_> = all_crates
            .par_iter()
            .map(move |&cid| {
                let mut local_db = LeafDatabase::new(
                    snapshot.user_operators.clone(),
                    snapshot.user_op_info.clone(),
                    snapshot.files_per_crate.clone(),
                    snapshot.dependencies.clone(),
                    snapshot.root_crate,
                    snapshot.source_entries.clone(),
                    snapshot.abs_path_map.clone(),
                );
                let (code, diags) = local_db.codegen_c(cid);
                if diags.is_empty() {
                    Ok((cid, code.to_string()))
                } else {
                    Err(diags.as_ref().clone())
                }
            })
            .collect();

        let mut map = HashMap::new();
        let mut all_errors = Vec::new();
        for res in results {
            match res {
                Ok((cid, src)) => {
                    map.insert(cid, src);
                }
                Err(errs) => all_errors.extend(errs),
            }
        }

        if all_errors.is_empty() {
            Ok(map)
        } else {
            Err(all_errors)
        }
    }

    pub fn set_crate_root(&mut self, dir: &str) -> Result<(), Box<dyn std::error::Error>> {
        let abs = fs::canonicalize(dir)?;
        *self = Self::new(abs)?;
        Ok(())
    }

    fn write_c_file(&self, src: &str) -> std::io::Result<PathBuf> {
        let out = self.crate_path.join("build/out.c");
        if let Some(p) = out.parent() {
            fs::create_dir_all(p)?;
        }
        fs::write(&out, src)?;
        Ok(out)
    }

    pub fn run_with_gcc(&self) -> std::io::Result<()> {
        println!("start compile & run via gcc");
        let build_dir = self.crate_path.join("build");
        let c_file = build_dir.join("out.c");
        if !c_file.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Missing C file: {}", c_file.display()),
            ));
        }

        let exe_name = if cfg!(windows) { "out.exe" } else { "out" };
        let exe_path = build_dir.join(exe_name);

        let gcc_status = process::Command::new("gcc")
            .arg(&c_file)
            .arg("-o")
            .arg(&exe_path)
            .arg("-std=c11")
            .status()?;

        if !gcc_status.success() {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("gcc compile failed")));
        }

        println!("execute binary");
        let _run = process::Command::new(&exe_path).status()?;
        Ok(())
    }
}

#[derive(Debug)]
struct CrateGraphData {
    files_per_crate: HashMap<CrateId, Vec<FileId>>,
    dependencies: HashMap<CrateId, Vec<CrateId>>,
    root_crate: CrateId,
}

fn build_decl_tree(file_unit: &FileRedUnit) -> intervaltree::IntervalTree<usize, Arc<GreenDecl>> {
    let elements: Vec<_> = file_unit
        .green
        .top_decls
        .iter()
        .map(|child| {
            let start = file_unit.span.start_off + child.relative_start;
            let end = start + child.node.text_len.0;
            intervaltree::Element::from((start..end, Arc::clone(&child.node)))
        })
        .collect();
    intervaltree::IntervalTree::from_iter(elements)
}

pub fn build_file_decl_maps(ast: &CrateAst) -> HashMap<FileId, intervaltree::IntervalTree<usize, Arc<GreenDecl>>> {
    todo!()
}