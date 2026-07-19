use crate::source::SourceId;

pub struct CompilerConfig {

}

pub trait CompilerApi {
    type Output;

    fn new() -> Self;
    fn get_version() -> &'static str;
    
    fn set_crate_path(&mut self, dir_path: &str) -> Option<&mut Self>;
    fn compile(&mut self, out: &mut Option<Self::Output>) -> &mut Self;
}

pub trait IncrementalCompiler {
    fn edit_append(&mut self, abs_path: String, line: &str, start_offset: usize) -> &mut Self;
    fn edit_remove(&mut self, abs_path: String, start_offset: usize) -> &mut Self;
}