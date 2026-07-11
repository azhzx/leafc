use std::fs;
use leafc_coreapi::ast::Require;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::TokenType;
use leafc_coreapi::parser::{ParserError};
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_use_decl(&mut self) -> Result<(), DiagMsg> {
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
                    source: self.current_source
                });
            }
        }

        if require_paths.len() == 0 {
            return Err(DiagMsg {
                title: format!("{:?}", ParserError::InvalidImportList),
                msg: "invalid import list".to_string(),
                span: self.current_token().span.clone(),
                source: self.current_source
            });
        }

        if self.current_token().kind == TokenType::Star {
            self.skip_token();
            is_open = true;
        }


        if is_external_module {
            self.ast.external_requires.push( Require {
                path: require_paths,
                only,
                is_open,
                span: start_span,
            });
        } else {
            let mut file_path = self.dir_abs_path.clone();

            for path in &require_paths {
                file_path.push(path);
            }

            file_path = file_path.with_extension("leaf");

            let content = fs::read_to_string(&file_path).unwrap();

            let source_id = self.source_pool.add_source(
                file_path.to_str().unwrap().to_string(), content.clone());

            let old_tokens = self.tokens.clone();
            let old_index = self.index;
            let old_current_source = self.current_source;

            let token = Self::lexer(source_id, &content)?;
            self.tokens = Self::pp(source_id, &token)?;
            self.index = 0;
            self.current_source = source_id;

            let module = self.parse_top(
                require_paths[require_paths.len() - 1].clone())?;

            self.ast.decl_pool.push(module);

            self.tokens = old_tokens;
            self.index = old_index;
            self.current_source = old_current_source;

        }

        while self.current_token().kind != TokenType::NewLine {
            return Err(DiagMsg {
                title: format!("{:?}", ParserError::InvalidUseDeclaration),
                msg: "invalid use declare".to_string(),
                span: self.current_token().span.clone(),
                source: self.current_source
            });
        }

        Ok(())
    }
}