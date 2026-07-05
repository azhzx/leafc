use std::any::TypeId;
use crate::source::Span;
use crate::symbol_name::SymName;

pub type TyId = usize;
const INVALID_TYPE_ID: TyId = !0;

pub enum TypeKind {
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Ptr { ref_to: TypeId },
    Ref { ref_to: TypeId },
    MutRef { ref_to: TypeId },
    Alias {
        ref_to: TypeId,
        def_span: Span,
    },
    Struct {
        fields: Vec<Symbol>,
        def_span: Span,
    },
    Union {
        unions: Vec<TyId>,
        def_span: Span,
    },
    Tuple {
        members: Vec<TyId>,
    }
}

pub struct TypeSymbol {
    kind: TypeKind,
    name: SymName
}

pub type TypePool = Vec<TypeSymbol>;

pub enum Symbol {
    LocalSymbol {
        name: SymName,
        ty: TyId,
        def_span: Span,
    },
    FieldSymbol {
        name: SymName,
        ty: TyId,
        def_span: Span,
    },
    CtorSymbol {
        of: TypeId,
        return_ty: TyId,
        def_span: Span,
    }
}

pub struct SymbolTable {
    symbols: Vec<Symbol>,
}