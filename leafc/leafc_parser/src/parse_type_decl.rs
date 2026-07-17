use leafc_coreapi::ast::{AnnotationDecl, Ctor, DeclNode, DeclNodeKind, Field, Param, TypeNameString, Visibility};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Token, TokenType};
use leafc_coreapi::parser::{ParserError};
use crate::Parser;

impl<'a> Parser<'a> {
    pub fn parse_type_decl(&mut self, visibility: Visibility, ann: Vec<AnnotationDecl>) -> Result<DeclNode, DiagMsg> {
        self.skip_token();
        let name_token = self.current_token();
        let name = name_token.text.clone();
        let name_span = name_token.span.clone();
        self.skip_token_only(TokenType::Ident)?;

        if self.current_token().kind == TokenType::Semicolon {
            self.skip_token();
            return Ok(DeclNode {
                name,
                visibility,
                span: name_span,
                kind: DeclNodeKind::TypeDecl,
                annotations: ann,
            })
        }

        let mut generic = if self.current_token().kind == TokenType::Lbracket {
            self.handle_generic_param()?
        } else { vec![] };

        let mut impls = vec![];

        if self.current_token().kind == TokenType::KwImpl {
            self.skip_token();
            while self.current_token().kind == TokenType::Ident {
                let name = self.current_token().text.clone();
                self.skip_token();

                if self.current_token().kind == TokenType::Plus {
                    self.skip_token();
                    impls.push(name);
                } else {
                    impls.push(name);
                    break;
                }
            }
        }

        match self.current_token().kind {
            TokenType::Eq => {
                self.skip_token();
                let ref_to = self.handle_type_name_string()?;


                self.skip_token_only(TokenType::NewLine)?;

                if self.current_token().kind == TokenType::KwWhere {
                    generic = self.handle_where(generic)?;
                }

                Ok(DeclNode {
                    name,
                    visibility,
                    span: name_span,
                    kind: DeclNodeKind::TypeAlias {
                        ref_to,
                        has_abst: impls,
                        generic_vars: generic,
                    },
                    annotations: ann,
                })
            }
            TokenType::NewLine => {
                self.skip_token();

                if self.current_token().kind == TokenType::KwWhere {
                    generic = self.handle_where(generic)?;
                }

                self.skip_token_only(TokenType::Indent)?;

                if self.current_token().kind == TokenType::Ident {
                    let mut fields = vec![];
                    while self.current_token().kind != TokenType::Dedent {
                        let field_token = self.current_token();
                        let field_span = field_token.span.clone();
                        let field_name = field_token.text.clone();
                        self.skip_token();

                        self.skip_token_only(TokenType::Colon)?;

                        let type_str = self.handle_type_name_string()?;
                        fields.push(Field {
                            name: field_name, type_str, span: field_span,
                        });
                        self.skip_token_only(TokenType::NewLine)?;
                        self.skip_token_if_newlines()?;
                    }

                    self.skip_token_if_newlines()?;
                    self.skip_token_only(TokenType::Dedent)?;
                    Ok(DeclNode {
                        name,
                        visibility,
                        span: name_span,

                        kind: DeclNodeKind::TypeStruct {
                            fields,
                            has_abst: impls,
                            generic_vars: generic,
                        },
                        annotations: ann,
                    })
                } else if self.current_token().kind == TokenType::Pipe {

                    let mut ctors = vec![];
                    while self.current_token().kind != TokenType::Dedent {
                        self.skip_token_only(TokenType::Pipe)?;
                        let ctor_token = self.current_token();
                        let ctor_span = ctor_token.span.clone();
                        let ctor_name = ctor_token.text.clone();
                        self.skip_token_only(TokenType::Ident)?;

                        let mut ctor_generic = if self.current_token().kind == TokenType::Lbracket {
                            self.handle_generic_param()?
                        } else { vec![] };

                        let mut ctor_from_type = self.unknown_type_name();
                        let mut ctor_return_type = self.unknown_type_name();
                        if self.current_token().kind == TokenType::KwOf {
                            self.skip_token();
                            ctor_from_type = self.handle_type_name_string()?;
                            if self.current_token().kind == TokenType::Arrow {
                                self.skip_token();
                                ctor_return_type = self.handle_type_name_string()?;
                            }
                        }


                        ctors.push(Ctor {
                            name: ctor_name,
                            from_type_str: ctor_from_type,
                            generic_vars: ctor_generic,
                            return_type_str: ctor_return_type,
                            visibility: visibility.clone(),
                            span: ctor_span,
                        });

                        self.skip_token_only(TokenType::NewLine)?;
                        self.skip_token_if_newlines()?;

                    }
                    self.skip_token_if_newlines()?;
                    self.skip_token_only(TokenType::Dedent)?;
                    return Ok(DeclNode {
                        name,
                        visibility,
                        span: name_span,
                        kind: DeclNodeKind::ADT {
                            ctors,
                            has_abst: impls,
                            generic_vars: generic,
                        },
                        annotations: ann,
                    })
                } else {
                    return Err(DiagMsg{
                        title: format!("{:?}", ParserError::InvalidTypeDeclaration),
                        msg: "invalid type declaration".to_string(),
                        span: self.current_token().span.clone(),
                    })
                }
            }
            _ => {
                Err(DiagMsg{
                    title: format!("{:?}", ParserError::InvalidTypeDeclaration),
                    msg: "invalid type declaration".to_string(),
                    span: self.current_token().span.clone(),
                })
            }
        }
    }
}