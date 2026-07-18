use std::sync::Arc;
use leafc_coreapi::ast::{AnnotationDecl, DeclNode, DeclNodeKind, DeclRedNode, MethodDecl, Param, TypeNameString, Visibility};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Token, TokenType};
use leafc_coreapi::parser::{ParserError};
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_abstract_decl(
        &mut self,
        visibility: Visibility,
        ann: Vec<AnnotationDecl>
    ) -> Result<DeclRedNode, DiagMsg> {
        self.skip_token();
        let name_token = self.current_token();
        let name = name_token.text.clone();
        let name_span = name_token.span.clone();
        self.skip_token_only(TokenType::Ident)?;

        let mut generic = if self.current_token().kind == TokenType::Lbracket {
            self.handle_generic_param()?
        } else { vec![] };

        let mut impls = vec![];

        if self.current_token().kind == TokenType::KwImpl {
            self.skip_token();
            while self.current_token().kind == TokenType::Ident {
                let name = self.current_token().text.clone();
                self.skip_token();

                if self.current_token().kind == TokenType::Plus {
                    self.skip_token();
                    impls.push(name);
                } else {
                    impls.push(name);
                    break;
                }
            }
        }

        self.skip_token_only(TokenType::NewLine)?;
        if self.current_token().kind == TokenType::KwWhere {
            generic = self.handle_where(generic)?;
        }

        self.skip_token_only(TokenType::Indent)?;
        let mut methods = vec![];

        while self.current_token().kind == TokenType::KwFun {
            self.skip_token();
            let fist_name_token = self.current_token().clone();
            self.skip_token_only(TokenType::Ident)?;

            if self.current_token().kind != TokenType::Lparen {
                return Err(DiagMsg{
                    title: format!("{:?}", ParserError::FunctionDeclarationMissingParameterList),
                    msg: "function declare missing parameter list".to_string(),
                    span: self.current_token().span.clone(),
                })
            }
            self.skip_token(); // '('
            let mut params = vec![];

            while self.current_token().kind != TokenType::Rparen {
                let param_name = self.current_token().text.clone();
                self.skip_token_only(TokenType::Ident)?;

                let type_str = if self.current_token().kind == TokenType::Colon {
                    self.skip_token();
                    self.handle_type_name_string()?
                } else {
                    self.unknown_type_name()
                };

                if self.current_token().kind == TokenType::Comma {
                    self.skip_token();
                    params.push(Param {
                        name: param_name,
                        type_str,
                        span: self.current_token().span.clone(),
                    });
                } else if self.current_token().kind == TokenType::Rparen {
                    params.push(Param {
                        name: param_name,
                        type_str,
                        span: self.current_token().span.clone(),
                    });
                    break;
                } else {
                    return Err(DiagMsg{
                        title: format!("{:?}", ParserError::InvalidFunctionParameterList),
                        msg: "invalid function parameter list".to_string(),
                        span: self.current_token().span.clone(),
                    })
                }
            }
            self.skip_token(); // ')'

            let return_type_str = if self.current_token().kind == TokenType::Arrow {
                self.skip_token();
                self.handle_type_name_string()?
            } else {
                self.unknown_type_name()
            };

            if self.current_token().kind == TokenType::Semicolon {
                self.skip_token();
                methods.push(MethodDecl {
                    name: fist_name_token.text.clone(),
                    params,
                    return_type_str,
                    span: fist_name_token.span.clone(),
                    visibility: visibility.clone(),
                });
            }
            self.skip_token_only(TokenType::NewLine)?;
            self.skip_token_if_newlines()?;
        }

        self.skip_token_if_newlines()?;
        self.skip_token_only(TokenType::Dedent)?;

        Ok(DeclRedNode {
            span: name_span,
            inner: Arc::new(DeclNode {
                name,
                visibility,
                annotations: ann,
                kind: DeclNodeKind::Abstract {
                    has_abst: impls,
                    generic_vars: generic,
                    methods,
                },
            }),
        })
    }
}