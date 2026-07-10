pub type SourceId = usize;

#[derive(Debug, Clone)]
pub struct Source {
    pub file_name: String,
    pub file_lines: Vec<String>,
    pub line_starts: Vec<usize>,
    pub source_len: usize,
}

pub type SourcePool = Vec<Source>;


#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    pub start_off: usize,
    pub end_off: usize,
}