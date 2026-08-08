use crate::Parser;
use leaf_coreapi::ast::{GreenChild, GreenRequire, IdentName, RequireRedNode};
use leaf_coreapi::diagnose::{CompileTimeErrorKind, ErrorKind, LocalizedMessage, MsgKind, ParserErrorKind};
use leaf_coreapi::token::TokenType;
use leaf_coreapi::source::Span;
use std::sync::Arc;

impl<'a> Parser<'a> {

    /// use x.y only foo *
    pub fn parse_use_decl(&mut self) -> Result<Option<RequireRedNode>, ()> {

        let start_span = self.current_token().span.clone();
        let require_start_off = start_span.start_off;
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
        while self.current_token().kind == TokenType::Ident {
            let ident_start_off = self.current_token().span.start_off;
            let name = self.current_token().text.clone();
            self.skip_token_only(TokenType::Ident)?;
            require_paths.push(GreenChild {
                relative_start: (ident_start_off - require_start_off) ,
                node: Arc::new(IdentName { name : name }),
            });

            if self.current_token().kind == TokenType::Dot {
                self.skip_token();
            } else {
                break;
            }
        }

        if self.current_token().kind == TokenType::KwOnly {
            self.skip_token();
            while self.current_token().kind == TokenType::Ident {
                let ident_start_off = self.current_token().span.start_off;
                let name = self.current_token().text.clone();
                self.skip_token_only(TokenType::Ident)?;
                only.push(GreenChild {
                    relative_start: (ident_start_off - require_start_off) ,
                    node: Arc::new(IdentName { name : name }),
                });

                if self.current_token().kind == TokenType::Comma {
                    self.skip_token();
                } else {
                    break;
                }
            }

            if only.len() == 0 {
                let span = self.current_token().span.clone();
                self.diag.emit_error(
                    ErrorKind::CompileTimeError(CompileTimeErrorKind::ParserError(ParserErrorKind::InvalidOnlyList)),
                    span.clone(),
                    LocalizedMessage::new(MsgKind::ParserInvalidOnlyList, ["invalid only list"]),
                );
                return Err(());
            }
        }

        if require_paths.len() == 0 {
            let span = self.current_token().span.clone();
            self.diag.emit_error(
                ErrorKind::CompileTimeError(CompileTimeErrorKind::ParserError(ParserErrorKind::InvalidImportList)),
                span.clone(),
                LocalizedMessage::new(MsgKind::ParserInvalidImportList, ["invalid import list"]),
            );
            return Err(());
        }

        if self.current_token().kind == TokenType::Star {
            self.skip_token();
            is_open = true;
        }

        let require_end_off = if self.index > 0 {
            self.tokens.data[self.index - 1].span.end_off
        } else {
            require_start_off // 不会发生
        };
        let text_len = (require_end_off - require_start_off);

        let green_require = GreenRequire {
            path: require_paths.clone(),
            only: only.clone(),
            is_open,
            text_len,
        };

        let req_red_node = RequireRedNode {
            span: Span {
                source_id: start_span.source_id,
                start_off: require_start_off,
                end_off: require_end_off,
            },
            green: Arc::new(green_require),
        };

        if is_external_module {
            self.ast.external_requires.push(req_red_node);
            while self.current_token().kind == TokenType::NewLine {
                self.skip_token();
            }
            Ok(None)
        } else {
            let mut file_path = self.dir_abs_path.clone();
            for path_seg in &require_paths {
                file_path.push(path_seg.node.name.clone());
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
                require_paths.last().unwrap().node.name.clone())?;

            self.ast.file_units.push(module);

            self.tokens = old_tokens;
            self.index = old_index;

            while self.current_token().kind == TokenType::NewLine {
                self.skip_token();
            }
            Ok(Some(req_red_node))
        }
    }
}