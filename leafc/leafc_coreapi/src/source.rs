use std::collections::HashMap;

pub type SourceId = usize;

pub type AbsPathSourceMap = HashMap<String, SourceId>;

#[derive(Debug, Clone)]
pub struct Source {
    pub file_abs_path: String,
    pub file_content: String,
    pub line_starts: Vec<usize>,
    pub source_len: usize,
}


#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    pub source_id: SourceId,
    pub start_off: usize,
    pub end_off: usize,
}

impl Span {
    pub fn len(&self) -> usize {
        self.end_off - self.start_off
    }
}

pub struct SourcePool(pub Vec<Source>);

impl SourcePool {
    pub fn add_source(&mut self, file_abs_path: String, text: String) -> SourceId {
        let source_len = text.len();
        let mut line_starts = vec![0usize];
        for (i, c) in text.char_indices() {
            if c == '\n' {
                line_starts.push(i + c.len_utf8());
            }
        }

        self.0.push(Source {
            file_abs_path,
            line_starts,
            source_len,
            file_content: text,
        });

        (self.0.len() - 1) as SourceId
    }

    pub fn find_source(&self, file_abs_path: String) -> Option<SourceId> {
        self.0.iter().position(|s|
            s.file_abs_path.as_str() == file_abs_path.as_str()
        )
    }

    pub fn update_source(&mut self, id: SourceId, new_content: String) {
        self.0[id].file_content = new_content;
    }
}