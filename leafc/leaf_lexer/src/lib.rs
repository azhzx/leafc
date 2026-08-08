#[cfg(test)]
mod test;

use std::collections::{HashMap, HashSet};
use leaf_coreapi::crate_meta::OperatorDef;
use unicode_xid::UnicodeXID;
use leaf_coreapi::diagnose::{CompileTimeErrorKind, DiagCollector, DiagCtx, ErrorKind, LexerErrorKind, LocalizedMessage, MsgKind};
use leaf_coreapi::id::FileId;
use leaf_coreapi::operators::build_operator_tables;
use leaf_coreapi::pass::LexerApi;
use leaf_coreapi::source::{Span};
use leaf_coreapi::token::{Document, DocumentString, Token, TokenStream, TokenType};

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
    byte_index: usize,

    source: FileId,

    code: Vec<char>,

    indent_level: isize,

    docstrings: Document,

    operator_table: HashMap<String, TokenType>,
    operator_prefixes: HashSet<String>,

    diag_collector: DiagCollector,
}

impl Lexer {
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

    fn keyword_map(&self, s: &str) -> TokenType {
        match s {
            "is" => TokenType::KwIs,
            "typeof" => TokenType::KwTypeOf,
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
            "with" => TokenType::KwWith,
            "let" => TokenType::KwLet,
            "const" => TokenType::KwConst,
            "bindto" => TokenType::KwBindTo,
            "binding" => TokenType::KwBinding,
            "move" => TokenType::KwMove,
            "copy" => TokenType::KwCopy,
            "do" => TokenType::KwDo,
            "it" => TokenType::KwIt,
            "global" => TokenType::KwGlobal,
            "share" => TokenType::KwShare,
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


    fn handle_escape(&mut self) -> char {
        let ch = match self.current_char() {
            Some(c) => c,
            None => {
                let span = Span {
                    source_id: self.source,
                    start_off: self.current_offset(),
                    end_off: self.current_offset(),
                };
                self.diag_collector.add_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::LexerError(LexerErrorKind::UnexpectedEof)),
                    span,
                    LocalizedMessage::new(MsgKind::LexerUnexpectedEof, std::iter::empty::<&str>()),
                );
                return '\0';
            }
        };
        let result = match ch {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '\\' => '\\',
            '"' => '\"',
            '0' => '\0',
            _ => {
                let span = Span {
                    source_id: self.source,
                    start_off: self.current_offset() - 1,
                    end_off: self.current_offset(),
                };
                self.emit_unexpected_eof(span);
                self.next_char();
                return ch;
            }
        };
        self.next_char();
        result
    }

    fn current_char(&self) -> Option<char> {
        self.code.get(self.index).copied()
    }

    fn next_char(&mut self) {
        if let Some(&ch) = self.code.get(self.index) {
            self.index += 1;
            self.byte_index += ch.len_utf8();
        }
    }

    fn emit_lexer_error(
        &mut self,
        kind: LexerErrorKind,
        span: Span,
        msg_kind: MsgKind,
        args: impl IntoIterator<Item = impl ToString>,
    ) {
        self.diag_collector.add_error(
            ErrorKind::CompileTimeError(CompileTimeErrorKind::LexerError(kind)),
            span,
            LocalizedMessage::new(msg_kind, args),
        );
    }

    fn emit_unexpected_eof(&mut self, span: Span) {
        self.emit_lexer_error(
            LexerErrorKind::UnexpectedEof,
            span,
            MsgKind::LexerUnexpectedEof,
            std::iter::empty::<&str>(),
        );
    }

    fn emit_invalid_string(&mut self, span: Span, extra: Option<char>) {
        let args = extra.map(|c| vec![c.to_string()]).unwrap_or_default();
        self.emit_lexer_error(
            LexerErrorKind::InvalidString,
            span,
            MsgKind::LexerInvalidString,
            args,
        );
    }

    fn emit_invalid_indent(&mut self, span: Span) {
        self.emit_lexer_error(
            LexerErrorKind::InvalidIndent,
            span,
            MsgKind::LexerInvalidIndent,
            std::iter::empty::<&str>(),
        );
    }

    fn emit_invalid_char(&mut self, span: Span, ch: char) {
        self.emit_lexer_error(
            LexerErrorKind::InvalidChar(ch),
            span,
            MsgKind::LexerInvalidChar,
            std::iter::once(ch.to_string()),
        );
    }

    fn main_loop(&mut self, tokens: &mut Vec<Token>) {
        let mut state = LexerState::Start;
        loop {
            let c = self.current_char();
            match state {
                LexerState::Start => {
                    match c {
                        None => break,
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
                            } else if ch == '_' || ch.is_xid_start() {
                                state = LexerState::Ident;
                            } else if ch == '/'
                                && self.index + 1 < self.code.len()
                                && self.code[self.index + 1] == '/'
                            {
                                self.next_char(); //  '/'
                                self.next_char(); // '/'

                                if self.index < self.code.len() && self.code[self.index] == '/' {
                                    let mut docstring = String::new();
                                    self.next_char(); // '/'
                                    let start_offset = self.current_offset();
                                    while self.index < self.code.len()
                                        && self.code[self.index] != '\n'
                                    {
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
                                    });
                                } else {
                                    while self.index < self.code.len()
                                        && self.code[self.index] != '\n'
                                    {
                                        self.next_char();
                                    }
                                }
                            } else if matches!(
                                ch,
                                '+' | '-' | '*' | '/' | '%' | '&'
                                | '|' | '^' | '!' | '=' | '<' | '>'
                                | '.' | '(' | ')' | '{' | '}' | '['
                                | ',' | ':' | ';' | '#' | '@' | '_' | ']'
                            ) {
                                state = LexerState::Symbol;
                            } else {
                                let off = self.current_offset();
                                let span = Span {
                                    source_id: self.source,
                                    start_off: off,
                                    end_off: off + ch.len_utf8(),
                                };
                                self.diag_collector.add_error(
                                    ErrorKind::CompileTimeError(CompileTimeErrorKind::LexerError(LexerErrorKind::InvalidChar(ch))),
                                    span,
                                    LocalizedMessage::new(MsgKind::LexerInvalidChar, std::iter::once(ch.to_string())),
                                );
                                self.next_char();
                            }
                            continue;
                        }
                    }
                }
                LexerState::String => {
                    let start_offset = self.current_offset();
                    self.next_char(); // '"'
                    let mut closed = false;
                    let mut text = String::new();
                    while self.index < self.code.len() {
                        let c = *self.code.get(self.index).unwrap();
                        if c == '"' {
                            closed = true;
                            self.next_char();
                            break;
                        } else if c == '\\' {
                            self.next_char();
                            text.push(self.handle_escape());
                        } else {
                            text.push(c);
                            self.next_char();
                        }
                    }
                    if !closed {
                        let span = Span {
                            source_id: self.source,
                            start_off: start_offset,
                            end_off: self.current_offset(),
                        };
                        self.emit_invalid_string(span, self.current_char());
                    }
                    tokens.push(Token {
                        kind: TokenType::String,
                        span: Span {
                            start_off: start_offset,
                            end_off: self.current_offset(),
                            source_id: self.source,
                        },
                        text,
                    });
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
                        } else if *c == '.' {
                            is_float = true;
                            text.push(*c);
                            self.next_char();
                        } else {
                            break;
                        }
                    }
                    tokens.push(Token {
                        kind: if is_float { TokenType::Float } else { TokenType::Int },
                        span: Span {
                            source_id: self.source,
                            start_off: start_offset,
                            end_off: self.current_offset(),
                        },
                        text,
                    });
                    state = LexerState::Start;
                }
                LexerState::Ident => {
                    let start_offset = self.current_offset();
                    let mut text = String::new();
                    while self.index < self.code.len() {
                        let c = self.code.get(self.index).unwrap();
                        if c.is_xid_continue() {
                            text.push(*c);
                            self.next_char();
                        } else {
                            break;
                        }
                    }
                    let try_keyword = self.keyword_map(&text);
                    let kind = if try_keyword != TokenType::Error { try_keyword } else { TokenType::Ident };
                    tokens.push(Token {
                        kind,
                        span: Span {
                            source_id: self.source,
                            start_off: start_offset,
                            end_off: self.current_offset(),
                        },
                        text,
                    });
                    state = LexerState::Start;
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
                        let span = Span {
                            source_id: self.source,
                            start_off: start_offset,
                            end_off: start_offset + ch.len_utf8(),
                        };
                        self.emit_invalid_char(span, ch);
                        self.next_char();
                    } else {
                        tokens.push(Token {
                            kind: token_type,
                            span: Span {
                                source_id: self.source,
                                start_off: start_offset,
                                end_off: self.current_offset(),
                            },
                            text: matched_text,
                        });
                    }
                    state = LexerState::Start;
                }
                LexerState::LineStart => {
                    let last_line_byte = self.current_offset();
                    self.next_char(); // '\n'
                    tokens.push(Token {
                        kind: TokenType::NewLine,
                        span: Span {
                            source_id: self.source,
                            start_off: last_line_byte,
                            end_off: last_line_byte,
                        },
                        text: "\n".to_string(),
                    });

                    let start_offset = self.current_offset();
                    let mut indent_text = String::new();
                    while self.index < self.code.len() {
                        let c = self.code[self.index];
                        if c == ' ' {
                            indent_text.push(c);
                            self.next_char();
                        } else if c == '\t' {
                            indent_text.push_str("    ");
                            self.next_char();
                        } else {
                            break;
                        }
                    }
                    let leading_space_width = indent_text.len();

                    if self.index >= self.code.len()
                        || self.code[self.index] == '\n'
                        || self.code[self.index] == '\r'
                    {
                        state = LexerState::Start;
                        continue;
                    }

                    if leading_space_width % INDENT_WIDTH != 0 {
                        let span = Span {
                            source_id: self.source,
                            start_off: start_offset,
                            end_off: self.current_offset(),
                        };
                        self.emit_error(
                            ErrorKind::CompileTimeError(CompileTimeErrorKind::LexerError(LexerErrorKind::InvalidIndent)),
                            span,
                            LocalizedMessage::new(MsgKind::LexerInvalidIndent, std::iter::empty::<&str>()),
                        );
                        self.indent_level = 0;
                        state = LexerState::Start;
                        continue;
                    }

                    let new_level = leading_space_width / INDENT_WIDTH;

                    while (new_level as isize) > self.indent_level {
                        self.indent_level += 1;
                        tokens.push(Token {
                            kind: TokenType::Indent,
                            span: Span {
                                source_id: self.source,
                                start_off: start_offset,
                                end_off: self.current_offset(),
                            },
                            text: indent_text.clone(),
                        });
                    }
                    while (new_level as isize) < self.indent_level {
                        self.indent_level -= 1;
                        tokens.push(Token {
                            kind: TokenType::Dedent,
                            span: Span {
                                source_id: self.source,
                                start_off: start_offset,
                                end_off: self.current_offset(),
                            },
                            text: indent_text.clone(),
                        });
                    }

                    state = LexerState::Start;
                }
            }
        }
    }

    fn new(
        source: FileId,
        text: &str,
        ser_operators: &HashMap<String, OperatorDef>
    ) -> Self {
        let code: Vec<char> = text.chars().collect();
        let (operator_table, operator_prefixes) = build_operator_tables(user_operators);

        Lexer {
            index: 0,
            byte_index: 0,
            source,
            code,
            indent_level: 0,
            docstrings: Document { data: Vec::new() },
            operator_table,
            operator_prefixes,
            diag_collector: Default::default(),
        }
    }

    fn tokenize(&mut self) -> (TokenStream, DiagCollector) {
        let mut tokens = Vec::new();
        self.main_loop(&mut tokens);

        if let Some(last) = tokens.last() {
            if last.kind != TokenType::NewLine {
                let off = self.current_offset();
                tokens.push(Token {
                    kind: TokenType::NewLine,
                    span: Span {
                        source_id: self.source,
                        start_off: off,
                        end_off: off,
                    },
                    text: "\n".to_string(),
                });
            }
        }

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

        tokens.push(self.eof());
        (TokenStream {
            data: tokens,
            document: self.docstrings,
        }, self.diag_collector)
    }
}

impl LexerApi for Lexer {
    fn pass(
        source: FileId,
        text: &str,
        user_operators: &HashMap<String, OperatorDef>
    ) -> (TokenStream, DiagCollector) {
        let mut lex = Lexer::new(source, text, user_operators);
        lex.tokenize()
    }
}