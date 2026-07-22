use crate::Parser;
use leafc_coreapi::ast::{AtomExprNode, ExprRedNode, GreenCatchClause, GreenChild, GreenElseIf, GreenExpr, GreenExprKind, GreenMatchArm, GreenPattern, GreenPureStaticPath, GreenStructFieldInit, HasTextLen, IdentName, TypeName};
use leafc_coreapi::crate_meta::OperatorKind;
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::TokenType;
use leafc_coreapi::operators::{token_type_to_operator, Operator};
use leafc_coreapi::parser::ParserError;
use leafc_coreapi::source::Span;
use std::sync::Arc;

impl<'a> Parser<'a> {

    /// pattern
    fn parse_pattern(&mut self) -> Result<GreenPattern, DiagMsg> {
        let start_off = self.current_token().span.start_off;
        let current = self.current_token().clone();

        // 通配符
        if current.kind == TokenType::Underline {
            self.skip_token();
            return Ok(GreenPattern::Wildcard);
        }

        if current.kind == TokenType::KwBinding {
            self.skip_token(); // 'binding'
            let name_token = self.current_token().clone();
            self.skip_token_only(TokenType::Ident)?;
            let ident = IdentName { name: name_token.text.clone() };
            return Ok(GreenPattern::Binding(ident));
        }

        // 字面量
        if matches!(current.kind, TokenType::Int | TokenType::Float | TokenType::String) {
            let atom = self.parse_atom_expr()?;
            return Ok(GreenPattern::Literal(atom));
        }

        if current.kind == TokenType::Ident {
            let type_start = self.current_token().span.start_off;
            let path = self.parse_pure_static_path()?;
            // 构造 type_name
            let type_name = TypeName::Named {
                path: GreenChild {
                    relative_start: 0,
                    node: Arc::new(path.clone()),
                },
                generics: vec![],
                text_len: path.text_len,
            };
            // 期望 '('
            if self.current_token().kind != TokenType::Lparen {
                // 如果没有 '('，则不是构造器，返回错误，或回退（这里要求一定是构造器）
                return Err(DiagMsg {
                    title: format!("{:?}", ParserError::InvalidPattern),
                    msg: "expected '(' after constructor name".to_string(),
                    span: self.current_token().span.clone(),
                });
            }
            self.skip_token(); // '('
            let mut args = vec![];
            while self.current_token().kind != TokenType::Rparen {
                let pat = self.parse_pattern()?;
                args.push(GreenChild {
                    relative_start: 0, // 相对偏移在外层调整
                    node: Arc::new(pat),
                });
                if self.current_token().kind == TokenType::Comma {
                    self.skip_token();
                } else if self.current_token().kind == TokenType::Rparen {
                    break;
                } else {
                    return Err(DiagMsg {
                        title: format!("{:?}", ParserError::InvalidPattern),
                        msg: "invalid pattern argument list".to_string(),
                        span: self.current_token().span.clone(),
                    });
                }
            }
            let rparen_off = self.current_token().span.start_off;
            self.skip_token_only(TokenType::Rparen)?;
            let end_off = self.tokens.data[self.index - 1].span.end_off;
            let text_len = (end_off - start_off) as usize;

            let args: Vec<_> = args.into_iter().map(|mut child| {
                child
            }).collect();

            return Ok(GreenPattern::Constructor {
                type_name: GreenChild {
                    relative_start: (type_start - start_off) as usize,
                    node: Arc::new(type_name),
                },
                args,
                text_len,
            });
        }

        Err(DiagMsg {
            title: format!("{:?}", ParserError::InvalidPattern),
            msg: "unexpected token in pattern".to_string(),
            span: current.span.clone(),
        })
    }

    /// pattern => expr
    fn parse_match_arm(&mut self, match_start: usize) -> Result<GreenMatchArm, DiagMsg> {
        let arm_start = self.current_token().span.start_off;
        let pattern = self.parse_pattern()?;
        let pattern_child = GreenChild {
            relative_start: (arm_start - match_start),
            node: Arc::new(pattern),
        };

        // guard
        let guard = if self.current_token().kind == TokenType::KwIf {
            self.skip_token(); // 'if'
            let guard_start = self.current_token().span.start_off;
            let guard_expr = self.parse_expr()?;
            Some(GreenChild {
                relative_start: (guard_start - match_start),
                node: guard_expr.inner.clone(),
            })
        } else {
            None
        };

        // 期望 `=>`
        self.skip_token_only(TokenType::FatArrow)?;

        let body_start = self.current_token().span.start_off;
        let body_expr = self.parse_expr()?;
        let body_child = GreenChild {
            relative_start: (body_start - match_start),
            node: body_expr.inner.clone(),
        };

        let arm_end = self.tokens.data[self.index - 1].span.end_off;
        let text_len = (arm_end - arm_start);

        Ok(GreenMatchArm {
            pattern: pattern_child,
            guard,
            body: body_child,
            text_len,
        })
    }

    /// when expr
    pub fn parse_match_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let match_token = self.current_token().clone();
        let match_start = match_token.span.start_off;
        self.skip_token_only(TokenType::KwWhen)?;

        let scrutinee_red = self.parse_expr()?;
        let scrutinee_start = scrutinee_red.span.start_off;
        let scrutinee_child = GreenChild {
            relative_start: scrutinee_start - match_start,
            node: scrutinee_red.inner.clone(),
        };

        self.skip_token_only(TokenType::NewLine)?;
        self.skip_token_only(TokenType::Indent)?;

        let mut arms = vec![];
        while self.current_token().kind != TokenType::Dedent {
            let arm = self.parse_match_arm(match_start)?;
            arms.push(GreenChild {
                relative_start: arm.pattern.relative_start,
                // 后面修正
                node: Arc::new(arm),
            });
            if self.current_token().kind == TokenType::NewLine {
                self.skip_token();
                self.skip_token_if_newlines()?;
            }
        }

        self.skip_token_only(TokenType::Dedent)?;
        let end_off = self.tokens.data[self.index - 1].span.end_off;
        let text_len = (end_off - match_start);

        let arms: Vec<_> = arms.into_iter().enumerate().map(|(i, mut child)| {
            child
        }).collect();

        let green = GreenExpr {
            kind: GreenExprKind::Match {
                for_match: scrutinee_child,
                arms,
            },
            text_len,
        };

        Ok(ExprRedNode {
            span: Span {
                source_id: match_token.span.source_id,
                start_off: match_start,
                end_off: end_off,
            },
            inner: Arc::new(green),
        })
    }

    /// raise expr
    pub fn parse_raise_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let raise_token = self.current_token().clone();
        let raise_start = raise_token.span.start_off;
        self.skip_token_only(TokenType::KwRaise)?;

        // 解析效应路径
        let path_start = self.current_token().span.start_off;
        let path = self.parse_pure_static_path()?;
        let segments = &path.segments;

        let effect_path_end = segments.len() - 1;
        let effect_segments = segments[..effect_path_end].to_vec();
        let effect_path = GreenPureStaticPath {
            segments: effect_segments,
            text_len: 0,
        };
        let effect_path_child = GreenChild {
            relative_start: (segments[0].relative_start),
            node: Arc::new(effect_path),
        };

        let control_name_seg = segments.last().unwrap();
        let control_name_child = GreenChild {
            relative_start: (path_start + control_name_seg.relative_start - raise_start) as usize,
            node: control_name_seg.node.clone(),
        };

        self.skip_token_only(TokenType::Lparen)?;
        let mut args = vec![];
        while self.current_token().kind != TokenType::Rparen {
            let arg_red = self.parse_expr()?;
            args.push(GreenChild {
                relative_start: arg_red.span.start_off - raise_start,
                node: arg_red.inner.clone(),
            });
            if self.current_token().kind == TokenType::Comma {
                self.skip_token();
            } else if self.current_token().kind == TokenType::Rparen {
                break;
            } else {
                return Err(DiagMsg {
                    title: format!("{:?}", ParserError::InvalidCallArgumentList),
                    msg: "invalid raise argument list".to_string(),
                    span: self.current_token().span.clone(),
                });
            }
        }
        self.skip_token_only(TokenType::Rparen)?;

        let end_off = self.tokens.data[self.index - 1].span.end_off;
        let text_len = (end_off - raise_start);

        let green = GreenExpr {
            kind: GreenExprKind::Raise {
                effect_path: effect_path_child,
                control_name: control_name_child,
                args,
            },
            text_len,
        };

        Ok(ExprRedNode {
            span: Span {
                source_id: raise_token.span.source_id,
                start_off: raise_start,
                end_off,
            },
            inner: Arc::new(green),
        })
    }

    /// AE handler
    pub fn parse_with_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let with_token = self.current_token().clone();
        let with_start = with_token.span.start_off;
        self.skip_token_only(TokenType::KwWith)?;

        let handler_red = self.parse_expr()?;
        let handler_start = handler_red.span.start_off;
        let handler_child = GreenChild {
            relative_start: handler_start - with_start,
            node: handler_red.inner.clone(),
        };

        self.skip_token_if_newlines()?;

        let mut clauses = vec![];
        while self.current_token().kind == TokenType::KwCatch {
            self.skip_token_only(TokenType::KwCatch)?;
            let catch_start = self.current_token().span.start_off;

            let control_path = self.parse_pure_static_path()?;
            let control_path_child = GreenChild {
                relative_start: 0,
                node: Arc::new(control_path),
            };

            self.skip_token_only(TokenType::Lparen)?;
            let mut params = vec![];
            while self.current_token().kind != TokenType::Rparen {
                let pat_start = self.current_token().span.start_off;
                let pat = self.parse_pattern()?;
                params.push(GreenChild {
                    relative_start: (pat_start - catch_start),
                    node: Arc::new(pat),
                });
                if self.current_token().kind == TokenType::Comma {
                    self.skip_token();
                } else if self.current_token().kind == TokenType::Rparen {
                    break;
                } else {
                    return Err(DiagMsg {
                        title: format!("{:?}", ParserError::InvalidPattern),
                        msg: "invalid catch parameter pattern".to_string(),
                        span: self.current_token().span.clone(),
                    });
                }
            }
            self.skip_token_only(TokenType::Rparen)?;

            self.skip_token_only(TokenType::NewLine)?;
            let body_red = self.parse_block_expr()?;
            let body_start = body_red.span.start_off;
            let body_child = GreenChild {
                relative_start: (body_start - catch_start),
                node: body_red.inner.clone(),
            };

            let catch_end = catch_start + body_child.relative_start + body_child.node.text_len;
            let catch_text_len = catch_end - catch_start;

            let catch_clause = GreenCatchClause {
                control_static_path: control_path_child,
                params,
                body: body_child,
                text_len: catch_text_len,
            };

            clauses.push(GreenChild {
                relative_start: (catch_start - with_start),
                node: Arc::new(catch_clause),
            });
        }

        let end_off = if let Some(last_clause) = clauses.last() {
            with_start + last_clause.relative_start + last_clause.node.text_len
        } else {
            handler_child.relative_start + handler_child.node.text_len + with_start
        };
        let text_len = end_off - with_start;

        let green = GreenExpr {
            kind: GreenExprKind::With {
                handler_expr: handler_child,
                clauses,
            },
            text_len,
        };

        Ok(ExprRedNode {
            span: Span {
                source_id: with_token.span.source_id,
                start_off: with_start,
                end_off,
            },
            inner: Arc::new(green),
        })
    }

    /// resume
    pub fn parse_resume_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let resume_token = self.current_token().clone();
        let resume_start = resume_token.span.start_off;
        self.skip_token_only(TokenType::KwResume)?;

        let expr_red = self.parse_expr()?;
        let expr_start = expr_red.span.start_off;
        let end_off = expr_red.span.end_off;
        let text_len = end_off - resume_start;

        let expr_child = GreenChild {
            relative_start: expr_start - resume_start,
            node: expr_red.inner.clone(),
        };

        let green = GreenExpr {
            kind: GreenExprKind::Resume { expr: expr_child },
            text_len,
        };

        Ok(ExprRedNode {
            span: Span {
                source_id: resume_token.span.source_id,
                start_off: resume_start,
                end_off,
            },
            inner: Arc::new(green),
        })
    }

    /// block
    pub fn parse_block_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let start_off = self.current_token().span.start_off; // 'indent' token
        self.skip_token_only(TokenType::Indent)?;
        let mut exprs: Vec<GreenChild<GreenExpr>> = vec![];

        while self.current_token().kind != TokenType::Dedent {
            let expr_red = self.parse_expr()?;
            let expr_start = expr_red.span.start_off;
            exprs.push(GreenChild {
                relative_start: (expr_start - start_off),
                node: expr_red.inner.clone(),
            });
            if self.current_token().kind == TokenType::NewLine {
                self.skip_token();
                self.skip_token_if_newlines()?;
            }
        }

        self.skip_token_only(TokenType::Dedent)?;
        let end_off = self.tokens.data[self.index - 1].span.end_off;
        let text_len = end_off - start_off;

        let green = GreenExpr {
            kind: GreenExprKind::Do { exprs },
            text_len,
        };

        Ok(ExprRedNode {
            span: Span {
                source_id: self.tokens.data.first().unwrap().span.source_id,
                start_off,
                end_off,
            },
            inner: Arc::new(green),
        })
    }

    /// let expr
    pub fn parse_let_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let let_start = self.current_token().span.start_off;
        self.skip_token_only(TokenType::KwLet)?;
        let mut mutable = false;

        if self.current_token().kind == TokenType::KwMut {
            self.skip_token();
            mutable = true;
        }

        let name_token = self.current_token().clone();
        let name_start = name_token.span.start_off;
        let name = name_token.text.clone();
        self.skip_token_only(TokenType::Ident)?;

        let type_str_opt: Option<(TypeName, usize)> = if self.current_token().kind == TokenType::Colon {
            self.skip_token();
            let ts = self.current_token().span.start_off;
            let t = self.parse_type_name()?;
            Some((t, ts))
        } else {
            None
        };

        self.skip_token_only(TokenType::Eq)?;
        let expr_red = self.parse_expr()?;
        let expr_start = expr_red.span.start_off;

        self.skip_token_only(TokenType::NewLine)?;
        let end_off = self.tokens.data[self.index - 1].span.end_off;
        let text_len = end_off - let_start;

        let name_child = GreenChild {
            relative_start: name_start - let_start,
            node: Arc::new(IdentName { name }),
        };

        let expr_child = GreenChild {
            relative_start: expr_start - let_start,
            node: expr_red.inner.clone(),
        };

        let type_str_child = type_str_opt.map(|(t, start)| GreenChild {
            relative_start: start - let_start,
            node: Arc::new(t),
        });

        let green = GreenExpr {
            kind: GreenExprKind::Let {
                name: name_child,
                expr: expr_child,
                type_str: type_str_child,
                mutable,
            },
            text_len,
        };

        Ok(ExprRedNode {
            span: Span {
                source_id: name_token.span.source_id,
                start_off: let_start,
                end_off,
            },
            inner: Arc::new(green),
        })
    }

    /// do expr
    pub fn parse_do_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let do_start = self.current_token().span.start_off;
        self.skip_token_only(TokenType::KwDo)?;
        self.skip_token_only(TokenType::NewLine)?;
        self.parse_block_expr()
    }

    /// if / elif / else expr
    pub fn parse_if_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let if_start = self.current_token().span.start_off;
        self.skip_token_only(TokenType::KwIf)?;
        let cond_red = self.parse_expr()?;
        let cond_start = cond_red.span.start_off;

        let (then_red, then_start) = if self.current_token().kind == TokenType::KwThen {
            self.skip_token();
            let expr = self.parse_expr()?;
            let start = expr.span.start_off;
            (expr, start)
        } else {
            self.skip_token_only(TokenType::NewLine)?;
            let expr = self.parse_block_expr()?;
            let start = expr.span.start_off;
            (expr, start)
        };

        let mut elifs: Vec<GreenElseIf> = vec![];
        while self.current_token().kind == TokenType::KwElif {
            let elif_start = self.current_token().span.start_off;
            self.skip_token(); // 'elif'
            let elif_cond_red = self.parse_expr()?;
            self.skip_token_only(TokenType::NewLine)?;
            let elif_body_red = self.parse_block_expr()?;
            let elif_end = self.tokens.data[self.index - 1].span.end_off;

            let elif_cond_child = GreenChild {
                relative_start: (elif_cond_red.span.start_off - elif_start) as usize,
                node: elif_cond_red.inner.clone(),
            };
            let elif_body_child = GreenChild {
                relative_start: (elif_body_red.span.start_off - elif_start) as usize,
                node: elif_body_red.inner.clone(),
            };

            elifs.push(GreenElseIf {
                cond: elif_cond_child,
                body: elif_body_child,
                text_len: (elif_end - elif_start),
            });

            if self.current_token().kind == TokenType::NewLine {
                self.skip_token();
            }
        }

        let else_red = if self.current_token().kind == TokenType::KwElse {
            self.skip_token();
            if self.current_token().kind == TokenType::NewLine {
                self.skip_token_only(TokenType::NewLine)?;
                Some(self.parse_block_expr()?)
            } else {
                Some(self.parse_expr()?)
            }
        } else {
            None
        };

        let end_off = self.tokens.data[self.index - 1].span.end_off;
        let text_len = end_off - if_start;

        let cond_child = GreenChild {
            relative_start: cond_start - if_start,
            node: cond_red.inner.clone(),
        };
        let then_child = GreenChild {
            relative_start: then_start - if_start,
            node: then_red.inner.clone(),
        };

        let else_child = else_red.map(|r| GreenChild {
            relative_start: r.span.start_off - if_start,
            node: r.inner.clone(),
        });

        let green = GreenExpr {
            kind: GreenExprKind::If {
                cond: cond_child,
                then_expr: then_child,
                elifs,
                else_expr: else_child,
            },
            text_len,
        };

        Ok(ExprRedNode {
            span: Span {
                source_id: self.tokens.data[0].span.source_id,
                start_off: if_start,
                end_off,
            },
            inner: Arc::new(green),
        })
    }

    /// return expr
    pub fn parse_return_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let return_start = self.current_token().span.start_off;
        self.skip_token_only(TokenType::KwReturn)?;

        if self.current_token().kind == TokenType::NewLine {
            let end_off = self.current_token().span.start_off;
            self.skip_token_only(TokenType::NewLine)?;

            let green = GreenExpr {
                kind: GreenExprKind::Return { expr: None },
                text_len: end_off - return_start,
            };
            Ok(ExprRedNode {
                span: Span {
                    source_id: self.tokens.data[0].span.source_id,
                    start_off: return_start,
                    end_off,
                },
                inner: Arc::new(green),
            })
        } else {
            let expr_red = self.parse_expr()?;
            let expr_start = expr_red.span.start_off;
            self.skip_token_only(TokenType::NewLine)?;
            let end_off = self.tokens.data[self.index - 1].span.end_off;

            let green = GreenExpr {
                kind: GreenExprKind::Return {
                    expr: Some(GreenChild {
                        relative_start: expr_start - return_start,
                        node: expr_red.inner.clone(),
                    }),
                },
                text_len: end_off - return_start,
            };
            Ok(ExprRedNode {
                span: Span {
                    source_id: self.tokens.data[0].span.source_id,
                    start_off: return_start,
                    end_off,
                },
                inner: Arc::new(green),
            })
        }
    }

    /// atom expr
    pub fn parse_atom_expr(&mut self) -> Result<AtomExprNode, DiagMsg> {
        let current_token = self.current_token().clone();
        let current_token_kind = current_token.kind;
        let current_token_text = current_token.text.clone();
        let start_off = current_token.span.start_off;
        let end_off = current_token.span.end_off;
        let text_len = end_off - start_off;

        self.skip_token();

        match current_token_kind {
            TokenType::Float => Ok(AtomExprNode::Decimal {
                dec: current_token_text,
                text_len,
            }),
            TokenType::Int => Ok(AtomExprNode::Int {
                int: current_token_text,
                text_len,
            }),
            TokenType::String => Ok(AtomExprNode::Str {
                string: current_token_text,
                text_len,
            }),
            TokenType::Ident => Ok(AtomExprNode::Name {
                name: current_token_text,
                text_len,
            }),
            TokenType::DotDotDot => Ok(AtomExprNode::Ellipsis { text_len }),
            TokenType::Hash => {
                let hash_start = start_off;
                self.skip_token_only(TokenType::Lbracket)?;
                let mut exprs = vec![];
                while self.current_token().kind != TokenType::Rbracket {
                    let expr_red = self.parse_expr()?;
                    exprs.push(GreenChild {
                        relative_start: (expr_red.span.start_off - hash_start) as usize,
                        node: expr_red.inner.clone(),
                    });
                    if self.current_token().kind == TokenType::Comma {
                        self.skip_token();
                    } else if self.current_token().kind == TokenType::Rbracket {
                        break;
                    } else {
                        return Err(DiagMsg {
                            title: format!("{:?}", ParserError::InvalidTupleLiteral),
                            msg: "invalid tuple literal".to_string(),
                            span: current_token.span.clone(),
                        });
                    }
                }
                let rbracket_token = self.current_token().clone();
                self.skip_token_only(TokenType::Rbracket)?;
                let end = rbracket_token.span.end_off;
                Ok(AtomExprNode::Tuple {
                    exprs,
                    text_len: (end - hash_start) as usize,
                })
            }
            _ => Err(DiagMsg {
                title: format!("{:?}", ParserError::InvalidExpression),
                msg: "invalid expression literal".to_string(),
                span: current_token.span,
            }),
        }
    }

    /// LBP
    /// 中缀运算符左绑定优先级
    fn lbp(token: TokenType) -> Option<usize> {
        match token {
            TokenType::Or => Some(10),
            TokenType::And => Some(20),
            TokenType::EqEq
            | TokenType::Ne
            | TokenType::Lt
            | TokenType::Gt
            | TokenType::Le
            | TokenType::Ge => Some(30),
            TokenType::Plus | TokenType::Minus => Some(40),
            TokenType::Star | TokenType::Slash | TokenType::Percent => Some(50),
            TokenType::Caret => Some(60),
            _ => None,
        }
    }

    /// RBP
    /// 前缀一元运算符右绑定优先级
    fn rbp(token: TokenType) -> Option<usize> {
        match token {
            TokenType::Minus | TokenType::Not => Some(70),
            _ => None,
        }
    }

    /// main dispatcher
    pub fn parse_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        match self.current_token().kind {
            TokenType::KwIf => return self.parse_if_expr(),
            TokenType::KwDo => return self.parse_do_expr(),
            TokenType::KwLet => return self.parse_let_expr(),
            TokenType::KwReturn => return self.parse_return_expr(),
            TokenType::KwWhen => return self.parse_match_expr(),
            TokenType::KwRaise => return self.parse_raise_expr(),
            TokenType::KwWith => return self.parse_with_expr(),
            TokenType::KwResume => return self.parse_resume_expr(),
            _ => {}
        }
        self.parse_expr_bp(0)
    }

    /// Pratt Core
    fn parse_expr_bp(&mut self, min_bp: usize) -> Result<ExprRedNode, DiagMsg> {
        let token = self.current_token().clone();
        let kind = token.kind;
        let start_off = token.span.start_off;

        let mut lhs = match kind {
            TokenType::Minus | TokenType::Not => {
                let op_start = start_off;
                self.skip_token();
                let operand_red = self.parse_expr_bp(Self::rbp(kind.clone()).unwrap())?;
                let expr_end = operand_red.span.end_off;
                let text_len = expr_end - op_start;

                let op_child = GreenChild {
                    relative_start: 0,
                    node: Arc::new(token_type_to_operator(&kind).unwrap()),
                };
                let right_child = GreenChild {
                    relative_start: operand_red.span.start_off - op_start,
                    node: operand_red.inner.clone(),
                };
                let green = GreenExpr {
                    kind: GreenExprKind::Unary {
                        op: op_child,
                        right: right_child,
                    },
                    text_len,
                };
                ExprRedNode {
                    span: Span {
                        source_id: token.span.source_id,
                        start_off: op_start,
                        end_off: expr_end,
                    },
                    inner: Arc::new(green),
                }
            }

            // 用户自定义前缀运算符
            TokenType::UserOp => {
                if let Some((prio, op_kind)) = self.user_op_info.get(&token.text) {
                    if *op_kind == OperatorKind::Prefix {
                        let rbp = *prio;
                        let op_start = start_off;
                        self.skip_token();
                        let operand_red = self.parse_expr_bp(rbp)?;
                        let expr_end = operand_red.span.end_off;
                        let op_child = GreenChild {
                            relative_start: 0,
                            node: Arc::new(Operator::UserOp(token.text.clone())),
                        };
                        let right_child = GreenChild {
                            relative_start: operand_red.span.start_off - op_start,
                            node: operand_red.inner.clone(),
                        };
                        let green = GreenExpr {
                            kind: GreenExprKind::Unary { op: op_child, right: right_child },
                            text_len: expr_end - op_start,
                        };
                        ExprRedNode {
                            span: Span {
                                source_id: token.span.source_id,
                                start_off: op_start,
                                end_off: expr_end,
                            },
                            inner: Arc::new(green),
                        }
                    } else {
                        return Err(DiagMsg {
                            title: format!("{:?}", ParserError::InvalidExpression),
                            msg: format!("Unexpected operator '{}' in prefix position", token.text),
                            span: token.span.clone(),
                        });
                    }
                } else {
                    unreachable!()
                }
            }

            // 括号分组
            TokenType::Lparen => {
                self.skip_token();
                let inner = self.parse_expr()?;
                self.skip_token_only(TokenType::Rparen)?;
                inner
            }

            // move / copy / share
            TokenType::KwMove | TokenType::KwCopy | TokenType::KwShare => {
                let kw_start = start_off;
                self.skip_token();
                let target_red = self.parse_expr_bp(60)?;
                let expr_end = target_red.span.end_off;

                let target_child = GreenChild {
                    relative_start: target_red.span.start_off - kw_start,
                    node: target_red.inner.clone(),
                };
                let kind = match kind {
                    TokenType::KwMove => GreenExprKind::Move { target: target_child },
                    TokenType::KwCopy => GreenExprKind::Copy { target: target_child },
                    TokenType::KwShare => GreenExprKind::Share { target: target_child },
                    _ => unreachable!(),
                };
                let green = GreenExpr {
                    kind,
                    text_len: expr_end - kw_start,
                };
                ExprRedNode {
                    span: Span {
                        source_id: token.span.source_id,
                        start_off: kw_start,
                        end_off: expr_end,
                    },
                    inner: Arc::new(green),
                }
            }

            // ref / ref mut
            TokenType::KwRef => {
                let ref_start = start_off;
                self.skip_token();
                let is_mut = self.current_token().kind == TokenType::KwMut;
                if is_mut {
                    self.skip_token();
                }
                let target_red = self.parse_expr_bp(60)?;
                let expr_end = target_red.span.end_off;
                let target_child = GreenChild {
                    relative_start: target_red.span.start_off - ref_start,
                    node: target_red.inner.clone(),
                };
                let kind = if is_mut {
                    GreenExprKind::MutRef { target: target_child }
                } else {
                    GreenExprKind::Ref { target: target_child }
                };
                let green = GreenExpr {
                    kind,
                    text_len: expr_end - ref_start,
                };
                ExprRedNode {
                    span: Span {
                        source_id: token.span.source_id,
                        start_off: ref_start,
                        end_off: expr_end,
                    },
                    inner: Arc::new(green),
                }
            }

            /// 标识符先尝试解析为静态路径
            TokenType::Ident => {
                let path_start = start_off;
                let path = self.parse_pure_static_path()?;
                let path_text_len = path.text_len;
                let path_child = GreenChild {
                    relative_start: 0,
                    node: Arc::new(path),
                };
                let green = GreenExpr {
                    kind: GreenExprKind::StaticPath { path: path_child },
                    text_len: path_text_len,
                };
                ExprRedNode {
                    span: Span {
                        source_id: token.span.source_id,
                        start_off: path_start,
                        end_off: path_start + path_text_len,
                    },
                    inner: Arc::new(green),
                }
            }

            _ => {
                let atom = self.parse_atom_expr()?;
                let atom_text_len = atom.text_len();
                let green = GreenExpr {
                    kind: GreenExprKind::Atom { expr: atom },
                    text_len: atom_text_len,
                };
                ExprRedNode {
                    span: Span {
                        source_id: token.span.source_id,
                        start_off,
                        end_off: start_off + atom_text_len,
                    },
                    inner: Arc::new(green),
                }
            }
        };

        /// LED
        loop {
            let token = self.current_token().clone();
            let kind = token.kind;
            let token_start = token.span.start_off;

            let lbp = match kind {
                TokenType::UserOp => self.user_op_info.get(&token.text).map(|(p, _)| *p),
                _ => Self::lbp(kind.clone()),
            };

            if let Some(lbp) = lbp {
                if lbp < min_bp {
                    break;
                }

                match kind {
                    TokenType::UserOp => {
                        let (prio, op_kind) = self.user_op_info.get(&token.text)
                            .expect("UserOp must be in user_op_info");
                        match op_kind {
                            OperatorKind::Infix => {
                                self.skip_token();
                                let rhs_red = self.parse_expr_bp(lbp + 1)?;
                                let expr_start = lhs.span.start_off;
                                let expr_end = rhs_red.span.end_off;

                                let left_child = GreenChild {
                                    relative_start: lhs.span.start_off - expr_start,
                                    node: lhs.inner.clone(),
                                };
                                let op_child = GreenChild {
                                    relative_start: token_start - expr_start,
                                    node: Arc::new(Operator::UserOp(token.text.clone())),
                                };
                                let right_child = GreenChild {
                                    relative_start: rhs_red.span.start_off - expr_start,
                                    node: rhs_red.inner.clone(),
                                };
                                let green = GreenExpr {
                                    kind: GreenExprKind::Binary {
                                        left: left_child,
                                        op: op_child,
                                        right: right_child,
                                    },
                                    text_len: expr_end - expr_start,
                                };
                                lhs = ExprRedNode {
                                    span: Span {
                                        source_id: token.span.source_id,
                                        start_off: expr_start,
                                        end_off: expr_end,
                                    },
                                    inner: Arc::new(green),
                                };
                                continue;
                            }
                            OperatorKind::Postfix => {
                                self.skip_token();
                                let expr_start = lhs.span.start_off;
                                let expr_end = token.span.end_off;
                                let op_child = GreenChild {
                                    relative_start: token_start - expr_start,
                                    node: Arc::new(Operator::UserOp(token.text.clone())),
                                };
                                let right_child = GreenChild {
                                    relative_start: lhs.span.start_off - expr_start,
                                    node: lhs.inner.clone(),
                                };
                                let green = GreenExpr {
                                    kind: GreenExprKind::Unary { op: op_child, right: right_child },
                                    text_len: expr_end - expr_start,
                                };
                                lhs = ExprRedNode {
                                    span: Span {
                                        source_id: token.span.source_id,
                                        start_off: expr_start,
                                        end_off: expr_end,
                                    },
                                    inner: Arc::new(green),
                                };
                                continue;
                            }
                            _ => {
                                return Err(DiagMsg {
                                    title: format!("{:?}", ParserError::InvalidExpression),
                                    msg: format!("Unexpected operator '{}' in infix/postfix position", token.text),
                                    span: token.span.clone(),
                                });
                            }
                        }
                    }
                    _ => {
                        self.skip_token();
                        let rhs_red = self.parse_expr_bp(lbp + 1)?;
                        let expr_start = lhs.span.start_off;
                        let expr_end = rhs_red.span.end_off;

                        let left_child = GreenChild {
                            relative_start: lhs.span.start_off - expr_start,
                            node: lhs.inner.clone(),
                        };
                        let op_child = GreenChild {
                            relative_start: token_start - expr_start,
                            node: Arc::new(token_type_to_operator(&kind).ok_or_else(|| DiagMsg {
                                title: format!("{:?}", ParserError::InvalidOperator),
                                msg: "invalid operator".to_string(),
                                span: token.span.clone(),
                            })?),
                        };
                        let right_child = GreenChild {
                            relative_start: rhs_red.span.start_off - expr_start,
                            node: rhs_red.inner.clone(),
                        };
                        let green = GreenExpr {
                            kind: GreenExprKind::Binary {
                                left: left_child,
                                op: op_child,
                                right: right_child,
                            },
                            text_len: expr_end - expr_start,
                        };
                        lhs = ExprRedNode {
                            span: Span {
                                source_id: token.span.source_id,
                                start_off: expr_start,
                                end_off: expr_end,
                            },
                            inner: Arc::new(green),
                        };
                        continue;
                    }
                }
            }

            match kind {
                /// call expr
                TokenType::Lparen => {
                    self.skip_token();
                    let mut args = vec![];
                    while self.current_token().kind != TokenType::Rparen {
                        let arg_red = self.parse_expr()?;
                        args.push(GreenChild {
                            relative_start: arg_red.span.start_off - token_start,
                            node: arg_red.inner.clone(),
                        });
                        if self.current_token().kind == TokenType::Comma {
                            self.skip_token();
                        } else if self.current_token().kind == TokenType::Rparen {
                            break;
                        } else {
                            return Err(DiagMsg {
                                title: format!("{:?}", ParserError::InvalidCallArgumentList),
                                msg: "invalid call argument list".to_string(),
                                span: token.span.clone(),
                            });
                        }
                    }
                    self.skip_token_only(TokenType::Rparen)?;
                    let rparen_span = self.tokens.data[self.index - 1].span.clone();
                    let expr_start = lhs.span.start_off;
                    let expr_end = rparen_span.end_off;

                    let callee_child = GreenChild {
                        relative_start: lhs.span.start_off - expr_start,
                        node: lhs.inner.clone(),
                    };

                    let lparen_offset = token_start - expr_start;
                    let adjusted_args: Vec<GreenChild<GreenExpr>> = args.into_iter().map(|mut child| {
                        child.relative_start += lparen_offset;
                        child
                    }).collect();

                    let green = GreenExpr {
                        kind: GreenExprKind::Call {
                            callee: callee_child,
                            args: adjusted_args,
                        },
                        text_len: expr_end - expr_start,
                    };
                    lhs = ExprRedNode {
                        span: Span {
                            source_id: token.span.source_id,
                            start_off: expr_start,
                            end_off: expr_end,
                        },
                        inner: Arc::new(green),
                    };
                    continue;
                }

                /// expr is E(binding payload)
                TokenType::KwIs => {
                    const IS_BP: usize = 30; // 与比较运算符同级
                    if IS_BP < min_bp {
                        break;
                    }
                    self.skip_token(); // 'is'

                    let pattern_start = self.current_token().span.start_off;
                    let pattern = self.parse_pattern()?;

                    let expr_start = lhs.span.start_off;
                    let expr_end = self.tokens.data[self.index - 1].span.end_off;
                    let text_len = expr_end - expr_start;

                    let expr_child = GreenChild {
                        relative_start: lhs.span.start_off - expr_start,
                        node: lhs.inner.clone(),
                    };
                    let pattern_child = GreenChild {
                        relative_start: pattern_start - expr_start,
                        node: Arc::new(pattern),
                    };
                    let green = GreenExpr {
                        kind: GreenExprKind::Is {
                            expr: expr_child,
                            pattern: pattern_child,
                        },
                        text_len,
                    };
                    lhs = ExprRedNode {
                        span: Span {
                            source_id: token.span.source_id,
                            start_off: expr_start,
                            end_off: expr_end,
                        },
                        inner: Arc::new(green),
                    };
                    continue;
                }

                /// expr as TypeName
                TokenType::KwAs => {
                    const AS_BP: usize = 20;
                    if AS_BP < min_bp {
                        break;
                    }
                    self.skip_token(); // 'as'
                    let type_start = self.current_token().span.start_off;
                    let into_type = self.parse_type_name()?;
                    let expr_start = lhs.span.start_off;
                    let expr_end = self.tokens.data[self.index - 1].span.end_off;

                    let expr_child = GreenChild {
                        relative_start: lhs.span.start_off - expr_start,
                        node: lhs.inner.clone(),
                    };
                    let type_child = GreenChild {
                        relative_start: type_start - expr_start,
                        node: Arc::new(into_type),
                    };

                    let green = GreenExpr {
                        kind: GreenExprKind::TypeCast {
                            expr: expr_child,
                            into_type: type_child,
                        },
                        text_len: expr_end - expr_start,
                    };
                    lhs = ExprRedNode {
                        span: Span {
                            source_id: token.span.source_id,
                            start_off: expr_start,
                            end_off: expr_end,
                        },
                        inner: Arc::new(green),
                    };
                    continue;
                }

                /// expr.ident  (仅当 lhs 不是静态路径时发生，因为静态路径已经在 nud 中完整解析)
                TokenType::Dot => {
                    self.skip_token(); // '.'
                    let member_token = self.current_token().clone();
                    let member = member_token.text.clone();
                    let member_start = member_token.span.start_off;
                    self.skip_token_only(TokenType::Ident)?;
                    let expr_start = lhs.span.start_off;
                    let expr_end = member_token.span.end_off;

                    let left_child = GreenChild {
                        relative_start: lhs.span.start_off - expr_start,
                        node: lhs.inner.clone(),
                    };
                    let member_child = GreenChild {
                        relative_start: member_start - expr_start,
                        node: Arc::new(IdentName { name: member }),
                    };
                    let green = GreenExpr {
                        kind: GreenExprKind::MemberAccess {
                            left: left_child,
                            member: member_child,
                        },
                        text_len: expr_end - expr_start,
                    };
                    lhs = ExprRedNode {
                        span: Span {
                            source_id: token.span.source_id,
                            start_off: expr_start,
                            end_off: expr_end,
                        },
                        inner: Arc::new(green),
                    };
                    continue;
                }

                // path { field: expr, ... }
                TokenType::Lbrace => {
                    self.skip_token(); // '{'
                    let mut fields = vec![];
                    let brace_start = token_start;

                    while self.current_token().kind != TokenType::Rbrace {
                        let field_name_token = self.current_token().clone();
                        if field_name_token.kind != TokenType::Ident {
                            return Err(DiagMsg {
                                title: format!("{:?}", ParserError::InvalidStructInit),
                                msg: "expected field name".to_string(),
                                span: field_name_token.span.clone(),
                            });
                        }
                        let name_start = field_name_token.span.start_off;
                        let name = field_name_token.text.clone();
                        self.skip_token(); // field name
                        self.skip_token_only(TokenType::Eq)?; // '='
                        let value_red = self.parse_expr()?;
                        let value_start = value_red.span.start_off;
                        let field_end = value_red.span.end_off;

                        let name_child = GreenChild {
                            relative_start: name_start - brace_start,
                            node: Arc::new(IdentName { name }),
                        };
                        let value_child = GreenChild {
                            relative_start: value_start - brace_start,
                            node: value_red.inner.clone(),
                        };
                        fields.push(GreenChild {
                            relative_start: name_start - brace_start,
                            node: Arc::new(GreenStructFieldInit {
                                name: name_child,
                                value: value_child,
                                text_len: field_end - name_start,
                            }),
                        });

                        if self.current_token().kind == TokenType::Comma {
                            self.skip_token();
                        } else if self.current_token().kind == TokenType::Rbrace {
                            break;
                        } else {
                            return Err(DiagMsg {
                                title: format!("{:?}", ParserError::InvalidStructInit),
                                msg: "expected ',' or '}'".to_string(),
                                span: self.current_token().span.clone(),
                            });
                        }
                    }

                    self.skip_token_only(TokenType::Rbrace)?;
                    let rbrace_end = self.tokens.data[self.index - 1].span.end_off;
                    let expr_start = lhs.span.start_off;
                    let expr_end = rbrace_end;

                    let path_child = GreenChild {
                        relative_start: lhs.span.start_off - expr_start,
                        node: lhs.inner.clone(),
                    };

                    let green = GreenExpr {
                        kind: GreenExprKind::MakeStruct {
                            path: path_child,
                            fields,
                        },
                        text_len: expr_end - expr_start,
                    };
                    lhs = ExprRedNode {
                        span: Span {
                            source_id: token.span.source_id,
                            start_off: expr_start,
                            end_off: expr_end,
                        },
                        inner: Arc::new(green),
                    };
                    continue;
                }

                _ => break,
            }
        }

        Ok(lhs)
    }
}