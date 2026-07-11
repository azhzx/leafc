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

pub trait DiagnosticianApi<'a> {
    fn new(source_pool: &'a SourcePool, colors: DiagTextColor) -> Self;
    fn reset_colors(&mut self, new_colors: DiagTextColor);
    fn report(&self, diag: DiagMsg) -> String;
}