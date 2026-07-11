use leafc_coreapi::compiler::CompilerApi;
use leafc_compiler::NativeCompiler;

fn main() {
    NativeCompiler::new().compile_a_crate("demo_leaf_module");
}

