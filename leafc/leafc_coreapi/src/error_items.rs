use crate::source::{SourceId, SourcePool, Span};
use crate::token::{LiteToken, Token, TokenType};
use serde::Deserialize;
use std::fmt::Write;
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MsgKind {
    LexerUnexpectedEof,
    LexerInvalidString,
    LexerInvalidIndent,
    LexerInvalidChar,

    TokenPassInvalidPreprocessorParameterDeclare,
    TokenPassInvalidPreprocessorArgumentList,
    TokenPassUserPreprocessorPanic,
    TokenPassInvalidIdentToString,
    TokenPassInvalidIdentConcat,
    TokenPassInvalidPreprocessorSyntax,
    TokenPassInvalidMacroExpand,

    ParserTokenExpect,
    ParserInvalidTopDeclaration,
    ParserInvalidImportList,
    ParserInvalidOnlyList,
    ParserInvalidUseDeclaration,
    ParserFunctionDeclarationMissingParameterList,
    ParserInvalidGenericList,
    ParserInvalidFunctionParameterList,
    ParserInvalidFunctionBody,
    ParserInvalidGenericParameterList,
    ParserInvalidWhereBody,
    ParserWhereBodyGenericMissingMatchGenericParameterList,
    ParserInvalidTypeDeclaration,
    ParserInvalidTupleLiteral,
    ParserInvalidExpression,
    ParserInvalidOperator,
    ParserInvalidCallArgumentList,
    ParserInvalidTupleElement,
    ParserInvalidFunctionType,
    ParserInvalidStructInit,
    ParserInvalidPattern,
    ParserInvalidCatch,
    ParserInvalidTypeOf,

    NamePassUndefinedName,
    NamePassDuplicateDefinition,
    NamePassUndefinedModule,
    NamePassInvalidMemberAccess,
    NamePassInvalidADTConstructor,

    TypeCheckerDuplicateType,
    TypeCheckerInfiniteType,
    TypeCheckerTypeMismatch,
    TypeCheckerGenericArityMismatch,
    TypeCheckerArityMismatch,
    TypeCheckerUndefinedVariable,
    TypeCheckerTypeNotChecked,
    TypeCheckerFieldNotFound,
    TypeCheckerInternalError,
    TypeCheckerUnknownField,
    TypeCheckerMissingTypeAnnotation,
    TypeCheckerUndefinedType,
    TypeCheckerRecursiveTypeAlias,
    TypeCheckerMissingResume,
    TypeCheckerInvalidControlType,
    TypeCheckerUnreachablePattern,
    TypeCheckerNonExhaustiveMatch,
    TypeCheckerMultipleResume,

    HirLowerEmptyPath,
    HirLowerPathNotFound,
    HirLowerModuleScopeNotFound,
    HirLowerFieldNotFound,
    HirLowerConstructorNotFound,
    HirLowerControlNotFound,
    HirLowerInvalidPath,
    HirLowerGenericNotFound,
    HirLowerBindingNotFound,
    HirLowerParamNotFound,
    HirLowerMethodNotFound,
    HirLowerCtorNotFound,
    HirLowerNameNotFound,
    HirLowerLetNameNotFound,
    HirLowerArityMismatch,
    HirLowerSymbolNotFound,
    HirLowerMissingArguments,
    HirLowerTooManyArguments,
    HirLowerUnexpectedKeywordArg,
    HirLowerDuplicateKeywordArg,
    HirLowerCannotResolveFunction,
    HirLowerArgumentConflict,
    HirLowerInvalidPipeLineTarget,

    MirLowerGeneric,
    MirConstEvalFailed,
    MirLifetimeGeneric,
    MirMonoGeneric,

    CodegenGeneric,

    MiscIo,
    MiscInternal,
    Unreachable,
    Todo,
    Deprecated,

    ContextMovedHere,
    HelpUseClone,
    SuggestionRemove,
    SuggestionAdd,
    WarningUnusedVariable,
    WarningDeprecated,
    WarningUnreachableCode,

    DefineStackFrom,
    DefineModule,
    DefineFunction,
    DefineStruct,
    DefineAdt,

    ContextLabel,
    ErrorLabel,
    HelpLabel,
    HelpCheckSyntax,
}

#[derive(Debug, Clone)]
pub struct LocalizedMessage {
    pub kind: MsgKind,
    pub args: Vec<String>,
}

impl LocalizedMessage {
    pub fn new(kind: MsgKind, args: impl IntoIterator<Item = impl ToString>) -> Self {
        Self {
            kind,
            args: args.into_iter().map(|x| x.to_string()).collect(),
        }
    }

    pub fn render(&self, localizer: &dyn Localizer) -> String {
        localizer.translate(self.kind, &self.args)
    }
}

pub trait Localizer {
    fn translate(&self, kind: MsgKind, args: &[String]) -> String;
}


#[derive(Debug, Clone, Error)]
pub enum LexerErrorKind {
    #[error("unexpected end of file")]
    UnexpectedEof,
    #[error("invalid string literal")]
    InvalidString,
    #[error("invalid indentation")]
    InvalidIndent,
    #[error("invalid character: {0}")]
    InvalidChar(char),
}

#[derive(Debug, Clone, Error)]
pub enum TokenPassErrorKind {
    #[error("invalid preprocessor parameter declaration")]
    InvalidPreprocessorParameterDeclare,
    #[error("invalid preprocessor argument list")]
    InvalidPreprocessorArgumentList,
    #[error("user preprocessor panic: {0}")]
    UserPreprocessorPanic(String),
    #[error("invalid identifier to string conversion")]
    InvalidIdentToString,
    #[error("invalid identifier concatenation")]
    InvalidIdentConcat,
    #[error("invalid preprocessor syntax")]
    InvalidPreprocessorSyntax,
    #[error("invalid macro expansion")]
    InvalidMacroExpand,
}

#[derive(Debug, Clone, Error)]
pub enum ParserErrorKind {
    #[error("expected {expected:?}, found {found:?}")]
    TokenExpect { expected: TokenType, found: TokenType },
    #[error("invalid top-level declaration")]
    InvalidTopDeclaration,
    #[error("invalid import list")]
    InvalidImportList,
    #[error("invalid only list")]
    InvalidOnlyList,
    #[error("invalid use declaration")]
    InvalidUseDeclaration,
    #[error("function declaration missing parameter list")]
    FunctionDeclarationMissingParameterList,
    #[error("invalid generic list")]
    InvalidGenericList,
    #[error("invalid function parameter list")]
    InvalidFunctionParameterList,
    #[error("invalid function body")]
    InvalidFunctionBody,
    #[error("invalid generic parameter list")]
    InvalidGenericParameterList,
    #[error("invalid where body")]
    InvalidWhereBody,
    #[error("where clause generic mismatch")]
    WhereBodyGenericMissingMatchGenericParameterList,
    #[error("invalid type declaration")]
    InvalidTypeDeclaration,
    #[error("invalid tuple literal")]
    InvalidTupleLiteral,
    #[error("invalid expression")]
    InvalidExpression,
    #[error("invalid operator")]
    InvalidOperator,
    #[error("invalid call argument list")]
    InvalidCallArgumentList,
    #[error("invalid tuple element")]
    InvalidTupleElement,
    #[error("invalid function type")]
    InvalidFunctionType,
    #[error("invalid struct init")]
    InvalidStructInit,
    #[error("invalid pattern")]
    InvalidPattern,
    #[error("invalid catch clause")]
    InvalidCatch,
    #[error("invalid typeof expression")]
    InvalidTypeOf,
}

#[derive(Debug, Clone, Error)]
pub enum NamePassErrorKind {
    #[error("undefined name: {0}")]
    UndefinedName(String),
    #[error("name already defined: {0}")]
    DuplicateDefinition(String),
    #[error("undefined module: {0}")]
    UndefinedModule(String),
    #[error("invalid member access")]
    InvalidMemberAccess,
    #[error("invalid ADT constructor")]
    InvalidADTConstructor,
}

#[derive(Debug, Clone, Error)]
pub enum TypeCheckerErrorKind {
    #[error("duplicate type")]
    DuplicateType,
    #[error("infinite type")]
    InfiniteType,
    #[error("type mismatch: expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },
    #[error("generic arity mismatch")]
    GenericArityMismatch,
    #[error("arity mismatch")]
    ArityMismatch,
    #[error("undefined variable: {0}")]
    UndefinedVariable(String),
    #[error("type not checked yet")]
    TypeNotChecked,
    #[error("field not found: {0}")]
    FieldNotFound(String),
    #[error("internal type error")]
    InternalError,
    #[error("unknown field: {0}")]
    UnknownField(String),
    #[error("missing type annotation")]
    MissingTypeAnnotation,
    #[error("undefined type: {0}")]
    UndefinedType(String),
    #[error("recursive type alias")]
    RecursiveTypeAlias,
    #[error("missing resume in handler")]
    MissingResume,
    #[error("invalid control type")]
    InvalidControlType,
    #[error("unreachable pattern")]
    UnreachablePattern,
    #[error("non-exhaustive match")]
    NonExhaustiveMatch,
    #[error("multiple resume statements in handler")]
    MultipleResume,
}

#[derive(Debug, Clone, Error)]
pub enum HirLowerErrorKind {
    #[error("empty path")]
    EmptyPath,
    #[error("path not found")]
    PathNotFound,
    #[error("module scope not found")]
    ModuleScopeNotFound,
    #[error("field not found: {0}")]
    FieldNotFound(String),
    #[error("constructor not found: {0}")]
    ConstructorNotFound(String),
    #[error("control not found: {0}")]
    ControlNotFound(String),
    #[error("invalid path")]
    InvalidPath,
    #[error("generic parameter not found: {0}")]
    GenericNotFound(String),
    #[error("binding not found: {0}")]
    BindingNotFound(String),
    #[error("parameter not found: {0}")]
    ParamNotFound(String),
    #[error("method not found: {0}")]
    MethodNotFound(String),
    #[error("constructor definition not found: {0}")]
    CtorNotFound(String),
    #[error("name not found: {0}")]
    NameNotFound(String),
    #[error("let name not found: {0}")]
    LetNameNotFound(String),
    #[error("arity mismatch")]
    ArityMismatch,
    #[error("symbol not found")]
    SymbolNotFound,
    #[error("missing arguments")]
    MissingArguments,
    #[error("too many arguments")]
    TooManyArguments,
    #[error("unexpected keyword argument")]
    UnexpectedKeywordArg,
    #[error("duplicate keyword argument")]
    DuplicateKeywordArg,
    #[error("cannot resolve function for named arguments")]
    CannotResolveFunction,
    #[error("argument conflict")]
    ArgumentConflict,
    #[error("invalid pipeline target")]
    InvalidPipeLineTarget,
}

#[derive(Debug, Clone, Error)]
pub enum MirLowerErrorKind {
    #[error("MIR lowering error")]
    Generic,
}

#[derive(Debug, Clone, Error)]
pub enum MirConstEvalErrorKind {
    #[error("constant evaluation failed")]
    EvalFailed,
}

#[derive(Debug, Clone, Error)]
pub enum MirLifetimeCheckerErrorKind {
    #[error("lifetime error")]
    Lifetime,
}

#[derive(Debug, Clone, Error)]
pub enum MirMonoErrorKind {
    #[error("monomorphization error")]
    Generic,
}

#[derive(Debug, Clone, Error)]
pub enum CodegenErrorKind {
    #[error("code generation error")]
    Generic,
}

#[derive(Debug, Clone, Error)]
pub enum CompileTimeErrorKind {
    #[error("lexer error: {0}")]
    LexerError(#[from] LexerErrorKind),
    #[error("preprocessor error: {0}")]
    TokenPassError(#[from] TokenPassErrorKind),
    #[error("parser error: {0}")]
    ParserError(#[from] ParserErrorKind),
    #[error("name pass error: {0}")]
    NamePassError(#[from] NamePassErrorKind),
    #[error("type checker error: {0}")]
    TypeCheckerError(#[from] TypeCheckerErrorKind),
    #[error("HIR lowering error: {0}")]
    HirLowerError(#[from] HirLowerErrorKind),
    #[error("MIR lowering error: {0}")]
    MirLowerError(#[from] MirLowerErrorKind),
    #[error("MIR const eval error: {0}")]
    MirConstEvalError(#[from] MirConstEvalErrorKind),
    #[error("MIR lifetime error: {0}")]
    MirLifetimeCheckerError(#[from] MirLifetimeCheckerErrorKind),
    #[error("MIR mono error: {0}")]
    MirMonoError(#[from] MirMonoErrorKind),
    #[error("codegen error: {0}")]
    CodegenError(#[from] CodegenErrorKind),
}

#[derive(Debug, Clone, Error)]
pub enum MiscErrorKind {
    #[error("IO error")]
    Io,
    #[error("internal error")]
    Internal,
}

#[derive(Debug, Clone, Error)]
pub enum ErrorKind {
    #[error("compile-time error: {0}")]
    CompileTimeError(#[from] CompileTimeErrorKind),
    #[error("misc error: {0}")]
    MiscError(#[from] MiscErrorKind),
    #[error("unreachable code reached")]
    Unreachable,
    #[error("not yet implemented")]
    Todo,
    #[error("deprecated feature")]
    Deprecated,
}

#[derive(Debug, Clone, Error)]
pub enum WarningKind {
    #[error("unused variable: {0}")]
    UnusedVariable(String),
    #[error("deprecated")]
    Deprecated,
    #[error("unreachable code")]
    UnreachableCode,
}


#[derive(Debug, Clone)]
pub enum DefineKind {
    Module(String),
    Function(String),
    Struct(String),
    ADT(String, String),
}

#[derive(Debug, Clone)]
pub enum ContextKind {
    FunctionSignature {
        name: String,
        params: Vec<(String, String)>,
        return_ty: String,
    },
    EnumDefinition {
        name: String,
        variants: Vec<String>,
    },
    StructDefinition {
        name: String,
        fields: Vec<String>,
    },
    Context {
        tokens: Vec<Token>,
    },
}

#[derive(Debug, Clone)]
pub struct Context {
    pub kind: ContextKind,
    pub message: LocalizedMessage,
    pub span: Span,
}

impl Context {
    pub fn new(kind: ContextKind, message: LocalizedMessage, span: Span) -> Self {
        Self { kind, message, span }
    }
}

#[derive(Debug, Clone)]
pub enum SuggestionKind {
    Remove { start_off: usize, end_off: usize },
    Add { after_off: usize, tokens: Vec<LiteToken> },
    Nop,
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub span: Span,
    pub kind: SuggestionKind,
    pub message: LocalizedMessage,
}

impl Suggestion {
    pub fn new(span: Span, kind: SuggestionKind, message: LocalizedMessage) -> Self {
        Self { span, kind, message }
    }
}


#[derive(Debug, Clone)]
pub struct DiagErrorItem {
    pub kind: ErrorKind,
    pub span: Span,
    pub message: LocalizedMessage,
}

/// 一条完整错误
#[derive(Debug, Clone)]
pub struct DiagError {
    pub define_stack: Option<Vec<DefineKind>>,
    pub context: Option<Context>,
    pub item: DiagErrorItem,
    pub suggestions: Option<Suggestion>,
}

impl DiagError {
    pub fn new(kind: ErrorKind, span: Span, message: LocalizedMessage) -> Self {
        Self {
            define_stack: None,
            context: None,
            item: DiagErrorItem { kind, span, message },
            suggestions: None,
        }
    }

    pub fn with_context(mut self, context: Context) -> Self {
        self.context = Some(context);
        self
    }

    pub fn with_stack(mut self, stack: Vec<DefineKind>) -> Self {
        self.define_stack = Some(stack);
        self
    }

    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Self {
        self.suggestions = Some(suggestion);
        self
    }
}

#[derive(Debug, Clone)]
pub struct DiagWarning {
    pub kind: WarningKind,
    pub message: LocalizedMessage,
    pub span: Span,
    pub suggestions: Option<Suggestion>,
}

#[derive(Debug, Clone, Default)]
pub struct DiagCollector {
    pub warnings: Vec<DiagWarning>,
    pub errors: Vec<DiagError>,
}

impl DiagCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_warning(&mut self, kind: WarningKind, span: Span, message: LocalizedMessage) {
        self.warnings.push(DiagWarning {
            kind,
            message,
            span,
            suggestions: None,
        });
    }

    pub fn add_error(&mut self, kind: ErrorKind, span: Span, message: LocalizedMessage) {
        self.errors.push(DiagError::new(kind, span, message));
    }

    pub fn add_error_with_suggestion(
        &mut self,
        kind: ErrorKind,
        span: Span,
        message: LocalizedMessage,
        suggestion: Option<Suggestion>,
    ) {
        let mut err = DiagError::new(kind, span, message);
        if let Some(s) = suggestion {
            err = err.with_suggestion(s);
        }
        self.errors.push(err);
    }
}




pub trait SourceProvider {
    fn get_line_info(&self, source_id: SourceId, offset: usize) -> Option<(usize, String, usize)>;
    fn source(&self, source_id: SourceId) -> Option<&str>;
}

pub struct SourcePoolProvider<'a> {
    pool: &'a SourcePool,
}

impl<'a> SourcePoolProvider<'a> {
    pub fn new(pool: &'a SourcePool) -> Self {
        Self { pool }
    }
}

impl<'a> SourceProvider for SourcePoolProvider<'a> {
    fn get_line_info(&self, source_id: SourceId, offset: usize) -> Option<(usize, String, usize)> {
        let source = self.pool.0.get(source_id)?;
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

    fn source(&self, source_id: SourceId) -> Option<&str> {
        self.pool.0.get(source_id).map(|s| s.file_content.as_str())
    }
}

pub struct TokenCache {
    tokens: HashMap<SourceId, Vec<Token>>,
}

impl TokenCache {
    pub fn new() -> Self {
        Self { tokens: HashMap::new() }
    }

    pub fn store_tokens(&mut self, source_id: SourceId, tokens: Vec<Token>) {
        self.tokens.insert(source_id, tokens);
    }

    pub fn get_tokens_in_span(&self, span: &Span) -> Option<&[Token]> {
        let tokens = self.tokens.get(&span.source_id)?;
        let start = tokens.binary_search_by_key(&span.start_off, |t| t.span.start_off).ok()?;
        let end = tokens[start..]
            .iter()
            .position(|t| t.span.end_off >= span.end_off)
            .map(|p| start + p + 1)
            .unwrap_or(tokens.len());
        Some(&tokens[start..end])
    }
}


#[derive(Debug, Clone)]
pub struct DiagColorConfig {
    pub error_title: &'static str,
    pub warning_title: &'static str,
    pub note: &'static str,
    pub help: &'static str,
    pub source_name: &'static str,
    pub highlight: &'static str,
    pub reset: &'static str,
}

impl Default for DiagColorConfig {
    fn default() -> Self {
        Self {
            error_title: "\x1b[31m",
            warning_title: "\x1b[33m",
            note: "\x1b[34m",
            help: "\x1b[32m",
            source_name: "\x1b[35m",
            highlight: "\x1b[1;31m",
            reset: "\x1b[0m",
        }
    }
}

pub struct SourceMap {
    pool: SourcePool,
    line_starts: HashMap<SourceId, Vec<usize>>,
}

impl SourceMap {
    pub fn new(pool: SourcePool) -> Self {
        let mut line_starts = HashMap::new();
        for (id, source) in pool.0.iter().enumerate() {
            let mut starts = vec![0usize];
            for (i, c) in source.file_content.char_indices() {
                if c == '\n' {
                    starts.push(i + 1);
                }
            }
            line_starts.insert(id, starts);
        }
        Self { pool, line_starts }
    }

    pub fn get_line_info(&self, source_id: SourceId, offset: usize) -> Option<(usize, String, usize)> {
        let source = self.pool.0.get(source_id)?;
        let line_starts = self.line_starts.get(&source_id)?;
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

    pub fn source(&self, source_id: SourceId) -> Option<&str> {
        self.pool.0.get(source_id).map(|s| s.file_content.as_str())
    }

    pub fn pool(&self) -> &SourcePool {
        &self.pool
    }

    pub fn pool_mut(&mut self) -> &mut SourcePool {
        &mut self.pool
    }

    pub fn add_source(&mut self, path: String, content: String) -> SourceId {
        let id = self.pool.add_source(path, content);
        let source = &self.pool.0[id];
        let mut starts = vec![0usize];
        for (i, c) in source.file_content.char_indices() {
            if c == '\n' {
                starts.push(i + 1);
            }
        }
        self.line_starts.insert(id, starts);
        id
    }
}


pub struct DiagCtx {
    pub collector: DiagCollector,
    pub source_map: SourceMap,
    pub token_cache: TokenCache,
    pub localizer: Box<dyn Localizer>,
    pub colors: DiagColorConfig,
}

impl DiagCtx {
    pub fn new(
        source_map: SourceMap,
        localizer: Box<dyn Localizer>,
        colors: DiagColorConfig,
    ) -> Self {
        Self {
            collector: DiagCollector::new(),
            source_map,
            token_cache: TokenCache::new(),
            localizer,
            colors,
        }
    }

    pub fn emit_error(&mut self, kind: ErrorKind, span: Span, msg: LocalizedMessage) {
        self.collector.add_error(kind, span, msg);
    }

    pub fn emit_warning(&mut self, kind: WarningKind, span: Span, msg: LocalizedMessage) {
        self.collector.add_warning(kind, span, msg);
    }

    pub fn cache_tokens(&mut self, source_id: SourceId, tokens: Vec<Token>) {
        self.token_cache.store_tokens(source_id, tokens);
    }

    pub fn has_errors(&self) -> bool {
        !self.collector.errors.is_empty()
    }

    pub fn emit_all(&self) -> String {
        let emitter = DiagEmitter::new(self);
        emitter.render(&self.collector)
    }
}

pub struct DiagEmitter<'a> {
    source_map: &'a SourceMap,
    token_cache: &'a TokenCache,
    localizer: &'a dyn Localizer,
    colors: &'a DiagColorConfig,
}

impl<'a> DiagEmitter<'a> {
    fn new(ctx: &'a DiagCtx) -> Self {
        DiagEmitter {
            source_map: &ctx.source_map,
            token_cache: &ctx.token_cache,
            localizer: ctx.localizer.as_ref(),
            colors: &ctx.colors,
        }
    }

    pub fn render(&self, collector: &DiagCollector) -> String {
        let mut output = String::new();
        for warn in &collector.warnings {
            self.render_warning(warn, &mut output);
            output.push('\n');
        }
        for err in &collector.errors {
            self.render_error(err, &mut output);
            output.push('\n');
        }
        output
    }

    // 以下方法保持不变，只需将 self.source_provider 替换为 self.source_map
    fn render_warning(&self, warn: &DiagWarning, out: &mut String) {
        writeln!(out, "{}warning:{} {}", self.colors.warning_title, self.colors.reset,
                 warn.message.render(self.localizer)).unwrap();
        self.render_source_line(warn.span.clone(), self.colors.warning_title, out);
    }

    fn render_error(&self, err: &DiagError, out: &mut String) {
        if let Some(stack) = &err.define_stack {
            self.render_define_stack(stack, out);
        }
        if let Some(ctx) = &err.context {
            self.render_context(ctx, out);
        }
        self.render_error_item(&err.item, out);
        if let Some(sug) = &err.suggestions {
            self.render_suggestion(sug, out);
        }
    }

    fn render_define_stack(&self, stack: &[DefineKind], out: &mut String) {
        writeln!(out, "{}From:{}", self.colors.note, self.colors.reset).unwrap();
        for (i, item) in stack.iter().enumerate() {
            let prefix = if i == 0 { "--" } else { "   \\" };
            match item {
                DefineKind::Module(name) => writeln!(out, "{} module {}", prefix, name).unwrap(),
                DefineKind::Function(name) => writeln!(out, "{} - fun {}", prefix, name).unwrap(),
                DefineKind::Struct(name) => writeln!(out, "{} struct {}", prefix, name).unwrap(),
                DefineKind::ADT(name, _) => writeln!(out, "{} ADT {}", prefix, name).unwrap(),
            }
        }
        out.push('\n');
    }

    fn render_context(&self, ctx: &Context, out: &mut String) {
        if let Some((line_no, line_content, col)) = self.source_map.get_line_info(ctx.span.source_id, ctx.span.start_off) {
            let prefix = format!("  {}| ", line_no);
            writeln!(out, "{}{}{}{}", self.colors.note, prefix, line_content.trim_end(), self.colors.reset).unwrap();
            // 缩进 = 前缀可见宽度 + 列前可见字符数，再减去 caret 行自带的 "| "
            let before_width = line_content[..col].chars().count();
            let indent = " ".repeat(prefix.len() + before_width - 2);
            writeln!(out, "{}| {}^--- {}", indent, self.colors.note, ctx.message.render(self.localizer)).unwrap();
        }
    }

    fn render_error_item(&self, item: &DiagErrorItem, out: &mut String) {
        let msg = item.message.render(self.localizer);
        self.render_source_line(item.span.clone(), self.colors.error_title, out);
        writeln!(out, "{}{}:{} {}", self.colors.error_title, "error", self.colors.reset, msg).unwrap();
    }

    fn render_suggestion(&self, sug: &Suggestion, out: &mut String) {
        writeln!(out, "{}Help:{} {}", self.colors.help, self.colors.reset, sug.message.render(self.localizer)).unwrap();
        self.render_source_line(sug.span.clone(), self.colors.help, out);
    }

    fn render_source_line(&self, span: Span, color: &str, out: &mut String) {
        if let Some((line_no, line_content, col)) = self.source_map.get_line_info(span.source_id, span.start_off) {
            let prefix = format!("{} {}| ", color, line_no);
            write!(out, "{}", prefix).unwrap();
            // 前缀含 ANSI 颜色码，缩进需按可见宽度计算，且列按可见字符数而非字节数对齐
            let prefix_width = prefix.len() - color.len();
            let before_width = line_content[..col].chars().count();
            if let Some(tokens) = self.token_cache.get_tokens_in_span(&span) {
                let start = span.start_off;
                let end = span.end_off;
                let before = &line_content[..col];
                let highlighted = &line_content[col..col + (end - start)];
                let after = &line_content[col + (end - start)..];
                write!(out, "{}{}{}{}{}\n", before, self.colors.highlight, highlighted, self.colors.reset, after).unwrap();
            } else {
                writeln!(out, "{}", line_content.trim_end()).unwrap();
            }
            let indent = " ".repeat(prefix_width + before_width);
            let carets = "^".repeat(span.len().max(1));
            writeln!(out, "{}{}{}{}", indent, self.colors.highlight, carets, self.colors.reset).unwrap();
        }
    }
}


pub fn make_error(
    kind: ErrorKind,
    span: Span,
    args: impl IntoIterator<Item = impl ToString>
) -> DiagError {
    let msg_kind = match get_message_kind_for_error(&kind) {
        Some((main, _)) => main,
        None => MsgKind::MiscInternal,
    };
    let message = LocalizedMessage::new(msg_kind, args);
    DiagError::new(kind, span, message)
}

pub fn make_error_with_suggestion(
    kind: ErrorKind,
    span: Span,
    args: impl IntoIterator<Item = impl ToString>,
    suggestion: Option<Suggestion>,
) -> DiagError {
    let mut err = make_error(kind, span, args);
    if let Some(s) = suggestion {
        err = err.with_suggestion(s);
    }
    err
}


pub fn get_message_kind_for_error(error_kind: &ErrorKind) -> Option<(MsgKind, MsgKind)> {
    match error_kind {
        ErrorKind::CompileTimeError(cte) => match cte {
            CompileTimeErrorKind::LexerError(le) => match le {
                LexerErrorKind::UnexpectedEof => Some((MsgKind::LexerUnexpectedEof, MsgKind::HelpCheckSyntax)),
                LexerErrorKind::InvalidString => Some((MsgKind::LexerInvalidString, MsgKind::HelpCheckSyntax)),
                LexerErrorKind::InvalidIndent => Some((MsgKind::LexerInvalidIndent, MsgKind::HelpCheckSyntax)),
                LexerErrorKind::InvalidChar(_) => Some((MsgKind::LexerInvalidChar, MsgKind::HelpCheckSyntax)),
            },
            CompileTimeErrorKind::TokenPassError(tpe) => match tpe {
                TokenPassErrorKind::InvalidPreprocessorParameterDeclare => Some((MsgKind::TokenPassInvalidPreprocessorParameterDeclare, MsgKind::HelpCheckSyntax)),
                TokenPassErrorKind::InvalidPreprocessorArgumentList => Some((MsgKind::TokenPassInvalidPreprocessorArgumentList, MsgKind::HelpCheckSyntax)),
                TokenPassErrorKind::UserPreprocessorPanic(_) => Some((MsgKind::TokenPassUserPreprocessorPanic, MsgKind::HelpCheckSyntax)),
                TokenPassErrorKind::InvalidIdentToString => Some((MsgKind::TokenPassInvalidIdentToString, MsgKind::HelpCheckSyntax)),
                TokenPassErrorKind::InvalidIdentConcat => Some((MsgKind::TokenPassInvalidIdentConcat, MsgKind::HelpCheckSyntax)),
                TokenPassErrorKind::InvalidPreprocessorSyntax => Some((MsgKind::TokenPassInvalidPreprocessorSyntax, MsgKind::HelpCheckSyntax)),
                TokenPassErrorKind::InvalidMacroExpand => Some((MsgKind::TokenPassInvalidMacroExpand, MsgKind::HelpCheckSyntax)),
            },
            CompileTimeErrorKind::ParserError(pe) => match pe {
                ParserErrorKind::TokenExpect { .. } => Some((MsgKind::ParserTokenExpect, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidTopDeclaration => Some((MsgKind::ParserInvalidTopDeclaration, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidImportList => Some((MsgKind::ParserInvalidImportList, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidOnlyList => Some((MsgKind::ParserInvalidOnlyList, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidUseDeclaration => Some((MsgKind::ParserInvalidUseDeclaration, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::FunctionDeclarationMissingParameterList => Some((MsgKind::ParserFunctionDeclarationMissingParameterList, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidGenericList => Some((MsgKind::ParserInvalidGenericList, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidFunctionParameterList => Some((MsgKind::ParserInvalidFunctionParameterList, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidFunctionBody => Some((MsgKind::ParserInvalidFunctionBody, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidGenericParameterList => Some((MsgKind::ParserInvalidGenericParameterList, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidWhereBody => Some((MsgKind::ParserInvalidWhereBody, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::WhereBodyGenericMissingMatchGenericParameterList => Some((MsgKind::ParserWhereBodyGenericMissingMatchGenericParameterList, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidTypeDeclaration => Some((MsgKind::ParserInvalidTypeDeclaration, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidTupleLiteral => Some((MsgKind::ParserInvalidTupleLiteral, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidExpression => Some((MsgKind::ParserInvalidExpression, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidOperator => Some((MsgKind::ParserInvalidOperator, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidCallArgumentList => Some((MsgKind::ParserInvalidCallArgumentList, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidTupleElement => Some((MsgKind::ParserInvalidTupleElement, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidFunctionType => Some((MsgKind::ParserInvalidFunctionType, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidStructInit => Some((MsgKind::ParserInvalidStructInit, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidPattern => Some((MsgKind::ParserInvalidPattern, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidCatch => Some((MsgKind::ParserInvalidCatch, MsgKind::HelpCheckSyntax)),
                ParserErrorKind::InvalidTypeOf => Some((MsgKind::ParserInvalidTypeOf, MsgKind::HelpCheckSyntax)),
            },
            CompileTimeErrorKind::NamePassError(npe) => match npe {
                NamePassErrorKind::UndefinedName(_) => Some((MsgKind::NamePassUndefinedName, MsgKind::HelpCheckSyntax)),
                NamePassErrorKind::DuplicateDefinition(_) => Some((MsgKind::NamePassDuplicateDefinition, MsgKind::HelpCheckSyntax)),
                NamePassErrorKind::UndefinedModule(_) => Some((MsgKind::NamePassUndefinedModule, MsgKind::HelpCheckSyntax)),
                NamePassErrorKind::InvalidMemberAccess => Some((MsgKind::NamePassInvalidMemberAccess, MsgKind::HelpCheckSyntax)),
                NamePassErrorKind::InvalidADTConstructor => Some((MsgKind::NamePassInvalidADTConstructor, MsgKind::HelpCheckSyntax)),
            },
            CompileTimeErrorKind::TypeCheckerError(tce) => match tce {
                TypeCheckerErrorKind::DuplicateType => Some((MsgKind::TypeCheckerDuplicateType, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::InfiniteType => Some((MsgKind::TypeCheckerInfiniteType, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::TypeMismatch { .. } => Some((MsgKind::TypeCheckerTypeMismatch, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::GenericArityMismatch => Some((MsgKind::TypeCheckerGenericArityMismatch, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::ArityMismatch => Some((MsgKind::TypeCheckerArityMismatch, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::UndefinedVariable(_) => Some((MsgKind::TypeCheckerUndefinedVariable, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::TypeNotChecked => Some((MsgKind::TypeCheckerTypeNotChecked, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::FieldNotFound(_) => Some((MsgKind::TypeCheckerFieldNotFound, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::InternalError => Some((MsgKind::TypeCheckerInternalError, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::UnknownField(_) => Some((MsgKind::TypeCheckerUnknownField, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::MissingTypeAnnotation => Some((MsgKind::TypeCheckerMissingTypeAnnotation, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::UndefinedType(_) => Some((MsgKind::TypeCheckerUndefinedType, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::RecursiveTypeAlias => Some((MsgKind::TypeCheckerRecursiveTypeAlias, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::MissingResume => Some((MsgKind::TypeCheckerMissingResume, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::InvalidControlType => Some((MsgKind::TypeCheckerInvalidControlType, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::UnreachablePattern => Some((MsgKind::TypeCheckerUnreachablePattern, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::NonExhaustiveMatch => Some((MsgKind::TypeCheckerNonExhaustiveMatch, MsgKind::HelpCheckSyntax)),
                TypeCheckerErrorKind::MultipleResume => Some((MsgKind::TypeCheckerMultipleResume, MsgKind::HelpCheckSyntax)),
            },
            CompileTimeErrorKind::HirLowerError(hle) => match hle {
                HirLowerErrorKind::EmptyPath => Some((MsgKind::HirLowerEmptyPath, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::PathNotFound => Some((MsgKind::HirLowerPathNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::ModuleScopeNotFound => Some((MsgKind::HirLowerModuleScopeNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::FieldNotFound(_) => Some((MsgKind::HirLowerFieldNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::ConstructorNotFound(_) => Some((MsgKind::HirLowerConstructorNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::ControlNotFound(_) => Some((MsgKind::HirLowerControlNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::InvalidPath => Some((MsgKind::HirLowerInvalidPath, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::GenericNotFound(_) => Some((MsgKind::HirLowerGenericNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::BindingNotFound(_) => Some((MsgKind::HirLowerBindingNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::ParamNotFound(_) => Some((MsgKind::HirLowerParamNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::MethodNotFound(_) => Some((MsgKind::HirLowerMethodNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::CtorNotFound(_) => Some((MsgKind::HirLowerCtorNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::NameNotFound(_) => Some((MsgKind::HirLowerNameNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::LetNameNotFound(_) => Some((MsgKind::HirLowerLetNameNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::ArityMismatch => Some((MsgKind::HirLowerArityMismatch, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::SymbolNotFound => Some((MsgKind::HirLowerSymbolNotFound, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::MissingArguments => Some((MsgKind::HirLowerMissingArguments, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::TooManyArguments => Some((MsgKind::HirLowerTooManyArguments, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::UnexpectedKeywordArg => Some((MsgKind::HirLowerUnexpectedKeywordArg, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::DuplicateKeywordArg => Some((MsgKind::HirLowerDuplicateKeywordArg, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::CannotResolveFunction => Some((MsgKind::HirLowerCannotResolveFunction, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::ArgumentConflict => Some((MsgKind::HirLowerArgumentConflict, MsgKind::HelpCheckSyntax)),
                HirLowerErrorKind::InvalidPipeLineTarget => Some((MsgKind::HirLowerInvalidPipeLineTarget, MsgKind::HelpCheckSyntax)),
            },
            CompileTimeErrorKind::MirLowerError(_) => Some((MsgKind::MirLowerGeneric, MsgKind::HelpCheckSyntax)),
            CompileTimeErrorKind::MirConstEvalError(_) => Some((MsgKind::MirConstEvalFailed, MsgKind::HelpCheckSyntax)),
            CompileTimeErrorKind::MirLifetimeCheckerError(_) => Some((MsgKind::MirLifetimeGeneric, MsgKind::HelpCheckSyntax)),
            CompileTimeErrorKind::MirMonoError(_) => Some((MsgKind::MirMonoGeneric, MsgKind::HelpCheckSyntax)),
            CompileTimeErrorKind::CodegenError(_) => Some((MsgKind::CodegenGeneric, MsgKind::HelpCheckSyntax)),
        },
        ErrorKind::MiscError(me) => match me {
            MiscErrorKind::Io => Some((MsgKind::MiscIo, MsgKind::HelpCheckSyntax)),
            MiscErrorKind::Internal => Some((MsgKind::MiscInternal, MsgKind::HelpCheckSyntax)),
        },
        ErrorKind::Unreachable => Some((MsgKind::Unreachable, MsgKind::HelpCheckSyntax)),
        ErrorKind::Todo => Some((MsgKind::Todo, MsgKind::HelpCheckSyntax)),
        ErrorKind::Deprecated => Some((MsgKind::Deprecated, MsgKind::HelpCheckSyntax)),
    }
}

#[derive(Debug, Clone)]
pub struct TomlLocalizer {
    current: HashMap<String, String>,
    fallback: HashMap<String, String>,
}

impl TomlLocalizer {
    pub fn new(fallback_toml: &str, current_toml: &str) -> Result<Self, toml::de::Error> {
        let fallback: HashMap<String, String> = toml::from_str(fallback_toml)?;
        let current: HashMap<String, String> = toml::from_str(current_toml)?;
        Ok(Self { current, fallback })
    }

    fn get_template(&self, kind: MsgKind) -> &str {
        let key = format!("{:?}", kind);
        self.current
            .get(&key)
            .or_else(|| self.fallback.get(&key))
            .map(|s| s.as_str())
            .unwrap_or("(missing message)")
    }
}

impl Localizer for TomlLocalizer {
    fn translate(&self, kind: MsgKind, args: &[String]) -> String {
        let template = self.get_template(kind);
        let mut result = template.to_string();
        for (i, arg) in args.iter().enumerate() {
            let placeholder = format!("{{{}}}", i);
            result = result.replace(&placeholder, arg);
        }
        result
    }
}

pub const DEFAULT_EN_TOML: &str = r#"
LexerUnexpectedEof = "unexpected end of file"
LexerInvalidString = "invalid string literal"
LexerInvalidIndent = "invalid indentation"
LexerInvalidChar = "invalid char: `{0}`"

TokenPassInvalidPreprocessorParameterDeclare = "invalid preprocessor parameter declaration"
TokenPassInvalidPreprocessorArgumentList = "invalid preprocessor argument list"
TokenPassUserPreprocessorPanic = "user preprocessor panic: {0}"
TokenPassInvalidIdentToString = "invalid identifier to string conversion"
TokenPassInvalidIdentConcat = "invalid identifier concatenation"
TokenPassInvalidPreprocessorSyntax = "invalid preprocessor syntax"
TokenPassInvalidMacroExpand = "invalid macro expansion"

ParserTokenExpect = "expected {0}, found {1}"
ParserInvalidTopDeclaration = "invalid top-level declaration"
ParserInvalidImportList = "invalid import list"
ParserInvalidOnlyList = "invalid only list"
ParserInvalidUseDeclaration = "invalid use declaration"
ParserFunctionDeclarationMissingParameterList = "function declaration missing parameter list"
ParserInvalidGenericList = "invalid generic list"
ParserInvalidFunctionParameterList = "invalid function parameter list"
ParserInvalidFunctionBody = "invalid function body"
ParserInvalidGenericParameterList = "invalid generic parameter list"
ParserInvalidWhereBody = "invalid where body"
ParserWhereBodyGenericMissingMatchGenericParameterList = "where clause generic mismatch"
ParserInvalidTypeDeclaration = "invalid type declaration"
ParserInvalidTupleLiteral = "invalid tuple literal"
ParserInvalidExpression = "invalid expression"
ParserInvalidOperator = "invalid operator"
ParserInvalidCallArgumentList = "invalid call argument list"
ParserInvalidTupleElement = "invalid tuple element"
ParserInvalidFunctionType = "invalid function type"
ParserInvalidStructInit = "invalid struct init"
ParserInvalidPattern = "invalid pattern"
ParserInvalidCatch = "invalid catch clause"
ParserInvalidTypeOf = "invalid typeof expression"

NamePassUndefinedName = "undefined name: `{0}`"
NamePassDuplicateDefinition = "name `{0}` already defined"
NamePassUndefinedModule = "undefined module: `{0}`"
NamePassInvalidMemberAccess = "invalid member access"
NamePassInvalidADTConstructor = "invalid ADT constructor"

TypeCheckerDuplicateType = "duplicate type"
TypeCheckerInfiniteType = "infinite type"
TypeCheckerTypeMismatch = "type mismatch: expected `{0}`, found `{1}`"
TypeCheckerGenericArityMismatch = "generic arity mismatch"
TypeCheckerArityMismatch = "arity mismatch"
TypeCheckerUndefinedVariable = "undefined variable: `{0}`"
TypeCheckerTypeNotChecked = "type not checked yet"
TypeCheckerFieldNotFound = "field not found: `{0}`"
TypeCheckerInternalError = "internal type error"
TypeCheckerUnknownField = "unknown field: `{0}`"
TypeCheckerMissingTypeAnnotation = "missing type annotation"
TypeCheckerUndefinedType = "undefined type: `{0}`"
TypeCheckerRecursiveTypeAlias = "recursive type alias"
TypeCheckerMissingResume = "missing resume in handler"
TypeCheckerInvalidControlType = "invalid control type"
TypeCheckerUnreachablePattern = "unreachable pattern"
TypeCheckerNonExhaustiveMatch = "non-exhaustive match"
TypeCheckerMultipleResume = "multiple resume statements in handler"

HirLowerEmptyPath = "empty path"
HirLowerPathNotFound = "path not found"
HirLowerModuleScopeNotFound = "module scope not found"
HirLowerFieldNotFound = "field not found: `{0}`"
HirLowerConstructorNotFound = "constructor not found: `{0}`"
HirLowerControlNotFound = "control not found: `{0}`"
HirLowerInvalidPath = "invalid path"
HirLowerGenericNotFound = "generic parameter not found: `{0}`"
HirLowerBindingNotFound = "binding not found: `{0}`"
HirLowerParamNotFound = "parameter not found: `{0}`"
HirLowerMethodNotFound = "method not found: `{0}`"
HirLowerCtorNotFound = "constructor definition not found: `{0}`"
HirLowerNameNotFound = "name not found: `{0}`"
HirLowerLetNameNotFound = "let name not found: `{0}`"
HirLowerArityMismatch = "arity mismatch"
HirLowerSymbolNotFound = "symbol not found"
HirLowerMissingArguments = "missing arguments"
HirLowerTooManyArguments = "too many arguments"
HirLowerUnexpectedKeywordArg = "unexpected keyword argument"
HirLowerDuplicateKeywordArg = "duplicate keyword argument"
HirLowerCannotResolveFunction = "cannot resolve function for named arguments"
HirLowerArgumentConflict = "argument conflict"
HirLowerInvalidPipeLineTarget = "invalid pipeline target"

MirLowerGeneric = "MIR lowering error"
MirConstEvalFailed = "constant evaluation failed"
MirLifetimeGeneric = "lifetime error"
MirMonoGeneric = "monomorphization error"

CodegenGeneric = "code generation error"

MiscIo = "I/O error"
MiscInternal = "internal error"
Unreachable = "reached unreachable code"
Todo = "not yet implemented"
Deprecated = "deprecated feature"

HelpCheckSyntax = "check the syntax and try again"
ContextMovedHere = "value moved here"
HelpUseClone = "consider using `.clone()`"
WarningUnusedVariable = "unused variable: `{0}`"
WarningDeprecated = "deprecated"
WarningUnreachableCode = "unreachable code"
DefineStackFrom = "from"
DefineModule = "module"
DefineFunction = "function"
DefineStruct = "struct"
DefineAdt = "ADT"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    struct TestLocalizer;

    impl Localizer for TestLocalizer {
        fn translate(&self, _kind: MsgKind, _args: &[String]) -> String {
            "test message".to_string()
        }
    }

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn render(content: &str, span: Span, ctx: Option<Context>) -> String {
        let mut pool = SourcePool(Vec::new());
        let id = pool.add_source("test.leaf".to_string(), content.to_string());
        assert_eq!(id, span.source_id);
        let map = SourceMap::new(pool);
        let mut diag = DiagCtx::new(map, Box::new(TestLocalizer), DiagColorConfig::default());
        let err = DiagError::new(
            ErrorKind::CompileTimeError(CompileTimeErrorKind::LexerError(
                LexerErrorKind::InvalidChar('~'),
            )),
            span,
            LocalizedMessage::new(MsgKind::LexerInvalidChar, ["~"]),
        );
        let err = match ctx {
            Some(ctx) => err.with_context(ctx),
            None => err,
        };
        diag.collector.errors.push(err);
        diag.emit_all()
    }

    fn caret_column(content: &str, span: Span, ctx: Option<Context>) -> (usize, usize) {
        let report = render(content, span.clone(), ctx);
        let lines: Vec<String> = report.lines().map(strip_ansi).collect();
        let (_line_no, line_content, col) = {
            let mut pool = SourcePool(Vec::new());
            pool.add_source("test.leaf".to_string(), content.to_string());
            SourceMap::new(pool)
                .get_line_info(span.source_id, span.start_off)
                .unwrap()
        };
        let content = line_content.trim_end();
        let source_idx = lines
            .iter()
            .position(|l| l.contains('|') && l.contains(content))
            .expect("source line not rendered");
        let content_start = lines[source_idx].find(content).unwrap();
        let target_col = content_start + line_content[..col].chars().count();
        let caret_idx = lines
            .iter()
            .skip(source_idx + 1)
            .find(|l| l.contains('^'))
            .expect("caret line not rendered");
        (caret_idx.find('^').unwrap(), target_col)
    }

    #[test]
    fn caret_aligns_after_ansi_prefix() {
        let content = "let x = 1\n    a~`\n";
        let start = "let x = 1\n".len() + 4 + 1;
        let span = Span { source_id: 0, start_off: start, end_off: start + 1 };
        let (caret_col, target_col) = caret_column(content, span, None);
        assert_eq!(caret_col, target_col, "caret drifted from offending char");
    }

    #[test]
    fn caret_aligns_with_multibyte_prefix() {
        let content = "变量~字\n";
        let start = "变量".len();
        let span = Span { source_id: 0, start_off: start, end_off: start + 1 };
        let (caret_col, target_col) = caret_column(content, span, None);
        assert_eq!(caret_col, target_col, "caret drifted with multibyte chars before it");
    }

    #[test]
    fn context_caret_aligns() {
        let content = "    变量~\n";
        let start = 5 + "变量".len();
        let span = Span { source_id: 0, start_off: start, end_off: start + 1 };
        let ctx = Context::new(
            ContextKind::Context { tokens: Vec::new() },
            LocalizedMessage::new(MsgKind::ContextLabel, ["x"]),
            span.clone(),
        );
        let (caret_col, target_col) = caret_column(content, span, Some(ctx));
        assert_eq!(caret_col, target_col, "context caret drifted");
    }
}