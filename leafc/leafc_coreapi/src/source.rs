pub type SourceId = usize;

#[derive(Debug, Clone)]
pub struct Source {
    pub file_name: String,
    pub file_lines: Vec<String>
}

pub type SourcePool = Vec<Source>;

#[derive(Debug, Clone)]
pub struct Pos {
    pub column: usize,
    pub lineno: usize,
}
#[derive(Debug, Clone)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}