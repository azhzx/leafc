use std::collections::HashMap;
use std::sync::Arc;
use crate::ast::DeclNode;
use crate::source::{SourceId, Span};

pub type ScopeId = usize;
pub type DeclNodeScopeMap = HashMap<Arc<DeclNode>, ScopeId>;


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeKind {
    File,
    Function,
    Adt,
    Abstract,
    Block,
    Struct,
    Crate,
}

pub type SymId = usize;


/// A single symbol definition stored inside a scope.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub def_span: Span,
    pub kind: SymbolKind,
    pub sym_id: SymId,
}

#[derive(Debug, Clone)]
pub enum SymbolKind {
    Local,

    /// Top-level function.
    Function,

    Struct {
        fields: Vec<SymId>
    },

    ADT {
        constructors: Vec<SymId>,
    },

    /// A type alias
    TypeAlias,

    CTypeDef,

    External,

    Abstract,

    Generic,

    /// an ADT Constructor
    Constructor,

    /// A field of a struct
    Field,

    /// A method signature inside an abstract type
    Method,

    File {
        source_id: SourceId
    }
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub parent: Option<ScopeId>,
    pub children: Vec<ScopeId>,
    pub kind: ScopeKind,
    pub symbols: Vec<SymId>,
    pub def_span: Option<Span>,
}

#[derive(Debug, Clone)]
pub struct ScopePool {
    scopes: Vec<Scope>,
    top_scopes: Vec<ScopeId>,
    sym_counter: usize,
    symbols: Vec<Symbol>,
    pub decl_node_scope_map: DeclNodeScopeMap,
}

impl ScopePool {
    pub fn new() -> Self {
        Self {
            scopes: Vec::new(),
            top_scopes: Vec::new(),
            sym_counter: 0,
            symbols: vec![],
            decl_node_scope_map: HashMap::new(),
        }
    }

    pub fn push_scope(
        &mut self,
        parent: Option<ScopeId>,
        kind: ScopeKind,
        bind_to_ast: Option<Arc<DeclNode>>,
        def_span: Option<Span>,
    ) -> ScopeId {
        let id = self.scopes.len();

        if bind_to_ast.is_some() {
            self.decl_node_scope_map.insert(bind_to_ast.unwrap(), id);
        }

        let scope = Scope {
            parent,
            children: Vec::new(),
            kind,
            symbols: Vec::new(),
            def_span,
        };
        self.scopes.push(scope);

        if let Some(p) = parent {
            self.scopes[p].children.push(id);
        } else {
            self.top_scopes.push(id);
        }

        id
    }
    pub fn add_symbol(
        &mut self,
        scope: ScopeId,
        name: String,
        def_span: Span,
        kind: SymbolKind,
    ) {
        let sym = Symbol {
            name,
            def_span,
            kind,
            sym_id: self.sym_counter,
        };
        self.symbols.push(sym);
        self.scopes[scope].symbols.push(self.sym_counter);
        self.sym_counter += 1;
    }

    pub fn add_symbol_and_get_sym_id(
        &mut self,
        scope: ScopeId,
        name: String,
        def_span: Span,
        kind: SymbolKind,
    ) -> SymId {
        let sym_id = self.sym_counter;
        let sym = Symbol {
            name,
            def_span,
            kind,
            sym_id,
        };
        self.symbols.push(sym);
        self.scopes[scope].symbols.push(self.sym_counter);
        self.sym_counter += 1;

        sym_id
    }

    pub fn lookup(&self, scope: ScopeId, name: &str) -> Option<(&Symbol, ScopeId)> {
        let mut current = Some(scope);
        while let Some(sid) = current {
            let s = &self.scopes[sid];
            for sym_id in &s.symbols {
                let sym = &self.symbols[*sym_id];
                if sym.name == name {
                    return Some((sym, sid));
                }
            }
            current = s.parent;
        }
        None
    }


    pub fn get_scope(&self, id: ScopeId) -> &Scope {
        &self.scopes[id]
    }

    pub fn get_scope_mut(&mut self, id: ScopeId) -> &mut Scope {
        &mut self.scopes[id]
    }

    pub fn top_scopes(&self) -> &[ScopeId] {
        &self.top_scopes
    }

    pub fn get_symbol_by_id(&self, id: SymId) -> Option<&Symbol> {
        self.symbols.get(id)
    }
}