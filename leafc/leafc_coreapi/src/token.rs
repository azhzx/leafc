use crate::source::Span;

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
    KwShare,
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
    UserOp,
    KwGlobal,
    KwBinding,
    KwWith,
    KwIs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenType,
    pub span: Span,
    pub text: String
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiteToken {
    pub kind: TokenType,
    pub text: String
}