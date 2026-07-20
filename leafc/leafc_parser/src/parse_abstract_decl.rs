use std::sync::Arc;
use leafc_coreapi::ast::{GreenAnnotation, GreenChild, GreenDecl, GreenDeclKind, GreenMethodDecl, GreenParam, DeclRedNode, TypeNameString, Visibility, IdentName};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Token, TokenType};
use leafc_coreapi::parser::ParserError;
use leafc_coreapi::source::Span;
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_abstract_decl(
        &mut self,
        visibility: Visibility,
        annotations: Vec<(GreenAnnotation, Span)>,
        decl_start_off: usize, // 整个声明的起始偏移（'abst' 关键字）
    ) -> Result<DeclRedNode, DiagMsg> {
        let abst_token = self.current_token().clone();
        self.skip_token(); // 'abst'
        let name_token = self.current_token().clone();
        let name_start_off = name_token.span.start_off;
        let name = name_token.text.clone();
        self.skip_token_only(TokenType::Ident)?;

        // 泛型参数
        let (mut generic_var_children
            , generics_start_off
        ) = if self.current_token().kind == TokenType::Lbracket {
            let (children, start) = self.handle_generic_param()?;
            let adjusted = children.into_iter().map(|mut child| {
                child.relative_start += (start - decl_start_off) as usize;
                child
            }).collect::<Vec<_>>();
            (adjusted, start)
        } else {
            (vec![], decl_start_off)
        };

        // impl 列表
        let mut impls = vec![];
        if self.current_token().kind == TokenType::KwImpl {
            self.skip_token();
            while self.current_token().kind == TokenType::Ident {
                let impl_name_start = self.current_token().span.start_off;
                let impl_name = self.current_token().text.clone();
                self.skip_token();

                impls.push(GreenChild {
                    relative_start: (impl_name_start - decl_start_off),
                    node: Arc::new(IdentName { name : impl_name}),
                });

                if self.current_token().kind == TokenType::Plus {
                    self.skip_token();
                } else {
                    break;
                }
            }
        }

        self.skip_token_only(TokenType::NewLine)?;
        if self.current_token().kind == TokenType::KwWhere {
            let raw_generics = generic_var_children.iter().map(|c| (*c.node).clone()).collect();
            let updated_raw = self.handle_where(raw_generics, decl_start_off)?;
            generic_var_children = generic_var_children.into_iter().zip(updated_raw.into_iter())
                .map(|(mut child, new_var)| {
                    child.node = Arc::new(new_var);
                    child
                }).collect();
        }

        self.skip_token_only(TokenType::Indent)?;
        let mut methods: Vec<GreenChild<GreenMethodDecl>> = vec![];

        while self.current_token().kind == TokenType::KwFun {
            let fun_token = self.current_token().clone();
            let method_start_off = fun_token.span.start_off;
            self.skip_token(); // 'fun'
            let method_name_token = self.current_token().clone();
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

                let name_child = GreenChild {
                    relative_start: 0,
                    node: Arc::new(IdentName { name : param_name_token.text.clone() }),
                };
                let type_child = GreenChild {
                    relative_start: (type_start_off - param_start_off),
                    node: Arc::new(type_str),
                };

                let green_param = GreenParam {
                    name: name_child,
                    type_str: type_child,
                    text_len: param_text_len,
                };

                params.push(GreenChild {
                    relative_start: (param_start_off - method_start_off),
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

            let return_type_start_off = self.current_token().span.start_off;
            let return_type_str = if self.current_token().kind == TokenType::Arrow {
                self.skip_token(); // '->'
                self.handle_type_name_string()?
            } else {
                self.unknown_type_name()
            };
            let return_type_child = GreenChild {
                relative_start: (return_type_start_off - method_start_off),
                node: Arc::new(return_type_str),
            };

            if self.current_token().kind == TokenType::Semicolon {
                self.skip_token();
            }

            self.skip_token_only(TokenType::NewLine)?;
            self.skip_token_if_newlines()?;

            let method_end_off = self.tokens.data[self.index - 1].span.end_off;
            let method_text_len = (method_end_off - method_start_off);

            let name_child = GreenChild {
                relative_start: (method_name_token.span.start_off - method_start_off),
                node: Arc::new(IdentName { name : method_name_token.text.clone()}),
            };

            let green_method = GreenMethodDecl {
                name: name_child,
                params,
                return_type_str: return_type_child,
                visibility: visibility.clone(),
                text_len: method_text_len,
            };

            methods.push(GreenChild {
                relative_start: (method_start_off - decl_start_off),
                node: Arc::new(green_method),
            });
        }

        self.skip_token_if_newlines()?;
        self.skip_token_only(TokenType::Dedent)?;

        let decl_end_off = self.tokens.data[self.index - 1].span.end_off;

        let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
            relative_start: (span.start_off - decl_start_off),
            node: Arc::new(ga),
        }).collect();

        let name_child = GreenChild {
            relative_start: (name_start_off - decl_start_off),
            node: Arc::new(IdentName { name : name}),
        };

        let green_decl = GreenDecl {
            name: name_child,
            visibility,
            kind: GreenDeclKind::Abstract {
                super_abst: impls,
                generic_vars: generic_var_children,
                methods,
            },
            annotations: ann_children,
            text_len: (decl_end_off - decl_start_off),
        };

        Ok(DeclRedNode {
            span: Span {
                source_id: abst_token.span.source_id,
                start_off: decl_start_off,
                end_off: decl_end_off,
            },
            inner: Arc::new(green_decl),
        })
    }
}