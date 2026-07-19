use std::sync::Arc;
use leafc_coreapi::ast::{
    GreenAnnotation, GreenChild, GreenDecl, GreenDeclKind, GreenParam, DeclRedNode,
    TypeNameString, Visibility,
};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::TokenType;
use leafc_coreapi::parser::ParserError;
use leafc_coreapi::source::Span;
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_external_decl(
        &mut self,
        visibility: Visibility,
        annotations: Vec<(GreenAnnotation, Span)>,
        decl_start_off: usize, // 整个声明的起始偏移
    ) -> Result<DeclRedNode, DiagMsg> {
        let external_token = self.current_token().clone();
        self.skip_token(); // 'external'

        // external ctype
        if self.current_token().kind == TokenType::KwCType {
            self.skip_token(); // 'ctype'
            let name_token = self.current_token().clone();
            let name_start_off = name_token.span.start_off;
            let name = name_token.text.clone();
            self.skip_token_only(TokenType::Ident)?;
            self.skip_token_only(TokenType::Semicolon)?;

            let decl_end_off = self.tokens.data[self.index - 1].span.end_off;

            let name_child = GreenChild {
                relative_start: (name_start_off - decl_start_off) as usize,
                node: Arc::new(name),
            };

            let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
                relative_start: (span.start_off - decl_start_off) as usize,
                node: Arc::new(ga),
            }).collect();

            let green_decl = GreenDecl {
                name: name_child,
                visibility,
                kind: GreenDeclKind::CType,
                annotations: ann_children,
                text_len: (decl_end_off - decl_start_off) as usize,
            };

            return Ok(DeclRedNode {
                span: Span {
                    source_id: external_token.span.source_id,
                    start_off: decl_start_off,
                    end_off: decl_end_off,
                },
                inner: Arc::new(green_decl),
            });
        }

        // external fun
        self.skip_token_only(TokenType::KwFun)?;
        let name_token = self.current_token().clone();
        let name_start_off = name_token.span.start_off;
        let name_text = name_token.text.clone();
        self.skip_token_only(TokenType::Ident)?;

        if self.current_token().kind != TokenType::Lparen {
            return Err(DiagMsg{
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

            let (type_str, type_start_off) = if self.current_token().kind == TokenType::Colon {
                self.skip_token();
                let ts = self.current_token().span.start_off;
                let type_name = self.handle_type_name_string()?;
                (type_name, ts)
            } else {
                let unknown = self.unknown_type_name();
                let ts = self.current_token().span.start_off;
                (unknown, ts)
            };

            let prev_token_end = self.tokens.data[self.index - 1].span.end_off;
            let param_text_len = (prev_token_end - param_start_off);

            let param_name_child = GreenChild {
                relative_start: 0,
                node: Arc::new(param_name_token.text.clone()),
            };
            let type_str_child = GreenChild {
                relative_start: (type_start_off - param_start_off),
                node: Arc::new(type_str),
            };

            let green_param = GreenParam {
                name: param_name_child,
                type_str: type_str_child,
                text_len: param_text_len,
            };

            params.push(GreenChild {
                relative_start: (param_start_off - decl_start_off),
                node: Arc::new(green_param),
            });

            if self.current_token().kind == TokenType::Comma {
                self.skip_token();
            } else if self.current_token().kind == TokenType::Rparen {
                break;
            } else {
                return Err(DiagMsg{
                    title: format!("{:?}", ParserError::InvalidFunctionParameterList),
                    msg: "invalid function parameter list".to_string(),
                    span: self.current_token().span.clone(),
                });
            }
        }
        self.skip_token(); // ')'

        // return type
        let return_type_start_off = self.current_token().span.start_off;
        let return_type_str = if self.current_token().kind == TokenType::Arrow {
            self.skip_token(); // '->'
            self.handle_type_name_string()?
        } else {
            self.unknown_type_name()
        };
        let return_type_child = GreenChild {
            relative_start: (return_type_start_off - decl_start_off) as usize,
            node: Arc::new(return_type_str),
        };

        // sym_name
        let sym_name_token = if self.current_token().kind == TokenType::Eq {
            self.skip_token(); // '='
            let token = self.current_token().clone();
            self.skip_token_only(TokenType::String)?;
            token
        } else {
            name_token.clone()
        };

        let sym_name_child = GreenChild {
            relative_start: (sym_name_token.span.start_off - decl_start_off) as usize,
            node: Arc::new(sym_name_token.text.clone()),
        };

        self.skip_token_only(TokenType::Semicolon)?;
        self.skip_token_only(TokenType::NewLine)?;

        let decl_end_off = self.tokens.data[self.index - 1].span.end_off;

        // annotations
        let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
            relative_start: (span.start_off - decl_start_off),
            node: Arc::new(ga),
        }).collect();

        let name_child = GreenChild {
            relative_start: (name_start_off - decl_start_off),
            node: Arc::new(name_text),
        };

        let green_decl = GreenDecl {
            name: name_child,
            visibility,
            kind: GreenDeclKind::External {
                sym_name: sym_name_child,
                params,
                return_type_str: return_type_child,
            },
            annotations: ann_children,
            text_len: (decl_end_off - decl_start_off),
        };

        Ok(DeclRedNode {
            span: Span {
                source_id: external_token.span.source_id,
                start_off: decl_start_off,
                end_off: decl_end_off,
            },
            inner: Arc::new(green_decl),
        })
    }
}