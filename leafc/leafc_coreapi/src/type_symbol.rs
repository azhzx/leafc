use std::any::TypeId;
use crate::scope::FieldSymbol;
use crate::source::Span;

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
        fields: Vec<FieldSymbol>,
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
}

pub type TypePool = Vec<TypeSymbol>;