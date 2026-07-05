use std::fmt::Debug;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Document, DocumentString, LexerApi, LexerError, Token, TokenStream, TokenType};
use leafc_coreapi::lexer::LexerError::{InvalidChar, InvalidString};
use leafc_coreapi::source::{Pos, SourceId, Span};

pub enum LexerState {
    Start,
    Ident,
    Number,
    String,
    Symbol,
    LineStart
}

const INDENT_WIDTH: usize = 4;

pub struct Lexer {
    index: usize,
    column: usize,
    lineno: usize,
    source: SourceId,
    code: Vec<char>,
    indent_level: isize,
    docstrings: Document,
}

impl Lexer {
    fn current_pos(&self) -> Pos {
        Pos {
            column : self.column,
            lineno : self.lineno
        }
    }

    fn eof(&self) -> Token {
        Token {
            kind: TokenType::Eof,
            span: Span {
                start: Pos { column: self.column, lineno: self.lineno },
                end:  Pos { column: self.column, lineno: self.lineno },
            },
            text: "".to_string(),
            source: self.source,
        }
    }

    fn keyword_map(&self, s: &String) -> TokenType {
        match s.as_str() {
            "use" => TokenType::KwUse,
            "or" => TokenType::KwOr,
            "and" => TokenType::KwAnd,
            "not" => TokenType::KwNot,
            "as" => TokenType::KwAs,
            "fun" => TokenType::KwFun,
            "return" => TokenType::KwReturn,
            "symdef" => TokenType::KwSymDef,
            "symexpr" => TokenType::KwSymExpr,
            "abst" => TokenType::KwAbst,
            "mut" => TokenType::KwMut,
            "let" => TokenType::KwLet,
            "const" => TokenType::KwConst,
            "bindto" => TokenType::KwBindTo,
            "move" => TokenType::KwMove,
            "copy" => TokenType::KwCopy,
            "do" => TokenType::KwDo,
            "it" => TokenType::KwIt,
            "shared" => TokenType::KwShared,
            "if" => TokenType::KwIf,
            "then" => TokenType::KwThen,
            "else" => TokenType::KwElse,
            "elif" => TokenType::KwElif,
            "when" => TokenType::KwWhen,
            "guard" => TokenType::KwGuard,
            "handle" => TokenType::KwHandle,
            "effect" => TokenType::KwEffect,
            "catch" => TokenType::KwCatch,
            "resume" => TokenType::KwResume,
            "raise" => TokenType::KwRaise,
            "external" => TokenType::KwExternal,
            "ctype" => TokenType::KwCType,
            "pub" => TokenType::KwPub,
            "unsafe_call_external" => TokenType::KwUnsafeCallExternal,
            "type" => TokenType::KwType,
            "where" => TokenType::KwWhere,
            "no" => TokenType::KwNo,
            "only" => TokenType::KwOnly,
            "impl" => TokenType::KwImpl,
            "for" => TokenType::KwFor,
            "subtype" => TokenType::KwSubType,
            "basetype" => TokenType::KwBaseType,
            _ => TokenType::Error,
        }
    }

    fn symbol_map(&self, s: &String) -> TokenType {
        match s.as_str() {
            "+" => TokenType::Plus,
            "-" => TokenType::Minus,
            "*" => TokenType::Star,
            "/" => TokenType::Slash,
            "%" => TokenType::Percent,
            "&" => TokenType::Amp,
            "|" => TokenType::Pipe,
            "^" => TokenType::Caret,
            "!" => TokenType::Not,
            "=" => TokenType::Eq,
            "==" => TokenType::EqEq,
            "!=" => TokenType::Ne,
            "<" => TokenType::Lt,
            ">" => TokenType::Gt,
            "<=" => TokenType::Le,
            ">=" => TokenType::Ge,
            "&&" => TokenType::And,
            "||" => TokenType::Or,
            "<<" => TokenType::Shl,
            ">>" => TokenType::Shr,
            "+=" => TokenType::PlusEq,
            "-=" => TokenType::MinusEq,
            "*=" => TokenType::StarEq,
            "/=" => TokenType::SlashEq,
            "%=" => TokenType::PercentEq,
            "&=" => TokenType::AmpEq,
            "|=" => TokenType::PipeEq,
            "^=" => TokenType::CaretEq,
            "<<=" => TokenType::ShlEq,
            ">>=" => TokenType::ShrEq,
            "->" => TokenType::Arrow,
            "=>" => TokenType::FatArrow,
            "." => TokenType::Dot,
            ".." => TokenType::DotDot,
            "..." => TokenType::DotDotDot,
            "(" => TokenType::Lparen,
            ")" => TokenType::Rparen,
            "{" => TokenType::Lbrace,
            "}" => TokenType::Rbrace,
            "[" => TokenType::Lbracket,
            "]" => TokenType::Rbracket,
            "," => TokenType::Comma,
            ":" => TokenType::Colon,
            ";" => TokenType::Semicolon,
            "#" => TokenType::Hash,
            "@" => TokenType::At,
            "_" => TokenType::Underline,
            _ => TokenType::Error,
        }
    }

    fn current_char(&self) -> Option<char> {
        self.code.get(self.index).copied()
    }

    fn next_char(&mut self) -> () {
        if self.index >= self.code.len() { return; }
        self.index += 1;
        self.column += 1;
    }

    fn main_loop(&mut self, tokens: &mut Vec<Token>) -> Result<(), DiagMsg> {
        let mut state = LexerState::Start;
        loop {
            let c = self.current_char();
            match state {
                LexerState::Start => {
                    match c {
                        None => return Ok(()),
                        Some('\n') => {
                            state = LexerState::LineStart;
                            continue;
                        }
                        Some(' ') => {
                            self.next_char();
                            continue;
                        }
                        Some('"') => {
                            state = LexerState::String;
                            continue;
                        }
                        Some(ch) => {
                            if ch.is_ascii_digit() {
                                state = LexerState::Number;
                            }
                            else if ch.is_ascii_alphabetic() || ch == '_' {
                                state = LexerState::Ident;
                            } else if ch == '/'
                                && self.index+1 < self.code.len()
                                && self.code[self.index+1] == '/' {

                                self.next_char();
                                self.next_char();

                                if self.code[self.index] == '/' {
                                    // 文档字符串
                                    let mut docstring = String::new();

                                    self.next_char();
                                     let start_pos = self.current_pos();
                                     while self.index < self.code.len()
                                         && self.code[self.index] != '\n' {
                                         docstring.push(self.code[self.index]);
                                         self.next_char();
                                     }
                                    self.docstrings.data.push(DocumentString {
                                        span: Span { start: start_pos, end: self.current_pos(), },
                                        data: docstring,
                                    })
                                } else {
                                    // 普通注释
                                    while self.index < self.code.len()
                                        && self.code[self.index] != '\n' {
                                        self.next_char();
                                    }
                                }

                            } else if matches!(ch,
                                '+' | '-' | '*' | '/' | '%' | '&'
                                | '|' | '^' | '!' | '=' | '<' | '>'
                                | '.' | '(' | ')' | '{' | '}' | '['
                                | ',' | ':' | ';' | '#' | '@' | '_'  | ']'
                            ) {
                                state = LexerState::Symbol;
                            } else {
                                return Err(DiagMsg {
                                    title : format!("{:?}", InvalidChar),
                                    msg : format!("Invalid char '{}'", ch),
                                    span : Span { start: self.current_pos(), end: self.current_pos() },
                                    source: self.source,
                                });
                            }
                            continue;
                        }
                    };
                }
                LexerState::String => {
                    let start_pos = self.current_pos();
                    self.next_char();
                    let mut closed = false;
                    let mut text = String::new();
                    while self.index < self.code.len() {
                        let c = self.code.get(self.index).unwrap();
                        if * c == '"' {
                            closed = true;
                            self.next_char();
                            break;
                        }
                        text.push(*c);
                        self.next_char();
                    }
                    if ! closed {
                        return Err( DiagMsg {
                            title : format!("{:?}", InvalidString),
                            msg : "Unclosed string literal".to_string(),
                            span : Span { start: start_pos, end: self.current_pos() },
                            source: self.source,
                        });
                    }
                    tokens.push(
                        Token {
                            kind: TokenType::String,
                            span: Span { start: start_pos, end:  self.current_pos(), },
                            text,
                            source: self.source,
                        }
                    );
                    state = LexerState::Start;
                }
                LexerState::Number => {
                    let start_pos = self.current_pos();
                    let mut text = String::new();
                    let mut is_float = false;
                    while self.index < self.code.len() {
                        let c = self.code.get(self.index).unwrap();
                        if c.is_ascii_digit() {
                            text.push(*c);
                            self.next_char();
                        } else if * c == '.'{
                            is_float = true;
                            text.push(*c);
                            self.next_char();
                        } else { break; }
                    }
                    tokens.push(
                        if is_float {
                            Token {
                                kind: TokenType::Float,
                                span: Span { start: start_pos, end:  self.current_pos(), },
                                text: text,
                                source: self.source,
                            }
                        } else {
                            Token {
                                kind: TokenType::Int,
                                span: Span { start: start_pos, end:  self.current_pos(), },
                                text: text,
                                source: self.source,
                            }
                        }
                    );
                    state = LexerState::Start;

                }
                LexerState::Ident => {
                    let start_pos = self.current_pos();
                    let mut text = String::new();
                    while self.index < self.code.len() {
                        let c = self.code.get(self.index).unwrap();
                        if c.is_ascii_alphabetic()
                            || c.is_ascii_digit()
                            || *c == '_'{
                            text.push(*c);
                            self.next_char()
                        } else { break; }
                    }
                    let try_keyword = self.keyword_map(&text);
                    if try_keyword != TokenType::Error {
                        tokens.push(
                            Token {
                                kind: try_keyword,
                                span: Span { start: start_pos, end:  self.current_pos(), },
                                text,
                                source: self.source,
                            });
                        state = LexerState::Start;
                    } else {
                        tokens.push(
                            Token {
                                kind: TokenType::Ident,
                                span: Span { start: start_pos,  end:  self.current_pos(), },
                                text,
                                source: self.source,
                            });
                        state = LexerState::Start;
                    }
                }
                LexerState::Symbol => {
                    let start = self.current_pos();
                    let mut text = String::new();
                    let mut matched_text = String::new();    // 最后一次成功匹配的符号文本
                    let mut token_type = TokenType::Error;   // 最后一次成功匹配的 Token 类型

                    // 不断尝试扩展符号
                    while self.index < self.code.len() {
                        let c = self.code[self.index];
                        text.push(c);

                        let t = self.symbol_map(&text);
                        if t != TokenType::Error {
                            token_type = t;
                            matched_text = text.clone();
                            self.next_char();
                        } else {
                            break;
                        }
                    }

                    if token_type == TokenType::Error {
                        let err_char = text.chars().next().unwrap(); // 取第一个字符
                        self.next_char(); // 消费该字符
                        tokens.push(Token {
                            kind: TokenType::Error,
                            span: Span { start, end: self.current_pos(), },
                            text: err_char.to_string(),
                            source: self.source,
                        });
                        state = LexerState::Start;
                    } else {
                        // 生成匹配到的符号 Token
                        tokens.push(Token {
                            kind: token_type,
                            span: Span { start, end: self.current_pos(), },
                            text: matched_text,
                            source: self.source,
                        });
                        state = LexerState::Start;
                    }
                }
                LexerState::LineStart => {
                    let last_line_pos = self.current_pos();
                    self.next_char(); // consume '\n'
                    self.lineno += 1;
                    self.column = 1;
                    tokens.push(Token {
                        kind: TokenType::NewLine,
                        span: Span { start: last_line_pos.clone(), end: last_line_pos },
                        source : self.source,
                        text: "\n".to_string(),
                    });

                    let start_pos = self.current_pos();
                    let mut text = String::new();
                    while self.index < self.code.len() {
                        let c = self.code[self.index];
                        if c == ' ' { // 明确只接受空格，或处理制表符
                            text.push(c);
                            self.next_char();
                        } else if c == '\t' {
                            // 定义制表符宽度，例如替换为4个空格
                            text.push_str("    ");
                            self.next_char();
                        } else {
                            break;
                        }
                    }
                    let leading_space_width = text.len();

                    // 忽略空行
                    if self.index >= self.code.len() || self.code[self.index] == '\n' || self.code[self.index] == '\r' {
                        state = LexerState::Start;
                        continue;
                    }

                    if leading_space_width % INDENT_WIDTH != 0 {
                        return Err(DiagMsg {
                            title : format!("{:?}", LexerError::InvalidIndent),
                            msg : "invalid indent".to_string(),
                            span : Span { start: start_pos, end: self.current_pos() },
                            source: self.source,
                        });
                    }

                    let new_level = leading_space_width / INDENT_WIDTH;

                    while new_level > self.indent_level as usize {
                        self.indent_level += 1;
                        tokens.push(Token {
                            kind: TokenType::Indent,
                            span: Span { start: start_pos.clone(), end:  self.current_pos() },
                            source: self.source,
                            text: text.clone(), });
                    }
                    while new_level < self.indent_level as usize {
                        self.indent_level -= 1;
                        tokens.push(Token {
                            kind: TokenType::Dedent,
                            span: Span { start: start_pos.clone(), end:  self.current_pos() },
                            source: self.source,
                            text: text.clone(),  });
                    }

                    state = LexerState::Start;
                }
            }
        }
    }
}

impl LexerApi for Lexer {
    fn new(source: SourceId, text: String, ) -> Self {
        let code = text.chars().collect();
        Lexer {
            index: 0,
            column: 1,
            lineno: 1,
            source,
            code,
            indent_level: 0,
            docstrings: Document { data: Vec::new() },
        }
    }

    fn tokenize(&mut self) -> Result<TokenStream, DiagMsg> {
        let mut tokens = Vec::new();

        self.main_loop(&mut tokens)?;

        for _ in 0 .. self.indent_level {
            tokens.push(Token {
                kind: TokenType::Dedent,
                span: Span {
                    start: self.current_pos(),
                    end:  self.current_pos(),
                },
                text: "".to_string(),
                source: self.source,
            });
        }

        // EOF
        tokens.push(self.eof());

        Ok(TokenStream { data: tokens })
    }

    fn get_document_strings(&self) -> &Document {
        &self.docstrings
    }
}