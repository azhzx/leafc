use std::ops::Deref;
use std::sync::Arc;
use leafc_coreapi::ast::{
    AtomExprNode, GreenChild, GreenElseIf, GreenExpr, GreenExprKind, ExprRedNode, Operator,
    TypeNameString,
};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::TokenType;
use leafc_coreapi::parser::ParserError;
use leafc_coreapi::crate_meta::OperatorKind;
use crate::Parser;

impl<'a> Parser<'a> {
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
        let text_len = (end_off - start_off);

        let green = GreenExpr {
            kind: GreenExprKind::Do { exprs },
            text_len,
        };

        Ok(ExprRedNode {
            span: leafc_coreapi::source::Span {
                source_id: self.tokens.data.first().unwrap().span.source_id,
                start_off,
                end_off,
            },
            inner: Arc::new(green),
        })
    }

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

        let type_str_opt: Option<(TypeNameString, usize)> = if self.current_token().kind == TokenType::Colon {
            self.skip_token();
            let ts = self.current_token().span.start_off;
            let t = self.handle_type_name_string()?;
            Some((t, ts))
        } else {
            None
        };

        self.skip_token_only(TokenType::Eq)?;
        let expr_red = self.parse_expr()?;
        let expr_start = expr_red.span.start_off;

        self.skip_token_only(TokenType::NewLine)?;
        let end_off = self.tokens.data[self.index - 1].span.end_off;
        let text_len = (end_off - let_start);

        let name_child = GreenChild {
            relative_start: (name_start - let_start),
            node: Arc::new(name),
        };

        let expr_child = GreenChild {
            relative_start: (expr_start - let_start),
            node: expr_red.inner.clone(),
        };

        let type_str_child = type_str_opt.map(|(t, start)| GreenChild {
            relative_start: (start - let_start),
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
            span: leafc_coreapi::source::Span {
                source_id: name_token.span.source_id,
                start_off: let_start,
                end_off,
            },
            inner: Arc::new(green),
        })
    }

    pub fn parse_do_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let do_start = self.current_token().span.start_off;
        self.skip_token_only(TokenType::KwDo)?;
        self.skip_token_only(TokenType::NewLine)?;
        self.parse_block_expr()
    }

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
                relative_start: (elif_body_red.span.start_off - elif_start),
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
        let text_len = (end_off - if_start);

        let cond_child = GreenChild {
            relative_start: (cond_start - if_start),
            node: cond_red.inner.clone(),
        };
        let then_child = GreenChild {
            relative_start: (then_start - if_start),
            node: then_red.inner.clone(),
        };

        let else_child = else_red.map(|r| GreenChild {
            relative_start: (r.span.start_off - if_start),
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
            span: leafc_coreapi::source::Span {
                source_id: self.tokens.data[0].span.source_id,
                start_off: if_start,
                end_off,
            },
            inner: Arc::new(green),
        })
    }

    pub fn parse_return_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let return_start = self.current_token().span.start_off;
        self.skip_token_only(TokenType::KwReturn)?;

        if self.current_token().kind == TokenType::NewLine {
            let end_off = self.current_token().span.start_off;
            self.skip_token_only(TokenType::NewLine)?;

            let green = GreenExpr {
                kind: GreenExprKind::Return { expr: None },
                text_len: (end_off - return_start),
            };
            Ok(ExprRedNode {
                span: leafc_coreapi::source::Span {
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
                        relative_start: (expr_start - return_start),
                        node: expr_red.inner.clone(),
                    }),
                },
                text_len: (end_off - return_start),
            };
            Ok(ExprRedNode {
                span: leafc_coreapi::source::Span {
                    source_id: self.tokens.data[0].span.source_id,
                    start_off: return_start,
                    end_off,
                },
                inner: Arc::new(green),
            })
        }
    }

    pub fn parse_atom_expr(&mut self) -> Result<AtomExprNode, DiagMsg> {

        let current_token = self.current_token().clone();
        let current_token_kind = current_token.kind;
        let current_token_text = current_token.text.clone();
        let start_off = current_token.span.start_off;
        let end_off = current_token.span.end_off;
        let text_len = (end_off - start_off);

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
                    text_len: (end - hash_start),
                })
            }
            _ => Err(DiagMsg {
                title: format!("{:?}", ParserError::InvalidExpression),
                msg: "invalid expression literal".to_string(),
                span: current_token.span,
            }),
        }
    }

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

    /// 前缀一元运算符右绑定优先级
    fn rbp(token: TokenType) -> Option<usize> {
        match token {
            TokenType::Minus | TokenType::Not => Some(70),
            _ => None,
        }
    }

    pub fn parse_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        match self.current_token().kind {
            TokenType::KwIf => return self.parse_if_expr(),
            TokenType::KwDo => return self.parse_do_expr(),
            TokenType::KwLet => return self.parse_let_expr(),
            TokenType::KwReturn => return self.parse_return_expr(),
            _ => {}
        }

        // 进入 Pratt 解析
        self.parse_expr_bp(0)
    }

    pub fn token_type_to_operator(token_type: TokenType) -> Option<Operator> {
        match token_type {
            TokenType::Plus => Some(Operator::Add),
            TokenType::Minus => Some(Operator::Sub),
            TokenType::Star => Some(Operator::Mul),
            TokenType::Slash => Some(Operator::Div),
            TokenType::Percent => Some(Operator::Mod),
            TokenType::And => Some(Operator::And),
            TokenType::Or => Some(Operator::Or),
            TokenType::Not => Some(Operator::Not),
            TokenType::EqEq => Some(Operator::Eq),
            TokenType::Ne => Some(Operator::Neq),
            TokenType::Lt => Some(Operator::Lt),
            TokenType::Gt => Some(Operator::Gt),
            TokenType::Le => Some(Operator::Le),
            TokenType::Ge => Some(Operator::Ge),
            _ => None,
        }
    }

    /// 以最小绑定强度 min_bp 继续解析表达式
    fn parse_expr_bp(&mut self, min_bp: usize) -> Result<ExprRedNode, DiagMsg> {
        let token = self.current_token().clone();
        let kind = token.kind;
        let start_off = token.span.start_off;

        let mut lhs = match kind {
            TokenType::Minus | TokenType::Not => {
                let op_start = start_off;
                self.skip_token();
                let operand_red = self.parse_expr_bp(Self::rbp(kind.clone()).unwrap())?;
                let op_end = token.span.end_off;
                let expr_end = operand_red.span.end_off;
                let text_len = (expr_end - op_start);

                let op_child = GreenChild {
                    relative_start: 0, // 操作符在表达式开始
                    node: Arc::new(Self::token_type_to_operator(kind).unwrap()),
                };
                let right_child = GreenChild {
                    relative_start: (operand_red.span.start_off - op_start),
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
                    span: leafc_coreapi::source::Span {
                        source_id: token.span.source_id,
                        start_off: op_start,
                        end_off: expr_end,
                    },
                    inner: Arc::new(green),
                }
            }

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
                            relative_start: (operand_red.span.start_off - op_start),
                            node: operand_red.inner.clone(),
                        };
                        let green = GreenExpr {
                            kind: GreenExprKind::Unary { op: op_child, right: right_child },
                            text_len: (expr_end - op_start),
                        };
                        ExprRedNode {
                            span: leafc_coreapi::source::Span {
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

            TokenType::Lparen => {
                self.skip_token(); // '('
                let inner = self.parse_expr()?;
                self.skip_token_only(TokenType::Rparen)?;
                inner
            }

            TokenType::KwMove | TokenType::KwCopy | TokenType::KwShared => {
                let kw_start = start_off;
                self.skip_token();
                let target_red = self.parse_expr_bp(60)?;
                let expr_end = target_red.span.end_off;

                let target_child = GreenChild {
                    relative_start: (target_red.span.start_off - kw_start),
                    node: target_red.inner.clone(),
                };
                let kind = match kind {
                    TokenType::KwMove => GreenExprKind::Move { target: target_child },
                    TokenType::KwCopy => GreenExprKind::Copy { target: target_child },
                    TokenType::KwShared => GreenExprKind::Share { target: target_child },
                    _ => unreachable!(),
                };
                let green = GreenExpr {
                    kind,
                    text_len: (expr_end - kw_start),
                };
                ExprRedNode {
                    span: leafc_coreapi::source::Span {
                        source_id: token.span.source_id,
                        start_off: kw_start,
                        end_off: expr_end,
                    },
                    inner: Arc::new(green),
                }
            }

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
                    relative_start: (target_red.span.start_off - ref_start),
                    node: target_red.inner.clone(),
                };
                let kind = if is_mut {
                    GreenExprKind::MutRef { target: target_child }
                } else {
                    GreenExprKind::Ref { target: target_child }
                };
                let green = GreenExpr {
                    kind,
                    text_len: (expr_end - ref_start),
                };
                ExprRedNode {
                    span: leafc_coreapi::source::Span {
                        source_id: token.span.source_id,
                        start_off: ref_start,
                        end_off: expr_end,
                    },
                    inner: Arc::new(green),
                }
            }

            _ => {
                let atom = self.parse_atom_expr()?;
                let atom_text_len = match &atom {
                    AtomExprNode::Decimal { text_len, .. } => *text_len,
                    AtomExprNode::Int { text_len, .. } => *text_len,
                    AtomExprNode::Str { text_len, .. } => *text_len,
                    AtomExprNode::Name { text_len, .. } => *text_len,
                    AtomExprNode::Tuple { text_len, .. } => *text_len,
                    AtomExprNode::Ellipsis { text_len } => *text_len,
                };
                let green = GreenExpr {
                    kind: GreenExprKind::Atom { expr: atom },
                    text_len: atom_text_len,
                };
                ExprRedNode {
                    span: leafc_coreapi::source::Span {
                        source_id: token.span.source_id,
                        start_off,
                        end_off: start_off + atom_text_len,
                    },
                    inner: Arc::new(green),
                }
            }
        };

        // led 部分
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
                                    relative_start: (lhs.span.start_off - expr_start),
                                    node: lhs.inner.clone(),
                                };
                                let op_child = GreenChild {
                                    relative_start: (token_start - expr_start),
                                    node: Arc::new(Operator::UserOp(token.text.clone())),
                                };
                                let right_child = GreenChild {
                                    relative_start: (rhs_red.span.start_off - expr_start),
                                    node: rhs_red.inner.clone(),
                                };
                                let green = GreenExpr {
                                    kind: GreenExprKind::Binary {
                                        left: left_child,
                                        op: op_child,
                                        right: right_child,
                                    },
                                    text_len: (expr_end - expr_start),
                                };
                                lhs = ExprRedNode {
                                    span: leafc_coreapi::source::Span {
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
                                    relative_start: (token_start - expr_start),
                                    node: Arc::new(Operator::UserOp(token.text.clone())),
                                };
                                let right_child = GreenChild {
                                    relative_start: (lhs.span.start_off - expr_start),
                                    node: lhs.inner.clone(),
                                };
                                let green = GreenExpr {
                                    kind: GreenExprKind::Unary { op: op_child, right: right_child },
                                    text_len: (expr_end - expr_start),
                                };
                                lhs = ExprRedNode {
                                    span: leafc_coreapi::source::Span {
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
                            relative_start: (lhs.span.start_off - expr_start),
                            node: lhs.inner.clone(),
                        };
                        let op_child = GreenChild {
                            relative_start: (token_start - expr_start),
                            node: Arc::new(Self::token_type_to_operator(kind).ok_or_else(|| DiagMsg {
                                title: format!("{:?}", ParserError::InvalidOperator),
                                msg: "invalid operator".to_string(),
                                span: token.span.clone(),
                            })?),
                        };
                        let right_child = GreenChild {
                            relative_start: (rhs_red.span.start_off - expr_start),
                            node: rhs_red.inner.clone(),
                        };
                        let green = GreenExpr {
                            kind: GreenExprKind::Binary {
                                left: left_child,
                                op: op_child,
                                right: right_child,
                            },
                            text_len: (expr_end - expr_start),
                        };
                        lhs = ExprRedNode {
                            span: leafc_coreapi::source::Span {
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

            // 其他后缀操作
            match kind {
                TokenType::Lparen => {
                    self.skip_token(); // '('
                    let mut args = vec![];
                    while self.current_token().kind != TokenType::Rparen {
                        let arg_red = self.parse_expr()?;
                        args.push(GreenChild {
                            relative_start: (arg_red.span.start_off - token_start),
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
                        relative_start: (lhs.span.start_off - expr_start),
                        node: lhs.inner.clone(),
                    };

                    let lparen_offset = (token_start - expr_start);
                    let adjusted_args: Vec<GreenChild<GreenExpr>> = args.into_iter().map(|mut child| {
                        child.relative_start += lparen_offset;
                        child
                    }).collect();

                    let green = GreenExpr {
                        kind: GreenExprKind::Call {
                            callee: callee_child,
                            args: adjusted_args,
                        },
                        text_len: (expr_end - expr_start),
                    };
                    lhs = ExprRedNode {
                        span: leafc_coreapi::source::Span {
                            source_id: token.span.source_id,
                            start_off: expr_start,
                            end_off: expr_end,
                        },
                        inner: Arc::new(green),
                    };
                    continue;
                }

                TokenType::KwAs => {
                    const AS_BP: usize = 20;
                    if AS_BP < min_bp {
                        break;
                    }
                    self.skip_token();
                    let into_red = self.parse_expr_bp(AS_BP)?;
                    let expr_start = lhs.span.start_off;
                    let expr_end = into_red.span.end_off;

                    let expr_child = Arc::new(GreenChild {
                        relative_start: (lhs.span.start_off - expr_start) as usize,
                        node: lhs.inner.clone(),
                    });
                    let type_start = token_start; // 'as' 后面类型开始
                    let into_child = GreenChild {
                        relative_start: (type_start - expr_start) as usize,
                        node: expr_child.clone(),
                    };
                    let green = GreenExpr {
                        kind: GreenExprKind::TypeCast {
                            expr: expr_child.as_ref().clone(),
                            into_type: into_child.node.as_ref().clone(),
                        },
                        text_len: (expr_end - expr_start),
                    };
                    lhs = ExprRedNode {
                        span: leafc_coreapi::source::Span {
                            source_id: token.span.source_id,
                            start_off: expr_start,
                            end_off: expr_end,
                        },
                        inner: Arc::new(green),
                    };
                    continue;
                }

                TokenType::Dot => {
                    self.skip_token(); // '.'
                    let member_token = self.current_token().clone();
                    let member = member_token.text.clone();
                    let member_start = member_token.span.start_off;
                    self.skip_token_only(TokenType::Ident)?;
                    let expr_start = lhs.span.start_off;
                    let expr_end = member_token.span.end_off;

                    let left_child = GreenChild {
                        relative_start: (lhs.span.start_off - expr_start) as usize,
                        node: lhs.inner.clone(),
                    };
                    let right_child = GreenChild {
                        relative_start: (member_start - expr_start) as usize,
                        node: Arc::new(member),
                    };
                    let green = GreenExpr {
                        kind: GreenExprKind::Member {
                            left: left_child,
                            right: right_child,
                        },
                        text_len: (expr_end - expr_start) as usize,
                    };
                    lhs = ExprRedNode {
                        span: leafc_coreapi::source::Span {
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