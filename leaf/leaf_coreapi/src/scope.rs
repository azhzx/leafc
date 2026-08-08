use crate::ast::{GreenCatchClause, GreenCtor, GreenDecl, GreenExpr, GreenMatchArm};
use crate::id::{FileId, ScopeId, SymId};
use crate::lang_items::LangItems;
use crate::source::Span;
use std::collections::HashMap;
use std::sync::Arc;

pub type DeclNodeScopeMap = HashMap<Arc<GreenDecl>, ScopeId>;
pub type DoScopeMap = HashMap<Arc<GreenExpr>, ScopeId>;
pub type FunScopeMap = HashMap<Arc<GreenDecl>, ScopeId>;
pub type CatchScopeMap = HashMap<Arc<GreenCatchClause>, ScopeId>;
pub type ArmScopeMap = HashMap<Arc<GreenMatchArm>, ScopeId>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeKind {
    File,
    Function,
    Adt,
    Abstract,
    Block,
    Struct,
    Crate,
    Effect,
    TypeAlias,
    Constructor,
}

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
        fields: Vec<SymId>,
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

    Control,

    Effect {
        scope_id: ScopeId,
    },

    Const,

    Global,

    TypeDecl,

    /// an ADT Constructor
    Constructor,

    /// A field of a struct
    Field,

    /// A method signature inside an abstract type
    Method,

    File {
        source_id: FileId,
    },
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
    pub scopes: Vec<Scope>,
    pub top_scopes: Vec<ScopeId>,
    pub sym_counter: usize,
    pub symbols: Vec<Symbol>,
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
        bind_to_ast: Option<Arc<GreenDecl>>,
        def_span: Option<Span>,
    ) -> ScopeId {
        let id = ScopeId(self.scopes.len());

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
            self.scopes[p.0].children.push(id);
        } else {
            self.top_scopes.push(id);
        }

        id
    }
    pub fn add_symbol(&mut self, scope: ScopeId, name: String, def_span: Span, kind: SymbolKind) {
        let sym = Symbol {
            name,
            def_span,
            kind,
            sym_id: SymId(self.sym_counter),
        };
        self.symbols.push(sym);
        self.scopes[scope.0].symbols.push(SymId(self.sym_counter));
        self.sym_counter += 1;
    }

    pub fn add_symbol_and_get_sym_id(
        &mut self,
        scope: ScopeId,
        name: String,
        def_span: Span,
        kind: SymbolKind,
    ) -> SymId {
        let sym_id = SymId(self.sym_counter);
        let sym = Symbol {
            name,
            def_span,
            kind,
            sym_id: sym_id,
        };
        self.symbols.push(sym);
        self.scopes[scope.0].symbols.push(SymId(self.sym_counter));
        self.sym_counter += 1;

        sym_id
    }

    pub fn lookup(&self, scope: ScopeId, name: &str) -> Option<(&Symbol, ScopeId)> {
        let mut current = Some(scope);
        while let Some(sid) = current {
            let s = &self.scopes[sid.0];
            for sym_id in &s.symbols {
                let sym = &self.symbols[sym_id.0];
                if sym.name == name {
                    return Some((sym, sid));
                }
            }
            current = s.parent;
        }
        None
    }

    pub fn get_scope(&self, id: ScopeId) -> &Scope {
        &self.scopes[id.0]
    }

    pub fn get_scope_mut(&mut self, id: ScopeId) -> &mut Scope {
        &mut self.scopes[id.0]
    }

    pub fn top_scopes(&self) -> &[ScopeId] {
        &self.top_scopes
    }

    pub fn get_symbol_by_id(&self, id: SymId) -> Option<&Symbol> {
        self.symbols.get(id.0)
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedNames {
    pub pool: ScopePool,
    pub do_scope_map: DoScopeMap,
    pub fun_scope_map: FunScopeMap,
    pub arm_scope_map: ArmScopeMap,
    pub catch_scope_map: CatchScopeMap,
    pub source_id_to_scope: HashMap<FileId, ScopeId>,
    pub ctor_scope_map: HashMap<Arc<GreenCtor>, ScopeId>,
    pub lang_items: LangItems,
}
