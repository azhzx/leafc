use leafc_coreapi::compiler::CompilerApi;
use leafc_compiler::NativeCompiler;

fn main() {
    NativeCompiler::new()
        .set_crate_path("demo_leaf_crate")
        .expect("fail to set crate path")
        .compile();
}

