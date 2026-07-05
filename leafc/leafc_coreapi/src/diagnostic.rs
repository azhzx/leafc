use crate::source::{SourceId, SourcePool, Span};

#[derive(Debug, Clone)]
pub struct DiagMsg {
    pub title: String,
    pub msg: String,
    pub span: Span,
    pub source: SourceId
}

pub struct DiagTextColor {
    pub diag_title: &'static str,
    pub diag_message: &'static str,
    pub diag_bar: &'static str,
    pub diag_reset: &'static str,
    pub diag_source_name: &'static str,
}

pub trait DiagnosticianApi {
    fn new(source_pool: SourcePool, colors: DiagTextColor) -> Self;
    fn reset_colors(&mut self, new_colors: DiagTextColor);
    fn add_source(&mut self, source_name: String, text: String) -> SourceId;
    fn report(&self, diag: DiagMsg) -> String;
}