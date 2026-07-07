mod parse_expr;
mod parse_use_decl;
mod parse_fun_decl;
mod parse_external_decl;
mod parse_type_decl;
mod parse_abstract_decl;

use leafc_coreapi;
use leafc_coreapi::ast::{AtomExprNode, DeclNode, DeclNodeId, ExprNode, FileAst, GenericVar, Param, TypeNameString, Visibility};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{Token, TokenStream, TokenType};
use leafc_coreapi::lexer::TokenType::Lparen;
use leafc_coreapi::parser::{ParserApi, ParserError, ParserResult, Require};
use leafc_coreapi::source::SourceId;

pub struct Parser<'a> {
    tokens: &'a TokenStream,
    index: usize,
    source: SourceId,
    ast: FileAst,
    requires: Vec<Require>
}

impl<'a> Parser<'a> {
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
    fn skip_token_only(&mut self, expected: TokenType) -> Result<(), DiagMsg> {
        let tok = self.current_token();
        if tok.kind == expected {
            self.skip_token();
            return Ok(());
        }
        
        Err(DiagMsg {
            title: format!("{:?}", ParserError::TokenExpect),
            msg: format!("expected <token \"{:?}\"> but got <token \"{:?}\">", expected, tok.kind),
            span: tok.span.clone(),
            source: self.source,
        })
    }

    fn skip_token_if_newlines(&mut self) -> Result<(), DiagMsg> {
        while self.current_token().kind == TokenType::NewLine {
            self.skip_token();
        }
        Ok(())
    }

    fn unknown_type_name(&self) -> TypeNameString {
        TypeNameString {
            name: "".to_string(), generics: vec![], span: self.current_token().span.clone()
        }
    }

    fn handle_type_name_string(&mut self) -> Result<TypeNameString, DiagMsg> {
        let start_token = self.current_token().clone();
        self.skip_token_only(TokenType::Ident)?;
        let mut generics = vec![];

        if self.current_token().kind == TokenType::Lbracket {
            self.skip_token();

            while self.current_token().kind != TokenType::Rbracket {
                let typename = self.handle_type_name_string()?;
                if self.current_token().kind == TokenType::Comma {
                    self.skip_token();
                    generics.push(typename);
                } else if self.current_token().kind == TokenType::Rbracket {
                    generics.push(typename);
                    break;
                } else {
                    return Err(DiagMsg{
                        title: format!("{:?}", ParserError::InvalidGenericList),
                        msg: "invalid generic list".to_string(),
                        span: self.current_token().span.clone(),
                        source: self.source
                    })
                }
            }
            self.skip_token(); // ']'
        }
        Ok(TypeNameString {
            name: start_token.text.clone(),
            generics: generics,
            span: start_token.span.clone(),
        })
    }
    fn handle_generic_param(&mut self) -> Result<Vec<GenericVar>, DiagMsg> {
        let mut generics = vec![];
        self.skip_token_only(TokenType::Lbracket)?;
        while self.current_token().kind != TokenType::Rbracket {
            let param_name = self.current_token().text.clone();
            self.skip_token_only(TokenType::Ident)?;

            if self.current_token().kind == TokenType::Comma {
                self.skip_token();
                generics.push(GenericVar {
                    name: param_name, constraint: vec![],
                });
            } else if self.current_token().kind == TokenType::Rbracket {
                generics.push(GenericVar {
                    name: param_name, constraint: vec![],
                });
                break;

            } else {
                return Err(DiagMsg{
                    title: format!("{:?}", ParserError::InvalidGenericParameterList),
                    msg: "invalid generic parameter list".to_string(),
                    span: self.current_token().span.clone(),
                    source: self.source
                })
            }
        }
        self.skip_token_only(TokenType::Rbracket)?;
        Ok(generics)
    }

    fn handle_where(&mut self, mut generics: Vec<GenericVar>) -> Result<Vec<GenericVar>, DiagMsg> {
        self.skip_token_only(TokenType::KwWhere)?;
        let mut current_generic_index = 0;

        while self.current_token().kind == TokenType::Ident {
            let mut constraint = vec![];

            self.skip_token();
            self.skip_token_only(TokenType::Colon)?;
            while self.current_token().kind != TokenType::NewLine {

                let type_str = self.handle_type_name_string()?;
                constraint.push(type_str);

                if self.current_token().kind == TokenType::Plus {
                    self.skip_token();
                } else {
                    break;
                }
            }

            if self.current_token().kind == TokenType::Comma {
                if current_generic_index >= generics.len() {
                    return Err(DiagMsg{
                        title: format!("{:?}", ParserError::WhereBodyGenericMissingMatchGenericParameterList),
                        msg: "where body generic missing generic parameter list".to_string(),
                        span: self.current_token().span.clone(),
                        source: self.source
                    })
                }

                generics[current_generic_index].constraint = constraint;
                self.skip_token();
            } else if self.current_token().kind == TokenType::NewLine {
                if current_generic_index >= generics.len() {
                    return Err(DiagMsg{
                        title: format!("{:?}", ParserError::WhereBodyGenericMissingMatchGenericParameterList),
                        msg: "where body generic missing generic parameter list".to_string(),
                        span: self.current_token().span.clone(),
                        source: self.source
                    })
                }

                generics[current_generic_index].constraint = constraint;
                self.skip_token();
            } else {
                return Err(DiagMsg{
                    title: format!("{:?}", ParserError::InvalidWhereBody),
                    msg: "invalid where body".to_string(),
                    span: self.current_token().span.clone(),
                    source: self.source
                })
            }
            current_generic_index += 1;
        }
        Ok(generics)
    }
}

impl<'a> ParserApi<'a> for Parser<'a> {
    fn new(source: SourceId, tokens: &'a TokenStream) -> Self {
        Parser {
            tokens,
            index: 0,
            source,
            ast: FileAst {
                file: source,
                atom_expr_pool: vec![],
                expr_pool: vec![],
                decl_pool: vec![],
                type_name_pool: vec![],
            },
            requires: vec![],
        }
    }


    /// main dispatcher
    fn parse(&mut self) -> Result<ParserResult, DiagMsg> {
        while self.current_token().kind != TokenType::Eof {
            let mut visibility = Visibility::Private;
            if self.current_token().kind == TokenType::KwPub {
                self.skip_token();
                if self.current_token().kind == Lparen {
                    self.skip_token();
                    self.skip_token_only(TokenType::KwExternal)?;
                    self.skip_token_only(TokenType::Rparen)?;
                    visibility = Visibility::PublicExternal;
                } else {
                    visibility = Visibility::Public;
                }
            }

            match self.current_token().kind {
                TokenType::KwUse => self.parse_use_decl()?,
                TokenType::KwFun => self.parse_fun_decl(visibility)?,
                TokenType::KwExternal => self.parse_external_decl(visibility)?,
                TokenType::KwType => self.parse_type_decl(visibility)?,
                TokenType::KwAbst => self.parse_abstract_decl(visibility)?,
                TokenType::NewLine => self.skip_token(),
                _ => {
                    return Err(DiagMsg{
                        title: format!("{:?}", ParserError::InvalidTopDeclaration),
                        msg: "invalid top declare".to_string(),
                        span: self.current_token().span.clone(),
                        source: self.source
                    })
                }
            }
        }
        Ok(ParserResult {
            ast: self.ast.clone(),
            requires: self.requires.clone(),
        })
    }
}