use crate::source::SourceId;

pub struct CompilerConfig {

}

pub trait CompilerApi {
    type Output;

    fn new() -> Self;
    fn get_version() -> &'static str;
    fn compile_a_crate(&mut self, dir_path: &str) -> Option<Self::Output>;
}