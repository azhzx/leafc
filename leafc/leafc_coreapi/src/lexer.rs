use crate::diagnostic::DiagMsg;
use crate::source::{SourceId, Span};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TokenType {
    KwUse,
    KwOr,
    KwAnd,
    KwNot,
    KwAs,
    KwFun,
    KwReturn,
    KwSymDef,
    KwSymExpr,
    KwAbst,
    KwMut,
    KwLet,
    KwConst,
    KwBindTo,
    KwMove,
    KwCopy,
    KwDo,
    KwIt,
    KwShared,
    KwIf,
    KwThen,
    KwElse,
    KwElif,
    KwWhen,
    KwGuard,
    KwHandle,
    KwEffect,
    KwCatch,
    KwResume,
    KwRaise,
    KwExternal,
    KwCType,
    KwPub,
    KwUnsafeCallExternal,
    KwType,
    KwNo,
    KwWhere,
    KwOf,
    KwOnly,
    KwImpl,
    KwFor,
    KwRef,
    KwSubType,
    KwBaseType,

    Ident,
    Int,
    Float,
    String,

    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /
    Percent,        // %
    Amp,            // &
    Pipe,           // |
    Caret,          // ^
    Not,            // !
    Eq,             // =
    EqEq,           // ==
    Ne,             // !=
    Lt,             // <
    Gt,             // >
    Le,             // <=
    Ge,             // >=
    And,            // &&
    Or,             // ||
    Shl,            // <<
    Shr,            // >>
    PlusEq,         // +=
    MinusEq,        // -=
    StarEq,         // *=
    SlashEq,        // /=
    PercentEq,      // %=
    AmpEq,          // &=
    PipeEq,         // |=
    CaretEq,        // ^=
    ShlEq,          // <<=
    ShrEq,          // >>=
    Arrow,          // ->
    FatArrow,       // =>
    Dot,            // .
    DotDot,         // ..
    DotDotDot,      // ...
    Lparen,         // (
    Rparen,         // )
    Lbrace,         // {
    Rbrace,         // }
    Lbracket,       // [
    Rbracket,       // ]
    Comma,          // ,
    Colon,          // :
    Semicolon,      // ;
    Hash,           // #
    At,             // @
    Underline,      // _

    Eof,
    NewLine,
    Indent,
    Dedent,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenType,
    pub span: Span,
    pub source: SourceId,
    pub text: String
}

#[derive(Debug)]
pub struct TokenStream {
    pub data: Vec<Token>
}

pub struct DocumentString {
    pub span: Span,
    pub data: String
}

pub struct Document {
    pub data: Vec<DocumentString>
}

#[derive(Debug)]
pub enum LexerError {
    UnexpectedEof,
    InvalidString,
    InvalidIndent,
    InvalidChar,
}

pub trait LexerApi {
    fn new(source: SourceId, text: &String) -> Self;
    fn tokenize(&mut self)
        -> Result<TokenStream, DiagMsg>;
    fn get_document_strings(&self) -> &Document;
}