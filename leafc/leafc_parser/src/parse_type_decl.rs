use std::sync::Arc;
use leafc_coreapi::ast::{
    GreenAnnotation, GreenChild, GreenCtor, GreenDecl, GreenDeclKind, GreenExpr,
    GreenField, GreenGenericVar, GreenParam, DeclRedNode, ExprRedNode,
    TypeNameString, Visibility,
};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Token, TokenType};
use leafc_coreapi::parser::ParserError;
use leafc_coreapi::source::Span;
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_type_decl(
        &mut self,
        visibility: Visibility,
        annotations: Vec<(GreenAnnotation, Span)>,
        decl_start_off: usize, // 整个声明的起始偏移
    ) -> Result<DeclRedNode, DiagMsg> {
        let type_token = self.current_token().clone();
        self.skip_token(); // 'type'
        let name_token = self.current_token().clone();
        let name_start_off = name_token.span.start_off;
        let name_text = name_token.text.clone();
        self.skip_token_only(TokenType::Ident)?;

        let name_green_child = GreenChild {
            relative_start: (name_start_off - decl_start_off),
            node: Arc::new(name_text),
        };

        if self.current_token().kind == TokenType::Semicolon {
            self.skip_token(); // ';'
            let decl_end_off = self.tokens.data[self.index - 1].span.end_off;

            let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
                relative_start: (span.start_off - decl_start_off),
                node: Arc::new(ga),
            }).collect();

            let text_len = (decl_end_off - decl_start_off);
            let green_decl = GreenDecl {
                name: name_green_child,
                visibility,
                kind: GreenDeclKind::TypeDecl,
                annotations: ann_children,
                text_len,
            };
            return Ok(DeclRedNode {
                span: Span {
                    source_id: type_token.span.source_id,
                    start_off: decl_start_off,
                    end_off: decl_end_off,
                },
                inner: Arc::new(green_decl),
            });
        }

        // 泛型参数
        let (mut generic_var_children,
            generics_start_off
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
        let mut impls: Vec<GreenChild<String>> = vec![];
        if self.current_token().kind == TokenType::KwImpl {
            self.skip_token();
            while self.current_token().kind == TokenType::Ident {
                let impl_name_start = self.current_token().span.start_off;
                let impl_name = self.current_token().text.clone();
                self.skip_token(); // ident

                impls.push(GreenChild {
                    relative_start: (impl_name_start - decl_start_off) as usize,
                    node: Arc::new(impl_name),
                });

                if self.current_token().kind == TokenType::Plus {
                    self.skip_token();
                } else {
                    break;
                }
            }
        }

        match self.current_token().kind {
            TokenType::Eq => {
                // 类型别名
                self.skip_token(); // '='
                let ref_to_start = self.current_token().span.start_off;
                let ref_to = self.handle_type_name_string()?;

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

                let decl_end_off = self.tokens.data[self.index - 1].span.end_off;

                let ref_to_child = GreenChild {
                    relative_start: (ref_to_start - decl_start_off),
                    node: Arc::new(ref_to),
                };

                let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
                    relative_start: (span.start_off - decl_start_off),
                    node: Arc::new(ga),
                }).collect();

                let green_decl = GreenDecl {
                    name: name_green_child,
                    visibility,
                    kind: GreenDeclKind::TypeAlias {
                        ref_to: ref_to_child,
                        has_abst: impls,
                        generic_vars: generic_var_children,
                    },
                    annotations: ann_children,
                    text_len: (decl_end_off - decl_start_off),
                };

                Ok(DeclRedNode {
                    span: Span {
                        source_id: type_token.span.source_id,
                        start_off: decl_start_off,
                        end_off: decl_end_off,
                    },
                    inner: Arc::new(green_decl),
                })
            }
            TokenType::NewLine => {
                self.skip_token();

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

                if self.current_token().kind == TokenType::Ident {
                    // 结构体
                    let mut fields: Vec<GreenChild<GreenField>> = vec![];

                    while self.current_token().kind != TokenType::Dedent {
                        let field_start = self.current_token().span.start_off;
                        let field_name_token = self.current_token().clone();
                        self.skip_token(); // field name

                        self.skip_token_only(TokenType::Colon)?;

                        let type_start = self.current_token().span.start_off;
                        let type_str = self.handle_type_name_string()?;

                        self.skip_token_only(TokenType::NewLine)?;
                        self.skip_token_if_newlines()?;

                        let field_end = self.tokens.data[self.index - 1].span.end_off;
                        let field_text_len = (field_end - field_start) as usize;

                        let name_child = GreenChild {
                            relative_start: 0,
                            node: Arc::new(field_name_token.text.clone()),
                        };
                        let type_child = GreenChild {
                            relative_start: (type_start - field_start),
                            node: Arc::new(type_str),
                        };

                        let green_field = GreenField {
                            name: name_child,
                            type_str: type_child,
                            text_len: field_text_len,
                        };

                        fields.push(GreenChild {
                            relative_start: (field_start - decl_start_off),
                            node: Arc::new(green_field),
                        });
                    }

                    self.skip_token_if_newlines()?;
                    self.skip_token_only(TokenType::Dedent)?;
                    let decl_end_off = self.tokens.data[self.index - 1].span.end_off;

                    let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
                        relative_start: (span.start_off - decl_start_off) as usize,
                        node: Arc::new(ga),
                    }).collect();

                    let green_decl = GreenDecl {
                        name: name_green_child,
                        visibility,
                        kind: GreenDeclKind::TypeStruct {
                            fields,
                            has_abst: impls,
                            generic_vars: generic_var_children,
                        },
                        annotations: ann_children,
                        text_len: (decl_end_off - decl_start_off) as usize,
                    };

                    Ok(DeclRedNode {
                        span: Span {
                            source_id: type_token.span.source_id,
                            start_off: decl_start_off,
                            end_off: decl_end_off,
                        },
                        inner: Arc::new(green_decl),
                    })
                } else if self.current_token().kind == TokenType::Pipe {
                    // ADT
                    let mut ctors: Vec<GreenChild<GreenCtor>> = vec![];

                    while self.current_token().kind != TokenType::Dedent {
                        self.skip_token_only(TokenType::Pipe)?;
                        let ctor_start = self.current_token().span.start_off;
                        let ctor_name_token = self.current_token().clone();
                        self.skip_token_only(TokenType::Ident)?;

                        let ctor_generic_children = if self.current_token().kind == TokenType::Lbracket {
                            let (children, gen_start) = self.handle_generic_param()?;
                            children.into_iter().map(|mut child| {
                                child.relative_start = (gen_start + child.relative_start - ctor_start) as usize;
                                child
                            }).collect::<Vec<_>>()
                        } else {
                            vec![]
                        };

                        let mut ctor_from_type = self.unknown_type_name();
                        let mut ctor_from_start = self.current_token().span.start_off;
                        let mut ctor_return_type = self.unknown_type_name();
                        let mut ctor_return_start = self.current_token().span.start_off;

                        if self.current_token().kind == TokenType::KwOf {
                            self.skip_token();
                            ctor_from_start = self.current_token().span.start_off;
                            ctor_from_type = self.handle_type_name_string()?;
                            if self.current_token().kind == TokenType::Arrow {
                                self.skip_token();
                                ctor_return_start = self.current_token().span.start_off;
                                ctor_return_type = self.handle_type_name_string()?;
                            }
                        }

                        self.skip_token_only(TokenType::NewLine)?;
                        self.skip_token_if_newlines()?;

                        let ctor_end = self.tokens.data[self.index - 1].span.end_off;
                        let ctor_text_len = (ctor_end - ctor_start);

                        let name_child = GreenChild {
                            relative_start: 0,
                            node: Arc::new(ctor_name_token.text.clone()),
                        };
                        let from_child = GreenChild {
                            relative_start: (ctor_from_start - ctor_start),
                            node: Arc::new(ctor_from_type),
                        };
                        let return_child = GreenChild {
                            relative_start: (ctor_return_start - ctor_start),
                            node: Arc::new(ctor_return_type),
                        };

                        let green_ctor = GreenCtor {
                            name: name_child,
                            generic_vars: ctor_generic_children,
                            from_type_str: from_child,
                            return_type_str: return_child,
                            visibility: visibility.clone(),
                            text_len: ctor_text_len,
                        };

                        ctors.push(GreenChild {
                            relative_start: (ctor_start - decl_start_off),
                            node: Arc::new(green_ctor),
                        });
                    }

                    self.skip_token_if_newlines()?;
                    self.skip_token_only(TokenType::Dedent)?;
                    let decl_end_off = self.tokens.data[self.index - 1].span.end_off;

                    let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
                        relative_start: (span.start_off - decl_start_off),
                        node: Arc::new(ga),
                    }).collect();

                    let green_decl = GreenDecl {
                        name: name_green_child,
                        visibility,
                        kind: GreenDeclKind::ADT {
                            ctors,
                            has_abst: impls,
                            generic_vars: generic_var_children,
                        },
                        annotations: ann_children,
                        text_len: (decl_end_off - decl_start_off),
                    };

                    Ok(DeclRedNode {
                        span: Span {
                            source_id: type_token.span.source_id,
                            start_off: decl_start_off,
                            end_off: decl_end_off,
                        },
                        inner: Arc::new(green_decl),
                    })
                } else {
                    Err(DiagMsg {
                        title: format!("{:?}", ParserError::InvalidTypeDeclaration),
                        msg: "invalid type declaration".to_string(),
                        span: self.current_token().span.clone(),
                    })
                }
            }
            _ => Err(DiagMsg {
                title: format!("{:?}", ParserError::InvalidTypeDeclaration),
                msg: "invalid type declaration".to_string(),
                span: self.current_token().span.clone(),
            }),
        }
    }
}