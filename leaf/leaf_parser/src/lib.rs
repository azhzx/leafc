use leaf_coreapi::ast::{CrateAst, FileRedUnit, GreenNodeBuilder, LeafLanguage, RequireRedNode, SyntaxKind, Visibility};
use leaf_coreapi::crate_meta::{OperatorDef, OperatorKind};
use leaf_coreapi::diagnose::{
    CompileTimeErrorKind, DiagCtx, ErrorKind, LocalizedMessage, MsgKind, ParserErrorKind,
};
use leaf_coreapi::id::FileId;
use leaf_coreapi::source::Span;
use leaf_coreapi::token::{Token, TokenType};
use rowan::Language;
use std::collections::HashMap;
use std::sync::Arc;

fn raw_kind(kind: SyntaxKind) -> rowan::SyntaxKind {
    LeafLanguage::kind_to_raw(kind)
}

fn token_to_syntax_kind(tok: &TokenType) -> rowan::SyntaxKind {
    let kind: SyntaxKind = match tok {
        TokenType::Ident => SyntaxKind::Ident,
        TokenType::Int => SyntaxKind::Int,
        TokenType::Float => SyntaxKind::Float,
        TokenType::String => SyntaxKind::String,
        TokenType::NewLine => SyntaxKind::NewLine,
        TokenType::Indent => SyntaxKind::Indent,
        TokenType::Dedent => SyntaxKind::Dedent,
        TokenType::KwFun => SyntaxKind::KwFun,
        TokenType::KwType => SyntaxKind::KwType,
        TokenType::KwImpl => SyntaxKind::KwImpl,
        TokenType::KwWhere => SyntaxKind::KwWhere,
        TokenType::KwIf => SyntaxKind::KwIf,
        TokenType::KwThen => SyntaxKind::KwThen,
        TokenType::KwElse => SyntaxKind::KwElse,
        TokenType::KwElif => SyntaxKind::KwElif,
        TokenType::KwLet => SyntaxKind::KwLet,
        TokenType::KwMut => SyntaxKind::KwMut,
        TokenType::KwReturn => SyntaxKind::KwReturn,
        TokenType::KwWhen => SyntaxKind::KwWhen,
        TokenType::KwRaise => SyntaxKind::KwRaise,
        TokenType::KwWith => SyntaxKind::KwWith,
        TokenType::KwCatch => SyntaxKind::KwCatch,
        TokenType::KwResume => SyntaxKind::KwResume,
        TokenType::KwConst => SyntaxKind::KwConst,
        TokenType::KwGlobal => SyntaxKind::KwGlobal,
        TokenType::KwEffect => SyntaxKind::KwEffect,
        TokenType::KwExternal => SyntaxKind::KwExternal,
        TokenType::KwCType => SyntaxKind::KwCType,
        TokenType::KwAbst => SyntaxKind::KwAbst,
        TokenType::KwUse => SyntaxKind::KwUse,
        TokenType::KwOnly => SyntaxKind::KwOnly,
        TokenType::KwPub => SyntaxKind::KwPub,
        TokenType::KwRef => SyntaxKind::KwRef,
        TokenType::KwShare => SyntaxKind::KwShare,
        TokenType::KwMove => SyntaxKind::KwMove,
        TokenType::KwCopy => SyntaxKind::KwCopy,
        TokenType::KwBinding => SyntaxKind::KwBinding,
        TokenType::KwIs => SyntaxKind::KwIs,
        TokenType::KwAs => SyntaxKind::KwAs,
        TokenType::KwDo => SyntaxKind::KwDo,
        TokenType::KwUnsafeCallExternal => SyntaxKind::KwUnsafeCallExternal,
        TokenType::KwTypeOf => SyntaxKind::KwTypeOf,
        TokenType::KwOf => SyntaxKind::KwOf,
        TokenType::Lparen => SyntaxKind::Lparen,
        TokenType::Rparen => SyntaxKind::Rparen,
        TokenType::Lbrace => SyntaxKind::Lbrace,
        TokenType::Rbrace => SyntaxKind::Rbrace,
        TokenType::Lbracket => SyntaxKind::Lbracket,
        TokenType::Rbracket => SyntaxKind::Rbracket,
        TokenType::Comma => SyntaxKind::Comma,
        TokenType::Dot => SyntaxKind::Dot,
        TokenType::DotDot => SyntaxKind::DotDot,
        TokenType::DotDotDot => SyntaxKind::DotDotDot,
        TokenType::Colon => SyntaxKind::Colon,
        TokenType::Semicolon => SyntaxKind::Semicolon,
        TokenType::Eq => SyntaxKind::Eq,
        TokenType::Arrow => SyntaxKind::Arrow,
        TokenType::FatArrow => SyntaxKind::FatArrow,
        TokenType::Pipe => SyntaxKind::Pipe,
        TokenType::PipeLine => SyntaxKind::PipeLine,
        TokenType::Plus => SyntaxKind::Plus,
        TokenType::Minus => SyntaxKind::Minus,
        TokenType::Star => SyntaxKind::Star,
        TokenType::Slash => SyntaxKind::Slash,
        TokenType::Percent => SyntaxKind::Percent,
        TokenType::Caret => SyntaxKind::Caret,
        TokenType::Not => SyntaxKind::Not,
        TokenType::Or => SyntaxKind::Or,
        TokenType::And => SyntaxKind::And,
        TokenType::EqEq => SyntaxKind::EqEq,
        TokenType::Ne => SyntaxKind::Ne,
        TokenType::Lt => SyntaxKind::Lt,
        TokenType::Gt => SyntaxKind::Gt,
        TokenType::Le => SyntaxKind::Le,
        TokenType::Ge => SyntaxKind::Ge,
        TokenType::At => SyntaxKind::At,
        TokenType::Hash => SyntaxKind::Hash,
        _ => SyntaxKind::Error,
    };
    raw_kind(kind)
}

pub struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    builder: GreenNodeBuilder<'a>,
    diag: &'a mut DiagCtx,
    user_operators: &'a HashMap<String, OperatorDef>,
    user_op_info: &'a HashMap<String, (usize, OperatorKind)>,
    source_id: FileId,
}

impl<'a> Parser<'a> {
    pub fn new(
        tokens: &'a [Token],
        builder: GreenNodeBuilder<'a>,
        diag: &'a mut DiagCtx,
        user_operators: &'a HashMap<String, OperatorDef>,
        user_op_info: &'a HashMap<String, (usize, OperatorKind)>,
        source_id: FileId,
    ) -> Self {
        Self {
            tokens,
            pos: 0,
            builder,
            diag,
            user_operators,
            user_op_info,
            source_id,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }
    fn current(&self) -> &Token {
        self.peek()
            .unwrap_or_else(|| &self.tokens[self.tokens.len() - 1])
    }
    fn current_kind(&self) -> TokenType {
        self.current().kind.clone()
    }
    fn current_text(&self) -> &str {
        &self.current().text
    }
    fn bump(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn eat(&mut self, kind: TokenType) -> bool {
        if self.current_kind() == kind {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenType) -> Result<(), ()> {
        if self.current_kind() == kind {
            let text = self.current_text().to_string();
            let syntax = token_to_syntax_kind(&kind);
            self.builder.token(syntax, &text);
            self.bump();
            Ok(())
        } else {
            self.error(&format!(
                "expected {:?}, found {:?}",
                kind,
                self.current_kind()
            ));
            Err(())
        }
    }

    fn error(&mut self, msg: &str) {
        let span = self.current().span.clone();
        self.diag.emit_error(
            ErrorKind::CompileTimeError(CompileTimeErrorKind::ParserError(
                ParserErrorKind::InvalidExpression,
            )),
            span,
            LocalizedMessage::new(MsgKind::ParserInvalidExpression, [msg.to_string()]),
        );
    }

    fn skip_layout(&mut self) {
        while matches!(
            self.current_kind(),
            TokenType::NewLine | TokenType::Indent | TokenType::Dedent
        ) {
            self.bump();
        }
    }

    fn start_node(&mut self, kind: SyntaxKind) {
        self.builder.start_node(raw_kind(kind));
    }
    fn finish_node(&mut self) {
        self.builder.finish_node();
    }

    fn parse_generic_params(&mut self) -> Result<(), ()> {
        self.expect(TokenType::Lbracket)?;
        while self.current_kind() != TokenType::Rbracket {
            self.start_node(SyntaxKind::GenericVar);
            let name = self.current_text().to_string();
            self.builder.token(raw_kind(SyntaxKind::Ident), &name);
            self.bump();
            if self.current_kind() == TokenType::Colon {
                self.builder.token(raw_kind(SyntaxKind::Colon), ":");
                self.bump();
                self.parse_type()?;
            }
            self.finish_node(); // GenericVar

            if self.current_kind() == TokenType::Comma {
                self.builder.token(raw_kind(SyntaxKind::Comma), ",");
                self.bump();
            } else if self.current_kind() == TokenType::Rbracket {
                break;
            }
        }
        self.expect(TokenType::Rbracket)?;
        Ok(())
    }

    fn parse_param_list(&mut self) -> Result<(), ()> {
        self.expect(TokenType::Lparen)?;
        while self.current_kind() != TokenType::Rparen {
            self.start_node(SyntaxKind::Param);
            let name = self.current_text().to_string();
            self.builder.token(raw_kind(SyntaxKind::Ident), &name);
            self.bump();
            if self.current_kind() == TokenType::Colon {
                self.builder.token(raw_kind(SyntaxKind::Colon), ":");
                self.bump();
                self.parse_type()?;
            }
            self.finish_node();

            if self.current_kind() == TokenType::Comma {
                self.builder.token(raw_kind(SyntaxKind::Comma), ",");
                self.bump();
            } else if self.current_kind() == TokenType::Rparen {
                break;
            }
        }
        self.expect(TokenType::Rparen)?;
        Ok(())
    }

    fn parse_impl_list(&mut self) -> Result<(), ()> {
        self.expect(TokenType::KwImpl)?;
        while self.current_kind() == TokenType::Ident {
            let name = self.current_text().to_string();
            self.builder.token(raw_kind(SyntaxKind::Ident), &name);
            self.bump();
            if self.current_kind() == TokenType::Plus {
                self.builder.token(raw_kind(SyntaxKind::Plus), "+");
                self.bump();
            } else {
                break;
            }
        }
        Ok(())
    }

    fn parse_visibility(&mut self) -> Visibility {
        if self.eat(TokenType::KwPub) {
            if self.current_kind() == TokenType::Lparen {
                self.builder.token(raw_kind(SyntaxKind::Lparen), "(");
                self.bump();
                let _ = self.expect(TokenType::KwExternal);
                let _ = self.expect(TokenType::Rparen);
                Visibility::PublicExternal
            } else {
                Visibility::Public
            }
        } else {
            Visibility::Private
        }
    }

    fn parse_annotations(&mut self) -> Result<(), ()> {
        while self.current_kind() == TokenType::Hash {
            self.start_node(SyntaxKind::Annotation);
            self.builder.token(raw_kind(SyntaxKind::Hash), "#");
            self.bump();
            let name = self.current_text().to_string();
            self.builder.token(raw_kind(SyntaxKind::Ident), &name);
            self.bump();
            if self.current_kind() == TokenType::Lparen {
                self.builder.token(raw_kind(SyntaxKind::Lparen), "(");
                self.bump();
                while self.current_kind() != TokenType::Rparen {
                    let arg = self.current_text().to_string();
                    self.builder.token(raw_kind(SyntaxKind::Ident), &arg);
                    self.bump();
                    if self.current_kind() == TokenType::Comma {
                        self.builder.token(raw_kind(SyntaxKind::Comma), ",");
                        self.bump();
                    }
                }
                self.expect(TokenType::Rparen)?;
            }
            self.expect(TokenType::NewLine)?;
            self.finish_node();
        }
        Ok(())
    }

    fn parse_type(&mut self) -> Result<(), ()> {
        match self.current_kind() {
            TokenType::KwRef => {
                self.start_node(SyntaxKind::RefType);
                self.builder.token(raw_kind(SyntaxKind::KwRef), "ref");
                self.bump();
                let _ = self.eat(TokenType::KwMut);
                self.parse_type()?;
                self.finish_node();
                Ok(())
            }
            TokenType::KwShare => {
                self.start_node(SyntaxKind::ShareType);
                self.builder.token(raw_kind(SyntaxKind::KwShare), "share");
                self.bump();
                self.parse_type()?;
                self.finish_node();
                Ok(())
            }
            TokenType::Lparen => {
                self.start_node(SyntaxKind::TupleType);
                self.expect(TokenType::Lparen)?;
                while self.current_kind() != TokenType::Rparen {
                    self.parse_type()?;
                    if self.current_kind() == TokenType::Comma {
                        self.expect(TokenType::Comma)?;
                    } else if self.current_kind() == TokenType::Rparen {
                        break;
                    }
                }
                self.expect(TokenType::Rparen)?;
                self.finish_node();
                Ok(())
            }
            TokenType::KwImpl => {
                self.start_node(SyntaxKind::ImplType);
                self.builder.token(raw_kind(SyntaxKind::KwImpl), "impl");
                self.bump();
                self.parse_type()?;
                self.finish_node();
                Ok(())
            }
            TokenType::KwFun => {
                self.start_node(SyntaxKind::FunType);
                self.builder.token(raw_kind(SyntaxKind::KwFun), "fun");
                self.bump();
                self.expect(TokenType::Lparen)?;
                while self.current_kind() != TokenType::Rparen {
                    self.parse_type()?;
                    if self.current_kind() == TokenType::Comma {
                        self.expect(TokenType::Comma)?;
                    } else if self.current_kind() == TokenType::Rparen {
                        break;
                    }
                }
                self.expect(TokenType::Rparen)?;
                self.expect(TokenType::Arrow)?;
                self.parse_type()?;
                self.finish_node();
                Ok(())
            }
            TokenType::KwTypeOf => {
                self.start_node(SyntaxKind::TypeofType);
                self.builder.token(raw_kind(SyntaxKind::KwTypeOf), "typeof");
                self.bump();
                self.expect(TokenType::Lparen)?;
                self.parse_expr()?;
                self.expect(TokenType::Rparen)?;
                self.finish_node();
                Ok(())
            }
            TokenType::Ident if self.current_text() == "_" => {
                self.start_node(SyntaxKind::WildcardType);
                self.builder.token(raw_kind(SyntaxKind::Ident), "_");
                self.bump();
                self.finish_node();
                Ok(())
            }
            _ => {
                self.start_node(SyntaxKind::NamedType);
                self.parse_path()?;
                if self.current_kind() == TokenType::Lbracket {
                    self.builder.token(raw_kind(SyntaxKind::Lbracket), "[");
                    self.bump();
                    while self.current_kind() != TokenType::Rbracket {
                        self.parse_type()?;
                        if self.current_kind() == TokenType::Comma {
                            self.builder.token(raw_kind(SyntaxKind::Comma), ",");
                            self.bump();
                        } else if self.current_kind() == TokenType::Rbracket {
                            break;
                        }
                    }
                    self.expect(TokenType::Rbracket)?;
                }
                self.finish_node();
                Ok(())
            }
        }
    }

    fn parse_path(&mut self) -> Result<(), ()> {
        self.start_node(SyntaxKind::Path);
        if self.current_kind() != TokenType::Ident {
            self.error("expected identifier in path");
            return Err(());
        }
        let name = self.current_text().to_string();
        self.builder.token(raw_kind(SyntaxKind::Ident), &name);
        self.bump();
        while self.current_kind() == TokenType::Dot {
            if self.pos + 1 < self.tokens.len()
                && self.tokens[self.pos + 1].kind == TokenType::Ident
            {
                self.builder.token(raw_kind(SyntaxKind::Dot), ".");
                self.bump();
                let seg = self.current_text().to_string();
                self.builder.token(raw_kind(SyntaxKind::Ident), &seg);
                self.bump();
            } else {
                break;
            }
        }
        self.finish_node();
        Ok(())
    }

    /// atom expr
    fn parse_atom_expr(&mut self) -> Result<(), ()> {
        let tok = self.current_kind();
        match tok {
            TokenType::Int | TokenType::Float | TokenType::String => {
                self.start_node(SyntaxKind::AtomExpr);
                let text = self.current_text().to_string();
                self.builder.token(token_to_syntax_kind(&tok), &text);
                self.bump();
                self.finish_node();
                Ok(())
            }
            TokenType::Ident => {
                self.start_node(SyntaxKind::StaticPathExpr);
                self.parse_path()?;
                self.finish_node();
                Ok(())
            }
            TokenType::Lparen => {
                self.start_node(SyntaxKind::AtomExpr);
                self.builder.token(raw_kind(SyntaxKind::Lparen), "(");
                self.bump();
                if self.current_kind() == TokenType::Rparen {
                    self.builder.token(raw_kind(SyntaxKind::Rparen), ")");
                    self.bump();
                    self.finish_node();
                    return Ok(());
                }
                self.parse_expr()?;
                if self.current_kind() == TokenType::Comma {
                    while self.eat(TokenType::Comma) {
                        if self.current_kind() == TokenType::Rparen {
                            break;
                        }
                        self.parse_expr()?;
                    }
                }
                self.expect(TokenType::Rparen)?;
                self.finish_node();
                Ok(())
            }
            TokenType::DotDotDot => {
                self.start_node(SyntaxKind::AtomExpr);
                self.builder.token(raw_kind(SyntaxKind::DotDotDot), "...");
                self.bump();
                self.finish_node();
                Ok(())
            }
            _ => {
                self.error("unexpected token in atom expression");
                Err(())
            }
        }
    }

    /// if / elif / else expr
    fn parse_if_expr(&mut self) -> Result<(), ()> {
        self.start_node(SyntaxKind::IfExpr);
        self.builder.token(raw_kind(SyntaxKind::KwIf), "if");
        self.bump();
        self.parse_expr()?;
        self.skip_layout();
        if self.eat(TokenType::KwThen) {
            self.parse_expr()?;
        } else {
            self.expect(TokenType::Indent)?;
            while self.current_kind() != TokenType::Dedent {
                self.parse_expr()?;
                self.skip_layout();
            }
            self.expect(TokenType::Dedent)?;
        }
        while self.current_kind() == TokenType::KwElif {
            self.start_node(SyntaxKind::ElseIf);
            self.builder.token(raw_kind(SyntaxKind::KwElif), "elif");
            self.bump();
            self.parse_expr()?;
            self.skip_layout();
            if self.current_kind() == TokenType::Indent {
                self.builder.token(raw_kind(SyntaxKind::Indent), "");
                self.bump();
                while self.current_kind() != TokenType::Dedent {
                    self.parse_expr()?;
                    self.skip_layout();
                }
                self.expect(TokenType::Dedent)?;
            } else {
                self.parse_expr()?;
            }
            self.finish_node(); // ElseIf
        }
        if self.eat(TokenType::KwElse) {
            self.skip_layout();
            if self.current_kind() == TokenType::Indent {
                self.builder.token(raw_kind(SyntaxKind::Indent), "");
                self.bump();
                while self.current_kind() != TokenType::Dedent {
                    self.parse_expr()?;
                    self.skip_layout();
                }
                self.expect(TokenType::Dedent)?;
            } else {
                self.parse_expr()?;
            }
        }
        self.finish_node(); // IfExpr
        Ok(())
    }

    /// do
    fn parse_do_expr(&mut self) -> Result<(), ()> {
        self.start_node(SyntaxKind::DoExpr);
        self.builder.token(raw_kind(SyntaxKind::KwDo), "do");
        self.bump();
        self.skip_layout();
        self.expect(TokenType::Indent)?;
        while self.current_kind() != TokenType::Dedent {
            self.parse_expr()?;
            self.skip_layout();
        }
        self.expect(TokenType::Dedent)?;
        self.finish_node();
        Ok(())
    }

    /// let
    fn parse_let_expr(&mut self) -> Result<(), ()> {
        self.start_node(SyntaxKind::LetExpr);
        self.builder.token(raw_kind(SyntaxKind::KwLet), "let");
        self.bump();
        let is_mut = self.eat(TokenType::KwMut);
        if is_mut {
            self.builder.token(raw_kind(SyntaxKind::KwMut), "mut");
        }
        self.expect(TokenType::Ident)?;
        if self.current_kind() == TokenType::Colon {
            self.builder.token(raw_kind(SyntaxKind::Colon), ":");
            self.bump();
            self.parse_type()?;
        }
        self.expect(TokenType::Eq)?;
        self.parse_expr()?;
        self.skip_layout();
        self.finish_node();
        Ok(())
    }

    /// when
    fn parse_match_expr(&mut self) -> Result<(), ()> {
        self.start_node(SyntaxKind::MatchExpr);
        self.builder.token(raw_kind(SyntaxKind::KwWhen), "when");
        self.bump();
        self.parse_expr()?;
        self.skip_layout();
        self.expect(TokenType::Indent)?;
        while self.current_kind() != TokenType::Dedent {
            self.start_node(SyntaxKind::MatchArm);
            self.parse_pattern()?;
            if self.eat(TokenType::KwIf) {
                self.parse_expr()?; // guard
            }
            self.expect(TokenType::FatArrow)?;
            self.parse_expr()?;
            self.skip_layout();
            self.finish_node(); // MatchArm
        }
        self.expect(TokenType::Dedent)?;
        self.finish_node();
        Ok(())
    }

    fn parse_raise_expr(&mut self) -> Result<(), ()> {
        self.start_node(SyntaxKind::RaiseExpr);
        self.builder.token(raw_kind(SyntaxKind::KwRaise), "raise");
        self.bump();

        self.start_node(SyntaxKind::Path);
        while self.current_kind() == TokenType::Ident {
            let has_more = self.pos + 1 < self.tokens.len()
                && self.tokens[self.pos + 1].kind == TokenType::Dot
                && self.pos + 2 < self.tokens.len()
                && self.tokens[self.pos + 2].kind == TokenType::Ident;
            if has_more {
                let seg = self.current_text().to_string();
                self.builder.token(raw_kind(SyntaxKind::Ident), &seg);
                self.bump();
                self.builder.token(raw_kind(SyntaxKind::Dot), ".");
                self.bump();
            } else {
                break;
            }
        }
        self.finish_node();

        if self.current_kind() != TokenType::Ident {
            self.error("expected control name in raise expression");
            return Err(());
        }
        let control = self.current_text().to_string();
        self.builder.token(raw_kind(SyntaxKind::Ident), &control);
        self.bump();

        self.expect(TokenType::Lparen)?;
        while self.current_kind() != TokenType::Rparen {
            self.parse_expr()?;
            if !self.eat(TokenType::Comma) {
                break;
            }
        }
        self.expect(TokenType::Rparen)?;
        self.finish_node(); // RaiseExpr
        Ok(())
    }

    /// with handler catch { control (binding ..) ... }
    fn parse_with_expr(&mut self) -> Result<(), ()> {
        self.start_node(SyntaxKind::WithExpr);
        self.builder.token(raw_kind(SyntaxKind::KwWith), "with");
        self.bump();
        self.parse_expr()?;
        self.skip_layout();

        while self.current_kind() == TokenType::KwCatch {
            self.start_node(SyntaxKind::CatchClause);
            self.builder.token(raw_kind(SyntaxKind::KwCatch), "catch");
            self.bump();

            // control path
            self.parse_path()?;

            self.expect(TokenType::Lparen)?;
            while self.current_kind() != TokenType::Rparen {
                if self.current_kind() == TokenType::DotDot {
                    self.start_node(SyntaxKind::CatchParam);
                    self.builder.token(raw_kind(SyntaxKind::DotDot), "..");
                    self.bump();
                    self.finish_node(); // CatchParam (rest)
                } else if self.current_kind() == TokenType::KwBinding {
                    self.start_node(SyntaxKind::CatchParam);
                    self.builder.token(raw_kind(SyntaxKind::KwBinding), "binding");
                    self.bump();
                    let name = self.current_text().to_string();
                    self.builder.token(raw_kind(SyntaxKind::Ident), &name);
                    self.bump();
                    self.finish_node(); // CatchParam (binding)
                } else {
                    self.error("catch parameter must be `binding <name>` or `..`");
                    return Err(());
                }

                if self.current_kind() == TokenType::Comma {
                    self.builder.token(raw_kind(SyntaxKind::Comma), ",");
                    self.bump();
                } else if self.current_kind() == TokenType::Rparen {
                    break;
                }
            }
            self.expect(TokenType::Rparen)?;
            self.skip_layout();

            // body block
            self.expect(TokenType::Indent)?;
            while self.current_kind() != TokenType::Dedent {
                self.parse_expr()?;
                self.skip_layout();
            }
            self.expect(TokenType::Dedent)?;
            self.finish_node(); // CatchClause
        }
        self.finish_node(); // WithExpr
        Ok(())
    }

    /// resume expr
    fn parse_resume_expr(&mut self) -> Result<(), ()> {
        self.start_node(SyntaxKind::ResumeExpr);
        self.builder.token(raw_kind(SyntaxKind::KwResume), "resume");
        self.bump();
        self.parse_expr()?;
        self.finish_node();
        Ok(())
    }

    /// unsafe_call_external callee (args)
    fn parse_unsafe_call_external_expr(&mut self) -> Result<(), ()> {
        self.start_node(SyntaxKind::UnsafeExternalCallExpr);
        self.builder.token(raw_kind(SyntaxKind::KwUnsafeCallExternal), "unsafe_call_external");
        self.bump();

        // callee as StaticPathExpr
        self.start_node(SyntaxKind::StaticPathExpr);
        self.parse_path()?;
        self.finish_node(); // StaticPathExpr

        self.expect(TokenType::Lparen)?;
        while self.current_kind() != TokenType::Rparen {
            self.parse_expr()?;
            if !self.eat(TokenType::Comma) {
                break;
            }
        }
        self.expect(TokenType::Rparen)?;
        self.finish_node();
        Ok(())
    }

    /// const(expr)
    fn parse_const_eval_expr(&mut self) -> Result<(), ()> {
        self.start_node(SyntaxKind::ConstEvalExpr);
        self.builder.token(raw_kind(SyntaxKind::KwConst), "const");
        self.bump();
        self.expect(TokenType::Lparen)?;
        self.parse_expr()?;
        self.expect(TokenType::Rparen)?;
        self.finish_node();
        Ok(())
    }

    /// return expr
    fn parse_return_expr(&mut self) -> Result<(), ()> {
        self.start_node(SyntaxKind::ReturnExpr);
        self.builder.token(raw_kind(SyntaxKind::KwReturn), "return");
        self.bump();
        if self.current_kind() != TokenType::NewLine && self.current_kind() != TokenType::Dedent {
            self.parse_expr()?;
        }
        self.finish_node();
        Ok(())
    }

    fn parse_expr(&mut self) -> Result<(), ()> {
        match self.current_kind() {
            TokenType::KwIf => self.parse_if_expr(),
            TokenType::KwDo => self.parse_do_expr(),
            TokenType::KwLet => self.parse_let_expr(),
            TokenType::KwWhen => self.parse_match_expr(),
            TokenType::KwRaise => self.parse_raise_expr(),
            TokenType::KwWith => self.parse_with_expr(),
            TokenType::KwResume => self.parse_resume_expr(),
            TokenType::KwUnsafeCallExternal => self.parse_unsafe_call_external_expr(),
            TokenType::KwConst => self.parse_const_eval_expr(),
            TokenType::KwReturn => self.parse_return_expr(),
            _ => self.parse_expr_bp(0),
        }
    }

    /// Pratt Core
    fn parse_expr_bp(&mut self, min_bp: usize) -> Result<(), ()> {
        let tok = self.current_kind();

        // NUD
        match tok {
            TokenType::Minus | TokenType::Not => {
                let rbp = 70;
                self.start_node(SyntaxKind::UnaryExpr);
                self.builder.token(token_to_syntax_kind(&tok), self.current_text());
                self.bump();
                self.parse_expr_bp(rbp)?;
                self.finish_node();
            }
            TokenType::UserOp => {
                if let Some((prio, op_kind)) = self.user_op_info.get(self.current_text()) {
                    if *op_kind == OperatorKind::Prefix {
                        let rbp = *prio;
                        self.start_node(SyntaxKind::UnaryExpr);
                        self.builder.token(raw_kind(SyntaxKind::Error), self.current_text());
                        self.bump();
                        self.parse_expr_bp(rbp)?;
                        self.finish_node();
                    } else {
                        self.error("unexpected user operator in prefix position");
                        return Err(());
                    }
                } else {
                    self.error("unknown user operator");
                    return Err(());
                }
            }
            TokenType::KwMove | TokenType::KwCopy | TokenType::KwShare => {
                let kind = match tok {
                    TokenType::KwMove => SyntaxKind::MoveExpr,
                    TokenType::KwCopy => SyntaxKind::CopyExpr,
                    TokenType::KwShare => SyntaxKind::ShareExpr,
                    _ => unreachable!(),
                };
                self.start_node(kind);
                self.builder.token(token_to_syntax_kind(&tok), self.current_text());
                self.bump();
                self.parse_expr_bp(60)?;
                self.finish_node();
            }
            TokenType::KwRef => {
                let is_mut = self.pos + 1 < self.tokens.len()
                    && self.tokens[self.pos + 1].kind == TokenType::KwMut;
                if is_mut {
                    self.start_node(SyntaxKind::MutRefExpr);
                    self.builder.token(raw_kind(SyntaxKind::KwRef), "ref");
                    self.bump();
                    self.builder.token(raw_kind(SyntaxKind::KwMut), "mut");
                    self.bump();
                } else {
                    self.start_node(SyntaxKind::RefExpr);
                    self.builder.token(raw_kind(SyntaxKind::KwRef), "ref");
                    self.bump();
                }
                self.parse_expr_bp(60)?;
                self.finish_node();
            }
            _ => {
                self.parse_atom_expr()?;
            }
        }

        // LED
        loop {
            let op = self.current_kind();
            let lbp = match op {
                TokenType::UserOp => self.user_op_info.get(self.current_text()).map(|(p, _)| *p),
                _ => self.infix_bp(&op).map(|(bp, _)| bp),
            };

            if let Some(lbp) = lbp {
                if lbp < min_bp {
                    break;
                }

                match op {
                    // 用户自定义中缀/后缀
                    TokenType::UserOp => {
                        if let Some((prio, op_kind)) = self.user_op_info.get(self.current_text()) {
                            match op_kind {
                                OperatorKind::Infix => {
                                    self.start_node(SyntaxKind::BinaryExpr);
                                    self.builder.token(raw_kind(SyntaxKind::Error), self.current_text()); // 占位
                                    self.bump();
                                    self.parse_expr_bp(prio + 1)?;
                                    self.finish_node();
                                }
                                OperatorKind::Postfix => {
                                    // 后缀运算符视为一元表达式
                                    self.start_node(SyntaxKind::UnaryExpr);
                                    self.builder.token(raw_kind(SyntaxKind::Error), self.current_text());
                                    self.bump();
                                    self.finish_node();
                                }
                                _ => {
                                    self.error("user operator used in unexpected position");
                                    return Err(());
                                }
                            }
                        } else {
                            self.error("unknown user operator");
                            return Err(());
                        }
                        continue;
                    }
                    // 内置二元运算符
                    TokenType::Plus
                    | TokenType::Minus
                    | TokenType::Star
                    | TokenType::Slash
                    | TokenType::Percent
                    | TokenType::Caret
                    | TokenType::EqEq
                    | TokenType::Ne
                    | TokenType::Lt
                    | TokenType::Gt
                    | TokenType::Le
                    | TokenType::Ge
                    | TokenType::And
                    | TokenType::Or
                    | TokenType::PipeLine => {
                        self.start_node(SyntaxKind::BinaryExpr);
                        self.builder.token(token_to_syntax_kind(&op), self.current_text());
                        self.bump();
                        self.parse_expr_bp(lbp + 1)?;
                        self.finish_node();
                        continue;
                    }
                    TokenType::Lparen => {
                        self.start_node(SyntaxKind::CallExpr);
                        self.builder.token(raw_kind(SyntaxKind::Lparen), "(");
                        self.bump();
                        self.skip_layout();
                        while self.current_kind() != TokenType::Rparen {
                            if self.current_kind() == TokenType::Ident
                                && self.pos + 1 < self.tokens.len()
                                && self.tokens[self.pos + 1].kind == TokenType::Eq
                            {
                                self.start_node(SyntaxKind::CallArg);
                                let name = self.current_text().to_string();
                                self.builder.token(raw_kind(SyntaxKind::Ident), &name);
                                self.bump();
                                self.builder.token(raw_kind(SyntaxKind::Eq), "=");
                                self.bump();
                                self.parse_expr()?;
                                self.finish_node(); // CallArg
                            } else {
                                // 位置参数
                                self.start_node(SyntaxKind::CallArg);
                                self.parse_expr()?;
                                self.finish_node(); // CallArg
                            }
                            if self.eat(TokenType::Comma) {
                                self.skip_layout();
                            } else {
                                break;
                            }
                        }
                        self.expect(TokenType::Rparen)?;
                        self.finish_node(); // CallExpr
                        continue;
                    }
                    // 成员访问 / 元组索引
                    TokenType::Dot => {
                        self.builder.token(raw_kind(SyntaxKind::Dot), ".");
                        self.bump();
                        if self.current_kind() == TokenType::Int {
                            // 元组索引
                            self.start_node(SyntaxKind::TupleIndexExpr);
                            let index = self.current_text().to_string();
                            self.builder.token(raw_kind(SyntaxKind::Int), &index);
                            self.bump();
                            self.finish_node();
                        } else if self.current_kind() == TokenType::Ident {
                            // 成员访问
                            self.start_node(SyntaxKind::MemberAccessExpr);
                            let member = self.current_text().to_string();
                            self.builder.token(raw_kind(SyntaxKind::Ident), &member);
                            self.bump();
                            self.finish_node();
                        } else {
                            self.error("expected field name or integer after '.'");
                            return Err(());
                        }
                        continue;
                    }
                    // 结构体构造 {}
                    TokenType::Lbrace => {
                        self.start_node(SyntaxKind::MakeStructExpr);
                        self.builder.token(raw_kind(SyntaxKind::Lbrace), "{");
                        self.bump();
                        while self.current_kind() != TokenType::Rbrace {
                            self.start_node(SyntaxKind::StructFieldInit);
                            // 字段名
                            self.expect(TokenType::Ident)?;
                            self.expect(TokenType::Eq)?;
                            self.parse_expr()?;
                            self.finish_node(); // StructFieldInit
                            if !self.eat(TokenType::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenType::Rbrace)?;
                        self.finish_node(); // MakeStructExpr
                        continue;
                    }
                    _ => break,
                }
            } else {
                match op {
                    TokenType::KwIs => {
                        const IS_BP: usize = 30;
                        if IS_BP < min_bp {
                            break;
                        }
                        self.start_node(SyntaxKind::IsExpr);
                        self.builder.token(raw_kind(SyntaxKind::KwIs), "is");
                        self.bump();
                        self.parse_pattern()?;
                        self.finish_node();
                        continue;
                    }
                    TokenType::KwAs => {
                        const AS_BP: usize = 20;
                        if AS_BP < min_bp {
                            break;
                        }
                        self.start_node(SyntaxKind::TypeCastExpr);
                        self.builder.token(raw_kind(SyntaxKind::KwAs), "as");
                        self.bump();
                        self.parse_type()?;
                        self.finish_node();
                        continue;
                    }
                    _ => break,
                }
            }
        }

        Ok(())
    }

    fn infix_bp(&self, tok: &TokenType) -> Option<(usize, bool)> {
        match tok {
            TokenType::PipeLine => Some((1, false)),
            TokenType::Or => Some((10, false)),
            TokenType::And => Some((20, false)),
            TokenType::EqEq | TokenType::Ne | TokenType::Lt | TokenType::Gt | TokenType::Le | TokenType::Ge => Some((30, false)),
            TokenType::Plus | TokenType::Minus => Some((40, false)),
            TokenType::Star | TokenType::Slash | TokenType::Percent => Some((50, false)),
            TokenType::Caret => Some((60, true)),
            TokenType::Lparen => Some((100, false)),
            TokenType::Dot => Some((90, false)),
            _ => None,
        }
    }

    fn skip_pattern(&self, start: usize) -> usize {
        let mut p = start;
        if p >= self.tokens.len() {
            return p;
        }
        let kind = &self.tokens[p].kind;
        match kind {
            TokenType::Ident if self.tokens[p].text == "_" => {
                p += 1; // _
            }
            TokenType::KwBinding => {
                p += 2; // binding name
            }
            TokenType::DotDot => {
                p += 1; // ..
            }
            TokenType::Int | TokenType::Float | TokenType::String => {
                p += 1; // Literal
            }
            TokenType::Lparen => {
                p += 1; // '('
                let mut depth = 1;
                while p < self.tokens.len() && depth > 0 {
                    match &self.tokens[p].kind {
                        TokenType::Lparen => depth += 1,
                        TokenType::Rparen => depth -= 1,
                        _ => {}
                    }
                    p += 1;
                }
            }
            TokenType::Ident => {
                let next = if p + 1 < self.tokens.len() {
                    &self.tokens[p + 1].kind
                } else {
                    &TokenType::NewLine
                };
                match next {
                    TokenType::Lparen => {
                        p += 2; // ident '('
                        let mut depth = 1;
                        while p < self.tokens.len() && depth > 0 {
                            match &self.tokens[p].kind {
                                TokenType::Lparen => depth += 1,
                                TokenType::Rparen => depth -= 1,
                                _ => {}
                            }
                            p += 1;
                        }
                    }
                    TokenType::Lbrace => {
                        p += 2; // ident '{'
                        let mut depth = 1;
                        while p < self.tokens.len() && depth > 0 {
                            match &self.tokens[p].kind {
                                TokenType::Lbrace => depth += 1,
                                TokenType::Rbrace => depth -= 1,
                                _ => {}
                            }
                            p += 1;
                        }
                    }
                    _ => {
                        p += 1;
                    }
                }
            }
            _ => {}
        }
        while p < self.tokens.len() && matches!(&self.tokens[p].kind,
            TokenType::NewLine | TokenType::Indent | TokenType::Dedent)
        {
            p += 1;
        }
        p
    }

    fn peek_pipe_after_pattern(&self) -> bool {
        let end = self.skip_pattern(self.pos);
        end < self.tokens.len() && self.tokens[end].kind == TokenType::Pipe
    }

    fn peek_alias_after_pattern(&self) -> bool {
        let end = self.skip_pattern(self.pos);
        end < self.tokens.len() && self.tokens[end].kind == TokenType::KwBinding
    }

    fn parse_pattern(&mut self) -> Result<(), ()> {
        self.parse_or_pattern()
    }

    /// a | b | c  => Or(Or(a,b), c)
    fn parse_or_pattern(&mut self) -> Result<(), ()> {
        if self.peek_pipe_after_pattern() {
            self.start_node(SyntaxKind::OrPat);
            self.parse_or_pattern()?;   // left
            self.expect(TokenType::Pipe)?;
            self.parse_or_pattern()?;   // right
            self.finish_node();         // OrPat
        } else {
            self.parse_single_pattern()?;
        }
        Ok(())
    }

    /// pattern binding x binding y ...
    fn parse_single_pattern(&mut self) -> Result<(), ()> {
        if self.peek_alias_after_pattern() {
            self.start_node(SyntaxKind::AliasPat);
            self.parse_single_pattern()?;
            self.expect(TokenType::KwBinding)?;
            self.expect(TokenType::Ident)?; // binding
            self.finish_node();
        } else {
            self.parse_atomic_pattern_base()?;
        }
        Ok(())
    }

    fn parse_atomic_pattern_base(&mut self) -> Result<(), ()> {
        let tok = self.current_kind();
        match tok {
            TokenType::Ident if self.current_text() == "_" => {
                self.start_node(SyntaxKind::WildcardPat);
                self.builder.token(raw_kind(SyntaxKind::Ident), "_");
                self.bump();
                self.finish_node();
            }
            TokenType::KwBinding => {
                self.start_node(SyntaxKind::BindingPat);
                self.builder.token(raw_kind(SyntaxKind::KwBinding), "binding");
                self.bump();
                let name = self.current_text().to_string();
                self.builder.token(raw_kind(SyntaxKind::Ident), &name);
                self.bump();
                self.finish_node();
            }
            TokenType::DotDot => {
                self.start_node(SyntaxKind::RestPat);
                self.builder.token(raw_kind(SyntaxKind::DotDot), "..");
                self.bump();
                self.finish_node();
            }
            TokenType::Int | TokenType::Float | TokenType::String => {
                self.start_node(SyntaxKind::LiteralPat);
                let text = self.current_text().to_string();
                self.builder.token(token_to_syntax_kind(&tok), &text);
                self.bump();
                self.finish_node();
            }
            TokenType::Lparen => {
                self.start_node(SyntaxKind::TuplePat);
                self.expect(TokenType::Lparen)?;
                while self.current_kind() != TokenType::Rparen {
                    if self.current_kind() == TokenType::DotDot {
                        self.builder.token(raw_kind(SyntaxKind::DotDot), "..");
                        self.bump();
                    } else {
                        self.parse_pattern()?;
                    }
                    if self.current_kind() == TokenType::Comma {
                        self.builder.token(raw_kind(SyntaxKind::Comma), ",");
                        self.bump();
                    } else if self.current_kind() == TokenType::Rparen {
                        break;
                    }
                }
                self.expect(TokenType::Rparen)?;
                self.finish_node();
            }
            TokenType::Ident => {
                let next = if self.pos + 1 < self.tokens.len() {
                    self.tokens[self.pos + 1].kind.clone()
                } else {
                    TokenType::NewLine
                };
                match next {
                    TokenType::Lparen => {
                        // 构造器模式
                        self.start_node(SyntaxKind::ConstructorPat);
                        self.start_node(SyntaxKind::Path);
                        let name = self.current_text().to_string();
                        self.builder.token(raw_kind(SyntaxKind::Ident), &name);
                        self.bump();
                        self.finish_node(); // Path
                        self.expect(TokenType::Lparen)?;
                        while self.current_kind() != TokenType::Rparen {
                            self.parse_pattern()?;
                            if self.current_kind() == TokenType::Comma {
                                self.builder.token(raw_kind(SyntaxKind::Comma), ",");
                                self.bump();
                            } else if self.current_kind() == TokenType::Rparen {
                                break;
                            }
                        }
                        self.expect(TokenType::Rparen)?;
                        self.finish_node(); // ConstructorPat
                    }
                    TokenType::Lbrace => {
                        // 结构体模式
                        self.start_node(SyntaxKind::StructPat);
                        self.start_node(SyntaxKind::Path);
                        let name = self.current_text().to_string();
                        self.builder.token(raw_kind(SyntaxKind::Ident), &name);
                        self.bump();
                        self.finish_node(); // Path
                        self.expect(TokenType::Lbrace)?;
                        while self.current_kind() != TokenType::Rbrace {
                            if self.current_kind() == TokenType::DotDot {
                                self.builder.token(raw_kind(SyntaxKind::DotDot), "..");
                                self.bump();
                                if self.current_kind() == TokenType::Comma {
                                    self.builder.token(raw_kind(SyntaxKind::Comma), ",");
                                    self.bump();
                                }
                                break;
                            }
                            self.start_node(SyntaxKind::StructPatternField);
                            let fname = self.current_text().to_string();
                            self.builder.token(raw_kind(SyntaxKind::Ident), &fname);
                            self.bump();
                            self.expect(TokenType::Eq)?;
                            self.parse_pattern()?;
                            self.finish_node(); // StructPatternField
                            if self.current_kind() == TokenType::Comma {
                                self.builder.token(raw_kind(SyntaxKind::Comma), ",");
                                self.bump();
                            } else if self.current_kind() == TokenType::Rbrace {
                                break;
                            }
                        }
                        self.expect(TokenType::Rbrace)?;
                        self.finish_node();
                    }
                    _ => {
                        self.start_node(SyntaxKind::BindingPat);
                        let name = self.current_text().to_string();
                        self.builder.token(raw_kind(SyntaxKind::Ident), &name);
                        self.bump();
                        self.finish_node();
                    }
                }
            }
            _ => {
                self.error("unexpected token in pattern");
                return Err(());
            }
        }
        Ok(())
    }

    fn parse_atomic_pattern(&mut self) -> Result<(), ()> {
        self.parse_atomic_pattern_base()
    }


    fn parse_where_clause(&mut self) -> Result<(), ()> {
        if self.current_kind() != TokenType::KwWhere {
            return Ok(());
        }
        self.start_node(SyntaxKind::WhereClause);
        self.builder.token(raw_kind(SyntaxKind::KwWhere), "where");
        self.bump();
        self.skip_layout();
        while self.current_kind() == TokenType::Ident {
            self.start_node(SyntaxKind::WhereConstraint);
            let name = self.current_text().to_string();
            self.builder.token(raw_kind(SyntaxKind::Ident), &name);
            self.bump();
            self.expect(TokenType::Colon)?;
            loop {
                self.parse_type()?;
                if self.current_kind() == TokenType::Plus {
                    self.builder.token(raw_kind(SyntaxKind::Plus), "+");
                    self.bump();
                } else {
                    break;
                }
            }
            self.finish_node(); // WhereConstraint
            if self.current_kind() == TokenType::Comma {
                self.expect(TokenType::Comma)?;
            } else if self.current_kind() == TokenType::NewLine {
                self.skip_layout();
            } else {
                break;
            }
        }
        self.finish_node();
        Ok(())
    }

    fn parse_decl(&mut self) -> Result<(), ()> {
        self.parse_annotations()?;
        let vis = self.parse_visibility();
        match self.current_kind() {
            TokenType::KwUse => {
                self.parse_use_decl()?;
            }
            TokenType::KwFun => {
                self.parse_fun_decl(vis)?;
            }
            TokenType::KwConst => {
                self.parse_const_decl(vis)?;
            }
            TokenType::KwGlobal => {
                self.parse_global_decl(vis)?;
            }
            TokenType::KwEffect => {
                self.parse_effect_decl(vis)?;
            }
            TokenType::KwAbst => {
                self.parse_abstract_decl(vis)?;
            }
            TokenType::KwType => {
                self.parse_type_decl(vis)?;
            }
            TokenType::KwExternal => {
                self.parse_external_decl(vis)?;
            }
            _ => {
                self.error("unknown declaration");
                return Err(());
            }
        }
        Ok(())
    }

    /// use [@] path [only path,...] [*]
    fn parse_use_decl(&mut self) -> Result<(), ()> {
        self.start_node(SyntaxKind::Require);
        self.builder.token(raw_kind(SyntaxKind::KwUse), "use");
        self.bump();

        if self.current_kind() == TokenType::At {
            self.builder.token(raw_kind(SyntaxKind::At), "@");
            self.bump();
        }

        self.parse_path()?;

        if self.current_kind() == TokenType::KwOnly {
            self.builder.token(raw_kind(SyntaxKind::KwOnly), "only");
            self.bump();
            loop {
                self.parse_path()?;
                if !self.eat(TokenType::Comma) {
                    break;
                }
            }
        }

        if self.eat(TokenType::Star) {
            self.builder.token(raw_kind(SyntaxKind::Star), "*");
        }

        self.skip_layout();
        self.finish_node();
        Ok(())
    }

    /// `fun name [T, U] (params) -> Ret where ... \n body`
    fn parse_fun_decl(&mut self, _vis: Visibility) -> Result<(), ()> {
        self.start_node(SyntaxKind::FunDef);
        self.builder.token(raw_kind(SyntaxKind::KwFun), "fun");
        self.bump();

        // function name
        let name = self.current_text().to_string();
        self.builder.token(raw_kind(SyntaxKind::Ident), &name);
        self.bump();

        // optional generic parameters
        if self.current_kind() == TokenType::Lbracket {
            self.parse_generic_params()?;
        }

        // parameter list
        self.parse_param_list()?;

        // return type
        if self.current_kind() == TokenType::Arrow {
            self.builder.token(raw_kind(SyntaxKind::Arrow), "->");
            self.bump();
            self.parse_type()?;
        }

        // where clause
        self.parse_where_clause()?;

        // body: indent block or semicolon for declaration only
        if self.current_kind() == TokenType::Indent {
            self.builder.token(raw_kind(SyntaxKind::Indent), "");
            self.bump();
            while self.current_kind() != TokenType::Dedent {
                self.parse_expr()?;
                self.skip_layout();
            }
            self.expect(TokenType::Dedent)?;
        } else if self.current_kind() == TokenType::Semicolon {
            // function declaration only (e.g., in abstract)
            self.builder.token(raw_kind(SyntaxKind::Semicolon), ";");
            self.bump();
            self.skip_layout();
        }

        self.finish_node(); // FunDef
        Ok(())
    }

    /// `const name : type = expr`
    fn parse_const_decl(&mut self, _vis: Visibility) -> Result<(), ()> {
        self.start_node(SyntaxKind::ConstDecl);
        self.builder.token(raw_kind(SyntaxKind::KwConst), "const");
        self.bump();

        let name = self.current_text().to_string();
        self.builder.token(raw_kind(SyntaxKind::Ident), &name);
        self.bump();

        if self.current_kind() == TokenType::Colon {
            self.builder.token(raw_kind(SyntaxKind::Colon), ":");
            self.bump();
            self.parse_type()?;
        }

        self.expect(TokenType::Eq)?;
        self.parse_expr()?;
        self.skip_layout();
        self.finish_node();
        Ok(())
    }

    /// `global name : type = expr`
    fn parse_global_decl(&mut self, _vis: Visibility) -> Result<(), ()> {
        self.start_node(SyntaxKind::GlobalDecl);
        self.builder.token(raw_kind(SyntaxKind::KwGlobal), "global");
        self.bump();

        let name = self.current_text().to_string();
        self.builder.token(raw_kind(SyntaxKind::Ident), &name);
        self.bump();

        if self.current_kind() == TokenType::Colon {
            self.builder.token(raw_kind(SyntaxKind::Colon), ":");
            self.bump();
            self.parse_type()?;
        }

        self.expect(TokenType::Eq)?;
        self.parse_expr()?;
        self.skip_layout();
        self.finish_node();
        Ok(())
    }

    /// `effect Name \n | control (params) -> Ret \n ...`
    fn parse_effect_decl(&mut self, _vis: Visibility) -> Result<(), ()> {
        self.start_node(SyntaxKind::EffectDecl);
        self.builder.token(raw_kind(SyntaxKind::KwEffect), "effect");
        self.bump();

        let name = self.current_text().to_string();
        self.builder.token(raw_kind(SyntaxKind::Ident), &name);
        self.bump();
        self.skip_layout();

        while self.current_kind() == TokenType::Pipe {
            self.start_node(SyntaxKind::EffectControl);
            self.builder.token(raw_kind(SyntaxKind::Pipe), "|");
            self.bump();

            let ctrl_name = self.current_text().to_string();
            self.builder.token(raw_kind(SyntaxKind::Ident), &ctrl_name);
            self.bump();

            if self.current_kind() == TokenType::Lparen {
                self.parse_param_list()?;
            }

            if self.current_kind() == TokenType::Arrow {
                self.builder.token(raw_kind(SyntaxKind::Arrow), "->");
                self.bump();
                self.parse_type()?;
            }

            self.finish_node(); // EffectControl
            self.skip_layout();
        }

        self.finish_node(); // EffectDecl
        Ok(())
    }

    /// `abst Name [T] impl Foo+Bar where ... \n methods...`
    fn parse_abstract_decl(&mut self, _vis: Visibility) -> Result<(), ()> {
        self.start_node(SyntaxKind::AbstractDecl);
        self.builder.token(raw_kind(SyntaxKind::KwAbst), "abst");
        self.bump();

        let name = self.current_text().to_string();
        self.builder.token(raw_kind(SyntaxKind::Ident), &name);
        self.bump();

        // optional generic parameters
        if self.current_kind() == TokenType::Lbracket {
            self.parse_generic_params()?;
        }

        // optional impl list
        if self.current_kind() == TokenType::KwImpl {
            self.parse_impl_list()?;
        }

        self.skip_layout();
        self.parse_where_clause()?;

        // methods block
        if self.current_kind() == TokenType::Indent {
            self.builder.token(raw_kind(SyntaxKind::Indent), "");
            self.bump();
            while self.current_kind() == TokenType::KwFun {
                self.start_node(SyntaxKind::MethodDecl);
                self.builder.token(raw_kind(SyntaxKind::KwFun), "fun");
                self.bump();

                let mname = self.current_text().to_string();
                self.builder.token(raw_kind(SyntaxKind::Ident), &mname);
                self.bump();

                self.parse_param_list()?;
                if self.current_kind() == TokenType::Arrow {
                    self.builder.token(raw_kind(SyntaxKind::Arrow), "->");
                    self.bump();
                    self.parse_type()?;
                }
                // method declaration ends with semicolon or newline
                if self.eat(TokenType::Semicolon) {
                    // already consumed
                }
                self.skip_layout();
                self.finish_node(); // MethodDecl
            }
            self.expect(TokenType::Dedent)?;
        }

        self.finish_node(); // AbstractDecl
        Ok(())
    }

    /// `type Name ...`
    fn parse_type_decl(&mut self, _vis: Visibility) -> Result<(), ()> {
        self.start_node(SyntaxKind::TypeDecl);
        self.builder.token(raw_kind(SyntaxKind::KwType), "type");
        self.bump();

        let name = self.current_text().to_string();
        self.builder.token(raw_kind(SyntaxKind::Ident), &name);
        self.bump();

        // optional generic parameters
        if self.current_kind() == TokenType::Lbracket {
            self.parse_generic_params()?;
        }

        // optional impl list (traits for ADT or struct)
        let mut has_impls = false;
        if self.current_kind() == TokenType::KwImpl {
            self.parse_impl_list()?;
            has_impls = true;
        }

        match self.current_kind() {
            TokenType::Semicolon => {
                // forward declaration
                self.builder.token(raw_kind(SyntaxKind::Semicolon), ";");
                self.bump();
            }
            TokenType::Eq => {
                // type alias
                self.builder.token(raw_kind(SyntaxKind::Eq), "=");
                self.bump();
                self.parse_type()?;
                self.parse_where_clause()?;
                self.skip_layout();
            }
            TokenType::NewLine => {
                self.skip_layout();
                self.parse_where_clause()?;
                if self.current_kind() == TokenType::Indent {
                    self.builder.token(raw_kind(SyntaxKind::Indent), "");
                    self.bump();
                    if self.current_kind() == TokenType::Ident {
                        // struct fields
                        while self.current_kind() == TokenType::Ident {
                            self.start_node(SyntaxKind::Field);
                            let fname = self.current_text().to_string();
                            self.builder.token(raw_kind(SyntaxKind::Ident), &fname);
                            self.bump();
                            self.expect(TokenType::Colon)?;
                            self.parse_type()?;
                            self.finish_node(); // Field
                            self.skip_layout();
                        }
                    } else if self.current_kind() == TokenType::Pipe {
                        // ADT constructors
                        while self.current_kind() == TokenType::Pipe {
                            self.start_node(SyntaxKind::Ctor);
                            self.builder.token(raw_kind(SyntaxKind::Pipe), "|");
                            self.bump();
                            let cname = self.current_text().to_string();
                            self.builder.token(raw_kind(SyntaxKind::Ident), &cname);
                            self.bump();

                            // optional generic params on the constructor
                            if self.current_kind() == TokenType::Lbracket {
                                self.parse_generic_params()?;
                            }
                            // optional 'of Type' and return type
                            if self.current_kind() == TokenType::KwOf {
                                self.builder.token(raw_kind(SyntaxKind::KwOf), "of");
                                self.bump();
                                self.parse_type()?;
                                if self.current_kind() == TokenType::Arrow {
                                    self.builder.token(raw_kind(SyntaxKind::Arrow), "->");
                                    self.bump();
                                    self.parse_type()?;
                                }
                            }
                            self.finish_node(); // Ctor
                            self.skip_layout();
                        }
                    } else {
                        self.error("expected struct fields or ADT constructors");
                        return Err(());
                    }
                    self.expect(TokenType::Dedent)?;
                }
            }
            _ => {
                self.error("unexpected token after type name");
                return Err(());
            }
        }

        self.finish_node(); // TypeDecl
        Ok(())
    }

    /// `external fun name (params) -> Ret = "sym" ;` or `external ctype name ;`
    fn parse_external_decl(&mut self, _vis: Visibility) -> Result<(), ()> {
        self.start_node(SyntaxKind::ExternalDecl);
        self.builder.token(raw_kind(SyntaxKind::KwExternal), "external");
        self.bump();

        if self.current_kind() == TokenType::KwCType {
            // external ctype
            self.builder.token(raw_kind(SyntaxKind::KwCType), "ctype");
            self.bump();
            let name = self.current_text().to_string();
            self.builder.token(raw_kind(SyntaxKind::Ident), &name);
            self.bump();
            self.expect(TokenType::Semicolon)?;
            self.skip_layout();
        } else {
            // external fun
            self.builder.token(raw_kind(SyntaxKind::KwFun), "fun");
            self.bump();
            let name = self.current_text().to_string();
            self.builder.token(raw_kind(SyntaxKind::Ident), &name);
            self.bump();

            self.parse_param_list()?;

            if self.current_kind() == TokenType::Arrow {
                self.builder.token(raw_kind(SyntaxKind::Arrow), "->");
                self.bump();
                self.parse_type()?;
            }

            // optional symbol name
            if self.current_kind() == TokenType::Eq {
                self.builder.token(raw_kind(SyntaxKind::Eq), "=");
                self.bump();
                self.expect(TokenType::String)?;
            }

            self.expect(TokenType::Semicolon)?;
            self.skip_layout();
        }

        self.finish_node(); // ExternalDecl
        Ok(())
    }
    pub fn parse_file(&mut self) -> Result<FileRedUnit, ()> {
        self.start_node(SyntaxKind::SourceFile);
        while self.peek().is_some() && self.current_kind() != TokenType::Eof {
            self.skip_layout();
            if self.current_kind() == TokenType::Eof {
                break;
            }
            self.parse_decl()?;
        }
        self.finish_node();
        let green = self.builder.finish();
        Ok(FileRedUnit {
            span: Span {
                source_id: self.source_id,
                start_off: 0,
                end_off: 0,
            },
            green: Arc::new(green),
        })
    }
}

pub fn parse_crate(
    tokens: &[Token],
    diag: &mut DiagCtx,
    user_operators: &HashMap<String, OperatorDef>,
    user_op_info: &HashMap<String, (usize, OperatorKind)>,
    source_id: FileId,
) -> Result<CrateAst, ()> {
    let builder = GreenNodeBuilder::new();
    let mut parser = Parser::new(tokens, builder, diag, user_operators, user_op_info, source_id);
    let file_unit = parser.parse_file()?;

    let syntax = file_unit.syntax();
    let mut external_requires = Vec::new();
    let mut file_units = vec![file_unit];

    for child in syntax.children() {
        if let Some(req) = RequireRedNode::cast(child.clone()) {
            let has_at = req
                .syntax()
                .children_with_tokens()
                .any(|e| e.kind() == SyntaxKind::At);
            if has_at {
                external_requires.push(req);
            }
        }
    }

    Ok(CrateAst {
        external_requires,
        file_units,
    })
}