use leafc_coreapi::compiler::{CompilerApi, IncrementalCompiler};
use leafc_compiler::NativeCompiler;

fn main() {
    let mut compile_result = None;
    let mut compiler = NativeCompiler::new();
    compiler
        .set_crate_path("demo_leaf_crate")
        .expect("fail to set crate path")
        .compile(&mut compile_result);

    if compile_result.is_none() {
        return;
    }
    compiler
        .write_to_path(compile_result.unwrap())
        .unwrap();

    compiler.run_by_gcc()
        .unwrap();
}

