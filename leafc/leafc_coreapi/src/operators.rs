use std::collections::{HashMap, HashSet};
use crate::crate_meta::OperatorDef;
use crate::lexer::TokenType;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Operator {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Not,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    UserOp(String),
}

impl Operator {
    pub fn text_len(&self) -> usize {
        match self {
            Operator::UserOp(s) => s.len(),
            _ => self.keyword().len(),
        }
    }

    pub fn keyword(&self) -> &str {
        match self {
            Operator::Add => "+",
            Operator::Sub => "-",
            Operator::Mul => "*",
            Operator::Div => "/",
            Operator::Mod => "%",
            Operator::And => "&&",
            Operator::Or => "||",
            Operator::Not => "!",
            Operator::Eq => "==",
            Operator::Neq => "!=",
            Operator::Lt => "<",
            Operator::Gt => ">",
            Operator::Le => "<=",
            Operator::Ge => ">=",
            Operator::UserOp(s) => s.as_str(),
        }
    }
}

pub fn token_type_to_operator(tt: &TokenType) -> Option<Operator> {
    match tt {
        TokenType::Plus => Some(Operator::Add),
        TokenType::Minus => Some(Operator::Sub),
        TokenType::Star => Some(Operator::Mul),
        TokenType::Slash => Some(Operator::Div),
        TokenType::Percent => Some(Operator::Mod),
        TokenType::And => Some(Operator::And),
        TokenType::Or => Some(Operator::Or),
        TokenType::Not => Some(Operator::Not),
        TokenType::EqEq => Some(Operator::Eq),
        TokenType::Ne => Some(Operator::Neq),
        TokenType::Lt => Some(Operator::Lt),
        TokenType::Gt => Some(Operator::Gt),
        TokenType::Le => Some(Operator::Le),
        TokenType::Ge => Some(Operator::Ge),
        _ => None,
    }
}

const BUILTIN_OPS: &[(&str, TokenType)] = &[
    ("+", TokenType::Plus),
    ("-", TokenType::Minus),
    ("*", TokenType::Star),
    ("/", TokenType::Slash),
    ("%", TokenType::Percent),
    ("&", TokenType::Amp),
    ("|", TokenType::Pipe),
    ("^", TokenType::Caret),
    ("!", TokenType::Not),
    ("=", TokenType::Eq),
    ("==", TokenType::EqEq),
    ("!=", TokenType::Ne),
    ("<", TokenType::Lt),
    (">", TokenType::Gt),
    ("<=", TokenType::Le),
    (">=", TokenType::Ge),
    ("&&", TokenType::And),
    ("||", TokenType::Or),
    ("<<", TokenType::Shl),
    (">>", TokenType::Shr),
    ("+=", TokenType::PlusEq),
    ("-=", TokenType::MinusEq),
    ("*=", TokenType::StarEq),
    ("/=", TokenType::SlashEq),
    ("%=", TokenType::PercentEq),
    ("&=", TokenType::AmpEq),
    ("|=", TokenType::PipeEq),
    ("^=", TokenType::CaretEq),
    ("<<=", TokenType::ShlEq),
    (">>=", TokenType::ShrEq),
    ("->", TokenType::Arrow),
    ("=>", TokenType::FatArrow),
    (".", TokenType::Dot),
    ("..", TokenType::DotDot),
    ("...", TokenType::DotDotDot),
    ("(", TokenType::Lparen),
    (")", TokenType::Rparen),
    ("{", TokenType::Lbrace),
    ("}", TokenType::Rbrace),
    ("[", TokenType::Lbracket),
    ("]", TokenType::Rbracket),
    (",", TokenType::Comma),
    (":", TokenType::Colon),
    (";", TokenType::Semicolon),
    ("#", TokenType::Hash),
    ("@", TokenType::At),
    ("_", TokenType::Underline),
];

pub fn build_operator_tables(
    user_operators: &HashMap<String, OperatorDef>
) -> (HashMap<String, TokenType>, HashSet<String>) {

    let mut operator_table = HashMap::new();
    for (text, tt) in BUILTIN_OPS {
        operator_table.insert(text.to_string(), tt.clone());
    }
    for def in user_operators.values() {
        operator_table.insert(def.text.clone(), TokenType::UserOp);
    }
    let mut operator_prefixes = HashSet::new();
    for key in operator_table.keys() {
        for end in 1..=key.len() {
            operator_prefixes.insert(key[..end].to_string());
        }
    }
    (operator_table, operator_prefixes)
}