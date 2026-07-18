use std::sync::Arc;
use leafc_coreapi::ast::{
    AtomExprNode, ElseIf, ExprNode, ExprNodeKind, ExprRedNode, Operator,
};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::TokenType;
use leafc_coreapi::parser::ParserError;
use leafc_coreapi::crate_meta::{OperatorKind};
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_block_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let mut exprs = vec![];
        let span = self.current_token().span.clone();
        self.skip_token_only(TokenType::Indent)?;

        while self.current_token().kind != TokenType::Dedent {
            let expr = self.parse_expr()?;
            if self.current_token().kind == TokenType::NewLine {
                self.skip_token();
                self.skip_token_if_newlines()?;
            }
            exprs.push(expr);
        }

        self.skip_token_only(TokenType::Dedent)?;

        Ok(ExprRedNode {
            span,
            inner: Arc::new(ExprNode {
                kind: ExprNodeKind::Do { exprs },
            }),
        })
    }

    pub fn parse_let_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        self.skip_token_only(TokenType::KwLet)?;
        let mut mutable = false;

        if self.current_token().kind == TokenType::KwMut {
            self.skip_token();
            mutable = true;
        }

        let name_token = self.current_token();
        let name = name_token.text.clone();
        let span = name_token.span.clone();
        self.skip_token_only(TokenType::Ident)?;

        let type_str = if self.current_token().kind == TokenType::Colon {
            self.skip_token();
            self.handle_type_name_string()?
        } else {
            self.unknown_type_name()
        };

        self.skip_token_only(TokenType::Eq)?;
        let expr = self.parse_expr()?;

        self.skip_token_only(TokenType::NewLine)?;

        Ok(ExprRedNode {
            span,
            inner: Arc::new(ExprNode {
                kind: ExprNodeKind::Let {
                    expr,
                    name,
                    type_str,
                    mutable,
                },
            }),
        })
    }

    pub fn parse_do_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let span = self.current_token().span.clone();
        self.skip_token_only(TokenType::KwDo)?;
        self.skip_token_only(TokenType::NewLine)?;

        self.parse_block_expr()
    }

    pub fn parse_if_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let span = self.current_token().span.clone();
        self.skip_token_only(TokenType::KwIf)?;
        let cond = self.parse_expr()?;

        let if_then_expr = if self.current_token().kind == TokenType::KwThen {
            self.skip_token();
            self.parse_expr()?
        } else {
            self.skip_token_only(TokenType::NewLine)?;
            self.parse_block_expr()?
        };

        let mut elif_body_exprs = vec![];
        if self.current_token().kind == TokenType::KwElif {
            while self.current_token().kind == TokenType::KwElif {
                self.skip_token();
                let cond = self.parse_expr()?;

                self.skip_token_only(TokenType::NewLine)?;
                let body = self.parse_block_expr()?;

                if self.current_token().kind == TokenType::NewLine {
                    self.skip_token();
                }
                elif_body_exprs.push(ElseIf { cond, body });
            }
        }

        let else_body_expr = if self.current_token().kind == TokenType::KwElse {
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

        Ok(ExprRedNode {
            span,
            inner: Arc::new(ExprNode {
                kind: ExprNodeKind::If {
                    cond,
                    then_expr: if_then_expr,
                    elifs: elif_body_exprs,
                    else_expr: else_body_expr,
                },
            }),
        })
    }

    pub fn parse_return_expr(&mut self) -> Result<ExprRedNode, DiagMsg> {
        let span = self.current_token().span.clone();
        self.skip_token_only(TokenType::KwReturn)?;
        let expr = if self.current_token().kind == TokenType::NewLine {
            None
        } else {
            let expr = self.parse_expr()?;
            self.skip_token_only(TokenType::NewLine);
            Some(expr)
        };

        Ok(ExprRedNode {
            span,
            inner: Arc::new(ExprNode {
                kind: ExprNodeKind::Return { expr },
            }),
        })
    }

    pub fn parse_atom_expr(&mut self) -> Result<AtomExprNode, DiagMsg> {
        let current_token = self.current_token();
        let current_token_kind = current_token.kind.clone();
        let current_token_text = current_token.text.clone();
        let current_token_span = current_token.span.clone();

        self.skip_token();

        let expr = match current_token_kind {
            TokenType::Float => AtomExprNode::Decimal {
                dec: current_token_text,
            },
            TokenType::Int => AtomExprNode::Int {
                int: current_token_text,
            },
            TokenType::String => AtomExprNode::Str {
                string: current_token_text,
            },
            TokenType::Ident => AtomExprNode::Name {
                name: current_token_text,
            },
            TokenType::DotDotDot => AtomExprNode::Ellipsis,
            TokenType::Hash => {
                let mut exprs = vec![];

                self.skip_token_only(TokenType::Lbracket)?;
                while self.current_token().kind != TokenType::Rbracket {
                    exprs.push(self.parse_expr()?);
                    if self.current_token().kind == TokenType::Comma {
                        self.skip_token();
                    } else if self.current_token().kind == TokenType::Rbracket {
                        break;
                    } else {
                        return Err(DiagMsg {
                            title: format!("{:?}", ParserError::InvalidTupleLiteral),
                            msg: "invalid tuple literal".to_string(),
                            span: current_token_span,
                        });
                    }
                }
                self.skip_token_only(TokenType::Rbracket)?;

                AtomExprNode::Tuple { exprs }
            }
            _ => {
                return Err(DiagMsg {
                    title: format!("{:?}", ParserError::InvalidExpression),
                    msg: "invalid expression literal".to_string(),
                    span: current_token_span,
                });
            }
        };

        Ok(expr)
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
        // nud
        let token = self.current_token().clone();
        let kind = token.kind;
        let start_span = token.span.clone();

        let mut lhs = match kind {
            // - / not
            TokenType::Minus | TokenType::Not => {
                let op_kind = kind;
                let op_span = start_span;
                self.skip_token();
                let operand = self.parse_expr_bp(Self::rbp(op_kind.clone()).unwrap())?;
                ExprRedNode {
                    span: op_span,
                    inner: Arc::new(ExprNode {
                        kind: ExprNodeKind::Unary {
                            op: Self::token_type_to_operator(op_kind).unwrap(),
                            right: operand,
                        },
                    }),
                }
            }

            TokenType::UserOp => {
                if let Some((prio, op_kind)) = self.user_op_info.get(&token.text) {
                    if *op_kind == OperatorKind::Prefix {
                        let rbp = *prio;   // 前缀运算符的右绑定强度
                        self.skip_token();
                        let operand = self.parse_expr_bp(rbp)?;
                        ExprRedNode {
                            span: start_span,
                            inner: Arc::new(ExprNode {
                                kind: ExprNodeKind::Unary {
                                    op: Operator::UserOp(token.text.clone()),
                                    right: operand,
                                },
                            }),
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

            // 括号
            TokenType::Lparen => {
                self.skip_token(); // '('
                let inner = self.parse_expr()?;
                self.skip_token_only(TokenType::Rparen)?;
                inner
            }

            // move / copy / shared
            TokenType::KwMove | TokenType::KwCopy | TokenType::KwShared => {
                let kw_span = start_span;
                self.skip_token();
                let target = self.parse_expr_bp(60)?;
                let kind = match kind {
                    TokenType::KwMove => ExprNodeKind::Move { target },
                    TokenType::KwCopy => ExprNodeKind::Copy { target },
                    TokenType::KwShared => ExprNodeKind::Share { target },
                    _ => unreachable!(),
                };
                ExprRedNode {
                    span: kw_span,
                    inner: Arc::new(ExprNode { kind }),
                }
            }

            // ref / ref mut
            TokenType::KwRef => {
                let ref_span = start_span;
                self.skip_token();
                let target = if self.current_token().kind == TokenType::KwMut {
                    self.skip_token();
                    self.parse_expr_bp(60)?
                } else {
                    self.parse_expr_bp(60)?
                };
                let kind = if self.current_token().kind == TokenType::KwMut {
                    ExprNodeKind::MutRef { target }
                } else {
                    ExprNodeKind::Ref { target }
                };
                ExprRedNode {
                    span: ref_span,
                    inner: Arc::new(ExprNode { kind }),
                }
            }

            // 其余一切视为原子
            _ => {
                let atom = self.parse_atom_expr()?;
                ExprRedNode {
                    span: start_span,
                    inner: Arc::new(ExprNode {
                        kind: ExprNodeKind::Atom { expr: atom },
                    }),
                }
            }
        };


        // led
        loop {
            let token = self.current_token().clone();
            let kind = token.kind;
            let token_span = token.span.clone();

            let lbp = match kind {
                TokenType::UserOp => self.user_op_info.get(&token.text).map(|(p, _)| *p),
                _ => Self::lbp(kind.clone()),
            };

            if let Some(lbp) = lbp {
                if lbp < min_bp {
                    break;
                }

                // 分支处理
                match kind {
                    TokenType::UserOp => {
                        let (prio, op_kind) = self.user_op_info.get(&token.text)
                            .expect("UserOp must be in user_op_info");
                        match op_kind {
                            OperatorKind::Infix => {
                                self.skip_token();
                                let rhs = self.parse_expr_bp(lbp + 1)?; // 左结合
                                lhs = ExprRedNode {
                                    span: token_span,
                                    inner: Arc::new(ExprNode {
                                        kind: ExprNodeKind::Binary {
                                            left: lhs,
                                            op: Operator::UserOp(token.text.clone()),
                                            right: rhs,
                                        },
                                    }),
                                };
                                continue;
                            }
                            OperatorKind::Postfix => {
                                self.skip_token();
                                lhs = ExprRedNode {
                                    span: token_span,
                                    inner: Arc::new(ExprNode {
                                        kind: ExprNodeKind::Unary {
                                            op: Operator::UserOp(token.text.clone()),
                                            right: lhs,
                                        },
                                    }),
                                };
                                continue;
                            }
                            _ => {
                               return Err(DiagMsg {
                                    title: format!("{:?}", ParserError::InvalidExpression),
                                    msg: format!("Unexpected prefix operator '{}' in infix position", token.text),
                                    span: token.span.clone(),
                                });
                            }
                        }
                    }
                    // 内置中缀运算符
                    _ => {
                        self.skip_token();
                        let rhs = self.parse_expr_bp(lbp + 1)?;
                        lhs = ExprRedNode {
                            span: token_span.clone(),
                            inner: Arc::new(ExprNode {
                                kind: ExprNodeKind::Binary {
                                    left: lhs,
                                    op: Self::token_type_to_operator(kind).ok_or_else(|| DiagMsg {
                                        title: format!("{:?}", ParserError::InvalidOperator),
                                        msg: "invalid operator".to_string(),
                                        span: token_span.clone(),
                                    })?,
                                    right: rhs,
                                },
                            }),
                        };
                        continue;
                    }
                }
            }

            match kind {
                TokenType::Lparen => {
                    self.skip_token(); // '('
                    let mut args = vec![];
                    while self.current_token().kind != TokenType::Rparen {
                        args.push(self.parse_expr()?);
                        if self.current_token().kind == TokenType::Comma {
                            self.skip_token();
                        } else if self.current_token().kind == TokenType::Rparen {
                            break;
                        } else {
                            return Err(DiagMsg {
                                title: format!("{:?}", ParserError::InvalidCallArgumentList),
                                msg: "invalid call argument list".to_string(),
                                span: token_span.clone(),
                            });
                        }
                    }
                    self.skip_token_only(TokenType::Rparen)?;
                    lhs = ExprRedNode {
                        span: token_span,
                        inner: Arc::new(ExprNode {
                            kind: ExprNodeKind::Call {
                                callee: lhs,
                                args,
                            },
                        }),
                    };
                    continue;
                }

                TokenType::KwAs => {
                    const AS_BP: usize = 20;
                    if AS_BP < min_bp {
                        break;
                    }
                    self.skip_token();
                    let into_type = self.parse_expr_bp(AS_BP)?;
                    lhs = ExprRedNode {
                        span: token_span.clone(),
                        inner: Arc::new(ExprNode {
                            kind: ExprNodeKind::TypeCast {
                                expr: lhs,
                                into_type,
                            },
                        }),
                    };
                    continue;
                }

                TokenType::Dot => {
                    self.skip_token(); // '.'
                    let member_token = self.current_token();
                    let member = member_token.text.clone();
                    self.skip_token_only(TokenType::Ident)?;
                    lhs = ExprRedNode {
                        span: token_span,
                        inner: Arc::new(ExprNode {
                            kind: ExprNodeKind::Member {
                                left: lhs,
                                right: member,
                            },
                        }),
                    };
                    continue;
                }

                _ => break,
            }
        }

        Ok(lhs)
    }
}