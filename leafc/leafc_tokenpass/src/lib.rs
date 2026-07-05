use std::collections::HashMap;
use std::fmt::format;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Token, TokenStream, TokenType};
use leafc_coreapi::lexer::TokenType::{Comma, Ident, KwAbst};
use leafc_coreapi::parser::ParserError;
use leafc_coreapi::source::{Pos, SourceId, Span};
use leafc_coreapi::tokens_pass::{TokenPassApi, TokenPassError};

const KEYWORD_PREPROCESS: &str = "__nightly__preprocess__";

#[derive(Debug, Clone)]
struct Preprocessor {
    name_token: Token,
    params: Vec<String>,
    body: Vec<Token>
}

pub struct TokenPass<'a> {
    tokens: &'a TokenStream,
    index: usize,
    insert_to_index_at_new_tokens: usize,
    source: SourceId,
    preprocessors: HashMap<String, Preprocessor>,
    new_tokens: TokenStream
}

impl<'a> TokenPass<'a> {
    fn current_token(&self) -> &Token {
        match self.tokens.data.get(self.index) {
            Some(token) => token,
            None => { &self.tokens.data[self.tokens.data.len() - 1] }
        }
    }
    fn skip_token(&mut self) {
        if self.index >= self.tokens.data.len() {
            return;
        }
        self.index += 1;
    }
    fn skip_token_and_get_current(&mut self) -> &Token {
        if self.index >= self.tokens.data.len() {
            return &self.tokens.data[self.tokens.data.len() - 1];
        }
        self.index += 1;
        self.current_token()
    }
    fn expect_token_type(&self, tok: &Token, expected: TokenType) -> Result<(), DiagMsg> {
        if tok.kind == expected {
            return Ok(());
        }

        Err(DiagMsg {
            title: format!("{:?}", ParserError::TokenExpect),
            msg: format!("expected {:?} but got {:?}", expected, tok.kind),
            span: tok.span.clone(),
            source: self.source,
        })
    }

    fn collect_and_apply_preprocessors(&mut self) -> Result<&TokenStream, DiagMsg> {
        self.index = 0;
        while self.current_token().kind != TokenType::Eof {
            let current = self.current_token().clone();

            // 注册预处理器
            if current.text == KEYWORD_PREPROCESS && current.kind == Ident {
                self.skip_token();

                let name_token = self.current_token().clone();
                self.skip_token();

                // 解析参数
                let mut params = Vec::new();
                let has_lparen = self.current_token().kind == TokenType::Lparen;
                if has_lparen {
                    self.skip_token(); // 跳过 '('
                    while self.current_token().kind != TokenType::Eof {
                        let token = self.current_token();
                        self.expect_token_type(token, Ident)?;
                        params.push(token.text.clone());
                        self.skip_token();

                        if self.current_token().kind == Comma {
                            self.skip_token();
                        } else if self.current_token().kind == TokenType::Rparen {
                            break;
                        } else {
                            self.expect_token_type(self.current_token(), Comma)?
                        }
                    }
                    self.skip_token(); // 跳过 ')'
                }
                let mut body = Vec::new();

                while self.current_token().kind != TokenType::NewLine {
                    body.push(self.current_token().clone());
                    self.skip_token();
                }

                // 注册预处理器
                self.preprocessors.entry(name_token.text.clone()).or_insert(
                    Preprocessor {
                        name_token: name_token.clone(),
                        params,
                        body
                    }
                );
                continue;
            }

            // 非使用预处理器则跳过
            if ! (current.kind == Ident && self.preprocessors.contains_key(&current.text)) {
                self.new_tokens.data.push(current);
                self.index += 1;
                continue;
            }

            let processor = self.preprocessors.get(&current.text).unwrap().clone();
            let header_span = current.span.clone();

            self.index += 1; // skip name
            let raw_index = self.index;

            // ===---------------------------
            // Main Handler
            // ===---------------------------
            if processor.params.is_empty() {
                self.insert_to_index_at_new_tokens = self.new_tokens.data.len();
                let raw_insert_to_index_at_new_tokens = self.insert_to_index_at_new_tokens;

                let mut new_body: Vec<Token> = processor.body
                    .iter()
                    // -2是来源自
                    // "__nightly__preprocessor__ M let x = y" 中
                    // -1: M和let之间的空格
                    // -1: 和区间减法的偏移
                    // 已确认 -2 的逻辑是正确的 (by azhz)
                    .map(|tok| Token {
                        kind: tok.kind.clone(),
                        source: tok.source,
                        span: Span {
                            start: Pos {
                                column: tok.span.start.column
                                    - processor.name_token.span.start.column
                                    - 2 + header_span.start.column,
                                lineno: header_span.start.lineno,
                            },
                            end: Pos {
                                column: tok.span.end.column
                                    - processor.name_token.span.end.column
                                    - 2 + header_span.end.column,
                                lineno: header_span.end.lineno,
                            },
                        },
                        text: tok.text.clone(),
                    })
                    .collect();

                self.new_tokens.data.splice(self.insert_to_index_at_new_tokens..self.insert_to_index_at_new_tokens,
                                            new_body);

                self.insert_to_index_at_new_tokens = raw_insert_to_index_at_new_tokens;
                self.index = raw_index;

            }
            else {
                //  __nightly__preprocessor__ M(x,y) let x = y
                //  fun main()
                //      M(x,100)
                //      M(y, 20)
                // 将被替换为
                //  fun main()
                //      let x = 100
                //      let y = 20
                let mut args: Vec<Vec<Token>> = vec![vec![]];
                let mut arg_count: usize = 0;
                self.index += 1;  // skip '('
                while self.current_token().kind != TokenType::Rparen {
                    while self.current_token().kind != TokenType::Rparen
                    && self.current_token().kind != Comma {
                        let token = self.current_token().clone();
                        self.index += 1;
                        args[arg_count].push(token);
                    }

                    if self.current_token().kind == Comma {
                        self.index += 1;
                        arg_count += 1;
                    } else if self.current_token().kind == TokenType::Rparen {
                        self.index += 1;
                        arg_count += 1;
                        break;
                    } else {
                        self.expect_token_type(self.current_token(), Comma)?
                    }
                    self.index += 1;
                }

                let mut processed_arg_count = 0;

                // 维护 insert to index at new tokens 以支持:
                // M(H+5)
                // 展开完M后, 下标回到M后面, 然后处理H
                self.insert_to_index_at_new_tokens = self.new_tokens.data.len();
                let raw_insert_to_index_at_new_tokens = self.insert_to_index_at_new_tokens;

                for tok in &processor.body {
                    if tok.kind == Ident && processor.params.contains(&tok.text) {
                        self.new_tokens.data.splice(
                            self.insert_to_index_at_new_tokens..self.insert_to_index_at_new_tokens,
                            args[processed_arg_count].drain(..));
                        self.insert_to_index_at_new_tokens += args[processed_arg_count].len();
                        processed_arg_count += 1;
                    } else {
                        self.new_tokens.data.insert(self.insert_to_index_at_new_tokens, tok.clone());
                        self.insert_to_index_at_new_tokens += 1;
                    }
                }

                self.insert_to_index_at_new_tokens = raw_insert_to_index_at_new_tokens;
                self.index = raw_index;

                if processed_arg_count != arg_count || processed_arg_count != processor.params.len() {
                    return Err(DiagMsg {
                        title: format!("{:?}", TokenPassError::MissingPreProcessorArgument),
                        msg: "missing preprocessor argument".to_string(),
                        span: self.current_token().span.clone(),
                        source: self.source,
                    })
                }
            }
        }
        Ok(&self.new_tokens)
    }
}

impl<'a> TokenPassApi<'a> for TokenPass<'a> {
    fn new(tokens: &'a TokenStream, source: SourceId) -> Self {
        TokenPass {
            tokens,
            preprocessors: HashMap::new(),
            index : 0,
            insert_to_index_at_new_tokens: 0,
            source,
            new_tokens: TokenStream { data: Vec::new() }
        }
    }
    fn pass(&mut self) -> Result<&TokenStream, DiagMsg> {
        self.collect_and_apply_preprocessors()
    }
}