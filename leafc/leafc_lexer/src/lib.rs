use std::collections::{HashMap, HashSet};
use leafc_coreapi::crate_meta::OperatorDef;
use unicode_xid::UnicodeXID;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Document, DocumentString, LexerApi, LexerError, Token, TokenStream, TokenType};
use leafc_coreapi::lexer::LexerError::{InvalidChar, InvalidString};
use leafc_coreapi::source::{SourceId, Span};

pub enum LexerState {
    Start,
    Ident,
    Number,
    String,
    Symbol,
    LineStart
}

const INDENT_WIDTH: usize = 4;

pub struct Lexer<'a> {
    index: usize,
    byte_index: usize,
    source: SourceId,
    code: Vec<char>,
    indent_level: isize,
    docstrings: Document,

    operator_table: HashMap<String, TokenType>,
    operator_prefixes: HashSet<String>,
    user_operators: &'a HashMap<String, OperatorDef>
}

impl<'a> Lexer<'a> {
    fn current_offset(&self) -> usize {
        self.byte_index
    }

    fn eof(&self) -> Token {
        let off = self.current_offset();
        Token {
            kind: TokenType::Eof,
            span: Span {
                source_id: self.source,
                start_off: off,
                end_off: off,
            },
            text: "".to_string(),
        }
    }

    fn keyword_map(&self, s: &String) -> TokenType {
        match s.as_str() {
            "use" => TokenType::KwUse,
            "of" => TokenType::KwOf,
            "ref" => TokenType::KwRef,
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

    fn current_char(&self) -> Option<char> {
        self.code.get(self.index).copied()
    }

    fn next_char(&mut self) -> () {
        if let Some(&ch) = self.code.get(self.index) {
            self.index += 1;
            self.byte_index += ch.len_utf8();
        }
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
                        Some('\r') => {
                            self.next_char();
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
                            else if ch == '_' || ch.is_xid_start() {
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
                                    let start_offset = self.current_offset();
                                    while self.index < self.code.len()
                                        && self.code[self.index] != '\n' {
                                        docstring.push(self.code[self.index]);
                                        self.next_char();
                                    }
                                    self.docstrings.data.push(DocumentString {
                                        span: Span {
                                            source_id: self.source,
                                            start_off: start_offset,
                                            end_off: self.current_offset(),
                                        },
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
                                let off = self.current_offset();
                                return Err(DiagMsg {
                                    title : format!("{:?}", InvalidChar),
                                    msg : format!("Invalid char '{}'", ch),
                                    span : Span {
                                        source_id: self.source,
                                        start_off: off,
                                        end_off: off
                                    },
                                });
                            }
                            continue;
                        }
                    };
                }
                LexerState::String => {
                    let start_offset = self.current_offset();
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
                            span : Span {
                                start_off: start_offset,
                                end_off: self.current_offset(),
                                source_id: self.source
                            },
                        });
                    }
                    tokens.push(
                        Token {
                            kind: TokenType::String,
                            span: Span {
                                start_off: start_offset,
                                end_off: self.current_offset(),
                                source_id: self.source
                            },
                            text,
                        }
                    );
                    state = LexerState::Start;
                }
                LexerState::Number => {
                    let start_offset = self.current_offset();
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
                        Token {
                            kind: if is_float { TokenType::Float } else { TokenType::Int },
                            span: Span {
                                source_id: self.source,
                                start_off: start_offset,
                                end_off: self.current_offset()
                            },
                            text,
                        }
                    );
                    state = LexerState::Start;

                }
                LexerState::Ident => {
                    let start_offset = self.current_offset();
                    let mut text = String::new();
                    while self.index < self.code.len() {
                        let c = self.code.get(self.index).unwrap();
                        if c.is_xid_continue() {
                            text.push(*c);
                            self.next_char()
                        } else { break; }
                    }
                    let try_keyword = self.keyword_map(&text);
                    if try_keyword != TokenType::Error {
                        tokens.push(
                            Token {
                                kind: try_keyword,
                                span: Span {
                                    source_id: self.source,
                                    start_off: start_offset,
                                    end_off: self.current_offset()
                                },
                                text,
                            });
                        state = LexerState::Start;
                    } else {
                        tokens.push(
                            Token {
                                kind: TokenType::Ident,
                                span: Span {
                                    source_id: self.source,
                                    start_off: start_offset,
                                    end_off: self.current_offset()
                                },
                                text,
                            });
                        state = LexerState::Start;
                    }
                }
                LexerState::Symbol => {
                    let start_offset = self.current_offset();
                    let mut text = String::new();
                    let mut matched_text = String::new();
                    let mut token_type = TokenType::Error;

                    loop {
                        let c = match self.current_char() {
                            Some(ch) => ch,
                            None => break,
                        };

                        text.push(c);

                        if let Some(tt) = self.operator_table.get(&text) {
                            matched_text = text.clone();
                            token_type = tt.clone();
                        }

                        if !self.operator_prefixes.contains(&text) {
                            break;
                        }

                        self.next_char();
                    }

                    if token_type == TokenType::Error {
                        let ch = text.chars().next().unwrap();
                        self.next_char(); // 至少消费一个字符, 防止死循环
                        return Err(DiagMsg {
                            title: format!("{:?}", InvalidChar),
                            msg: format!("Invalid character '{}'", ch),
                            span: Span {
                                source_id: self.source,
                                start_off: start_offset,
                                end_off: self.current_offset(),
                            },
                        });
                    }

                    tokens.push(Token {
                        kind: token_type,
                        span: Span {
                            source_id: self.source,
                            start_off: start_offset,
                            end_off: self.current_offset(),
                        },
                        text: matched_text,
                    });
                    state = LexerState::Start;
                }
                LexerState::LineStart => {
                    let last_line_byte = self.current_offset();
                    self.next_char(); // consume '\n'
                    tokens.push(Token {
                        kind: TokenType::NewLine,
                        span: Span {
                            source_id: self.source,
                            start_off: last_line_byte,
                            end_off: last_line_byte },
                        text: "\n".to_string(),
                    });

                    let start_offset = self.current_offset();
                    let mut text = String::new();
                    while self.index < self.code.len() {
                        let c = self.code[self.index];
                        if c == ' ' {
                            text.push(c);
                            self.next_char();
                        } else if c == '\t' {
                            // 替换为4个空格
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
                            span : Span {
                                source_id: self.source,
                                start_off: start_offset,
                                end_off: self.current_offset() },
                        });
                    }

                    let new_level = leading_space_width / INDENT_WIDTH;

                    while new_level > self.indent_level as usize {
                        self.indent_level += 1;
                        tokens.push(Token {
                            kind: TokenType::Indent,
                            span: Span {
                                source_id: self.source,
                                start_off: start_offset,
                                end_off: self.current_offset()
                            },
                            text: text.clone(), });
                    }
                    while new_level < self.indent_level as usize {
                        self.indent_level -= 1;
                        tokens.push(Token {
                            kind: TokenType::Dedent,
                            span: Span {
                                source_id: self.source,
                                start_off: start_offset,
                                end_off: self.current_offset()
                            },
                            text: text.clone(),  });
                    }

                    state = LexerState::Start;
                }
            }
        }
    }
}

impl<'a> LexerApi<'a> for Lexer<'a> {
    fn new(
        source: SourceId,
        text: &String,
        user_operators: &'a HashMap<String, OperatorDef>,
    ) -> Self {
        let code = text.chars().collect();

        let builtin_ops: &[(&str, TokenType)] = &[
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

        let mut operator_table = HashMap::new();
        for (text, tt) in builtin_ops {
            operator_table.insert(text.to_string(), tt.clone());
        }

        for def in user_operators.values() {
            operator_table.insert(def.text.clone(), TokenType::UserOp);
        }

        // 构建前缀集合
        let mut operator_prefixes = HashSet::new();
        for key in operator_table.keys() {
            for end in 1..=key.len() {
                operator_prefixes.insert(key[..end].to_string());
            }
        }

        Lexer {
            index: 0,
            byte_index: 0,
            source,
            code,
            indent_level: 0,
            docstrings: Document { data: Vec::new() },
            operator_table,
            operator_prefixes,
            user_operators: user_operators,
        }
    }

    fn tokenize(&mut self) -> Result<TokenStream, DiagMsg> {
        let mut tokens = Vec::new();

        self.main_loop(&mut tokens)?;

        let off = self.current_offset();
        for _ in 0..self.indent_level {
            tokens.push(Token {
                kind: TokenType::Dedent,
                span: Span {
                    source_id: self.source,
                    start_off: off,
                    end_off: off,
                },
                text: "".to_string(),
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