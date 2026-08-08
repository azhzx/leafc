use leaf_compiler::MainDispatcher;

fn main() {
    let crate_path: std::path::PathBuf = "demo_leaf_crate".parse().unwrap();
    let mut compiler = MainDispatcher::new(crate_path.clone())
        .unwrap();
}