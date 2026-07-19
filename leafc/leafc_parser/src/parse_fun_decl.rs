use std::sync::Arc;
use leafc_coreapi::ast::{
    GreenAnnotation, GreenChild, GreenDecl, GreenDeclKind, GreenExpr, GreenParam,
    DeclRedNode, ExprRedNode, TypeNameString, Visibility,
};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Token, TokenType};
use leafc_coreapi::parser::ParserError;
use leafc_coreapi::source::Span;
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_fun_decl(
        &mut self,
        visibility: Visibility,
        annotations: Vec<(GreenAnnotation, Span)>,
    ) -> Result<DeclRedNode, DiagMsg> {

        let first_ann_start = annotations.first().map(|(_, sp)| sp.start_off);
        let fn_keyword_token = self.current_token().clone(); // 'fn'
        let decl_start_off = first_ann_start.unwrap_or(fn_keyword_token.span.start_off);

        self.skip_token(); // 'fun'

        let name_token = self.current_token().clone();
        let name_start_off = name_token.span.start_off;
        self.skip_token_only(TokenType::Ident)?;
        let name_text_len = name_token.span.len();
        let name_green_child = GreenChild {
            relative_start: (name_start_off - decl_start_off),
            node: Arc::new(name_token.text.clone()),
        };

        if self.current_token().kind != TokenType::Lparen {
            return Err(DiagMsg {
                title: format!("{:?}", ParserError::FunctionDeclarationMissingParameterList),
                msg: "function declare missing parameter list".to_string(),
                span: self.current_token().span.clone(),
            });
        }
        self.skip_token(); // '('
        let mut params: Vec<GreenChild<GreenParam>> = vec![];

        while self.current_token().kind != TokenType::Rparen {
            let param_start_off = self.current_token().span.start_off;
            let param_name_token = self.current_token().clone();
            self.skip_token_only(TokenType::Ident)?;
            let param_name_text_len = param_name_token.span.len();
            let param_name_child = GreenChild {
                relative_start: 0,
                node: Arc::new(param_name_token.text.clone()),
            };

            let (type_str, type_start_off) = if self.current_token().kind == TokenType::Colon {
                self.skip_token(); // ':'
                let ts = self.current_token().span.start_off;
                let type_name = self.handle_type_name_string()?;
                (type_name, ts)
            } else {
                let unknown = self.unknown_type_name();
                let ts = self.current_token().span.start_off;
                (unknown, ts)
            };

            let prev_token_end = self.tokens.data[self.index - 1].span.end_off;
            let param_text_len = (prev_token_end - param_start_off) as usize;

            let type_str_child = GreenChild {
                relative_start: (type_start_off - param_start_off),
                node: Arc::new(type_str),
            };

            let green_param = GreenParam {
                name: param_name_child,
                type_str: type_str_child,
                text_len: param_text_len,
            };

            let param_relative_start = (param_start_off - decl_start_off);
            params.push(GreenChild {
                relative_start: param_relative_start,
                node: Arc::new(green_param),
            });

            if self.current_token().kind == TokenType::Comma {
                self.skip_token();
            } else if self.current_token().kind == TokenType::Rparen {
                break;
            } else {
                return Err(DiagMsg {
                    title: format!("{:?}", ParserError::InvalidFunctionParameterList),
                    msg: "invalid function parameter list".to_string(),
                    span: self.current_token().span.clone(),
                });
            }
        }
        self.skip_token(); // ')'

        // 返回类型
        let return_type_start_off = self.current_token().span.start_off;
        let return_type_str = if self.current_token().kind == TokenType::Arrow {
            self.skip_token(); // '->'
            self.handle_type_name_string()?
        } else {
            self.unknown_type_name()
        };
        let return_type_child = GreenChild {
            relative_start: (return_type_start_off - decl_start_off),
            node: Arc::new(return_type_str),
        };

        let mut block_children: Vec<GreenChild<GreenExpr>> = vec![];
        let mut decl_end_off;

        if self.current_token().kind == TokenType::Semicolon {
            self.skip_token();
            decl_end_off = self.tokens.data[self.index - 1].span.end_off;
        } else {
            self.skip_token_only(TokenType::NewLine)?;
            self.skip_token_only(TokenType::Indent)?; // indent

            while self.current_token().kind != TokenType::Dedent {
                let expr_red: ExprRedNode = self.parse_expr()?;
                let expr_span = expr_red.span;
                block_children.push(GreenChild {
                    relative_start: (expr_span.start_off - decl_start_off),
                    node: expr_red.inner.clone(),
                });
                if self.current_token().kind == TokenType::NewLine {
                    self.skip_token();
                    self.skip_token_if_newlines()?;
                }
            }

            self.skip_token_only(TokenType::Dedent)?;
            decl_end_off = self.tokens.data[self.index - 1].span.end_off;
        }

        let ann_children: Vec<GreenChild<GreenAnnotation>> = annotations
            .into_iter()
            .map(|(green_ann, span)| GreenChild {
                relative_start: (span.start_off - decl_start_off) as usize,
                node: Arc::new(green_ann),
            })
            .collect();

        let decl_kind = if block_children.is_empty() {
            GreenDeclKind::FunDecl {
                params,
                return_type_str: return_type_child,
            }
        } else {
            GreenDeclKind::Fun {
                params,
                return_type_str: return_type_child,
                block: block_children,
            }
        };

        let text_len = (decl_end_off - decl_start_off) as usize;
        let green_decl = GreenDecl {
            name: name_green_child,
            visibility,
            kind: decl_kind,
            annotations: ann_children,
            text_len,
        };

        Ok(DeclRedNode {
            span: Span {
                source_id: fn_keyword_token.span.source_id,
                start_off: decl_start_off,
                end_off: decl_end_off,
            },
            inner: Arc::new(green_decl),
        })
    }
}