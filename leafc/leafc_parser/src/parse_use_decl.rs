use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::TokenType;
use leafc_coreapi::parser::{ParserError, Require};
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_use_decl(&mut self) -> Result<(), DiagMsg> {
        self.skip_token_only(TokenType::KwUse)?;
        let mut require_paths = vec![];
        let mut is_external_module = false;
        let mut only = vec![];
        let mut is_open = false;

        if self.current_token().kind == TokenType::At {
            self.skip_token();
            is_external_module = true;
        }

        // use a.b.c
        while self.current_token().kind == TokenType::Ident{
            let name = self.current_token().text.clone();
            self.skip_token_only(TokenType::Ident)?;
            require_paths.push(name);

            if self.current_token().kind == TokenType::Dot {
                self.skip_token();
            } else {
                break;
            }
        }

        if self.current_token().kind == TokenType::KwOnly {
            self.skip_token();
            while self.current_token().kind == TokenType::Ident{
                let name = self.current_token().text.clone();
                self.skip_token_only(TokenType::Ident)?;
                only.push(name);

                if self.current_token().kind == TokenType::Comma {
                    self.skip_token();
                } else {
                    break;
                }
            }

            if only.len() == 0 {
                return Err(DiagMsg {
                    title: format!("{:?}", ParserError::InvalidOnlyList),
                    msg: "invalid only list".to_string(),
                    span: self.current_token().span.clone(),
                    source: self.source
                });
            }
        }

        if require_paths.len() == 0 {
            return Err(DiagMsg {
                title: format!("{:?}", ParserError::InvalidImportList),
                msg: "invalid import list".to_string(),
                span: self.current_token().span.clone(),
                source: self.source
            });
        }

        if self.current_token().kind == TokenType::Star {
            self.skip_token();
            is_open = true;
        }


        self.ast.requires.push( Require {
            path: require_paths,
            is_external_module,
            only,
            is_open,
        });

        while self.current_token().kind != TokenType::NewLine {
            return Err(DiagMsg {
                title: format!("{:?}", ParserError::InvalidUseDeclaration),
                msg: "invalid use declare".to_string(),
                span: self.current_token().span.clone(),
                source: self.source
            });
        }

        Ok(())
    }
}