use std::fs;
use leafc_coreapi::ast::Require;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::TokenType;
use leafc_coreapi::parser::{ParserError};
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_use_decl(&mut self) -> Result<Option<Require>, DiagMsg> {
        let start_span = self.current_token().span.clone();
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
                });
            }
        }

        if require_paths.len() == 0 {
            return Err(DiagMsg {
                title: format!("{:?}", ParserError::InvalidImportList),
                msg: "invalid import list".to_string(),
                span: self.current_token().span.clone(),
            });
        }

        if self.current_token().kind == TokenType::Star {
            self.skip_token();
            is_open = true;
        }


        let req = if is_external_module {
            self.ast.external_requires.push( Require {
                path: require_paths,
                only,
                is_open,
                span: start_span,
            });
            None
        } else {
            let mut file_path = self.dir_abs_path.clone();

            for path in &require_paths {
                file_path.push(path);
            }

            file_path = file_path.with_extension("leaf");

            let source_id = self.abs_path_sources.get(
                &file_path.to_str().unwrap().to_string()).unwrap();

            let content = &self.source_pool.0[*source_id];

            let old_tokens = self.tokens.clone();
            let old_index = self.index;

            let token = Self::lexer(*source_id, &content.file_content, self.user_operators)?;
            self.tokens = Self::pp(*source_id, &token)?;
            self.index = 0;

            let module = self.parse_top(
                require_paths[require_paths.len() - 1].clone())?;

            self.ast.file_units.push(module);;

            self.tokens = old_tokens;
            self.index = old_index;

            Some(Require {
                path: require_paths,
                only,
                is_open,
                span: start_span,
            })
        };

        while self.current_token().kind != TokenType::NewLine {
            return Err(DiagMsg {
                title: format!("{:?}", ParserError::InvalidUseDeclaration),
                msg: "invalid use declare".to_string(),
                span: self.current_token().span.clone(),
            });
        }

        Ok(req)
    }
}