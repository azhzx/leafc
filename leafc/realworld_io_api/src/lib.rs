use std::path::PathBuf;

pub trait RealWorldIOApi {
    fn println(text: &String);
    fn print(text: &String);
    fn read_file(path: &PathBuf) -> std::io::Result<String>;
}