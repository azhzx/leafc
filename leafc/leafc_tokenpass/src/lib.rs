use std::collections::{HashMap, HashSet};
use std::fmt::format;
use leafc_coreapi::ast::ExprNode;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Token, TokenStream, TokenType};
use leafc_coreapi::lexer::TokenType::{Comma, Eof, Ident, KwAbst, Lparen, Rparen, NewLine};
use leafc_coreapi::parser::ParserError;
use leafc_coreapi::source::{SourceId, Span};
use leafc_coreapi::tokens_pass::{TokenPassApi, TokenPassError};

const KEYWORD_PREPROCESS: &str = "__define";

const KEYWORD_DELETE_PREPROCESS: &str = "__delete";

const KEYWORD_IF: &str = "__if";

const KEYWORD_ELIF: &str = "__elif";

const KEYWORD_ELSE: &str = "__else";

const KEYWORD_ENDIF: &str = "__endif";

const KEYWORD_PANIC: &str = "__panic";

const KEYWORD_WARNING: &str = "__warning";

const KEYWORD_COUNTER: &str = "__counter";

const PP_FUNCTION_IS_DEFINED: &str = "__is_defined";

const PP_FUNCTION_EVAL: &str = "__eval";

const PP_FUNCTION_REPEAT: &str = "__repeat";

const PP_FUNCTION_IDENT_TO_STRING: &str = "__ident_to_string";

const PP_FUNCTION_IDENT_CONCAT: &str = "__ident_concat";

const PP_FUNCTION_ARGS_OPTION: &str = "__only_has_rest_args";

const REST_ARGS_MARKER: &str = "__rest_args";

const PRE_DEFINES: [(&str, isize); 2] = [
    if cfg!(target_os = "windows") {
        ("__windows", 1)
    } else if cfg!(target_os = "macos") {
        ("__mac", 1)
    } else if cfg!(target_os = "linux") {
        ("__linux", 1)
    } else {
        ("__unknown", 1)
    },
    ("version", 1),
];

#[derive(PartialEq)]
enum IfKeyword {
    Elif,
    Else,
    Endif,
}

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
    counter: usize,
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

    fn process_if(
        &mut self,
        current_tokens: &Vec<Token>,
        index: &mut usize,
        result: &mut Vec<Token>,
    ) -> Result<(), DiagMsg> {
        let mut cond_tokens = Vec::new();

        while *index < current_tokens.len()
            && current_tokens[*index].kind != TokenType::NewLine
        {
            cond_tokens.push(current_tokens[*index].clone());
            *index += 1;
        }

        *index += 1; // newline

        cond_tokens = self.expand_all(cond_tokens)?;
        let mut cond_true = self.eval(cond_tokens) > 0;

        if cond_true {

            let mut depth = 0;
            while *index < current_tokens.len() {
                let token = &current_tokens[*index];
                match token.text.as_str() {
                    KEYWORD_IF => depth += 1,
                    KEYWORD_ENDIF => {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                    }
                    KEYWORD_ELIF | KEYWORD_ELSE => {
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                result.push(token.clone());
                *index += 1;
            }

            let mut depth = 0;
            loop {
                if *index >= current_tokens.len() { break; }
                match current_tokens[*index].text.as_str() {
                    KEYWORD_IF => depth += 1,
                    KEYWORD_ENDIF => {
                        if depth == 0 {
                            *index += 1; // endif
                            break;
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
                *index += 1;
            }
        } else {
            let mut depth = 0;
            loop {
                if *index >= current_tokens.len() {
                    panic!()
                }

                match current_tokens[*index].text.as_str() {
                    KEYWORD_IF => depth += 1,

                    KEYWORD_ENDIF => {
                        if depth == 0 {
                            // 同级的 endif：整个 if 没有 elif/else，直接结束
                            *index += 1;  // 跳过 endif 本身
                            break;
                        }
                        depth -= 1;
                    }

                    KEYWORD_ELIF => {
                        if depth == 0 {
                            *index += 1;
                            // 将这个 elif 当成 if 递归处理
                            self.process_if(current_tokens, index, result)?;
                            break;
                        }
                    }

                    KEYWORD_ELSE => {
                        if depth == 0 {
                            *index += 1;  // 跳过 else 关键字
                            let mut depth2 = 0;
                            while *index < current_tokens.len() {
                                let t = &current_tokens[*index];
                                match t.text.as_str() {
                                    KEYWORD_IF => depth2 += 1,
                                    KEYWORD_ENDIF => {
                                        if depth2 == 0 {
                                            break;
                                        }
                                        depth2 -= 1;
                                    }
                                    _ => {}
                                }
                                result.push(t.clone());
                                *index += 1;
                            }
                            *index += 1;
                            break;
                        }
                    }

                    _ => {}
                }
                *index += 1;
            }
        }
        Ok(())
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

                    // 跳过 '('
                    index += 1;

                    let mut args: Vec<Vec<Token>> = Vec::new();
                    let mut current_arg: Vec<Token> = Vec::new();
                    let mut depth = 1; // 最外层'('是'1'

                    if index >= current_tokens.len() {
                        unreachable!()
                    }

                    while depth > 0 && index < current_tokens.len() {
                        match current_tokens[index].kind {
                            Lparen => {
                                depth += 1;
                                current_arg.push(current_tokens[index].clone());
                            }
                            TokenType::Rparen => {
                                depth -= 1;
                                if depth == 0 {
                                    // 这是最外层的右括号
                                    index += 1;
                                    break;
                                } else {
                                    current_arg.push(current_tokens[index].clone());
                                }
                            }
                            TokenType::Comma => {
                                if depth == 1 {
                                    args.push(current_arg.clone());
                                    current_arg.clear();
                                } else {
                                    current_arg.push(current_tokens[index].clone());
                                }
                            }
                            _ => {
                                current_arg.push(current_tokens[index].clone());
                            }
                        }
                        index += 1;
                    }

                    if !args.is_empty() || !current_arg.is_empty() {
                        args.push(current_arg);
                    }

                    if depth != 0 { unreachable!() }

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
                        let mut body_idx = 0;
                        while body_idx < def.body.len() {
                            let body_token = &def.body[body_idx];

                            if body_token.kind == Ident
                                && body_token.text == REST_ARGS_MARKER
                                && def.has_rest_args
                            {
                                result.extend(rest_tokens.clone());
                                body_idx += 1;

                            } else if body_token.kind == Ident
                                && arg_map.contains_key(&body_token.text)
                            {
                                let mut v = arg_map.remove(&body_token.text).unwrap();
                                result.append(&mut v);
                                body_idx += 1;

                            } else if body_token.kind == Ident
                                && body_token.text == PP_FUNCTION_ARGS_OPTION
                            {
                                // 就地处理 __only_has_rest_args(...)
                                body_idx += 1;

                                if body_idx >= def.body.len() || def.body[body_idx].kind != Lparen {
                                    continue;
                                }
                                body_idx += 1; // '('

                                let mut va_content = Vec::new();
                                let mut depth = 1;
                                while body_idx < def.body.len() && depth > 0 {
                                    match def.body[body_idx].kind {
                                        Lparen => {
                                            depth += 1;
                                            va_content.push(def.body[body_idx].clone());
                                        }
                                        Rparen => {
                                            depth -= 1;
                                            if depth == 0 {
                                                body_idx += 1; // 跳过 ')' 本身
                                                break;
                                            } else {
                                                va_content.push(def.body[body_idx].clone());
                                            }
                                        }
                                        _ => {
                                            va_content.push(def.body[body_idx].clone());
                                        }
                                    }
                                    body_idx += 1;
                                }

                                if !rest_tokens.is_empty() {
                                    result.extend(va_content);
                                }
                            } else {
                                result.push(body_token.clone());
                                body_idx += 1;
                            }
                        }

                        self.expanding.remove(&macro_name.text);
                        changed = true;
                    }
                }

                else if current_token.kind == Ident
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
                }

                else if current_token.kind == TokenType::Ident
                    && current_token.text == KEYWORD_DELETE_PREPROCESS {
                    index += 1;
                    self.preprocessors.remove(&current_token.text);
                }

                else if current_token.kind == TokenType::Ident
                    && current_token.text == KEYWORD_PANIC {
                    index += 1;
                    return Err(DiagMsg {
                        title: format!("{:?}", TokenPassError::UserPreprocessorPanic),
                        msg: current_tokens[index].text.clone(),
                        span: current_tokens[index].span.clone(),
                    });
                }

                else if current_token.kind == TokenType::Ident
                    && current_token.text == KEYWORD_WARNING {
                    index += 1;
                    println!("[warning] {}", &current_tokens[index].text);
                    index += 1;
                }

                else if current_token.kind == TokenType::Ident
                    && current_token.text == KEYWORD_COUNTER {

                    let current_span = current_tokens[index].span.clone();
                    index += 1;
                    result.push(Token {
                        kind: TokenType::Int,
                        span: current_span,
                        text: self.counter.to_string(),
                    });
                    self.counter += 1;
                }

                else if current_token.kind == TokenType::Ident
                    && current_token.text == PP_FUNCTION_EVAL
                {

                    index += 1;
                    index += 1;                     // '('

                    let mut expr = Vec::new();
                    let current_span = current_tokens[index].span.clone();
                    let mut depth = 1;              // 已进入最外层 '('
                    while index < current_tokens.len() && depth > 0 {
                        match current_tokens[index].kind {
                            Lparen => depth += 1,
                            TokenType::Rparen => {
                                depth -= 1;
                                if depth == 0 {
                                    index += 1;     // ')'
                                    break;
                                }
                            }
                            _ => {}
                        }
                        expr.push(current_tokens[index].clone());
                        index += 1;
                    }

                    let eval = self.eval(expr);
                    result.push(Token {
                        kind: TokenType::Int,
                        span: current_span,
                        text: eval.to_string(),
                    });
                }

                else if current_token.kind == TokenType::Ident
                    && current_token.text == PP_FUNCTION_IDENT_TO_STRING
                {
                    index += 1; // __ident_to_string
                    index += 1; // '('

                    let mut arg_tokens = Vec::new();
                    let mut depth = 1;
                    while index < current_tokens.len() && depth > 0 {
                        match current_tokens[index].kind {
                            Lparen => {
                                depth += 1;
                                arg_tokens.push(current_tokens[index].clone());
                            }
                            Rparen => {
                                depth -= 1;
                                if depth == 0 {
                                    index += 1; // ')'
                                    break;
                                }
                                arg_tokens.push(current_tokens[index].clone());
                            }
                            _ => {
                                arg_tokens.push(current_tokens[index].clone());
                            }
                        }
                        index += 1;
                    }

                    if arg_tokens.len() != 1 || arg_tokens[0].kind != Ident {
                        return Err(DiagMsg {
                            title: format!("{:?}", TokenPassError::InvalidIdentToString),
                            msg: "ident to string expects exactly one identifier argument".to_string(),
                            span: current_token.span.clone(),
                        });
                    }

                    let ident_text = arg_tokens[0].text.clone();
                    result.push(Token {
                        kind: TokenType::String,
                        span: current_token.span.clone(),
                        text: ident_text,
                    });
                    changed = true;
                }

                else if current_token.kind == TokenType::Ident
                    && current_token.text == PP_FUNCTION_IDENT_CONCAT
                {
                    index += 1; // __ident_concat
                    index += 1; // '('

                    let mut concat_parts = Vec::new();
                    let mut current_arg = Vec::new();
                    let mut depth = 1;

                    while index < current_tokens.len() && depth > 0 {
                        match current_tokens[index].kind {
                            Lparen => {
                                depth += 1;
                                current_arg.push(current_tokens[index].clone());
                            }
                            Rparen => {
                                depth -= 1;
                                if depth == 0 {
                                    if !current_arg.is_empty() {
                                        concat_parts.push(current_arg);
                                    }
                                    index += 1; // ')'
                                    break;
                                }
                                current_arg.push(current_tokens[index].clone());
                            }
                            TokenType::Comma => {
                                if depth == 1 {
                                    // 外层逗号，分隔参数
                                    concat_parts.push(current_arg.clone());
                                    current_arg.clear();
                                } else {
                                    current_arg.push(current_tokens[index].clone());
                                }
                            }
                            _ => {
                                current_arg.push(current_tokens[index].clone());
                            }
                        }
                        index += 1;
                    }

                    let mut final_ident = String::new();
                    for arg in concat_parts {
                        if arg.len() != 1 || arg[0].kind != Ident {
                            return Err(DiagMsg {
                                title: format!("{:?}", TokenPassError::InvalidIdentConcat),
                                msg: "ident concat expects only identifier arguments".to_string(),
                                span: current_token.span.clone(),
                            });
                        }
                        final_ident.push_str(&arg[0].text);
                    }

                    result.push(Token {
                        kind: Ident,
                        span: current_token.span.clone(),
                        text: final_ident,
                    });
                    changed = true;
                }

                else if current_token.kind == TokenType::Ident
                    && current_token.text == KEYWORD_IF {

                    index += 1;

                    self.process_if(&current_tokens, &mut index, &mut result)?;

                    changed = true;

                }

                else if current_token.kind == TokenType::Ident
                    && current_token.text == PP_FUNCTION_REPEAT
                {
                    // __repeat(times_expr, body)
                    index += 1; // __repeat
                    index += 1; // '('

                    let mut times_tokens = Vec::new();
                    let mut depth = 1;
                    while index < current_tokens.len() && depth > 0 {
                        match current_tokens[index].kind {
                            Lparen => {
                                depth += 1;
                                times_tokens.push(current_tokens[index].clone());
                            }
                            Rparen => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                                times_tokens.push(current_tokens[index].clone());
                            }
                            Comma => {
                                if depth == 1 {
                                    index += 1; // ','
                                    break;
                                } else {
                                    times_tokens.push(current_tokens[index].clone());
                                }
                            }
                            _ => {
                                times_tokens.push(current_tokens[index].clone());
                            }
                        }
                        index += 1;
                    }

                    // 展开并求值times
                    let expanded_times = self.expand_all(times_tokens)?;
                    let times_val = self.eval(expanded_times);
                    if times_val < 0 {
                        return Err(DiagMsg {
                            title: "repeat count error".to_string(),
                            msg: "repeat count must be non-negative".to_string(),
                            span: current_token.span.clone(),
                        });
                    }
                    let times = times_val as usize;

                    let mut body_tokens = Vec::new();
                    let mut depth = 0;
                    while index < current_tokens.len() {
                        match current_tokens[index].kind {
                            Lparen => depth += 1,
                            Rparen => {
                                if depth == 0 {
                                    index += 1; // ')'
                                    break;
                                }
                                depth -= 1;
                            }
                            _ => {}
                        }
                        body_tokens.push(current_tokens[index].clone());
                        index += 1;
                    }

                    for _ in 0..times {
                        let expanded_body = self.expand_all(body_tokens.clone())?;
                        result.extend(expanded_body);
                    }

                    changed = true;
                }

                else if current_token.kind == TokenType::Ident
                    && current_token.text == PP_FUNCTION_IS_DEFINED {

                    index += 1;

                    let is_pp = self.preprocessors.contains_key(&current_tokens[index].text.clone());

                    result.push(Token {
                        kind: TokenType::Int,
                        span: current_tokens[index].span.clone(),
                        text: (if is_pp { 1 } else { 0 }).to_string(),
                    });

                }

                else {
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

    fn pre_definitions(&mut self, names: &[(String, isize)]) {
        for (name, value) in names {
            self.preprocessors.entry(name.clone()).or_insert(PPDef {
                name_token: Token {
                    kind: TokenType::Ident,
                    span: Span {
                        source_id: self.source,
                        start_off: 0,
                        end_off: 0
                    },
                    text: name.clone(),
                },
                params: vec![],
                has_rest_args: false,
                body: vec![
                    Token {
                        kind: TokenType::Int,
                        span: Span {
                            source_id: self.source,
                            start_off: 0,
                            end_off: 0
                        },
                        text: value.to_string(),
                    }
                ],
            });
        }
    }

}

impl<'a> TokenPassApi<'a> for Preprocessor<'a> {
    fn new(tokens: &'a TokenStream, source: SourceId) -> Self {
        Preprocessor {
            tokens,
            preprocessors: HashMap::new(),
            source,
            expanding: HashSet::new(),
            counter: 0,
            new_tokens: TokenStream { data: Vec::new() },
        }
    }
    fn pass(&mut self) -> Result<TokenStream, DiagMsg> {
        let defines: Vec<(String, isize)> = PRE_DEFINES
            .iter()
            .map(|(name, val)| (name.to_string(), *val))
            .collect();
        self.pre_definitions(&defines);
        Ok(TokenStream {
            data: self.expand_all(self.tokens.data.clone())?
        })
    }
}