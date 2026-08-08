mod parse_expr;
mod parse_import;
mod parse_decl;

use intervaltree::IntervalTree;
use leaf_coreapi;
use leaf_coreapi::ast::{CrateAst, FileRedUnit, GreenAnnotation, GreenChild, GreenDecl, GreenFileUnit, GreenPureStaticPath, GreenTupleElement, GreenWhereClause, GreenWhereConstraint, HasTextLen, IdentName, TypeName, Visibility};
use leaf_coreapi::crate_meta::{BuiltinOperator, OperatorDef, OperatorKind};
use leaf_coreapi::diagnose::{CompileTimeErrorKind, DiagCtx, ErrorKind, LocalizedMessage, MsgKind, ParserErrorKind};
use leaf_coreapi::lexer::{LexerApi, TokenStream};
use leaf_coreapi::parser::ParserApi;
use leaf_coreapi::source::{AbsPathSourceMap, SourceId, SourcePool, Span};
use leaf_coreapi::tokens_pass::TokenPassApi;
use leaf_lexer::Lexer;
use leaf_tokenpass::Preprocessor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use leaf_coreapi::token::{Token, TokenType};

fn make_parser_error(kind: ParserErrorKind, span: Span, args: impl IntoIterator<Item = impl ToString>) -> ErrorKind {
    let _ = args;
    ErrorKind::CompileTimeError(CompileTimeErrorKind::ParserError(kind))
}

pub struct Parser<'a> {
    pub diag: &'a mut DiagCtx,
    pub dir_abs_path: PathBuf,
    pub tokens: TokenStream,
    pub index: usize,
    pub source_pool: &'a SourcePool,
    pub abs_path_sources: &'a AbsPathSourceMap,
    pub ast: CrateAst,
    pub user_operators: &'a HashMap<String, OperatorDef>,
    pub user_op_info: &'a HashMap<String, (usize, OperatorKind)>,
}

impl<'a> Parser<'a> {
    pub const PRIORITY_OFFSET: usize = 5;

    pub fn builtin_priority(op: BuiltinOperator) -> usize {
        match op {
            BuiltinOperator::Or => 10,
            BuiltinOperator::And => 20,
            BuiltinOperator::Eq | BuiltinOperator::Neq
            | BuiltinOperator::Lt | BuiltinOperator::Gt
            | BuiltinOperator::Le | BuiltinOperator::Ge => 30,
            BuiltinOperator::Add | BuiltinOperator::Sub => 40,
            BuiltinOperator::Mul | BuiltinOperator::Div | BuiltinOperator::Mod => 50,
            BuiltinOperator::Not => 70,
        }
    }

    fn mark_start(&self) -> usize {
        self.current_token().span.start_off
    }

    fn text_len_from(&self, start_off: usize) -> usize {
        self.tokens.data[self.index - 1].span.end_off - start_off
    }

    fn child_span(&self, start_off: usize) -> Span {
        Span {
            source_id: self.current_token().span.source_id,
            start_off,
            end_off: self.tokens.data[self.index - 1].span.end_off,
        }
    }

    fn current_token(&self) -> &Token {
        match self.tokens.data.get(self.index) {
            Some(t) => t,
            None => &self.tokens.data[self.index - 1]
        }
    }

    fn skip_token(&mut self) {
        if self.index >= self.tokens.data.len() {
            return;
        }
        self.index += 1;
    }

    fn skip_token_and_get_current(&mut self) -> &Token {
        self.skip_token();
        self.current_token()
    }

    fn skip_layout_tokens(&mut self) -> Result<(), ()> {
        loop {
            match self.current_token().kind {
                TokenType::NewLine | TokenType::Indent | TokenType::Dedent => {
                    self.skip_token();
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn skip_token_only(&mut self, expected: TokenType) -> Result<(), ()> {
        let tok = self.current_token();
        if tok.kind == expected {
            self.skip_token();
            return Ok(());
        }
        let found = tok.kind;
        let span = tok.span.clone();
        self.diag.emit_error(
            make_parser_error(ParserErrorKind::TokenExpect { expected, found }, span.clone(), std::iter::empty::<&str>()),
            span,
            LocalizedMessage::new(MsgKind::ParserTokenExpect, [format!("{:?}", expected), format!("{:?}", found)]),
        );
        Err(())
    }

    fn skip_token_if_newlines(&mut self) -> Result<(), ()> {
        while self.current_token().kind == TokenType::NewLine {
            self.skip_token();
        }
        Ok(())
    }

    fn get_unknown_type_name(&self) -> TypeName {
        TypeName::Named {
            path: GreenChild {
                relative_start: 0,
                node: Arc::new(GreenPureStaticPath {
                    segments: vec![],
                    text_len: 0,
                }),
            },
            generics: vec![],
            text_len: 0,
        }
    }

    fn parse_type_name(&mut self) -> Result<TypeName, ()> {
        let start_off = self.mark_start();

        match self.current_token().kind {
            TokenType::KwRef => {
                self.skip_token();
                let is_mut = if self.current_token().kind == TokenType::KwMut {
                    self.skip_token();
                    true
                } else {
                    false
                };
                let inner_start = self.mark_start();
                let inner = self.parse_type_name()?;
                let text_len = self.text_len_from(start_off);
                let inner_child = GreenChild {
                    relative_start: inner_start - start_off,
                    node: Arc::new(inner),
                };
                if is_mut {
                    Ok(TypeName::MutRef { inner: inner_child, text_len })
                } else {
                    Ok(TypeName::Ref { inner: inner_child, text_len })
                }
            }

            TokenType::KwShare => {
                self.skip_token();
                let inner_start = self.mark_start();
                let inner = self.parse_type_name()?;
                let text_len = self.text_len_from(start_off);
                let inner_child = GreenChild {
                    relative_start: inner_start - start_off,
                    node: Arc::new(inner),
                };
                Ok(TypeName::Share { inner: inner_child, text_len })
            }

            TokenType::Lparen => {
                let mut peek_idx = self.index + 1;
                while peek_idx < self.tokens.data.len()
                    && self.tokens.data[peek_idx].kind == TokenType::NewLine
                {
                    peek_idx += 1;
                }
                self.parse_tuple_type(start_off)
            }

            TokenType::KwImpl => {
                self.skip_token();
                let trait_start = self.mark_start();
                let trait_type = self.parse_type_name()?;
                let text_len = self.text_len_from(start_off);
                let trait_child = GreenChild {
                    relative_start: trait_start - start_off,
                    node: Arc::new(trait_type),
                };
                Ok(TypeName::Impl { trait_type: trait_child, text_len })
            }

            TokenType::KwFun => {
                self.parse_fun_type(start_off)
            }

            TokenType::KwTypeOf => {
                self.skip_token();
                self.skip_token_only(TokenType::Lparen)?;
                let expr_start = self.mark_start();
                let expr_red = self.parse_expr()?;
                let expr_child = GreenChild {
                    relative_start: expr_start - start_off,
                    node: Arc::clone(&expr_red.inner),
                };
                self.skip_token_only(TokenType::Rparen)?;
                let text_len = self.text_len_from(start_off);
                Ok(TypeName::Typeof { expr: expr_child, text_len })
            }

            TokenType::Ident if self.current_token().text == "_" => {
                let s = self.mark_start();
                self.skip_token();
                let text_len = self.text_len_from(s);
                Ok(TypeName::Wildcard { text_len })
            }

            _ => {
                let path_start = self.mark_start();
                let path_node = self.parse_pure_static_path()?;
                let mut generics = vec![];

                if self.current_token().kind == TokenType::Lbracket {
                    self.skip_token();
                    while self.current_token().kind != TokenType::Rbracket {
                        let ty = self.parse_type_name()?;
                        generics.push(ty);
                        if self.current_token().kind == TokenType::Comma {
                            self.skip_token();
                        } else if self.current_token().kind == TokenType::Rbracket {
                            break;
                        } else {
                            let span = self.current_token().span.clone();
                            self.diag.emit_error(
                                make_parser_error(ParserErrorKind::InvalidGenericList, span.clone(), std::iter::empty::<&str>()),
                                span,
                                LocalizedMessage::new(MsgKind::ParserInvalidGenericList, std::iter::empty::<String>()),
                            );
                            return Err(());
                        }
                    }
                    self.skip_token();
                }

                let text_len = self.text_len_from(start_off);
                let path_child = GreenChild {
                    relative_start: path_start - start_off,
                    node: Arc::new(path_node),
                };

                Ok(TypeName::Named { path: path_child, generics, text_len })
            }
        }
    }

    fn parse_tuple_type(&mut self, start_off: usize) -> Result<TypeName, ()> {
        self.skip_token_only(TokenType::Lparen)?;
        let mut elements = vec![];

        while self.current_token().kind != TokenType::Rparen {
            let elem_start = self.mark_start();
            let ty_start = self.mark_start();
            let ty = self.parse_type_name()?;
            let mut repeat = None;

            if self.current_token().kind == TokenType::Star {
                self.skip_token();
                let num_token = self.current_token().clone();
                if num_token.kind != TokenType::Int {
                    let span = num_token.span.clone();
                    self.diag.emit_error(
                        make_parser_error(ParserErrorKind::InvalidTupleElement, span.clone(), std::iter::empty::<&str>()),
                        span,
                        LocalizedMessage::new(MsgKind::ParserInvalidTupleElement, std::iter::empty::<String>()),
                    );
                    return Err(());
                }
                let count: usize = match num_token.text.parse() {
                    Ok(c) => c,
                    Err(_) => {
                        let span = num_token.span.clone();
                        self.diag.emit_error(
                            make_parser_error(ParserErrorKind::InvalidTupleElement, span.clone(), std::iter::empty::<&str>()),
                            span,
                            LocalizedMessage::new(MsgKind::ParserInvalidTupleElement, std::iter::empty::<String>()),
                        );
                        return Err(());
                    }
                };
                repeat = Some(count);
                self.skip_token();
            }

            let text_len = self.text_len_from(elem_start);
            let ty_child = GreenChild {
                relative_start: ty_start - elem_start,
                node: Arc::new(ty),
            };

            elements.push(GreenTupleElement {
                ty: ty_child,
                repeat,
                text_len,
            });

            if self.current_token().kind == TokenType::Comma {
                self.skip_token();
            } else if self.current_token().kind == TokenType::Rparen {
                break;
            } else {
                let span = self.current_token().span.clone();
                self.diag.emit_error(
                    make_parser_error(ParserErrorKind::InvalidTupleElement, span.clone(), std::iter::empty::<&str>()),
                    span,
                    LocalizedMessage::new(MsgKind::ParserInvalidTupleElement, std::iter::empty::<String>()),
                );
                return Err(());
            }
        }

        self.skip_token_only(TokenType::Rparen)?;
        let text_len = self.text_len_from(start_off);
        Ok(TypeName::Tuple { elements, text_len })
    }

    fn parse_fun_type(&mut self, start_off: usize) -> Result<TypeName, ()> {
        self.skip_token_only(TokenType::KwFun)?;
        self.skip_token_only(TokenType::Lparen)?;
        let mut params = vec![];

        while self.current_token().kind != TokenType::Rparen {
            let param_start = self.mark_start();
            let param_type = self.parse_type_name()?;
            params.push(GreenChild {
                relative_start: param_start - start_off,
                node: Arc::new(param_type),
            });

            if self.current_token().kind == TokenType::Comma {
                self.skip_token();
            } else if self.current_token().kind == TokenType::Rparen {
                break;
            } else {
                let span = self.current_token().span.clone();
                self.diag.emit_error(
                    make_parser_error(ParserErrorKind::InvalidFunctionType, span.clone(), std::iter::empty::<&str>()),
                    span,
                    LocalizedMessage::new(MsgKind::ParserInvalidFunctionType, std::iter::empty::<String>()),
                );
                return Err(());
            }
        }
        self.skip_token_only(TokenType::Rparen)?;
        self.skip_token_only(TokenType::Arrow)?;
        let ret_start = self.mark_start();
        let return_type = self.parse_type_name()?;
        let text_len = self.text_len_from(start_off);
        let ret_child = GreenChild {
            relative_start: ret_start - start_off,
            node: Arc::new(return_type),
        };
        Ok(TypeName::Fun { params, return_type: ret_child, text_len })
    }

    fn parse_where(
        &mut self,
        base_offset: usize,
    ) -> Result<Option<GreenChild<GreenWhereClause>>, ()> {
        if self.current_token().kind != TokenType::KwWhere {
            return Ok(None);
        }

        let where_start_off = self.mark_start();
        self.skip_token();
        let mut constraints = vec![];

        while self.current_token().kind == TokenType::Ident {
            let constraint_start_off = self.mark_start();
            let name_text = self.current_token().text.clone();
            self.skip_token_only(TokenType::Ident)?;
            let name_child = GreenChild {
                relative_start: 0,
                node: Arc::new(IdentName { name: name_text }),
            };

            self.skip_token_only(TokenType::Colon)?;

            let mut bounds = vec![];
            loop {
                let bound_start_off = self.mark_start();
                let bound_type = self.parse_type_name()?;
                bounds.push(GreenChild {
                    relative_start: bound_start_off - constraint_start_off,
                    node: Arc::new(bound_type),
                });

                if self.current_token().kind == TokenType::Plus {
                    self.skip_token();
                } else {
                    break;
                }
            }

            let constraint_text_len = self.text_len_from(constraint_start_off);
            let constraint = GreenWhereConstraint {
                name: name_child,
                bounds,
                text_len: constraint_text_len,
            };

            constraints.push(GreenChild {
                relative_start: constraint_start_off - where_start_off,
                node: Arc::new(constraint),
            });

            if self.current_token().kind == TokenType::Comma {
                self.skip_token();
            } else if self.current_token().kind == TokenType::NewLine {
                self.skip_token_if_newlines()?;
            } else {
                break;
            }
        }

        let where_text_len = self.text_len_from(where_start_off);
        let where_clause = GreenWhereClause {
            constraints,
            text_len: where_text_len,
        };

        Ok(Some(GreenChild {
            relative_start: where_start_off - base_offset,
            node: Arc::new(where_clause),
        }))
    }

    fn parse_pure_static_path(&mut self) -> Result<GreenPureStaticPath, ()> {
        let start_off = self.mark_start();
        let mut segments = vec![];

        if self.current_token().kind != TokenType::Ident {
            let span = self.current_token().span.clone();
            self.diag.emit_error(
                make_parser_error(ParserErrorKind::TokenExpect { expected: TokenType::Ident, found: self.current_token().kind }, span.clone(), std::iter::empty::<&str>()),
                span,
                LocalizedMessage::new(MsgKind::ParserTokenExpect, [format!("Ident"), format!("{:?}", self.current_token().kind)]),
            );
            return Err(());
        }

        let first_start = self.mark_start();
        let first_name = self.current_token().text.clone();
        self.skip_token();
        segments.push(GreenChild {
            relative_start: first_start - start_off,
            node: Arc::new(IdentName { name: first_name }),
        });

        loop {
            if self.current_token().kind == TokenType::Dot {
                if self.index + 1 < self.tokens.data.len()
                    && self.tokens.data[self.index + 1].kind == TokenType::Ident
                {
                    self.skip_token();
                    let seg_start = self.mark_start();
                    let seg_name = self.current_token().text.clone();
                    self.skip_token();
                    segments.push(GreenChild {
                        relative_start: seg_start - start_off,
                        node: Arc::new(IdentName { name: seg_name }),
                    });
                    continue;
                }
            }
            break;
        }

        let end_off = segments.last().map_or(start_off, |seg| {
            start_off + seg.relative_start + seg.node.text_len()
        });
        let text_len = end_off - start_off;

        Ok(GreenPureStaticPath { segments, text_len })
    }

    pub fn lexer(
        source_id: SourceId,
        code: &String,
        user_operators: &'a HashMap<String, OperatorDef>
    ) -> Result<TokenStream, ()> {
        todo!()
    }

    pub fn pp(diag: &mut DiagCtx, source_id: SourceId, token_stream: &TokenStream) -> Result<TokenStream, ()> {
        let mut pp = Preprocessor::new(&token_stream, source_id);
        let new_tokens = pp.pass().map_err(|err| {
            diag.emit_diag_error(err);
        })?;

        println!("\n\n== token pass ==\n\n");
        for token in &new_tokens.data {
            println!("{:?}", token);
        }
        println!("== === ==");
        Ok(new_tokens)
    }

    fn parse_top(&mut self, module_name: String) -> Result<FileRedUnit, ()> {
        let file_start_off = self.mark_start();
        let mut top_decl_green_children = vec![];
        let mut file_unit_requires_green_children = vec![];

        while self.current_token().kind != TokenType::Eof {
            let anns = self.parse_annotations()?;
            let visibility = self.parse_visibility()?;
            let first_ann_start = anns.first().map(|(_, sp)| sp.start_off);
            let decl_start_off = first_ann_start.unwrap_or_else(|| self.mark_start());

            match self.current_token().kind {
                TokenType::KwUse => {
                    if let Some(req_red) = self.parse_use_decl()? {
                        let relative_start = req_red.span.start_off - file_start_off;
                        file_unit_requires_green_children.push(GreenChild {
                            relative_start,
                            node: Arc::clone(&req_red.green),
                        });
                    }
                },
                TokenType::KwFun => {
                    let decl_red = self.parse_fun_decl(visibility, anns, decl_start_off, false)?;
                    let relative_start = decl_red.span.start_off - file_start_off;
                    top_decl_green_children.push(GreenChild {
                        relative_start,
                        node: Arc::clone(&decl_red.inner),
                    });
                },
                TokenType::KwExternal => {
                    let decl_red = self.parse_external_decl(visibility, anns, decl_start_off)?;
                    let relative_start = decl_red.span.start_off - file_start_off;
                    top_decl_green_children.push(GreenChild {
                        relative_start,
                        node: Arc::clone(&decl_red.inner),
                    });
                },
                TokenType::KwType => {
                    let decl_red = self.parse_type_decl(visibility, anns, decl_start_off)?;
                    let relative_start = decl_red.span.start_off - file_start_off;
                    top_decl_green_children.push(GreenChild {
                        relative_start,
                        node: Arc::clone(&decl_red.inner),
                    });
                },
                TokenType::KwAbst => {
                    let decl_red = self.parse_abstract_decl(visibility, anns, decl_start_off)?;
                    let relative_start = decl_red.span.start_off - file_start_off;
                    top_decl_green_children.push(GreenChild {
                        relative_start,
                        node: Arc::clone(&decl_red.inner),
                    });
                },
                TokenType::KwEffect => {
                    let decl_red = self.parse_effect_decl(visibility, anns, decl_start_off)?;
                    let relative_start = decl_red.span.start_off - file_start_off;
                    top_decl_green_children.push(GreenChild {
                        relative_start,
                        node: Arc::clone(&decl_red.inner),
                    });
                }
                TokenType::KwConst => {
                    let mut peek = self.index + 1;
                    while peek < self.tokens.data.len()
                        && self.tokens.data[peek].kind == TokenType::NewLine
                    {
                        peek += 1;
                    }
                    if peek < self.tokens.data.len()
                        && self.tokens.data[peek].kind == TokenType::KwFun
                    {
                        self.skip_token_only(TokenType::KwConst)?;
                        let decl_red = self.parse_fun_decl(visibility, anns, decl_start_off, true)?;
                        let relative_start = decl_red.span.start_off - file_start_off;
                        top_decl_green_children.push(GreenChild {
                            relative_start,
                            node: Arc::clone(&decl_red.inner),
                        });
                    } else {
                        let decl_red = self.parse_const_decl(visibility, anns, decl_start_off)?;
                        let relative_start = decl_red.span.start_off - file_start_off;
                        top_decl_green_children.push(GreenChild {
                            relative_start,
                            node: Arc::clone(&decl_red.inner),
                        });
                    }
                }
                TokenType::KwGlobal => {
                    let decl_red = self.parse_global_decl(visibility, anns, decl_start_off)?;
                    let relative_start = decl_red.span.start_off - file_start_off;
                    top_decl_green_children.push(GreenChild {
                        relative_start,
                        node: Arc::clone(&decl_red.inner),
                    });
                }
                TokenType::NewLine => self.skip_token(),
                _ => {
                    let span = self.current_token().span.clone();
                    self.diag.emit_error(
                        make_parser_error(ParserErrorKind::InvalidTopDeclaration, span.clone(), std::iter::empty::<&str>()),
                        span,
                        LocalizedMessage::new(MsgKind::ParserInvalidTopDeclaration, std::iter::empty::<String>()),
                    );
                    return Err(());
                }
            }
        }

        let file_end_off = self.current_token().span.end_off;
        let file_text_len = file_end_off - file_start_off;
        let green_file_unit = GreenFileUnit {
            name: GreenChild {
                relative_start: 0,
                node: Arc::new(IdentName { name: module_name }),
            },
            top_decls: top_decl_green_children,
            file_unit_requires: file_unit_requires_green_children,
            text_len: file_text_len,
        };
        let file_span = Span {
            source_id: self.current_token().span.source_id,
            start_off: file_start_off,
            end_off: file_end_off,
        };

        Ok(FileRedUnit {
            span: file_span,
            green: Arc::new(green_file_unit),
        })
    }

    pub fn parse_file_incremental(
        &mut self,
        module_name: String,
        old_file: &GreenFileUnit,
        old_tree: &IntervalTree<usize, Arc<GreenDecl>>,
        affected_range: std::ops::Range<usize>,
    ) -> Result<FileRedUnit, ()> {
        let file_start_off = self.mark_start();
        let mut top_decl_green_children = vec![];
        let mut file_unit_requires_green_children = vec![];

        let mut unaffected_ranges: Vec<std::ops::Range<usize>> = old_tree
            .iter()
            .filter(|e| {
                let r = &e.range;
                r.end <= affected_range.start || r.start >= affected_range.end
            }).map(|e| e.range.clone())
            .collect();
        unaffected_ranges.sort_by_key(|r| r.start);

        let old_decl_map: HashMap<std::ops::Range<usize>, Arc<GreenDecl>> = old_tree
            .iter()
            .map(|e| (e.range.clone(), Arc::clone(&e.value)))
            .collect();

        let skip_to_offset = |tokens: &TokenStream, index: &mut usize, target: usize| {
            while *index < tokens.data.len()
                && tokens.data[*index].span.start_off < target
            {
                *index += 1;
            }
        };

        while self.current_token().kind != TokenType::Eof {
            let current_off = self.current_token().span.start_off;

            if let Some(range) = unaffected_ranges.iter().find(|r| r.contains(&current_off)) {
                let decl = old_decl_map.get(range).expect("decl must exist");
                let relative_start = range.start - file_start_off;
                top_decl_green_children.push(GreenChild {
                    relative_start,
                    node: Arc::clone(decl),
                });
                skip_to_offset(&self.tokens, &mut self.index, range.end);
                continue;
            }

            let anns = self.parse_annotations()?;
            let visibility = self.parse_visibility()?;
            let first_ann_start = anns.first().map(|(_, sp)| sp.start_off);
            let decl_start_off = first_ann_start.unwrap_or_else(|| self.mark_start());

            match self.current_token().kind {
                TokenType::KwUse => {
                    if let Some(req_red) = self.parse_use_decl()? {
                        let relative_start = req_red.span.start_off - file_start_off;
                        file_unit_requires_green_children.push(GreenChild {
                            relative_start,
                            node: Arc::clone(&req_red.green),
                        });
                    }
                }
                TokenType::KwFun => {
                    let decl_red = self.parse_fun_decl(visibility, anns, decl_start_off, false)?;
                    let relative_start = decl_red.span.start_off - file_start_off;
                    top_decl_green_children.push(GreenChild {
                        relative_start,
                        node: Arc::clone(&decl_red.inner),
                    });
                }
                TokenType::KwExternal => {
                    let decl_red = self.parse_external_decl(visibility, anns, decl_start_off)?;
                    let relative_start = decl_red.span.start_off - file_start_off;
                    top_decl_green_children.push(GreenChild {
                        relative_start,
                        node: Arc::clone(&decl_red.inner),
                    });
                }
                TokenType::KwType => {
                    let decl_red = self.parse_type_decl(visibility, anns, decl_start_off)?;
                    let relative_start = decl_red.span.start_off - file_start_off;
                    top_decl_green_children.push(GreenChild {
                        relative_start,
                        node: Arc::clone(&decl_red.inner),
                    });
                }
                TokenType::KwAbst => {
                    let decl_red = self.parse_abstract_decl(visibility, anns, decl_start_off)?;
                    let relative_start = decl_red.span.start_off - file_start_off;
                    top_decl_green_children.push(GreenChild {
                        relative_start,
                        node: Arc::clone(&decl_red.inner),
                    });
                }
                TokenType::KwEffect => {
                    let decl_red = self.parse_effect_decl(visibility, anns, decl_start_off)?;
                    let relative_start = decl_red.span.start_off - file_start_off;
                    top_decl_green_children.push(GreenChild {
                        relative_start,
                        node: Arc::clone(&decl_red.inner),
                    });
                }
                TokenType::KwConst => {
                    let mut peek = self.index + 1;
                    while peek < self.tokens.data.len()
                        && self.tokens.data[peek].kind == TokenType::NewLine
                    {
                        peek += 1;
                    }
                    if peek < self.tokens.data.len()
                        && self.tokens.data[peek].kind == TokenType::KwFun
                    {
                        self.skip_token_only(TokenType::KwConst)?;
                        let decl_red = self.parse_fun_decl(visibility, anns, decl_start_off, true)?;
                        let relative_start = decl_red.span.start_off - file_start_off;
                        top_decl_green_children.push(GreenChild {
                            relative_start,
                            node: Arc::clone(&decl_red.inner),
                        });
                    } else {
                        let decl_red = self.parse_const_decl(visibility, anns, decl_start_off)?;
                        let relative_start = decl_red.span.start_off - file_start_off;
                        top_decl_green_children.push(GreenChild {
                            relative_start,
                            node: Arc::clone(&decl_red.inner),
                        });
                    }
                }
                TokenType::KwGlobal => {
                    let decl_red = self.parse_global_decl(visibility, anns, decl_start_off)?;
                    let relative_start = decl_red.span.start_off - file_start_off;
                    top_decl_green_children.push(GreenChild {
                        relative_start,
                        node: Arc::clone(&decl_red.inner),
                    });
                }
                TokenType::NewLine => self.skip_token(),
                _ => {
                    let span = self.current_token().span.clone();
                    self.diag.emit_error(
                        make_parser_error(ParserErrorKind::InvalidTopDeclaration, span.clone(), std::iter::empty::<&str>()),
                        span,
                        LocalizedMessage::new(MsgKind::ParserInvalidTopDeclaration, std::iter::empty::<String>()),
                    );
                    return Err(());
                }
            }
        }

        let file_end_off = self.current_token().span.end_off;
        let file_text_len = file_end_off - file_start_off;
        let green_file_unit = GreenFileUnit {
            name: GreenChild {
                relative_start: 0,
                node: Arc::new(IdentName { name: module_name }),
            },
            top_decls: top_decl_green_children,
            file_unit_requires: file_unit_requires_green_children,
            text_len: file_text_len,
        };
        let file_span = Span {
            source_id: self.current_token().span.source_id,
            start_off: file_start_off,
            end_off: file_end_off,
        };
        Ok(FileRedUnit {
            span: file_span,
            green: Arc::new(green_file_unit),
        })
    }

    fn parse_annotations(&mut self) -> Result<Vec<(GreenAnnotation, Span)>, ()> {
        let mut anns = vec![];
        while self.current_token().kind == TokenType::Hash {
            let hash_span = self.current_token().span.clone();
            self.skip_token();

            let ann_name = self.current_token().text.clone();
            self.skip_token_only(TokenType::Ident)?;

            let mut ann_args = vec![];
            if self.current_token().kind == TokenType::Lparen {
                self.skip_token();
                let call_span = self.current_token().span.clone();
                while self.current_token().kind != TokenType::Rparen {
                    ann_args.push(self.current_token().text.clone());
                    self.skip_token();
                    if self.current_token().kind == TokenType::Comma {
                        self.skip_token();
                    } else if self.current_token().kind == TokenType::Rparen {
                        break;
                    } else {
                        self.diag.emit_error(
                            make_parser_error(ParserErrorKind::InvalidCallArgumentList, call_span.clone(), std::iter::empty::<&str>()),
                            call_span,
                            LocalizedMessage::new(MsgKind::ParserInvalidCallArgumentList, std::iter::empty::<String>()),
                        );
                        return Err(());
                    }
                }
                self.skip_token_only(TokenType::Rparen)?;
            }
            self.skip_token_only(TokenType::NewLine)?;
            let nl_span = self.tokens.data[self.index - 1].span.clone();
            let text_len = nl_span.end_off - hash_span.start_off;
            let ann_span = Span {
                source_id: hash_span.source_id,
                start_off: hash_span.start_off,
                end_off: nl_span.end_off,
            };
            anns.push((GreenAnnotation {
                name: ann_name,
                args: ann_args,
                text_len,
            }, ann_span));
        }
        Ok(anns)
    }

    fn parse_visibility(&mut self) -> Result<Visibility, ()> {
        if self.current_token().kind == TokenType::KwPub {
            self.skip_token();
            if self.current_token().kind == TokenType::Lparen {
                self.skip_token();
                self.skip_token_only(TokenType::KwExternal)?;
                self.skip_token_only(TokenType::Rparen)?;
                Ok(Visibility::PublicExternal)
            } else {
                Ok(Visibility::Public)
            }
        } else {
            Ok(Visibility::Private)
        }
    }
}

impl<'a> ParserApi<'a> for Parser<'a> {
    fn new(
        dir_abs_path: PathBuf,
        source_pool: &'a SourcePool,
        abs_path_source_map: &'a AbsPathSourceMap,
        user_operators: &'a HashMap<String, OperatorDef>,
        user_op_info: &'a HashMap<String, (usize, OperatorKind)>,
    ) -> Self {
        Parser {
            diag: todo!(),
            dir_abs_path,
            tokens: TokenStream { data: vec![] },
            index: 0,
            source_pool,
            abs_path_sources: abs_path_source_map,
            ast: CrateAst {
                external_requires: vec![],
                file_units: vec![],
            },
            user_operators,
            user_op_info,
        }
    }

    fn parse(mut self, diag: &'a mut DiagCtx) -> Result<CrateAst, ()> {
        self.diag = diag;
        let main_file_path = self.dir_abs_path.join("main.leaf");

        let main_file_source_id = self.abs_path_sources.get(
            &main_file_path.to_str().unwrap().to_string()).unwrap();

        let content = &self.source_pool.0[*main_file_source_id];

        let token = Self::lexer(*main_file_source_id, &content.file_content, &self.user_operators)?;
        self.tokens = Self::pp(self.diag, *main_file_source_id, &token)?;
        self.index = 0;

        let main_module = self.parse_top("main".to_string())?;
        self.ast.file_units.push(main_module);

        Ok(self.ast)
    }
}
