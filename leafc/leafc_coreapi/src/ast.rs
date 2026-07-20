use std::sync::Arc;
use crate::source::{SourceId, Span};

pub type TextLen = usize;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CrateAst {
    pub external_requires: Vec<RequireRedNode>,
    pub file_units: Vec<FileRedUnit>,
}

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
pub struct TypeNameString {
    pub name: String,
    pub generics: Vec<TypeNameString>,
    pub text_len: TextLen,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TypeNameRedNode {
    pub span: Span,
    pub green: Arc<TypeNameString>,
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

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Operator {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Not,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    UserOp(String),
}

impl Operator {
    pub fn text_len(&self) -> TextLen {
        match self {
            Operator::UserOp(s) => s.len(),
            _ => self.keyword().len(),
        }
    }

    pub fn keyword(&self) -> &str {
        match self {
            Operator::Add => "+",
            Operator::Sub => "-",
            Operator::Mul => "*",
            Operator::Div => "/",
            Operator::Mod => "%",
            Operator::And => "&&",
            Operator::Or => "||",
            Operator::Not => "!",
            Operator::Eq => "==",
            Operator::Neq => "!=",
            Operator::Lt => "<",
            Operator::Gt => ">",
            Operator::Le => "<=",
            Operator::Ge => ">=",
            Operator::UserOp(s) => s.as_str(),
        }
    }
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
    Member {
        left: GreenChild<GreenExpr>,
        right: GreenChild<IdentName>,
    },
    TypeCast {
        expr: GreenChild<GreenExpr>,
        into_type: GreenChild<GreenExpr>,
    },
    Do {
        exprs: Vec<GreenChild<GreenExpr>>,
    },
    Let {
        name: GreenChild<IdentName>,
        expr: GreenChild<GreenExpr>,
        type_str: Option<GreenChild<TypeNameString>>,
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
    pub type_str: GreenChild<TypeNameString>,
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
    pub type_str: GreenChild<TypeNameString>,
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
    pub constraint: Vec<GreenChild<TypeNameString>>,
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
    pub return_type_str: GreenChild<TypeNameString>,
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
    pub from_type_str: GreenChild<TypeNameString>,
    pub return_type_str: GreenChild<TypeNameString>,
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
        return_type_str: GreenChild<TypeNameString>,
        block: Vec<GreenChild<GreenExpr>>,
    },
    FunDecl {
        params: Vec<GreenChild<GreenParam>>,
        return_type_str: GreenChild<TypeNameString>,
    },
    Abstract {
        super_abst: Vec<GreenChild<IdentName>>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
        methods: Vec<GreenChild<GreenMethodDecl>>,
    },
    TypeStruct {
        fields: Vec<GreenChild<GreenField>>,
        has_abst: Vec<GreenChild<IdentName>>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
    },
    TypeAlias {
        ref_to: GreenChild<TypeNameString>,
        has_abst: Vec<GreenChild<IdentName>>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
    },
    TypeDecl,
    ADT {
        has_abst: Vec<GreenChild<IdentName>>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
        ctors: Vec<GreenChild<GreenCtor>>,
    },
    CType,
    External {
        sym_name: GreenChild<IdentName>,
        params: Vec<GreenChild<GreenParam>>,
        return_type_str: GreenChild<TypeNameString>,
    },
}

// ===----------------------------
// Text Length
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
impl HasTextLen for TypeNameString { fn text_len(&self) -> TextLen { self.text_len } }

impl HasTextLen for GreenDecl { fn text_len(&self) -> TextLen { self.text_len } }

impl HasTextLen for GreenElseIf { fn text_len(&self) -> TextLen { self.text_len } }

impl HasTextLen for GreenRequire { fn text_len(&self) -> TextLen { self.text_len } }

impl HasTextLen for GreenFileUnit { fn text_len(&self) -> TextLen { self.text_len } }

impl HasTextLen for IdentName { fn text_len(&self) -> usize { self.name.len() } }