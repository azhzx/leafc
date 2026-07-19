use leafc_coreapi::compiler::{CompilerApi, IncrementalCompiler};
use leafc_compiler::NativeCompiler;

fn main() {
    let mut compile_result = None;
    NativeCompiler::new()
        .set_crate_path("demo_leaf_crate")
        .expect("fail to set crate path")
        .compile(&mut compile_result)
        // .edit_append(
        //     r"D:\leafc\demo_leaf_crate\main.leaf".to_string(),
        //     "100",
        //     24
        // )
        ;
}

