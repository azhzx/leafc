use std::fmt::Write;
use leafc_coreapi::diagnostic::{DiagTextColor, DiagMsg, DiagnosticianApi};
use leafc_coreapi::source::{Source, SourceId, SourcePool, Span};

pub struct Diagnostician<'a> {
    source_pool: &'a SourcePool,
    colors: DiagTextColor,
}

impl<'a> Diagnostician<'a> {
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

        let source_id = diag.span.source_id;
        let source = &self.source_pool.0[source_id];
        let source_name = &source.file_abs_path;

        let line_starts = &source.line_starts;

        // 二分定位行号（1‑based）
        let start_line = match line_starts.binary_search(&diag.span.start_off) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        };
        let end_line = match line_starts.binary_search(&diag.span.end_off.saturating_sub(1)) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        };

        let line_start_off = line_starts[start_line - 1];
        let start_col = source.file_content[line_start_off..diag.span.start_off]
            .chars()
            .count();
        let line_start_off_end = line_starts[end_line - 1];
        let end_col = source.file_content[line_start_off_end..diag.span.end_off]
            .chars()
            .count();

        writeln!(
            &mut out,
            "{}  -->    {}{}",
            self.colors.diag_source_name, source_name, self.colors.diag_reset
        )
            .unwrap();

        let first = if start_line > 1 {
            start_line - 1
        } else {
            start_line
        };

        let last = if end_line < line_starts.len() {
            end_line + 1
        } else {
            end_line
        };

        let lines: Vec<&str> = (first..=last)
            .map(|lineno| {
                let line_idx = lineno - 1;
                let start = line_starts[line_idx];
                let end = if line_idx + 1 < line_starts.len() {
                    line_starts[line_idx + 1]
                } else {
                    source.source_len
                };
                source.file_content[start..end].trim_end_matches(|c| c == '\n' || c == '\r')
            })
            .collect();

        let indicator_prefix = format!(
            "{}{}{}",
            self.colors.diag_bar, "  ╭─➜  |", self.colors.diag_reset
        );
        let indicator_len = indicator_prefix.chars().count();

        for lineno in first..=last {
            let line = &lines[lineno - first];

            let prefix = if lineno < start_line {
                format!(
                    "{}  {:>4} | {}",
                    self.colors.diag_bar, lineno, self.colors.diag_reset
                )
            } else if lineno == start_line {
                format!(
                    "{}  {:>4} | {}",
                    self.colors.diag_bar, lineno, self.colors.diag_reset
                )
            } else {
                format!(
                    "{}  |  {} | {}",
                    self.colors.diag_bar, lineno, self.colors.diag_reset
                )
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