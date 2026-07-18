use std::sync::Arc;
use leafc_coreapi::ast::{AnnotationDecl, DeclNode, DeclNodeKind, DeclRedNode, Param, Visibility};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::TokenType;
use leafc_coreapi::parser::ParserError;
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_external_decl(
        &mut self,
        visibility: Visibility,
        ann: Vec<AnnotationDecl>
    ) -> Result<DeclRedNode, DiagMsg> {
        self.skip_token();

        if self.current_token().kind == TokenType::KwCType {
            self.skip_token();
            let name_token = self.current_token();
            let name = name_token.text.clone();
            let span = name_token.span.clone();
            self.skip_token_only(TokenType::Ident)?;
            self.skip_token_only(TokenType::Semicolon)?;
            self.skip_token_only(TokenType::NewLine)?;


            return Ok(DeclRedNode {
                span,
                inner: Arc::new(DeclNode {
                    name,
                    visibility,
                    kind: DeclNodeKind::CType,
                    annotations: ann,
                }),
            });
        }

        self.skip_token_only(TokenType::KwFun)?;
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

        let sym_name = if self.current_token().kind == TokenType::Eq {
            self.skip_token();
            let name = self.current_token().text.clone();
            self.skip_token_only(TokenType::String)?;
            name
        } else {
            fist_name_token.text.clone()
        };

        self.skip_token_only(TokenType::Semicolon)?;

        self.skip_token_only(TokenType::NewLine)?;
        Ok(DeclRedNode {
            span: fist_name_token.span.clone(),
            inner: Arc::new(DeclNode {
                name: fist_name_token.text,
                visibility,
                kind: DeclNodeKind::External {
                    sym_name,
                    params,
                    return_type_str,
                },
                annotations: ann,
            }),
        })
    }
}