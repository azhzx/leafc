use crate::ast::{DeclNode, DeclNodeId, Field};
use crate::source::Span;

pub type ScopeId = usize;

pub type TopScopePool = Vec<ScopeId>;
pub type ScopePool = Vec<Scope>;


#[derive(Debug)]
pub enum Scope {
    Scope {
        parent: Option<ScopeId>,
        symbols: Vec<LocalSymbol>,
        children: Vec<ScopeId>,
        bind_to_ast: DeclNodeId
    },
    Struct {
        name: String,
        fields: Vec<FieldSymbol>,
        bind_to_ast: DeclNodeId
    },
    ADT {
        name: String,
        ctors: Vec<CtorSymbol>,
        bind_to_ast: DeclNodeId
    },
    TypeAlias {
        name: String,
        bind_to_ast: DeclNodeId
    },
    CTypeDef {
        name: String,
        bind_to_ast: DeclNodeId
    },
    External {
        name: String,
        bind_to_ast: DeclNodeId
    },
    FunDecl {
        name: String,
        bind_to_ast: DeclNodeId
    },
    Abstract {
        name: String,
        methods: Vec<MethodSymbol>,
        bind_to_ast: DeclNodeId
    }
}

#[derive(Debug)]
pub struct LocalSymbol {
    pub name: String,
    pub def_span: Span,
}

#[derive(Debug)]
pub struct FieldSymbol {
    pub name: String,
    pub def_span: Span,
}

#[derive(Debug)]
pub struct CtorSymbol {
    pub name: String,
    pub def_span: Span,
}

#[derive(Debug)]
pub struct MethodSymbol {
    pub name: String,
    pub def_span: Span,
}