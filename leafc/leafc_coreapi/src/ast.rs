use std::sync::Arc;
use crate::operators::Operator;
use crate::source::{Span};

// ===----------------------------
// Text Len
// ===----------------------------

pub type TextLen = usize;

// ===----------------------------
// Crate
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CrateAst {
    pub external_requires: Vec<RequireRedNode>,
    pub file_units: Vec<FileRedUnit>,
}

// ===----------------------------
// File Unit
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenFileUnit {
    pub name: GreenChild<IdentName>,
    pub top_decls: Vec<GreenChild<GreenDecl>>,
    pub file_unit_requires: Vec<GreenChild<GreenRequire>>,
    pub text_len: TextLen,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FileRedUnit {
    pub span: Span,
    pub green: Arc<GreenFileUnit>,
}

// ===----------------------------
// Require
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenRequire {
    pub path: Vec<GreenChild<IdentName>>,
    pub only: Vec<GreenChild<IdentName>>,
    pub is_open: bool, // 将被导入模块的顶层声明塞入当前模块的中(不递归展开)
    pub text_len: TextLen,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct RequireRedNode {
    pub span: Span,
    pub green: Arc<GreenRequire>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Visibility {
    Private,
    Public,
    PublicExternal,
}


/// warp a green node
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenChild<T> {
    pub relative_start: usize,
    pub node: Arc<T>,
}

// ===----------------------------
// Name
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TypeName {
    Named {
        path: GreenChild<GreenPureStaticPath>,
        generics: Vec<TypeName>,
        text_len: TextLen,
    },
    Ref {
        inner: GreenChild<TypeName>,
        text_len: TextLen,
    },
    MutRef {
        inner: GreenChild<TypeName>,
        text_len: TextLen,
    },
    Share {
        inner: GreenChild<TypeName>,
        text_len: TextLen,
    },
    Tuple {
        elements: Vec<GreenTupleElement>,
        text_len: TextLen,
    },
    Fun {
        params: Vec<GreenChild<TypeName>>,
        return_type: GreenChild<TypeName>,
        text_len: TextLen,
    },
    Impl {
        trait_type: GreenChild<TypeName>,
        text_len: TextLen,
    },
}

/// 元组
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenTupleElement {
    pub ty: GreenChild<TypeName>,
    pub repeat: Option<usize>,
    pub text_len: TextLen,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TypeNameRedNode {
    pub span: Span,
    pub green: Arc<TypeName>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct IdentName {
    pub name: String,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct IdentNameRedNode {
    pub span: Span,
    pub ident: IdentName,
}


// ===----------------------------
// Operator
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct OperatorRedNode {
    pub span: Span,
    pub op: Operator,
}

// ===----------------------------
// Static Path
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenPureStaticPath {
    pub segments: Vec<GreenChild<IdentName>>,
    pub text_len: TextLen,
}


// ===----------------------------
// Where
// ===----------------------------
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenWhereClause {
    pub constraints: Vec<GreenChild<GreenWhereConstraint>>,
    pub text_len: TextLen,
}



#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenWhereConstraint {
    pub name: GreenChild<IdentName>,
    pub bounds: Vec<GreenChild<TypeName>>,
    pub text_len: TextLen,
}



// ===----------------------------
// Init Struct Fields
// ===----------------------------
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenStructFieldInit {
    pub name: GreenChild<IdentName>,
    pub value: GreenChild<GreenExpr>,
    pub text_len: TextLen,
}


// ===----------------------------
// Pattern
// ===----------------------------
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum GreenPattern {
    Wildcard,
    Literal(AtomExprNode),
    Binding(IdentName),
    Constructor {
        type_name: GreenChild<TypeName>,
        args: Vec<GreenChild<GreenPattern>>,
        text_len: TextLen,
    },
}

// ===----------------------------
// Match
// ===----------------------------
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenMatchArm {
    pub pattern: GreenChild<GreenPattern>,
    pub guard: Option<GreenChild<GreenExpr>>,
    pub body: GreenChild<GreenExpr>,
    pub text_len: TextLen,
}

// ===----------------------------
// Effect
// ===----------------------------
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenEffectControl {
    pub name: GreenChild<IdentName>,
    pub params: Vec<GreenChild<GreenParam>>,
    pub return_type: GreenChild<TypeName>,
    pub text_len: TextLen,
}

// ===----------------------------
// Catch clause (for with expression)
// ===----------------------------
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenCatchClause {
    pub effect_path: GreenChild<GreenPureStaticPath>,
    pub control_name: GreenChild<IdentName>,
    pub params: Vec<GreenChild<GreenPattern>>,
    pub body: GreenChild<GreenExpr>,
    pub text_len: TextLen,
}


// ===----------------------------
// Expr
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum AtomExprNode {
    Decimal {
        dec: String,
        text_len: TextLen
    },
    Int {
        int: String,
        text_len: TextLen
    },
    Str {
        string: String,
        text_len: TextLen
    },
    Name {
        name: String,
        text_len: TextLen
    },
    Tuple {
        exprs: Vec<GreenChild<GreenExpr>>,
        text_len: TextLen
    },
    Ellipsis {
        text_len: TextLen
    },
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ExprRedNode {
    pub span: Span,
    pub inner: Arc<GreenExpr>,
}

impl ExprRedNode {
    pub fn child_to_red(&self, child: &GreenChild<GreenExpr>) -> ExprRedNode {
        let start = self.span.start_off + child.relative_start;
        let len = child.node.text_len;
        ExprRedNode {
            span: Span {
                source_id: self.span.source_id,
                start_off: start,
                end_off: start + len,
            },
            inner: Arc::clone(&child.node),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenExpr {
    pub kind: GreenExprKind,
    pub text_len: TextLen,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum GreenExprKind {
    Atom {
        expr: AtomExprNode,
    },
    Binary {
        left: GreenChild<GreenExpr>,
        op: GreenChild<Operator>,
        right: GreenChild<GreenExpr>,
    },
    Unary {
        op: GreenChild<Operator>,
        right: GreenChild<GreenExpr>,
    },
    Move {
        target: GreenChild<GreenExpr>,
    },
    Copy {
        target: GreenChild<GreenExpr>,
    },
    Ref {
        target: GreenChild<GreenExpr>,
    },
    MutRef {
        target: GreenChild<GreenExpr>,
    },
    Share {
        target: GreenChild<GreenExpr>,
    },
    Call {
        callee: GreenChild<GreenExpr>,
        args: Vec<GreenChild<GreenExpr>>,
    },
    UnsafeExternalCall {
        callee: GreenChild<GreenExpr>,
        args: Vec<GreenChild<GreenExpr>>,
    },
    StaticPath {
        path: GreenChild<GreenPureStaticPath>,
    },
    MemberAccess {
        left: GreenChild<GreenExpr>,
        member: GreenChild<IdentName>,
    },
    MakeStruct {
        path: GreenChild<GreenExpr>,
        fields: Vec<GreenChild<GreenStructFieldInit>>,
    },
    TypeCast {
        expr: GreenChild<GreenExpr>,
        into_type: GreenChild<TypeName>,
    },
    Do {
        exprs: Vec<GreenChild<GreenExpr>>,
    },
    Let {
        name: GreenChild<IdentName>,
        expr: GreenChild<GreenExpr>,
        type_str: Option<GreenChild<TypeName>>,
        mutable: bool,
    },
    If {
        cond: GreenChild<GreenExpr>,
        then_expr: GreenChild<GreenExpr>,
        elifs: Vec<GreenElseIf>,
        else_expr: Option<GreenChild<GreenExpr>>,
    },
    Return {
        expr: Option<GreenChild<GreenExpr>>,
    },
    Match {
        for_match: GreenChild<GreenExpr>,
        arms: Vec<GreenChild<GreenMatchArm>>,
    },
    Is {
        expr: GreenChild<GreenExpr>,
        pattern: GreenChild<GreenPattern>,
    },
    Raise {
        effect_path: GreenChild<GreenPureStaticPath>,
        control_name: GreenChild<IdentName>,
        args: Vec<GreenChild<GreenExpr>>,
    },
    With {
        handler_expr: GreenChild<GreenExpr>,
        clauses: Vec<GreenChild<GreenCatchClause>>,
    },
    Resume {
        expr: GreenChild<GreenExpr>,
    },
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenElseIf {
    pub cond: GreenChild<GreenExpr>,
    pub body: GreenChild<GreenExpr>,
    pub text_len: TextLen,
}

// ===----------------------------
// Param
// ===----------------------------
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenParam {
    pub name: GreenChild<IdentName>,
    pub type_str: GreenChild<TypeName>,
    pub text_len: TextLen,
}
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ParamRedNode {
    pub span: Span,
    pub green: Arc<GreenParam>,
}

// ===----------------------------
// Field
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FieldRedNode {
    pub span: Span,
    pub green: Arc<GreenField>,
}
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenField {
    pub name: GreenChild<IdentName>,
    pub type_str: GreenChild<TypeName>,
    pub text_len: TextLen,
}

// ===----------------------------
// GenericVar
// ===----------------------------
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GenericVarRedNode {
    pub span: Span,
    pub green: Arc<GreenGenericVar>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenGenericVar {
    pub name: GreenChild<IdentName>,
    pub constraint: Vec<GreenChild<TypeName>>,
    pub text_len: TextLen,
}


// ===----------------------------
// Method
// ===----------------------------
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct MethodRedNode {
    pub span: Span,
    pub green: Arc<GreenMethodDecl>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenMethodDecl {
    pub name: GreenChild<IdentName>,
    pub params: Vec<GreenChild<GreenParam>>,
    pub return_type_str: GreenChild<TypeName>,
    pub visibility: Visibility,
    pub text_len: TextLen,
}

// ===----------------------------
// Annotation
// ===----------------------------
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct AnnotationRedNode {
    pub span: Span,
    pub green: Arc<GreenAnnotation>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenAnnotation {
    pub name: String,
    pub args: Vec<String>,
    pub text_len: TextLen,
}

// ===----------------------------
// Ctor
// ===----------------------------
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CtorRedNode {
    pub span: Span,
    pub green: Arc<GreenCtor>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenCtor {
    pub name: GreenChild<IdentName>,
    pub generic_vars: Vec<GreenChild<GreenGenericVar>>,
    pub from_type_str: GreenChild<TypeName>,
    pub return_type_str: GreenChild<TypeName>,
    pub visibility: Visibility,
    pub text_len: TextLen,
}


// ===----------------------------
// Decl
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct DeclRedNode {
    pub span: Span,
    pub inner: Arc<GreenDecl>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenDecl {
    pub name: GreenChild<IdentName>,
    pub visibility: Visibility,
    pub kind: GreenDeclKind,
    pub annotations: Vec<GreenChild<GreenAnnotation>>,
    pub text_len: TextLen,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum GreenDeclKind {
    Fun {
        params: Vec<GreenChild<GreenParam>>,
        return_type_str: GreenChild<TypeName>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
        block: Vec<GreenChild<GreenExpr>>,
        where_clause: Option<GreenChild<GreenWhereClause>>,
    },
    FunDecl {
        params: Vec<GreenChild<GreenParam>>,
        return_type_str: GreenChild<TypeName>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
        where_clause: Option<GreenChild<GreenWhereClause>>,
    },
    TypeStruct {
        fields: Vec<GreenChild<GreenField>>,
        has_abst: Vec<GreenChild<IdentName>>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
        where_clause: Option<GreenChild<GreenWhereClause>>,
    },
    TypeAlias {
        ref_to: GreenChild<TypeName>,
        has_abst: Vec<GreenChild<IdentName>>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
        where_clause: Option<GreenChild<GreenWhereClause>>,
    },
    Abstract {
        super_abst: Vec<GreenChild<IdentName>>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
        methods: Vec<GreenChild<GreenMethodDecl>>,
        where_clause: Option<GreenChild<GreenWhereClause>>,
    },
    ADT {
        has_abst: Vec<GreenChild<IdentName>>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
        ctors: Vec<GreenChild<GreenCtor>>,
        where_clause: Option<GreenChild<GreenWhereClause>>,
    },
    Const {
        expr: GreenChild<GreenExpr>,
    },
    Global {
        expr: GreenChild<GreenExpr>,
    },
    Effect {
        controls: Vec<GreenChild<GreenEffectControl>>,
    },
    TypeDecl,
    CType,
    External {
        sym_name: GreenChild<IdentName>,
        params: Vec<GreenChild<GreenParam>>,
        return_type_str: GreenChild<TypeName>,
    },
}


// ===----------------------------
// Text Len
// ===----------------------------

pub trait HasTextLen {
    fn text_len(&self) -> TextLen;
}

impl HasTextLen for GreenExpr { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenParam { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenField { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenGenericVar { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenCtor { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenMethodDecl { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenAnnotation { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenStructFieldInit { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenWhereConstraint { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenPureStaticPath { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenWhereClause { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenDecl { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenElseIf { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenRequire { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenFileUnit { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for IdentName { fn text_len(&self) -> usize { self.name.len() } }
impl HasTextLen for GreenMatchArm { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenCatchClause { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenEffectControl { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenTupleElement { fn text_len(&self) -> TextLen { self.text_len } }
impl HasTextLen for GreenPattern {
    fn text_len(&self) -> TextLen {
        match self {
            GreenPattern::Wildcard => 1,
            GreenPattern::Literal(lit) => lit.text_len(),
            GreenPattern::Binding(id) => id.text_len(),
            GreenPattern::Constructor { text_len, .. } => *text_len,
        }
    }
}

impl HasTextLen for AtomExprNode {
    fn text_len(&self) -> TextLen {
        match self {
            AtomExprNode::Decimal { text_len, .. } => *text_len,
            AtomExprNode::Int { text_len, .. } => *text_len,
            AtomExprNode::Str { text_len, .. } => *text_len,
            AtomExprNode::Name { text_len, .. } => *text_len,
            AtomExprNode::Tuple { text_len, .. } => *text_len,
            AtomExprNode::Ellipsis { text_len } => *text_len,
        }
    }
}

impl HasTextLen for TypeName {
    fn text_len(&self) -> TextLen {
        match self {
            TypeName::Named { text_len, .. }
            | TypeName::Ref { text_len, .. }
            | TypeName::MutRef { text_len, .. }
            | TypeName::Share { text_len, .. }
            | TypeName::Tuple { text_len, .. }
            | TypeName::Impl { text_len, .. }
            | TypeName::Fun { text_len, .. } => *text_len,
        }
    }
}