use crate::source::{SourceId, SourcePool, Span};

#[derive(Debug, Clone)]
pub struct DiagMsg {
    pub title: String,
    pub msg: String,
    pub span: Span,
    pub source: SourceId
}

pub struct Colors {
    pub red: &'static str,
    pub green: &'static str,
    pub blue: &'static str,
    pub cyan: &'static str,
    pub pink: &'static str,
    pub purple: &'static str,
    pub reset_color: &'static str,
}

pub trait DiagnosticianApi {
    fn new(source_pool: SourcePool, colors: Colors) -> Self;
    fn reset_colors(&mut self, new_colors: Colors);
    fn add_source(&mut self, source_name: String, text: String) -> SourceId;
    fn report(&self, diag: DiagMsg) -> String;
}