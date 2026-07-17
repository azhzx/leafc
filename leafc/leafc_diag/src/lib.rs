use std::fmt::Write;
use leafc_coreapi::diagnostic::{DiagTextColor, DiagMsg, DiagnosticianApi};
use leafc_coreapi::source::{Source, SourceId, SourcePool, Span};

pub struct Diagnostician<'a> {
    source_pool: &'a SourcePool,
    colors: DiagTextColor,
}

impl<'a> Diagnostician<'a> {
    /// 计算字符串在终端上的实际显示宽度（跳过 ANSI 颜色序列）
    fn visible_len(s: &str) -> usize {
        let mut len = 0;
        let mut chars = s.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                while let Some(&c) = chars.peek() {
                    if c.is_alphabetic() {
                        chars.next();
                        break;
                    }
                    chars.next();
                }
            } else {
                len += 1;
            }
        }
        len
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

        // 最大行号的位数，用于动态对齐
        let lineno_width = last.to_string().len();

        // 指示器前缀：固定的箭头行，不随行号宽度变化（保持原风格）
        let indicator_prefix = format!(
            "{}  ╭─➜  |{}",
            self.colors.diag_bar,
            self.colors.diag_reset
        );
        let indicator_visible_len = Self::visible_len(&indicator_prefix);

        for lineno in first..=last {
            let line = &lines[lineno - first];

            // 根据行号选择前缀格式
            let (prefix, prefix_visible_len) = if lineno < start_line {
                let p = format!(
                    "{}    {:>width$} | {}",
                    self.colors.diag_bar,
                    lineno,
                    self.colors.diag_reset,
                    width = lineno_width
                );
                let vlen = Self::visible_len(&p);
                (p, vlen)
            } else if lineno == start_line {
                let p = format!(
                    "{}    {:>width$} | {}",
                    self.colors.diag_bar,
                    lineno,
                    self.colors.diag_reset,
                    width = lineno_width
                );
                let vlen = Self::visible_len(&p);
                (p, vlen)
            } else {
                let p = format!(
                    "{}  | {:>width$} | {}",
                    self.colors.diag_bar,
                    lineno,
                    self.colors.diag_reset,
                    width = lineno_width
                );
                let vlen = Self::visible_len(&p);
                (p, vlen)
            };

            writeln!(&mut out, "{}{}", prefix, line).unwrap();

            // 仅错误起始行后打印指示符
            if lineno == start_line {
                let fill = (prefix_visible_len + start_col).saturating_sub(indicator_visible_len);

                let caret_len = if end_col > start_col {
                    end_col - start_col
                } else {
                    1
                };

                let mut indicator = String::with_capacity(
                    indicator_visible_len + fill + caret_len + 20,
                );
                indicator.push_str(&indicator_prefix);
                for _ in 0..fill {
                    indicator.push(' ');
                }
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

        // 结尾竖线行
        writeln!(
            &mut out,
            "{}  |{}",
            self.colors.diag_bar,
            self.colors.diag_reset
        )
            .unwrap();

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