pub type SourceId = usize;

#[derive(Debug, Clone)]
pub struct Source {
    pub file_abs_path: String,
    pub file_lines: Vec<String>,
    pub line_starts: Vec<usize>,
    pub source_len: usize,
}



#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    pub start_off: usize,
    pub end_off: usize,
}

pub struct SourcePool(pub Vec<Source>);

impl SourcePool {
    pub fn add_source(&mut self, file_abs_path: String, text: String) -> SourceId {
        let file_lines = {
            let mut lines: Vec<String> = text.lines().map(String::from).collect();
            lines.push(String::new());
            lines
        };

        let source_len = text.len();

        let mut line_starts = Vec::with_capacity(file_lines.len());
        line_starts.push(0);
        let mut pos = 0;
        let bytes = text.as_bytes();


        for line in &file_lines[..file_lines.len() - 1] {
            pos += line.len();

            if pos < bytes.len() && bytes[pos] == b'\r' {
                pos += 1;
            }
            if pos < bytes.len() && bytes[pos] == b'\n' {
                pos += 1;
            }
            line_starts.push(pos);
        }


        line_starts.push(source_len);

        self.0.push(Source {
            file_abs_path,
            file_lines,
            line_starts,
            source_len,
        });

        (self.0.len() - 1) as SourceId
    }
}