use leafc_coreapi::ast::{DeclNode, Param, TypeNameString, Visibility};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Token, TokenType};
use leafc_coreapi::parser::{ParserError, ParserResult};
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_fun_decl(&mut self, visibility: Visibility) -> Result<(), DiagMsg> {
        self.skip_token();
        let fist_name_token = self.current_token().clone();
        self.skip_token_if(TokenType::Ident)?;

        if self.current_token().kind != TokenType::Lparen {
            return Err(DiagMsg{
                title: format!("{:?}", ParserError::FunctionDeclareMissingParameterList),
                msg: "function declare missing parameter list".to_string(),
                span: self.current_token().span.clone(),
                source: self.source
            })
        }
        self.skip_token(); // '('
        let mut params = vec![];

        while self.current_token().kind != TokenType::Rparen {
            let param_name = self.current_token().text.clone();
            self.skip_token_if(TokenType::Ident)?;

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
                    source: self.source
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
            self.ast.decl_pool.push( DeclNode::FunDecl {
                name: fist_name_token.text.clone(),
                params,
                return_type_str,
                span: fist_name_token.span.clone(),
                visibility
            });
            return Ok(())
        }

        self.skip_token_if(TokenType::NewLine)?;
        self.skip_token_if(TokenType::Indent)?; // indent

        let mut body = vec![];

        while self.current_token().kind != TokenType::Dedent {
            body.push(self.parse_expr()?);
        }

        self.skip_token_if(TokenType::Dedent)?; // dedent

        self.ast.decl_pool.push(DeclNode::Fun {
            name: fist_name_token.text.clone(),
            params,
            return_type_str,
            span: fist_name_token.span.clone(),
            visibility,
            block: body,
        });
        Ok(())
    }
}