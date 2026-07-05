use std::fmt::Write;
use leafc_coreapi;
use leafc_coreapi::diagnostic::{DiagTextColor, DiagMsg, DiagnosticianApi};
use leafc_coreapi::source::{Source, SourceId, SourcePool};

pub struct Diagnostician {
    source_pool: SourcePool,
    colors: DiagTextColor
}

impl DiagnosticianApi for Diagnostician {
    fn new(source_pool: SourcePool, colors: DiagTextColor) -> Diagnostician {
        Diagnostician { source_pool, colors }
    }

    fn reset_colors(&mut self, new_colors: DiagTextColor) {
        self.colors = new_colors;
    }

    fn add_source(&mut self, source_name: String, text: String) -> SourceId {
        self.source_pool.push(Source {
            file_name: source_name,
            file_lines : text.lines().map(String::from).collect(),
        });

        (self.source_pool.len() - 1) as SourceId
    }

    fn report(&self, diag: DiagMsg) -> String {
        // 格式
        // 上下两行 + 源代码无色, ^红色, Title红色, error_message红色 + source_name紫色, 其它无色
        //  --> source_name
        //     32 |
        //     33 | fun foo() -> String
        //  ╭─➜  |              ^^^^^^
        //  |  34 |     let m = 0
        //  |
        //  Title: error_message

        let mut out = String::new();

        let source_id = diag.source;
        let source_name = &self.source_pool[source_id].file_name;
        let lines = &self.source_pool[source_id].file_lines;

        let start_line = diag.span.start.lineno; // 1‑based
        let end_line   = diag.span.end.lineno;
        let start_col  = diag.span.start.column;
        let end_col    = diag.span.end.column;

        // 第一行：源文件位置（紫色）
        writeln!(&mut out, "{}  -->    {}{}", self.colors.diag_source_name, source_name, self.colors.diag_reset).unwrap();

        let first = if start_line > 1 { start_line - 1 } else { start_line };
        let last  = if end_line < lines.len() { end_line + 1 } else { end_line };

        let indicator_prefix = format!("{}{}{}",
                                       self.colors.diag_bar,
                                       "  ╭─➜  |",
                                       self.colors.diag_reset);
        let indicator_len = indicator_prefix.chars().count();

        for lineno in first..=last {
            let line_idx = (lineno - 1) as usize;
            let line = &lines[line_idx];

            // 根据行号决定前缀
            let prefix = if lineno < start_line {
                // 错误行之前
                format!("{}  {:>4} | {}", self.colors.diag_bar, lineno, self.colors.diag_reset)
            } else if lineno == start_line {
                // 错误行本身
                format!("{}  {:>4} | {}", self.colors.diag_bar, lineno, self.colors.diag_reset)
            } else {
                // 错误行之后
                format!("{}  |  {} | {}", self.colors.diag_bar, lineno, self.colors.diag_reset)
            };

            writeln!(&mut out, "{}{}", prefix, line).unwrap();

            // 如果是错误起始行，打印指示行
            if lineno == start_line {
                // 计算填充空格，使 ^ 对齐到 start_col
                let fill = (prefix.len() + start_col).saturating_sub(indicator_len + 1);

                let mut indicator = String::with_capacity(indicator_len + fill + (end_col - start_col).max(1) + 10);
                indicator += indicator_prefix.as_str();
                for _ in 0..fill {
                    indicator.push(' ');
                }

                let caret_len = if end_col > start_col { end_col - start_col } else { 1 };
                write!(&mut indicator, "{}{}{}", self.colors.diag_title, "^".repeat(caret_len), self.colors.diag_reset).unwrap();
                writeln!(&mut out, "{}", indicator).unwrap();
            }
        }

        writeln!(&mut out, "{}  |{}", self.colors.diag_bar, self.colors.diag_reset).unwrap();
        writeln!(&mut out, "  {}{}: {}{}{}",
                 self.colors.diag_title,
                 diag.title,
                 self.colors.diag_message,
                 diag.msg, self.colors.diag_reset).unwrap();

        out
    }
}
