use std::fmt::format;
use leafc_coreapi::ast::{AtomExprNode, DeclNode, ElseIf, ExprNode, ExprNodeId, ExprNodeKind, Operator};
use leafc_coreapi::ast::ExprNodeKind::{Atom, If};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::TokenType;
use leafc_coreapi::lexer::TokenType::Pipe;
use leafc_coreapi::parser::ParserError;
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn push_expr(&mut self, expr: ExprNode) -> ExprNodeId {
        self.ast.expr_pool.push(expr);
        self.ast.expr_pool.len() - 1
    }

    pub fn parse_block_expr(&mut self) -> Result<ExprNodeId, DiagMsg> {
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

        Ok(self.push_expr(ExprNode {
            span,
            kind: ExprNodeKind::Do { exprs },
        }))
    }

    pub fn parse_let_expr(&mut self) -> Result<ExprNodeId, DiagMsg> {
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

        Ok(self.push_expr(ExprNode {
            span, kind: ExprNodeKind::Let { expr, name, type_str, mutable }
        }))
    }

    pub fn parse_do_expr(&mut self) -> Result<ExprNodeId, DiagMsg> {
        let span = self.current_token().span.clone();
        self.skip_token_only(TokenType::KwDo)?;
        self.skip_token_only(TokenType::NewLine)?;

        self.parse_block_expr()
    }

    pub fn parse_if_expr(&mut self) -> Result<ExprNodeId, DiagMsg> {
        let span = self.current_token().span.clone();
        self.skip_token_only(TokenType::KwIf)?;
        let cond = self.parse_expr()?;
        let if_then_exprs = if self.current_token().kind == TokenType::KwThen {
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
                elif_body_exprs.push(ElseIf{cond, body});
            }
        }


        let else_body_exprs: Option<ExprNodeId> = if self.current_token().kind == TokenType::KwElse {
            self.skip_token();

            if self.current_token().kind == TokenType::NewLine {
                self.skip_token_only(TokenType::NewLine)?;
                Some(self.parse_block_expr()?)
            } else {
                Some(self.parse_expr()?)
            }
        } else { None };

        Ok(self.push_expr(ExprNode {
            span,
            kind: ExprNodeKind::If {
                cond,
                then_expr: if_then_exprs,
                elifs: elif_body_exprs,
                else_expr: else_body_exprs,
            },
        }))

    }

    pub fn parse_atom_expr(&mut self) -> Result<AtomExprNode, DiagMsg> {
        let current_token = self.current_token();
        let current_token_kind = current_token.kind.clone();
        let current_token_text = current_token.text.clone();
        let current_token_span = current_token.span.clone();

        self.skip_token();

        let expr = match current_token_kind {
            TokenType::Float => AtomExprNode::Decimal {
                dec:current_token_text, span: current_token_span },
            TokenType::Int => AtomExprNode::Int {
                int:current_token_text, span: current_token_span
            },
            TokenType::String => AtomExprNode::Str {
                string:current_token_text, span: current_token_span
            },
            TokenType::Ident => AtomExprNode::Name {
                name:current_token_text, span: current_token_span
            },
            TokenType::DotDotDot => AtomExprNode::Ellipsis {
                span: current_token_span,
            },
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
                            source: self.source,
                        })
                    }
                }
                self.skip_token_only(TokenType::Rbracket)?;

                AtomExprNode::Tuple {
                    exprs,
                    span: current_token_span,
                }
            }
            _ => {
                return Err(DiagMsg {
                    title: format!("{:?}", ParserError::InvalidExpression),
                    msg: "invalid expression literal".to_string(),
                    span: current_token_span,
                    source: self.source,
                })
            }
        };

        Ok(expr)
    }

    /// 中缀运算符左绑定优先级
    fn lbp(token: TokenType) -> Option<usize> {
        match token {
            TokenType::Or   => Some(10),
            TokenType::And  => Some(20),

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


    pub fn parse_expr(&mut self) -> Result<ExprNodeId, DiagMsg> {
        match self.current_token().kind {
            TokenType::KwIf  => return self.parse_if_expr(),
            TokenType::KwDo  => return self.parse_do_expr(),
            TokenType::KwLet => return self.parse_let_expr(),
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
            TokenType::EqEq   => Some(Operator::Eq),
            TokenType::Ne  => Some(Operator::Neq),
            TokenType::Lt     => Some(Operator::Lt),
            TokenType::Gt     => Some(Operator::Gt),
            TokenType::Le   => Some(Operator::Le),
            TokenType::Ge   => Some(Operator::Ge),
            _ => None
        }
    }

    /// 以最小绑定强度 min_bp 继续解析表达式
    fn parse_expr_bp(&mut self, min_bp: usize) -> Result<ExprNodeId, DiagMsg> {
        // nud
        let token = self.current_token().clone();
        let kind = token.kind;
        let start_span = token.span.clone();

        let mut lhs = match kind {
            // =- / not
            TokenType::Minus | TokenType::Not => {
                let op_kind = kind;
                let op_span = start_span;
                self.skip_token();
                let operand = self.parse_expr_bp(Self::rbp(op_kind.clone()).unwrap())?;
                self.push_expr(ExprNode {
                    span: op_span,
                    kind: ExprNodeKind::Unary {
                        op: Self::token_type_to_operator(op_kind).unwrap(),
                        right: operand,
                    }
                })
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
                    TokenType::KwMove  => ExprNodeKind::Move  { target },
                    TokenType::KwCopy  => ExprNodeKind::Copy  { target },
                    TokenType::KwShared => ExprNodeKind::Share { target },
                    _ => unreachable!(),
                };
                self.push_expr(ExprNode {span: kw_span, kind })
            }

            // ref / ref mut
            TokenType::KwRef => {
                let ref_span = start_span;
                self.skip_token();
                let target = if self.current_token().kind == TokenType::KwMut {
                    self.skip_token();
                    let t = self.parse_expr_bp(60)?;
                    self.push_expr(ExprNode {
                        kind: ExprNodeKind::MutRef { target: t },
                        span: ref_span
                    })
                } else {
                    let t = self.parse_expr_bp(60)?;
                    self.push_expr(ExprNode {
                        kind: ExprNodeKind::Ref { target: t },
                        span: ref_span
                    })
                };
                target
            }

            // 其余一切视为原子
            _ => {
                let atom = self.parse_atom_expr()?;
                self.push_expr(ExprNode {
                    kind: ExprNodeKind::Atom { expr: atom },
                    span: start_span,
                })
            }
        };

        // led
        loop {
            let token = self.current_token().clone();
            let kind = token.kind;
            let token_span = token.span.clone();

            // 中缀二元运算符
            if let Some(lbp) = Self::lbp(kind.clone()) {
                if lbp < min_bp {
                    break;
                }
                self.skip_token();
                let rhs = self.parse_expr_bp(lbp + 1)?;
                lhs = self.push_expr(ExprNode {
                    kind: ExprNodeKind::Binary {
                        left: lhs,
                        op: Self::token_type_to_operator(kind).ok_or_else(
                            || DiagMsg {
                                title: format!("{:?}", ParserError::InvalidOperator),
                                msg: "invalid operator".to_string(),
                                span: token_span.clone(),
                                source: self.source,
                            })?,
                        right: rhs,
                    },
                    span: token_span,
                });
                continue;
            }

            // 调用 / 成员访问
            match kind {
                TokenType::Lparen => {
                    self.skip_token(); // '('
                    let call_span = token_span;
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
                                span: call_span,
                                source: self.source,
                            });
                        }
                    }
                    self.skip_token_only(TokenType::Rparen)?;
                    lhs = self.push_expr(ExprNode {
                        kind: ExprNodeKind::Call {
                            callee: lhs,
                            args,
                        },
                        span: call_span,
                    });
                    continue;
                }

                TokenType::Dot => {
                    self.skip_token(); // '.'
                    let member_token = self.current_token();
                    let member = member_token.text.clone();
                    let member_span = member_token.span.clone();
                    self.skip_token_only(TokenType::Ident)?;
                    lhs = self.push_expr(ExprNode {
                        kind: ExprNodeKind::Member {
                            left: lhs,
                            right: member,
                        },

                        span: token_span,
                    });
                    continue;
                }

                _ => break,
            }
        }

        Ok(lhs)
    }
}