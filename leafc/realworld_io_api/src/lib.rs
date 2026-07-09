
pub trait RealWorldIO {
    fn println(text: String);
    fn print(text: String);
    fn read_file(path: String);
    fn write_file(path: String, text: String);
}