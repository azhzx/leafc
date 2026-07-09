use std::collections::{HashMap, HashSet};
use std::fmt::format;
use leafc_coreapi::ast::ExprNode;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Token, TokenStream, TokenType};
use leafc_coreapi::lexer::TokenType::{Comma, Eof, Ident, KwAbst, Lparen, NewLine};
use leafc_coreapi::parser::ParserError;
use leafc_coreapi::source::{Pos, SourceId, Span};
use leafc_coreapi::tokens_pass::{TokenPassApi, TokenPassError};

const KEYWORD_PREPROCESS: &str = "preprocessor_define";
const KEYWORD_DELETE_PREPROCESS: &str = "preprocessor_delete";
const KEYWORD_IF: &str = "preprocessor_if";
const KEYWORD_ELIF: &str = "preprocessor_elif";
const KEYWORD_ELSE: &str = "preprocessor_else";
const KEYWORD_ENDIF: &str = "preprocessor_endif";

const KEYWORD_PANIC: &str = "preprocessor_panic";
const KEYWORD_WARNING: &str = "preprocessor_warning";

const PP_FUNCTION_IS_DEFINED: &str = "preprocessor_is_defined";

const REST_ARGS_MARKER: &str = "__PreprocessorRestArgs";

#[derive(Debug, Clone)]
struct PPDef {
    name_token: Token,
    params: Vec<String>,
    has_rest_args: bool,
    body: Vec<Token>,
}

pub struct Preprocessor<'a> {
    tokens: &'a TokenStream,
    source: SourceId,
    preprocessors: HashMap<String, PPDef>,
    expanding: HashSet<String>,
    new_tokens: TokenStream,
}

impl<'a> Preprocessor<'a> {
    pub fn eval(&self, tokens: Vec<Token>) -> isize {
        let mut index = 0;
        self.eval_logic(&tokens, &mut index)
    }

    fn eval_logic(&self, tokens: &[Token], index: &mut usize) -> isize {
        let left = self.eval_not(tokens, index);
        while *index < tokens.len() {
            match tokens[*index].kind {
                TokenType::KwAnd => {
                    *index += 1;
                    let right = self.eval_not(tokens, index);
                    return (left != 0 && right != 0) as isize;
                }
                TokenType::KwOr => {
                    *index += 1;
                    let right = self.eval_not(tokens, index);
                    return (left != 0 || right != 0) as isize;
                }
                _ => break,
            }
        }
        left
    }

    fn eval_not(&self, tokens: &[Token], index: & mut usize) -> isize {
        match tokens[*index].kind {
            TokenType::KwNot => {
                *index += 1;
                let val = self.eval_comparison(tokens, index);
                ! val
            }
            _ => self.eval_comparison(tokens, index),
        }
    }

    fn eval_comparison(&self, tokens: &[Token], index: &mut usize) -> isize {
        let left = self.eval_add_sub(tokens, index);
        while *index < tokens.len() {
            match tokens[*index].kind {
                TokenType::EqEq => {
                    *index += 1;
                    let right = self.eval_add_sub(tokens, index);
                    return (left == right) as isize;
                }
                TokenType::Ne => {
                    *index += 1;
                    let right = self.eval_add_sub(tokens, index);
                    return (left != right) as isize;
                }
                TokenType::Gt => {
                    *index += 1;
                    let right = self.eval_add_sub(tokens, index);
                    return (left > right) as isize;
                }
                TokenType::Lt => {
                    *index += 1;
                    let right = self.eval_add_sub(tokens, index);
                    return (left < right) as isize;
                }
                TokenType::Ge => {
                    *index += 1;
                    let right = self.eval_add_sub(tokens, index);
                    return (left >= right) as isize;
                }
                TokenType::Le => {
                    *index += 1;
                    let right = self.eval_add_sub(tokens, index);
                    return (left <= right) as isize;
                }
                _ => break,
            }
        }
        left
    }

    fn eval_add_sub(&self, tokens: &[Token], index: &mut usize) -> isize {
        let mut left = self.eval_mul_div(tokens, index);
        while *index < tokens.len() {
            match tokens[*index].kind {
                TokenType::Plus => {
                    *index += 1;
                    let right = self.eval_mul_div(tokens, index);
                    left += right;
                }
                TokenType::Minus => {
                    *index += 1;
                    let right = self.eval_mul_div(tokens, index);
                    left -= right;
                }
                _ => break,
            }
        }
        left
    }

    fn eval_mul_div(&self, tokens: &[Token], index: &mut usize) -> isize {
        let mut left = self.eval_unary(tokens, index);
        while *index < tokens.len() {
            match tokens[*index].kind {
                TokenType::Star => {
                    *index += 1;
                    let right = self.eval_unary(tokens, index);
                    left *= right;
                }
                TokenType::Slash => {
                    *index += 1;
                    let right = self.eval_unary(tokens, index);
                    left /= right;
                }
                _ => break,
            }
        }
        left
    }

    fn eval_unary(&self, tokens: &[Token], index: &mut usize) -> isize {
        match tokens[*index].kind {
            TokenType::Minus => {
                *index += 1;
                let val = self.eval_unary(tokens, index);
                0 - val
            }
            _ => self.eval_primary(tokens, index),
        }
    }

    fn eval_primary(&self, tokens: &[Token], index: &mut usize) -> isize {
        let token = &tokens[*index];
        match token.kind {
            TokenType::Int => {
                *index += 1;
                token.text.parse::<isize>().unwrap()
            }
            _ => {
                panic!("cannot eval expression");
            }
        }
    }


    fn expand_all(&mut self, tokens: Vec<Token>) -> Result<Vec<Token>, DiagMsg> {
        let mut current_tokens = tokens;
        loop {
            let mut result = Vec::new();
            let mut index = 0;
            let mut changed = false;

            while index < current_tokens.len() {
                let current_token = &current_tokens[index];

                if current_token.kind == Ident
                    && self.preprocessors.contains_key(&current_token.text)
                    && index + 1 < current_tokens.len()
                    && current_tokens[index + 1].kind == Lparen
                {
                    let macro_name = current_token.clone();
                    let def = self.preprocessors[&macro_name.text].clone();

                    // 跳过宏名
                    index += 1;

                    let index_before_args = index;

                    // 跳过'('
                    index += 1;

                    let mut args: Vec<Vec<Token>> = Vec::new();
                    let mut current_arg_index = 0;

                    while current_tokens[index].kind != TokenType::Rparen {
                        args.push(Vec::new());
                        while current_tokens[index].kind != TokenType::Comma
                            && current_tokens[index].kind != TokenType::Rparen {
                            args[current_arg_index].push(current_tokens[index].clone());
                            index += 1;
                        }

                        if current_tokens[index].kind == Comma {
                            index += 1;
                            current_arg_index += 1;
                        } else if current_tokens[index].kind == TokenType::Rparen {
                            break;
                        } else {
                            return Err(DiagMsg {
                                title: format!("{:?}", TokenPassError::InvalidPreprocessorArgumentList),
                                msg: "invalid call argument list".to_string(),
                                span: current_token.span.clone(),
                                source: self.source,
                            });
                        }
                    }

                    // 跳过 ')'
                    index += 1;

                    let rest_tokens = if def.has_rest_args {
                        let current_token_span = current_tokens[index].span.clone();

                        if args.len() < def.params.len() {
                            return Err(DiagMsg {
                                title: "macro expansion error".to_string(),
                                msg: format!(
                                    "macro {} expects at least {} arguments, got {}",
                                    macro_name.text,
                                    def.params.len(),
                                    args.len()
                                ),
                                span: current_token.span.clone(),
                                source: self.source,
                            });
                        }

                        let extra_args = &args[def.params.len()..];
                        let mut rest = Vec::new();
                        for (i, arg) in extra_args.iter().enumerate() {
                            if i > 0 {
                                // 在实参之间插入逗号 token
                                rest.push(Token {
                                    kind: Comma,
                                    span: current_token_span.clone(),
                                    source: self.source,
                                    text: ",".to_string(),
                                });
                            }
                            rest.extend(arg.clone());
                        }
                        rest
                    } else {
                        if def.params.len() != args.len() {
                            return Err(DiagMsg {
                                title: "macro expansion error".to_string(),
                                msg: format!(
                                    "macro {} expects {} arguments, got {}",
                                    macro_name.text,
                                    def.params.len(),
                                    args.len()
                                ),
                                span: current_token.span.clone(),
                                source: self.source,
                            });
                        }
                        Vec::new()
                    };


                    // 展开
                    if self.expanding.contains(&macro_name.text) {
                        // 不替换
                        result.push(macro_name.clone());
                        result.append(&mut current_tokens[index_before_args..index].to_vec());
                    } else {
                        self.expanding.insert(macro_name.text.clone());


                        let regular_args = &args[..def.params.len()]; // 仅取前 N 个
                        let mut arg_map: HashMap<String, Vec<Token>> = def
                            .params
                            .iter()
                            .zip(regular_args.iter())
                            .map(|(p, a)| (p.clone(), a.clone()))
                            .collect();

                        // 展开 body
                        for body_token in &def.body {
                            if body_token.kind == Ident
                                && body_token.text == REST_ARGS_MARKER
                                && def.has_rest_args {
                                result.extend(rest_tokens.clone());

                            } else if body_token.kind == Ident
                                && arg_map.contains_key(&body_token.text) {

                                let mut v = arg_map.remove(&body_token.text).unwrap();
                                result.append(&mut v);

                            } else {
                                result.push(body_token.clone());
                            }
                        }

                        self.expanding.remove(&macro_name.text);
                        changed = true;
                    }
                } else if current_token.kind == Ident
                    && current_token.text == KEYWORD_PREPROCESS
                {
                    index += 1;
                    let name_token = current_tokens[index].clone();
                    index += 1;


                    // 解析参数
                    let mut params = Vec::new();
                    let mut has_rest_args = false;

                    if current_tokens[index].kind == Lparen {
                        index += 1;

                        let span = name_token.span.clone();

                        while current_tokens[index].kind != TokenType::Rparen {
                            if current_tokens[index].kind == TokenType::DotDotDot {
                                index += 1;
                                has_rest_args = true;
                                break
                            } else {
                                params.push(current_tokens[index].text.clone());
                                index += 1;
                            }


                            if current_tokens[index].kind == TokenType::Comma {
                                index += 1;
                            } else if current_tokens[index].kind == TokenType::Rparen {
                                break;
                            } else {
                                return Err(DiagMsg {
                                    title: format!("{:?}", TokenPassError::InvalidPreprocessorParameterDeclare),
                                    msg: "invalid call argument list".to_string(),
                                    span,
                                    source: self.source,
                                });
                            }
                        }
                        index += 1;
                    }
                    let mut body = Vec::new();

                    while current_tokens[index].kind != TokenType::NewLine {
                        body.push(current_tokens[index].clone());
                        index += 1;
                    }

                    // 注册预处理器
                    self.preprocessors.entry(name_token.text.clone()).or_insert(
                        PPDef {
                            name_token: name_token.clone(),
                            params,
                            has_rest_args,
                            body
                        }
                    );
                } else if current_token.kind == TokenType::Ident
                    && current_token.text == KEYWORD_DELETE_PREPROCESS {
                    index += 1;
                    self.preprocessors.remove(&current_token.text);
                } else if current_token.kind == TokenType::Ident
                    && current_token.text == KEYWORD_PANIC {
                    index += 1;
                    panic!("{}", &current_tokens[index].text);
                } else if current_token.kind == TokenType::Ident
                    && current_token.text == KEYWORD_WARNING {
                    index += 1;
                    println!("[warning] {}", &current_tokens[index].text);
                    index += 1;
                } else if current_token.kind == TokenType::Ident
                    && current_token.text == KEYWORD_IF {

                    index += 1;

                    let mut if_cond = Vec::new();
                    while current_tokens[index].kind != TokenType::NewLine {
                        if_cond.push(current_tokens[index].clone());
                        index += 1;
                    }
                    index += 1;

                    if_cond = self.expand_all(if_cond)?;
                    let eval_result = self.eval(if_cond);

                    if eval_result <= 0 {
                        while  current_tokens[index].text != KEYWORD_ELSE
                            && current_tokens[index].text != KEYWORD_ENDIF
                        {
                            index += 1;
                        }

                        if current_tokens[index].text == KEYWORD_ELSE {
                            index += 1;
                            while current_tokens[index].text != KEYWORD_ENDIF  {
                                result.push(current_tokens[index].clone());
                                index += 1;
                            }
                        }

                        index += 1;
                        changed = true;
                    } else {
                        while index < current_tokens.len()
                            && current_tokens[index].text != KEYWORD_ELSE
                            && current_tokens[index].text != KEYWORD_ENDIF
                        {
                            result.push(current_tokens[index].clone());
                            index += 1;
                        }

                        if current_tokens[index].text == KEYWORD_ELSE {
                            index += 1;
                            while current_tokens[index].text != KEYWORD_ENDIF {
                                index += 1;
                            }
                        }

                        // skip endif
                        index += 1;

                        changed = true;
                    }

                } else if current_token.kind == TokenType::Ident
                    && current_token.text == PP_FUNCTION_IS_DEFINED {

                    index += 1;

                    let is_pp = self.preprocessors.contains_key(&current_tokens[index].text.clone());

                    result.push(Token {
                        kind: TokenType::Int,
                        span: current_tokens[index].span.clone(),
                        source: self.source,
                        text: (if is_pp { 1 } else { 0 }).to_string(),
                    });

                } else {
                    let expanded = self.expand_one(current_token)?;
                        if expanded.len() != 1 || expanded[0] != *current_token {
                            changed = true;
                        }
                        result.extend(expanded);
                        index += 1;
                    }
            }

            if !changed || result == current_tokens {
                return Ok(result);
            }
            current_tokens = result;
        }
    }

    fn expand_one(&mut self, token: &Token) -> Result<Vec<Token>, DiagMsg> {
        if !self.preprocessors.contains_key(&token.text) {
            return Ok(vec![token.clone()]);
        }

        let macro_name = token.text.clone();

        if self.expanding.contains(&macro_name) {
            return Ok(vec![token.clone()]);
        }

        self.expanding.insert(macro_name.clone());

        let pp_body = self.preprocessors[&macro_name].clone().body;

        if pp_body.is_empty() {
            self.expanding.remove(&macro_name);
            return Ok(vec![]);
        }

        let expanded = self.expand_all(pp_body)?;

        self.expanding.remove(&macro_name);

        Ok(expanded)
    }

    pub fn pre_definitions(&mut self, names: Vec<String>) -> &mut Self {
        for name in names {
            self.preprocessors.entry(name.clone()).or_insert(PPDef {
                name_token: Token {
                    kind: TokenType::Ident,
                    span: Span { start: Pos { column: 0, lineno: 0 }, end: Pos { column: 0, lineno: 0 } },
                    source: self.source,
                    text: name.clone(),
                },
                params: vec![],
                has_rest_args: false,
                body: vec![],
            });
        }
        self
    }

}

impl<'a> TokenPassApi<'a> for Preprocessor<'a> {
    fn new(tokens: &'a TokenStream, source: SourceId) -> Self {
        Preprocessor {
            tokens,
            preprocessors: HashMap::new(),
            source,
            expanding: HashSet::new(),
            new_tokens: TokenStream { data: Vec::new() },
        }
    }
    fn pass(&mut self) -> Result<&TokenStream, DiagMsg> {
        self.new_tokens = TokenStream {
            data: self.expand_all(self.tokens.data.clone())?
        };

        Ok(&self.new_tokens)
    }
}