use std::fmt::Write;
use leafc_coreapi::diagnostic::{DiagTextColor, DiagMsg, DiagnosticianApi};
use leafc_coreapi::source::{Source, SourceId, SourcePool, Span};

pub struct Diagnostician<'a> {
    source_pool: &'a SourcePool,
    colors: DiagTextColor,
}

impl<'a> Diagnostician<'a> {
    fn offset_to_line_col(offset: usize, line_starts: &[usize]) -> (usize, usize) {
        let line = line_starts.partition_point(|&x| x <= offset) - 1;
        let col = offset - line_starts[line];
        (line, col)
    }
}

impl<'a> DiagnosticianApi<'a> for Diagnostician<'a> {
    fn new(source_pool: &'a SourcePool, colors: DiagTextColor) -> Self {
        Diagnostician { source_pool, colors }
    }

    fn reset_colors(&mut self, new_colors: DiagTextColor) {
        self.colors = new_colors;
    }

    fn report(&self, diag: DiagMsg) -> String {
        let mut out = String::new();

        let source_id = diag.source;
        let source = &self.source_pool.0[source_id];
        let source_name = &source.file_abs_path;
        let lines = &source.file_lines;
        let line_starts = &source.line_starts;


        let (start_line0, start_col) = Self::offset_to_line_col(diag.span.start_off, line_starts);
        let (end_line0, end_col) = Self::offset_to_line_col(diag.span.end_off, line_starts);
        let start_line = start_line0 + 1; // 转为 1‑based
        let end_line = end_line0 + 1;

        writeln!(&mut out, "{}  -->    {}{}",
            self.colors.diag_source_name, source_name, self.colors.diag_reset
        ).unwrap();

        let first = if start_line > 1 {
            start_line - 1
        } else {
            start_line
        };


        let last = if end_line < lines.len() {
            end_line + 1
        } else {
            end_line
        };

        let indicator_prefix = format!(
            "{}{}{}", self.colors.diag_bar, "  ╭─➜  |", self.colors.diag_reset
        );
        let indicator_len = indicator_prefix.chars().count();

        for lineno in first..=last {
            let line_idx = lineno - 1;
            let line = &lines[line_idx];

            let prefix = if lineno < start_line {
                format!("{}  {:>4} | {}", self.colors.diag_bar, lineno, self.colors.diag_reset)
            } else if lineno == start_line {
                format!(
                    "{}  {:>4} | {}", self.colors.diag_bar, lineno, self.colors.diag_reset)
            } else {
                format!("{}  |  {} | {}", self.colors.diag_bar, lineno, self.colors.diag_reset)
            };

            writeln!(&mut out, "{}{}", prefix, line).unwrap();

            if lineno == start_line {
                let fill = (prefix.len() + start_col).saturating_sub(indicator_len + 1);

                let mut indicator =
                    String::with_capacity(indicator_len + fill + (end_col - start_col).max(1) + 10);
                indicator.push_str(&indicator_prefix);
                for _ in 0..fill {
                    indicator.push(' ');
                }

                let caret_len = if end_col > start_col {
                    end_col - start_col
                } else {
                    1
                };
                write!(
                    &mut indicator,
                    "{}{}{}",
                    self.colors.diag_title,
                    "^".repeat(caret_len),
                    self.colors.diag_reset
                )
                    .unwrap();
                writeln!(&mut out, "{}", indicator).unwrap();
            }
        }

        writeln!(&mut out, "{}  |{}", self.colors.diag_bar, self.colors.diag_reset).unwrap();
        writeln!(
            &mut out,
            "  {}{}: {}{}{}",
            self.colors.diag_title,
            diag.title,
            self.colors.diag_message,
            diag.msg,
            self.colors.diag_reset
        )
            .unwrap();

        out
    }
}