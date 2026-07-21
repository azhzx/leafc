use crate::Parser;
use leafc_coreapi::ast::{
    DeclRedNode, GreenAnnotation, GreenChild, GreenCtor, GreenDecl, GreenDeclKind,
    GreenEffectControl, GreenField, GreenGenericVar, GreenMethodDecl, GreenParam
    , IdentName, Visibility,
};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::TokenType;
use leafc_coreapi::parser::ParserError;
use leafc_coreapi::source::Span;
use std::sync::Arc;

impl<'a> Parser<'a> {
    fn parse_generic_params(&mut self)
        -> Result<(Vec<GreenChild<GreenGenericVar>>, usize), DiagMsg> {
        let list_start_off = self.current_token().span.start_off;
        self.skip_token_only(TokenType::Lbracket)?;
        let mut children = vec![];

        while self.current_token().kind != TokenType::Rbracket {
            let param_start_off = self.current_token().span.start_off;
            let param_name_token = self.current_token().clone();
            self.skip_token_only(TokenType::Ident)?;

            let end_off = if self.current_token().kind == TokenType::Comma {
                self.current_token().span.start_off
            } else if self.current_token().kind == TokenType::Rbracket {
                self.current_token().span.start_off
            } else {
                return Err(DiagMsg {
                    title: format!("{:?}", ParserError::InvalidGenericParameterList),
                    msg: "invalid generic parameter list".to_string(),
                    span: self.current_token().span.clone(),
                });
            };

            let param_text_len = (end_off - param_start_off) as usize;
            let name_child = GreenChild {
                relative_start: 0,
                node: Arc::new(IdentName { name: param_name_token.text.clone() }),
            };

            let green_var = GreenGenericVar {
                name: name_child,
                constraint: vec![], // 行内约束暂不处理，统一由 where 子句承载
                text_len: param_text_len,
            };

            children.push(GreenChild {
                relative_start: (param_start_off - list_start_off) as usize,
                node: Arc::new(green_var),
            });

            if self.current_token().kind == TokenType::Comma {
                self.skip_token();
            } else if self.current_token().kind == TokenType::Rbracket {
                break;
            }
        }
        self.skip_token_only(TokenType::Rbracket)?;
        Ok((children, list_start_off))
    }

    /// const name = expr
    pub fn parse_const_decl(
        &mut self,
        visibility: Visibility,
        annotations: Vec<(GreenAnnotation, Span)>,
        decl_start_off: usize,
    ) -> Result<DeclRedNode, DiagMsg> {
        let const_token = self.current_token().clone();
        self.skip_token_only(TokenType::KwConst)?;

        let name_token = self.current_token().clone();
        let name_start_off = name_token.span.start_off;
        self.skip_token_only(TokenType::Ident)?;

        self.skip_token_only(TokenType::Eq)?;
        let expr_red = self.parse_expr()?;
        let expr_start = expr_red.span.start_off;
        self.skip_token_only(TokenType::NewLine)?;

        let decl_end_off = self.tokens.data[self.index - 1].span.end_off;
        let text_len = (decl_end_off - decl_start_off) as usize;

        let name_child = GreenChild {
            relative_start: (name_start_off - decl_start_off) as usize,
            node: Arc::new(IdentName { name: name_token.text.clone() }),
        };
        let expr_child = GreenChild {
            relative_start: (expr_start - decl_start_off) as usize,
            node: expr_red.inner.clone(),
        };

        let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
            relative_start: (span.start_off - decl_start_off) as usize,
            node: Arc::new(ga),
        }).collect();

        let green_decl = GreenDecl {
            name: name_child,
            visibility,
            kind: GreenDeclKind::Const { expr: expr_child },
            annotations: ann_children,
            text_len,
        };

        Ok(DeclRedNode {
            span: Span {
                source_id: const_token.span.source_id,
                start_off: decl_start_off,
                end_off: decl_end_off,
            },
            inner: Arc::new(green_decl),
        })
    }

    /// global name = expr
    pub fn parse_global_decl(
        &mut self,
        visibility: Visibility,
        annotations: Vec<(GreenAnnotation, Span)>,
        decl_start_off: usize,
    ) -> Result<DeclRedNode, DiagMsg> {
        let global_token = self.current_token().clone();
        self.skip_token_only(TokenType::KwGlobal)?;

        let name_token = self.current_token().clone();
        let name_start_off = name_token.span.start_off;
        self.skip_token_only(TokenType::Ident)?;

        self.skip_token_only(TokenType::Eq)?;
        let expr_red = self.parse_expr()?;
        let expr_start = expr_red.span.start_off;
        self.skip_token_only(TokenType::NewLine)?;

        let decl_end_off = self.tokens.data[self.index - 1].span.end_off;
        let text_len = (decl_end_off - decl_start_off) as usize;

        let name_child = GreenChild {
            relative_start: (name_start_off - decl_start_off) as usize,
            node: Arc::new(IdentName { name: name_token.text.clone() }),
        };
        let expr_child = GreenChild {
            relative_start: (expr_start - decl_start_off) as usize,
            node: expr_red.inner.clone(),
        };

        let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
            relative_start: (span.start_off - decl_start_off) as usize,
            node: Arc::new(ga),
        }).collect();

        let green_decl = GreenDecl {
            name: name_child,
            visibility,
            kind: GreenDeclKind::Global { expr: expr_child },
            annotations: ann_children,
            text_len,
        };

        Ok(DeclRedNode {
            span: Span {
                source_id: global_token.span.source_id,
                start_off: decl_start_off,
                end_off: decl_end_off,
            },
            inner: Arc::new(green_decl),
        })
    }

    /// effect decl
    pub fn parse_effect_decl(
        &mut self,
        visibility: Visibility,
        annotations: Vec<(GreenAnnotation, Span)>,
        decl_start_off: usize,
    ) -> Result<DeclRedNode, DiagMsg> {
        let effect_token = self.current_token().clone();
        self.skip_token_only(TokenType::KwEffect)?;

        let name_token = self.current_token().clone();
        let name_start_off = name_token.span.start_off;
        self.skip_token_only(TokenType::Ident)?;

        // 效应没有泛型参数和 where 子句，直接进入效应体
        self.skip_token_only(TokenType::NewLine)?;
        self.skip_token_only(TokenType::Indent)?;

        let mut controls = vec![];

        while self.current_token().kind != TokenType::Dedent {
            // 每个控制操作以 `|` 开头
            self.skip_token_only(TokenType::Pipe)?;

            let control_start = self.current_token().span.start_off;
            let control_name_token = self.current_token().clone();
            self.skip_token_only(TokenType::Ident)?;

            // 解析参数列表 (可选)
            let mut params = vec![];
            if self.current_token().kind == TokenType::Lparen {
                self.skip_token(); // '('
                while self.current_token().kind != TokenType::Rparen {
                    let param_start_off = self.current_token().span.start_off;
                    let param_name_token = self.current_token().clone();
                    self.skip_token_only(TokenType::Ident)?;

                    let (type_str, type_start_off) = if self.current_token().kind == TokenType::Colon {
                        self.skip_token();
                        let ts = self.current_token().span.start_off;
                        let type_name = self.parse_type_name()?;
                        (type_name, ts)
                    } else {
                        let unknown = self.get_unknown_type_name();
                        let ts = self.current_token().span.start_off;
                        (unknown, ts)
                    };

                    let prev_token_end = self.tokens.data[self.index - 1].span.end_off;
                    let param_text_len = (prev_token_end - param_start_off) as usize;

                    let name_child = GreenChild {
                        relative_start: 0,
                        node: Arc::new(IdentName { name: param_name_token.text.clone() }),
                    };
                    let type_child = GreenChild {
                        relative_start: (type_start_off - param_start_off) as usize,
                        node: Arc::new(type_str),
                    };
                    let green_param = GreenParam {
                        name: name_child,
                        type_str: type_child,
                        text_len: param_text_len,
                    };
                    params.push(GreenChild {
                        relative_start: (param_start_off - control_start) as usize,
                        node: Arc::new(green_param),
                    });

                    if self.current_token().kind == TokenType::Comma {
                        self.skip_token();
                    } else if self.current_token().kind == TokenType::Rparen {
                        break;
                    } else {
                        return Err(DiagMsg {
                            title: format!("{:?}", ParserError::InvalidFunctionParameterList),
                            msg: "invalid parameter list in effect control".to_string(),
                            span: self.current_token().span.clone(),
                        });
                    }
                }
                self.skip_token_only(TokenType::Rparen)?;
            }

            // 解析返回类型 (可选)
            let return_type_start_off = self.current_token().span.start_off;
            let return_type = if self.current_token().kind == TokenType::Arrow {
                self.skip_token();
                self.parse_type_name()?
            } else {
                self.get_unknown_type_name()
            };

            let control_end_off = self.tokens.data[self.index - 1].span.end_off;
            let control_text_len = (control_end_off - control_start) as usize;

            let name_child = GreenChild {
                relative_start: 0,
                node: Arc::new(IdentName { name: control_name_token.text.clone() }),
            };
            let return_child = GreenChild {
                relative_start: (return_type_start_off - control_start) as usize,
                node: Arc::new(return_type),
            };

            let control = GreenEffectControl {
                name: name_child,
                params,
                return_type: return_child,
                text_len: control_text_len,
            };
            controls.push(GreenChild {
                relative_start: (control_start - decl_start_off) as usize,
                node: Arc::new(control),
            });

            self.skip_token_only(TokenType::NewLine)?;
            self.skip_token_if_newlines()?;
        }

        self.skip_token_only(TokenType::Dedent)?;
        let decl_end_off = self.tokens.data[self.index - 1].span.end_off;
        let text_len = (decl_end_off - decl_start_off) as usize;

        let name_child = GreenChild {
            relative_start: (name_start_off - decl_start_off) as usize,
            node: Arc::new(IdentName { name: name_token.text.clone() }),
        };
        let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
            relative_start: (span.start_off - decl_start_off) as usize,
            node: Arc::new(ga),
        }).collect();

        let green_decl = GreenDecl {
            name: name_child,
            visibility,
            kind: GreenDeclKind::Effect { controls },
            annotations: ann_children,
            text_len,
        };

        Ok(DeclRedNode {
            span: Span {
                source_id: effect_token.span.source_id,
                start_off: decl_start_off,
                end_off: decl_end_off,
            },
            inner: Arc::new(green_decl),
        })
    }

    /// abst Name [T] impl Foo+Bar where ... \n methods...
    pub fn parse_abstract_decl(
        &mut self,
        visibility: Visibility,
        annotations: Vec<(GreenAnnotation, Span)>,
        decl_start_off: usize,
    ) -> Result<DeclRedNode, DiagMsg> {
        let abst_token = self.current_token().clone();
        self.skip_token_only(TokenType::KwAbst)?;
        let name_token = self.current_token().clone();
        let name_start_off = name_token.span.start_off;
        self.skip_token_only(TokenType::Ident)?;

        // 泛型参数
        let (mut generic_var_children, _) = if self.current_token().kind == TokenType::Lbracket {
            let (children, start) = self.parse_generic_params()?;
            let adjusted: Vec<_> = children.into_iter().map(|mut child| {
                child.relative_start += (start - decl_start_off) as usize;
                child
            }).collect();
            (adjusted, start)
        } else {
            (vec![], decl_start_off)
        };

        // impl 列表 (super_abst)
        let mut impls = vec![];
        if self.current_token().kind == TokenType::KwImpl {
            self.skip_token();
            while self.current_token().kind == TokenType::Ident {
                let impl_name_start = self.current_token().span.start_off;
                let impl_name = self.current_token().text.clone();
                self.skip_token();
                impls.push(GreenChild {
                    relative_start: (impl_name_start - decl_start_off) as usize,
                    node: Arc::new(IdentName { name: impl_name }),
                });
                if self.current_token().kind == TokenType::Plus {
                    self.skip_token();
                } else {
                    break;
                }
            }
        }

        self.skip_token_only(TokenType::NewLine)?;

        // where 子句
        let where_clause = if self.current_token().kind == TokenType::KwWhere {
            let wc = self.parse_where(decl_start_off)?;
            wc
        } else {
            None
        };

        self.skip_token_only(TokenType::Indent)?;
        let mut methods = vec![];

        while self.current_token().kind == TokenType::KwFun {
            let fun_token = self.current_token().clone();
            let method_start_off = fun_token.span.start_off;
            self.skip_token(); // 'fun'
            let method_name_token = self.current_token().clone();
            self.skip_token_only(TokenType::Ident)?;

            // 方法没有泛型参数，直接解析参数列表
            self.skip_token_only(TokenType::Lparen)?;
            let mut params = vec![];
            while self.current_token().kind != TokenType::Rparen {
                let param_start_off = self.current_token().span.start_off;
                let param_name_token = self.current_token().clone();
                self.skip_token_only(TokenType::Ident)?;

                let (type_str, type_start_off) = if self.current_token().kind == TokenType::Colon {
                    self.skip_token();
                    let ts = self.current_token().span.start_off;
                    let type_name = self.parse_type_name()?;
                    (type_name, ts)
                } else {
                    let unknown = self.get_unknown_type_name();
                    let ts = self.current_token().span.start_off;
                    (unknown, ts)
                };

                let prev_token_end = self.tokens.data[self.index - 1].span.end_off;
                let param_text_len = (prev_token_end - param_start_off) as usize;
                let name_child = GreenChild {
                    relative_start: 0,
                    node: Arc::new(IdentName { name: param_name_token.text.clone() }),
                };
                let type_child = GreenChild {
                    relative_start: (type_start_off - param_start_off) as usize,
                    node: Arc::new(type_str),
                };
                let green_param = GreenParam {
                    name: name_child,
                    type_str: type_child,
                    text_len: param_text_len,
                };
                params.push(GreenChild {
                    relative_start: (param_start_off - method_start_off) as usize,
                    node: Arc::new(green_param),
                });

                if self.current_token().kind == TokenType::Comma {
                    self.skip_token();
                } else if self.current_token().kind == TokenType::Rparen {
                    break;
                } else {
                    return Err(DiagMsg {
                        title: format!("{:?}", ParserError::InvalidFunctionParameterList),
                        msg: "invalid method parameter list".to_string(),
                        span: self.current_token().span.clone(),
                    });
                }
            }
            self.skip_token_only(TokenType::Rparen)?;

            let return_type_start_off = self.current_token().span.start_off;
            let return_type_str = if self.current_token().kind == TokenType::Arrow {
                self.skip_token();
                self.parse_type_name()?
            } else {
                self.get_unknown_type_name()
            };
            let return_type_child = GreenChild {
                relative_start: (return_type_start_off - method_start_off) as usize,
                node: Arc::new(return_type_str),
            };

            if self.current_token().kind == TokenType::Semicolon {
                self.skip_token();
            }
            self.skip_token_only(TokenType::NewLine)?;
            self.skip_token_if_newlines()?;

            let method_end_off = self.tokens.data[self.index - 1].span.end_off;
            let method_text_len = (method_end_off - method_start_off) as usize;

            let name_child = GreenChild {
                relative_start: (method_name_token.span.start_off - method_start_off) as usize,
                node: Arc::new(IdentName { name: method_name_token.text.clone() }),
            };
            let green_method = GreenMethodDecl {
                name: name_child,
                params,
                return_type_str: return_type_child,
                visibility: visibility.clone(),
                text_len: method_text_len,
            };
            methods.push(GreenChild {
                relative_start: (method_start_off - decl_start_off) as usize,
                node: Arc::new(green_method),
            });
        }

        self.skip_token_if_newlines()?;
        self.skip_token_only(TokenType::Dedent)?;
        let decl_end_off = self.tokens.data[self.index - 1].span.end_off;
        let text_len = (decl_end_off - decl_start_off) as usize;

        let name_child = GreenChild {
            relative_start: (name_start_off - decl_start_off) as usize,
            node: Arc::new(IdentName { name: name_token.text.clone() }),
        };
        let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
            relative_start: (span.start_off - decl_start_off) as usize,
            node: Arc::new(ga),
        }).collect();

        let green_decl = GreenDecl {
            name: name_child,
            visibility,
            kind: GreenDeclKind::Abstract {
                super_abst: impls,
                generic_vars: generic_var_children,
                methods,
                where_clause,
            },
            annotations: ann_children,
            text_len,
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

    /// type Name ...
    pub fn parse_type_decl(
        &mut self,
        visibility: Visibility,
        annotations: Vec<(GreenAnnotation, Span)>,
        decl_start_off: usize,
    ) -> Result<DeclRedNode, DiagMsg> {
        let type_token = self.current_token().clone();
        self.skip_token_only(TokenType::KwType)?;
        let name_token = self.current_token().clone();
        let name_start_off = name_token.span.start_off;
        self.skip_token_only(TokenType::Ident)?;

        // 仅 `type Name;` 前向声明
        if self.current_token().kind == TokenType::Semicolon {
            self.skip_token();
            let decl_end_off = self.tokens.data[self.index - 1].span.end_off;
            let text_len = (decl_end_off - decl_start_off) as usize;
            let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
                relative_start: (span.start_off - decl_start_off) as usize,
                node: Arc::new(ga),
            }).collect();
            let name_child = GreenChild {
                relative_start: (name_start_off - decl_start_off) as usize,
                node: Arc::new(IdentName { name: name_token.text.clone() }),
            };
            let green_decl = GreenDecl {
                name: name_child,
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
        let generic_var_children = if self.current_token().kind == TokenType::Lbracket {
            let (children, start) = self.parse_generic_params()?;
            let adjusted: Vec<_> = children.into_iter().map(|mut child| {
                child.relative_start += (start - decl_start_off) as usize;
                child
            }).collect();
            adjusted
        } else {
            vec![]
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
                    relative_start: (impl_name_start - decl_start_off) as usize,
                    node: Arc::new(IdentName { name: impl_name }),
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
                let ref_to = self.parse_type_name()?;
                self.skip_token_only(TokenType::NewLine)?;

                let where_clause = if self.current_token().kind == TokenType::KwWhere {
                    self.parse_where(decl_start_off)?
                } else {
                    None
                };

                let decl_end_off = self.tokens.data[self.index - 1].span.end_off;
                let text_len = (decl_end_off - decl_start_off) as usize;

                let name_child = GreenChild {
                    relative_start: (name_start_off - decl_start_off) as usize,
                    node: Arc::new(IdentName { name: name_token.text.clone() }),
                };
                let ref_to_child = GreenChild {
                    relative_start: (ref_to_start - decl_start_off) as usize,
                    node: Arc::new(ref_to),
                };
                let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
                    relative_start: (span.start_off - decl_start_off) as usize,
                    node: Arc::new(ga),
                }).collect();

                let green_decl = GreenDecl {
                    name: name_child,
                    visibility,
                    kind: GreenDeclKind::TypeAlias {
                        ref_to: ref_to_child,
                        has_abst: impls,
                        generic_vars: generic_var_children,
                        where_clause,
                    },
                    annotations: ann_children,
                    text_len,
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

                let where_clause = if self.current_token().kind == TokenType::KwWhere {
                    self.parse_where(decl_start_off)?
                } else {
                    None
                };

                self.skip_token_only(TokenType::Indent)?;

                if self.current_token().kind == TokenType::Ident {
                    // 结构体
                    let mut fields = vec![];
                    while self.current_token().kind != TokenType::Dedent {
                        let field_start = self.current_token().span.start_off;
                        let field_name_token = self.current_token().clone();
                        self.skip_token(); // name
                        self.skip_token_only(TokenType::Colon)?;
                        let type_start = self.current_token().span.start_off;
                        let type_str = self.parse_type_name()?;
                        self.skip_token_only(TokenType::NewLine)?;
                        self.skip_token_if_newlines()?;

                        let field_end = self.tokens.data[self.index - 1].span.end_off;
                        let field_text_len = (field_end - field_start) as usize;
                        let name_child = GreenChild {
                            relative_start: 0,
                            node: Arc::new(IdentName { name: field_name_token.text.clone() }),
                        };
                        let type_child = GreenChild {
                            relative_start: (type_start - field_start) as usize,
                            node: Arc::new(type_str),
                        };
                        let green_field = GreenField {
                            name: name_child,
                            type_str: type_child,
                            text_len: field_text_len,
                        };
                        fields.push(GreenChild {
                            relative_start: (field_start - decl_start_off) as usize,
                            node: Arc::new(green_field),
                        });
                    }

                    self.skip_token_if_newlines()?;
                    self.skip_token_only(TokenType::Dedent)?;
                    let decl_end_off = self.tokens.data[self.index - 1].span.end_off;
                    let text_len = (decl_end_off - decl_start_off) as usize;

                    let name_child = GreenChild {
                        relative_start: (name_start_off - decl_start_off) as usize,
                        node: Arc::new(IdentName { name: name_token.text.clone() }),
                    };
                    let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
                        relative_start: (span.start_off - decl_start_off) as usize,
                        node: Arc::new(ga),
                    }).collect();

                    let green_decl = GreenDecl {
                        name: name_child,
                        visibility,
                        kind: GreenDeclKind::TypeStruct {
                            fields,
                            has_abst: impls,
                            generic_vars: generic_var_children,
                            where_clause,
                        },
                        annotations: ann_children,
                        text_len,
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
                    let mut ctors = vec![];
                    while self.current_token().kind != TokenType::Dedent {
                        self.skip_token_only(TokenType::Pipe)?;
                        let ctor_start = self.current_token().span.start_off;
                        let ctor_name_token = self.current_token().clone();
                        self.skip_token_only(TokenType::Ident)?;

                        let ctor_generic_children = if self.current_token().kind == TokenType::Lbracket {
                            let (children, gen_start) = self.parse_generic_params()?;
                            children.into_iter().map(|mut child| {
                                child.relative_start = (gen_start + child.relative_start - ctor_start) as usize;
                                child
                            }).collect::<Vec<_>>()
                        } else {
                            vec![]
                        };

                        let mut ctor_from_type = self.get_unknown_type_name();
                        let mut ctor_from_start = self.current_token().span.start_off;
                        let mut ctor_return_type = self.get_unknown_type_name();
                        let mut ctor_return_start = self.current_token().span.start_off;

                        if self.current_token().kind == TokenType::KwOf {
                            self.skip_token();
                            ctor_from_start = self.current_token().span.start_off;
                            ctor_from_type = self.parse_type_name()?;
                            if self.current_token().kind == TokenType::Arrow {
                                self.skip_token();
                                ctor_return_start = self.current_token().span.start_off;
                                ctor_return_type = self.parse_type_name()?;
                            }
                        }

                        self.skip_token_only(TokenType::NewLine)?;
                        self.skip_token_if_newlines()?;

                        let ctor_end = self.tokens.data[self.index - 1].span.end_off;
                        let ctor_text_len = (ctor_end - ctor_start) as usize;

                        let name_child = GreenChild {
                            relative_start: 0,
                            node: Arc::new(IdentName { name: ctor_name_token.text.clone() }),
                        };
                        let from_child = GreenChild {
                            relative_start: (ctor_from_start - ctor_start) as usize,
                            node: Arc::new(ctor_from_type),
                        };
                        let return_child = GreenChild {
                            relative_start: (ctor_return_start - ctor_start) as usize,
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
                            relative_start: (ctor_start - decl_start_off) as usize,
                            node: Arc::new(green_ctor),
                        });
                    }

                    self.skip_token_if_newlines()?;
                    self.skip_token_only(TokenType::Dedent)?;
                    let decl_end_off = self.tokens.data[self.index - 1].span.end_off;
                    let text_len = (decl_end_off - decl_start_off) as usize;

                    let name_child = GreenChild {
                        relative_start: (name_start_off - decl_start_off) as usize,
                        node: Arc::new(IdentName { name: name_token.text.clone() }),
                    };
                    let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
                        relative_start: (span.start_off - decl_start_off) as usize,
                        node: Arc::new(ga),
                    }).collect();

                    let green_decl = GreenDecl {
                        name: name_child,
                        visibility,
                        kind: GreenDeclKind::ADT {
                            ctors,
                            has_abst: impls,
                            generic_vars: generic_var_children,
                            where_clause,
                        },
                        annotations: ann_children,
                        text_len,
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
                        msg: "invalid type declaration body".to_string(),
                        span: self.current_token().span.clone(),
                    })
                }
            }
            _ => Err(DiagMsg {
                title: format!("{:?}", ParserError::InvalidTypeDeclaration),
                msg: "unexpected token after type name".to_string(),
                span: self.current_token().span.clone(),
            }),
        }
    }

    /// fun Name[T] (params) -> Ret where ... \n body
    pub fn parse_fun_decl(
        &mut self,
        visibility: Visibility,
        annotations: Vec<(GreenAnnotation, Span)>,
    ) -> Result<DeclRedNode, DiagMsg> {
        let first_ann_start = annotations.first().map(|(_, sp)| sp.start_off);
        let fun_token = self.current_token().clone();
        let decl_start_off = first_ann_start.unwrap_or(fun_token.span.start_off);
        self.skip_token_only(TokenType::KwFun)?;

        let name_token = self.current_token().clone();
        let name_start_off = name_token.span.start_off;
        self.skip_token_only(TokenType::Ident)?;

        // 泛型参数
        let generic_var_children = if self.current_token().kind == TokenType::Lbracket {
            let (children, start) = self.parse_generic_params()?;
            let adjusted: Vec<_> = children.into_iter().map(|mut child| {
                child.relative_start += (start - decl_start_off) as usize;
                child
            }).collect();
            adjusted
        } else {
            vec![]
        };

        // 参数列表
        self.skip_token_only(TokenType::Lparen)?;
        let mut params = vec![];
        while self.current_token().kind != TokenType::Rparen {
            let param_start_off = self.current_token().span.start_off;
            let param_name_token = self.current_token().clone();
            self.skip_token_only(TokenType::Ident)?;

            let (type_str, type_start_off) = if self.current_token().kind == TokenType::Colon {
                self.skip_token();
                let ts = self.current_token().span.start_off;
                let type_name = self.parse_type_name()?;
                (type_name, ts)
            } else {
                let unknown = self.get_unknown_type_name();
                let ts = self.current_token().span.start_off;
                (unknown, ts)
            };

            let prev_token_end = self.tokens.data[self.index - 1].span.end_off;
            let param_text_len = (prev_token_end - param_start_off) as usize;
            let name_child = GreenChild {
                relative_start: 0,
                node: Arc::new(IdentName { name: param_name_token.text.clone() }),
            };
            let type_child = GreenChild {
                relative_start: (type_start_off - param_start_off) as usize,
                node: Arc::new(type_str),
            };
            let green_param = GreenParam {
                name: name_child,
                type_str: type_child,
                text_len: param_text_len,
            };
            params.push(GreenChild {
                relative_start: (param_start_off - decl_start_off) as usize,
                node: Arc::new(green_param),
            });

            if self.current_token().kind == TokenType::Comma {
                self.skip_token();
            } else if self.current_token().kind == TokenType::Rparen {
                break;
            } else {
                return Err(DiagMsg {
                    title: format!("{:?}", ParserError::InvalidFunctionParameterList),
                    msg: "invalid function parameter list".to_string(),
                    span: self.current_token().span.clone(),
                });
            }
        }
        self.skip_token_only(TokenType::Rparen)?;

        // 返回类型
        let return_type_start_off = self.current_token().span.start_off;
        let return_type_str = if self.current_token().kind == TokenType::Arrow {
            self.skip_token();
            self.parse_type_name()?
        } else {
            self.get_unknown_type_name()
        };

        self.skip_token_if_newlines()?;

        // where 子句
        let where_clause = if self.current_token().kind == TokenType::KwWhere {
            self.parse_where(decl_start_off)?
        } else {
            None
        };

        let mut block_children = vec![];
        let decl_end_off;

        if self.current_token().kind == TokenType::Semicolon {
            self.skip_token();
            decl_end_off = self.tokens.data[self.index - 1].span.end_off;
        } else {
            self.skip_token_only(TokenType::Indent)?;
            while self.current_token().kind != TokenType::Dedent {
                let expr_red = self.parse_expr()?;
                let expr_span = expr_red.span;
                block_children.push(GreenChild {
                    relative_start: (expr_span.start_off - decl_start_off) as usize,
                    node: expr_red.inner.clone(),
                });
                if self.current_token().kind == TokenType::NewLine {
                    self.skip_token();
                    self.skip_token_if_newlines()?;
                }
            }
            self.skip_token_only(TokenType::Dedent)?;
            decl_end_off = self.tokens.data[self.index - 1].span.end_off;
        }

        let text_len = (decl_end_off - decl_start_off) as usize;

        let name_child = GreenChild {
            relative_start: (name_start_off - decl_start_off) as usize,
            node: Arc::new(IdentName { name: name_token.text.clone() }),
        };
        let return_type_child = GreenChild {
            relative_start: (return_type_start_off - decl_start_off) as usize,
            node: Arc::new(return_type_str),
        };
        let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
            relative_start: (span.start_off - decl_start_off) as usize,
            node: Arc::new(ga),
        }).collect();

        let decl_kind = if block_children.is_empty() {
            GreenDeclKind::FunDecl {
                params,
                return_type_str: return_type_child,
                generic_vars: generic_var_children,
                where_clause,
            }
        } else {
            GreenDeclKind::Fun {
                params,
                return_type_str: return_type_child,
                generic_vars: generic_var_children,
                block: block_children,
                where_clause,
            }
        };

        let green_decl = GreenDecl {
            name: name_child,
            visibility,
            kind: decl_kind,
            annotations: ann_children,
            text_len,
        };

        Ok(DeclRedNode {
            span: Span {
                source_id: fun_token.span.source_id,
                start_off: decl_start_off,
                end_off: decl_end_off,
            },
            inner: Arc::new(green_decl),
        })
    }

    /// 解析外部声明
    pub fn parse_external_decl(
        &mut self,
        visibility: Visibility,
        annotations: Vec<(GreenAnnotation, Span)>,
        decl_start_off: usize,
    ) -> Result<DeclRedNode, DiagMsg> {
        let external_token = self.current_token().clone();
        self.skip_token_only(TokenType::KwExternal)?;

        if self.current_token().kind == TokenType::KwCType {
            self.skip_token(); // 'ctype'
            let name_token = self.current_token().clone();
            let name_start_off = name_token.span.start_off;
            self.skip_token_only(TokenType::Ident)?;
            self.skip_token_only(TokenType::Semicolon)?;

            let decl_end_off = self.tokens.data[self.index - 1].span.end_off;
            let text_len = (decl_end_off - decl_start_off) as usize;

            let name_child = GreenChild {
                relative_start: (name_start_off - decl_start_off) as usize,
                node: Arc::new(IdentName { name: name_token.text.clone() }),
            };
            let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
                relative_start: (span.start_off - decl_start_off) as usize,
                node: Arc::new(ga),
            }).collect();

            let green_decl = GreenDecl {
                name: name_child,
                visibility,
                kind: GreenDeclKind::CType,
                annotations: ann_children,
                text_len,
            };
            return Ok(DeclRedNode {
                span: Span {
                    source_id: external_token.span.source_id,
                    start_off: decl_start_off,
                    end_off: decl_end_off,
                },
                inner: Arc::new(green_decl),
            });
        }

        self.skip_token_only(TokenType::KwFun)?;
        let name_token = self.current_token().clone();
        let name_start_off = name_token.span.start_off;
        self.skip_token_only(TokenType::Ident)?;

        self.skip_token_only(TokenType::Lparen)?;
        let mut params = vec![];
        while self.current_token().kind != TokenType::Rparen {
            let param_start_off = self.current_token().span.start_off;
            let param_name_token = self.current_token().clone();
            self.skip_token_only(TokenType::Ident)?;

            let (type_str, type_start_off) = if self.current_token().kind == TokenType::Colon {
                self.skip_token();
                let ts = self.current_token().span.start_off;
                let type_name = self.parse_type_name()?;
                (type_name, ts)
            } else {
                let unknown = self.get_unknown_type_name();
                let ts = self.current_token().span.start_off;
                (unknown, ts)
            };

            let prev_token_end = self.tokens.data[self.index - 1].span.end_off;
            let param_text_len = (prev_token_end - param_start_off) as usize;
            let name_child = GreenChild {
                relative_start: 0,
                node: Arc::new(IdentName { name: param_name_token.text.clone() }),
            };
            let type_child = GreenChild {
                relative_start: (type_start_off - param_start_off) as usize,
                node: Arc::new(type_str),
            };
            let green_param = GreenParam {
                name: name_child,
                type_str: type_child,
                text_len: param_text_len,
            };
            params.push(GreenChild {
                relative_start: (param_start_off - decl_start_off) as usize,
                node: Arc::new(green_param),
            });

            if self.current_token().kind == TokenType::Comma {
                self.skip_token();
            } else if self.current_token().kind == TokenType::Rparen {
                break;
            } else {
                return Err(DiagMsg {
                    title: format!("{:?}", ParserError::InvalidFunctionParameterList),
                    msg: "invalid external function parameter list".to_string(),
                    span: self.current_token().span.clone(),
                });
            }
        }
        self.skip_token_only(TokenType::Rparen)?;

        let return_type_start_off = self.current_token().span.start_off;
        let return_type_str = if self.current_token().kind == TokenType::Arrow {
            self.skip_token();
            self.parse_type_name()?
        } else {
            self.get_unknown_type_name()
        };

        let sym_name_token = if self.current_token().kind == TokenType::Eq {
            self.skip_token(); // '='
            let token = self.current_token().clone();
            self.skip_token_only(TokenType::String)?;
            token
        } else {
            name_token.clone()
        };

        self.skip_token_only(TokenType::Semicolon)?;
        self.skip_token_only(TokenType::NewLine)?;

        let decl_end_off = self.tokens.data[self.index - 1].span.end_off;
        let text_len = (decl_end_off - decl_start_off) as usize;

        let name_child = GreenChild {
            relative_start: (name_start_off - decl_start_off) as usize,
            node: Arc::new(IdentName { name: name_token.text.clone() }),
        };
        let return_type_child = GreenChild {
            relative_start: (return_type_start_off - decl_start_off) as usize,
            node: Arc::new(return_type_str),
        };
        let sym_name_child = GreenChild {
            relative_start: (sym_name_token.span.start_off - decl_start_off) as usize,
            node: Arc::new(IdentName { name: sym_name_token.text.clone() }),
        };
        let ann_children = annotations.into_iter().map(|(ga, span)| GreenChild {
            relative_start: (span.start_off - decl_start_off) as usize,
            node: Arc::new(ga),
        }).collect();

        let green_decl = GreenDecl {
            name: name_child,
            visibility,
            kind: GreenDeclKind::External {
                sym_name: sym_name_child,
                params,
                return_type_str: return_type_child,
            },
            annotations: ann_children,
            text_len,
        };

        Ok(DeclRedNode {
            span: Span {
                source_id: external_token.span.source_id,
                start_off: decl_start_off,
                end_off: decl_end_off,
            },
            inner: Arc::new(green_decl),
        })
    }
}