mod parse_expr;
mod parse_use_decl;
mod parse_fun_decl;
mod parse_external_decl;
mod parse_type_decl;
mod parse_abstract_decl;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use leafc_coreapi;
use leafc_coreapi::ast::{AtomExprNode, DeclNode, ExprNode, CrateAst, GenericVar, Param, TypeNameString, Visibility, DeclNodeKind, AnnotationDecl, DeclRedNode, FileRedUnit, Operator};
use leafc_coreapi::crate_meta::{BuiltinOperator, OperatorDef, PriorityRelation, OperatorKind};
use leafc_coreapi::diagnostic::DiagMsg;
use leafc_coreapi::lexer::{LexerApi, Token, TokenStream, TokenType};
use leafc_coreapi::lexer::TokenType::Lparen;
use leafc_coreapi::parser::{ParserApi, ParserError};
use leafc_coreapi::scope::ScopePool;
use leafc_coreapi::source::{AbsPathSourceMap, SourceId, SourcePool};
use leafc_coreapi::tokens_pass::TokenPassApi;
use leafc_lexer::Lexer;
use leafc_tokenpass::Preprocessor;

pub struct Parser<'a> {
    dir_abs_path: PathBuf,
    tokens: TokenStream,
    index: usize,
    source_pool: &'a SourcePool,
    abs_path_sources: &'a AbsPathSourceMap,
    ast: CrateAst,
    user_operators: &'a HashMap<String, OperatorDef>,

    /// op_text => (precedence, kind)
    user_op_info: HashMap<String, (usize, OperatorKind)>,
}


impl<'a> Parser<'a> {
    const PRIORITY_OFFSET: usize = 5;

    fn builtin_priority(op: BuiltinOperator) -> usize {
        match op {
            BuiltinOperator::Or => 10,
            BuiltinOperator::And => 20,
            BuiltinOperator::Eq | BuiltinOperator::Neq
            | BuiltinOperator::Lt | BuiltinOperator::Gt
            | BuiltinOperator::Le | BuiltinOperator::Ge => 30,
            BuiltinOperator::Add | BuiltinOperator::Sub => 40,
            BuiltinOperator::Mul | BuiltinOperator::Div | BuiltinOperator::Mod => 50,
            BuiltinOperator::Not => 70,  // 前缀 not 的优先级，用户若引用则以此为基准
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
                    })
                }
            }
            self.skip_token(); // ']'
        }
        Ok(TypeNameString {
            name: start_token.text.clone(),
            generics,
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
                    })
                }

                generics[current_generic_index].constraint = constraint;
                self.skip_token();
            } else {
                return Err(DiagMsg{
                    title: format!("{:?}", ParserError::InvalidWhereBody),
                    msg: "invalid where body".to_string(),
                    span: self.current_token().span.clone(),
                })
            }
            current_generic_index += 1;
        }
        Ok(generics)
    }

    fn lexer(
        source_id: SourceId,
        code: &String,
        user_operators: &'a HashMap<String, OperatorDef>
    ) -> Result<TokenStream, DiagMsg> {
        let mut lex = Lexer::new(source_id, &code, user_operators);
        let tokens = lex.tokenize()?;
        for token in &tokens.data {
            println!("{:?}", token);
        }
        Ok(tokens)
    }

    fn pp(source_id: SourceId, token_stream: &TokenStream) -> Result<TokenStream, DiagMsg> {
        // 预处理
        let mut pp = Preprocessor::new(&token_stream, source_id);
        let new_tokens = pp.pass()?;

        println!("\n\n== token pass ==\n\n");

        for token in &new_tokens.data {
            println!("{:?}", token);
        }
        println!("== === ==");
        Ok(new_tokens)
    }

    fn parse_top(&mut self, module_name: String) -> Result<FileRedUnit, DiagMsg> {
        let mut top_decls = vec![];
        let mut file_unit_requires = vec![];

        while self.current_token().kind != TokenType::Eof {
            // handle ann
            let mut ann = vec![];
            while self.current_token().kind == TokenType::Hash {
                self.skip_token();

                let ann_name = self.current_token().text.clone();
                self.skip_token_only(TokenType::Ident)?;

                let mut ann_args = vec![];

                if self.current_token().kind == TokenType::Lparen {
                    self.skip_token(); // '('
                    let call_span = self.current_token().span.clone();
                    while self.current_token().kind != TokenType::Rparen {
                        ann_args.push(self.current_token().text.clone());
                        self.skip_token();
                        if self.current_token().kind == TokenType::Comma {
                            self.skip_token();
                        } else if self.current_token().kind == TokenType::Rparen {
                            break;
                        } else {
                            return Err(DiagMsg {
                                title: format!("{:?}", ParserError::InvalidCallArgumentList),
                                msg: "invalid call argument list".to_string(),
                                span: call_span,
                            });
                        }
                    }
                    self.skip_token_only(TokenType::Rparen)?;
                }

                self.skip_token_only(TokenType::NewLine)?;

                ann.push(AnnotationDecl {
                    name: ann_name,
                    args: ann_args,
                });
            }


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
                TokenType::KwUse => {
                    if let Some(req) = self.parse_use_decl()? {
                        file_unit_requires.push(req);
                    }
                },
                TokenType::KwFun => {
                    let decl_node = self.parse_fun_decl(visibility, ann)?;
                    top_decls.push(decl_node);
                },
                TokenType::KwExternal => {
                    let decl_node =self.parse_external_decl(visibility, ann)?;
                    top_decls.push(decl_node);
                },
                TokenType::KwType => {
                    let decl_node =self.parse_type_decl(visibility, ann)?;
                    top_decls.push(decl_node);
                },
                TokenType::KwAbst => {
                    let decl_node =self.parse_abstract_decl(visibility, ann)?;
                    top_decls.push(decl_node);
                },
                TokenType::NewLine => self.skip_token(),
                _ => {
                    return Err(DiagMsg{
                        title: format!("{:?}", ParserError::InvalidTopDeclaration),
                        msg: "invalid top declare".to_string(),
                        span: self.current_token().span.clone(),
                    })
                }
            }
        }

        Ok(FileRedUnit {
            span: self.current_token().span.clone(),
            name: module_name,
            top_decls,
            file_unit_requires,
        })
    }

}

impl<'a> ParserApi<'a> for Parser<'a> {
    fn new(
        dir_abs_path: PathBuf,
        source_pool: &'a SourcePool,
        abs_path_source_map: &'a AbsPathSourceMap,
        user_operators: &'a HashMap<String, OperatorDef>,
    ) -> Self {
        let mut user_op_info = HashMap::new();
        for (_op_name, def) in user_operators {
            let base_prio = match def.priority_relation() {
                PriorityRelation::HigherThan(op) => Self::builtin_priority(op) + Self::PRIORITY_OFFSET,
                PriorityRelation::LowerThan(op) => Self::builtin_priority(op) - Self::PRIORITY_OFFSET,
            };
            user_op_info.insert(def.text.clone(), (base_prio, def.kind));
        }

        Parser {
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


    /// main dispatcher
    fn parse(mut self) -> Result<CrateAst, DiagMsg> {
        let main_file_path = self.dir_abs_path.join("main.leaf");

        let main_file_source_id = self.abs_path_sources.get(
            &main_file_path.to_str().unwrap().to_string()).unwrap();

        let content = &self.source_pool.0[*main_file_source_id];

        let token = Self::lexer(*main_file_source_id, &content.file_content, &self.user_operators)?;
        self.tokens = Self::pp(*main_file_source_id, &token)?;
        self.index = 0;

        let main_module = self.parse_top("main".to_string())?;

        self.ast.file_units.push(main_module);

        Ok(self.ast)
    }
}