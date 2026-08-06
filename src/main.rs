use leafc_compiler::NativeCompiler;

fn main() {
    let crate_path: std::path::PathBuf = "demo_leaf_crate".parse().unwrap();
    let mut compiler = NativeCompiler::new(crate_path.clone()).unwrap();

    compiler
        .set_crate_path("demo_leaf_crate")
        .expect("fail to set crate path");

    let c_code = match compiler.compile() {
        Ok(code) => code,
        Err(errors) => {
            let diag_ctx = &compiler.session.diag;
            let report = diag_ctx.emit_all();
            eprintln!("{}", report);
            eprintln!("Compilation aborted due to above errors");
            return;
        }
    };

    compiler
        .write_to_path(&c_code)
        .expect("fail to write output");

    match compiler.run_by_gcc() {
        Ok(_) => (),
        Err(e) => {
            let text = format!("{}", e);
            for line in text.lines() {
                eprintln!("{}", line);
            }
        }
    }
}