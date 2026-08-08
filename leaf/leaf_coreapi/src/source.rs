use crate::id::FileId;
use std::collections::HashMap;

pub type AbsPathSourceMap = HashMap<String, FileId>;

#[derive(Debug, Clone)]
pub struct Source {
    pub file_abs_path: String,
    pub file_content: String,
    pub line_starts: Vec<usize>,
    pub source_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    pub source_id: FileId,
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
    pub fn add_source(&mut self, file_abs_path: String, text: String) -> FileId {
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

        FileId(self.0.len() - 1)
    }

    pub fn find_source(&self, file_abs_path: String) -> Option<FileId> {
        Option::from(FileId(
            self.0
                .iter()
                .position(|s| s.file_abs_path.as_str() == file_abs_path.as_str())?,
        ))
    }

    pub fn update_source(&mut self, id: FileId, new_content: String) {
        self.0[id.0].file_content = new_content;
    }

    pub fn get_line_info(
        &self,
        source_id: FileId,
        offset: usize,
    ) -> Option<(usize, String, usize)> {
        let source = self.0.get(source_id.0)?;
        let line_starts = &source.line_starts;
        let line_idx = line_starts.binary_search(&offset).unwrap_or_else(|e| e - 1);
        let line_start = line_starts[line_idx];
        let line_end = line_starts
            .get(line_idx + 1)
            .copied()
            .unwrap_or(source.source_len);
        let line_content = source.file_content[line_start..line_end].to_string();
        let col = offset - line_start;
        Some((line_idx + 1, line_content, col))
    }
}
