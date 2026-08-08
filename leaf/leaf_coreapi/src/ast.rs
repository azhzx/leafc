use crate::operators::Operator;
use crate::source::Span;
use std::sync::Arc;

// ===----------------------------
// rowan infrastructure
// ===----------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    Ident = 0,
    Int,
    Float,
    String,
    NewLine,
    Indent,
    Dedent,
    KwFun,
    KwType,
    KwImpl,
    KwWhere,
    KwIf,
    KwThen,
    KwElse,
    KwElif,
    KwLet,
    KwMut,
    KwReturn,
    KwWhen,
    KwRaise,
    KwWith,
    KwCatch,
    KwResume,
    KwConst,
    KwGlobal,
    KwEffect,
    KwExternal,
    KwCType,
    KwAbst,
    KwUse,
    KwOnly,
    KwPub,
    KwRef,
    KwShare,
    KwMove,
    KwCopy,
    KwBinding,
    KwIs,
    KwAs,
    KwDo,
    KwUnsafeCallExternal,
    KwTypeOf,
    KwOf,
    Lparen,
    Rparen,
    Lbrace,
    Rbrace,
    Lbracket,
    Rbracket,
    Comma,
    Dot,
    DotDot,
    DotDotDot,
    Colon,
    Semicolon,
    Eq,
    Arrow,
    FatArrow,
    Pipe,
    PipeLine,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Not,
    Or,
    And,
    EqEq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    At,
    Hash,
    SourceFile,
    Require,
    Param,
    Field,
    GenericVar,
    MethodDecl,
    Annotation,
    Ctor,
    Decl,
    ConstDecl,
    GlobalDecl,
    EffectDecl,
    AbstractDecl,
    TypeDecl,
    TypeAlias,
    TypeStruct,
    AdtDecl,
    FunDecl,
    FunDef,
    ExternalDecl,
    CTypeDecl,
    Pattern,
    WildcardPat,
    LiteralPat,
    BindingPat,
    ConstructorPat,
    OrPat,
    RestPat,
    TuplePat,
    StructPat,
    AliasPat,
    StructPatternField,
    Expr,
    AtomExpr,
    BinaryExpr,
    UnaryExpr,
    MoveExpr,
    CopyExpr,
    RefExpr,
    MutRefExpr,
    ShareExpr,
    CallExpr,
    UnsafeExternalCallExpr,
    StaticPathExpr,
    MemberAccessExpr,
    MakeStructExpr,
    TypeCastExpr,
    DoExpr,
    LetExpr,
    IfExpr,
    ReturnExpr,
    MatchExpr,
    IsExpr,
    RaiseExpr,
    WithExpr,
    ResumeExpr,
    TupleIndexExpr,
    PipeLineExpr,
    ConstEvalExpr,
    TypeName,
    NamedType,
    RefType,
    MutRefType,
    ShareType,
    TupleType,
    FunType,
    ImplType,
    TypeofType,
    WildcardType,
    TupleElement,
    Path,
    WhereClause,
    WhereConstraint,
    StructFieldInit,
    CallArg,
    MatchArm,
    EffectControl,
    CatchClause,
    CatchParam,
    ElseIf,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LeafLanguage {}
impl rowan::Language for LeafLanguage {
    type Kind = SyntaxKind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        unsafe { std::mem::transmute(raw) }
    }
    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

pub type SyntaxNode = rowan::SyntaxNode<LeafLanguage>;
pub type GreenNode = rowan::GreenNode;
pub type GreenNodeBuilder<'a> = rowan::GreenNodeBuilder<'a>;
pub type SyntaxToken = rowan::SyntaxToken<LeafLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<LeafLanguage>;

// ===----------------------------
// Helper trait
// ===----------------------------

pub trait AstNode: Sized {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(node: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;
    fn clone_for_update(&self) -> Self;
    fn wrap(node: SyntaxNode) -> Self;
    fn cast_element(element: SyntaxElement) -> Option<Self> {
        element.into_node().and_then(Self::cast)
    }
}


fn cast_ident(element: SyntaxElement) -> Option<IdentName> {
    element.into_node().and_then(IdentName::cast)
}

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
pub struct FileRedUnit {
    pub span: Span,
    pub green: Arc<GreenNode>,
}

impl FileRedUnit {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.as_ref().clone())
    }

    pub fn name(&self) -> Option<IdentName> {
        self.syntax()
            .children_with_tokens()
            .find_map(cast_ident)
    }

    pub fn top_decls(&self) -> impl Iterator<Item = DeclRedNode> {
        self.syntax()
            .children()
            .filter_map(DeclRedNode::cast)
    }

    pub fn requires(&self) -> impl Iterator<Item = RequireRedNode> {
        self.syntax()
            .children()
            .filter_map(RequireRedNode::cast)
    }
}

// ===----------------------------
// Require
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct RequireRedNode {
    syntax: SyntaxNode,
}

impl RequireRedNode {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::Require {
            Some(Self { syntax: node })
        } else {
            None
        }
    }

    pub fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }

    pub fn path(&self) -> impl Iterator<Item = IdentName> {
        self.syntax
            .children_with_tokens()
            .filter_map(cast_ident)
    }

    pub fn only(&self) -> Vec<IdentName> {
        self.syntax
            .children_with_tokens()
            .filter_map(cast_ident)
            .collect()
    }

    pub fn is_open(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .any(|t| t.kind() == SyntaxKind::Star)
    }
}

impl AstNode for RequireRedNode {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Require
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        RequireRedNode::cast(node)
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
    fn clone_for_update(&self) -> Self {
        RequireRedNode {
            syntax: self.syntax.clone_for_update(),
        }
    }
    fn wrap(node: SyntaxNode) -> Self {
        RequireRedNode { syntax: node }
    }
}

// ===----------------------------
// Visibility
// ===----------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Visibility {
    Private,
    Public,
    PublicExternal,
}

// ===----------------------------
// IdentName
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct IdentName {
    syntax: SyntaxNode,
}

impl IdentName {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::Ident {
            Some(Self { syntax: node })
        } else {
            None
        }
    }

    pub fn name(&self) -> String {
        self.syntax.text().to_string()
    }
}

impl AstNode for IdentName {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Ident
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        IdentName::cast(node)
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
    fn clone_for_update(&self) -> Self {
        IdentName {
            syntax: self.syntax.clone_for_update(),
        }
    }
    fn wrap(node: SyntaxNode) -> Self {
        IdentName { syntax: node }
    }
}

// ===----------------------------
// Operator helper
// ===----------------------------

pub fn syntax_kind_to_operator(kind: SyntaxKind) -> Option<Operator> {
    match kind {
        SyntaxKind::Plus => Some(Operator::Add),
        SyntaxKind::Minus => Some(Operator::Sub),
        SyntaxKind::Star => Some(Operator::Mul),
        SyntaxKind::Slash => Some(Operator::Div),
        SyntaxKind::Percent => Some(Operator::Mod),
        SyntaxKind::EqEq => Some(Operator::Eq),
        SyntaxKind::Ne => Some(Operator::Neq),
        SyntaxKind::Lt => Some(Operator::Lt),
        SyntaxKind::Gt => Some(Operator::Gt),
        SyntaxKind::Le => Some(Operator::Le),
        SyntaxKind::Ge => Some(Operator::Ge),
        SyntaxKind::And => Some(Operator::And),
        SyntaxKind::Or => Some(Operator::Or),
        SyntaxKind::Not => Some(Operator::Not),
        SyntaxKind::PipeLine => Some(Operator::PipeLine),
        _ => None,
    }
}

// ===----------------------------
// Path
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Path {
    syntax: SyntaxNode,
}

impl Path {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::Path {
            Some(Self { syntax: node })
        } else {
            None
        }
    }

    pub fn segments(&self) -> impl Iterator<Item = IdentName> {
        self.syntax
            .children_with_tokens()
            .filter_map(cast_ident)
    }
}

impl AstNode for Path {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::Path
    }
    fn cast(node: SyntaxNode) -> Option<Self> {
        Path::cast(node)
    }
    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
    fn clone_for_update(&self) -> Self {
        Path {
            syntax: self.syntax.clone_for_update(),
        }
    }
    fn wrap(node: SyntaxNode) -> Self {
        Path { syntax: node }
    }
}

// ===----------------------------
// Where
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct WhereClause {
    syntax: SyntaxNode,
}

impl WhereClause {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::WhereClause {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn constraints(&self) -> impl Iterator<Item = WhereConstraint> {
        self.syntax.children().filter_map(WhereConstraint::cast)
    }
}
impl AstNode for WhereClause {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::WhereClause }
    fn cast(node: SyntaxNode) -> Option<Self> { WhereClause::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { WhereClause { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { WhereClause { syntax: node } }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct WhereConstraint {
    syntax: SyntaxNode,
}

impl WhereConstraint {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::WhereConstraint {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn bounds(&self) -> impl Iterator<Item = TypeName> {
        self.syntax.children().filter_map(TypeName::cast)
    }
}
impl AstNode for WhereConstraint {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::WhereConstraint }
    fn cast(node: SyntaxNode) -> Option<Self> { WhereConstraint::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { WhereConstraint { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { WhereConstraint { syntax: node } }
}

// ===----------------------------
// TypeName (and its variants)
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TypeName {
    syntax: SyntaxNode,
}

impl TypeName {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::TypeName
            | SyntaxKind::NamedType
            | SyntaxKind::RefType
            | SyntaxKind::MutRefType
            | SyntaxKind::ShareType
            | SyntaxKind::TupleType
            | SyntaxKind::FunType
            | SyntaxKind::ImplType
            | SyntaxKind::TypeofType
            | SyntaxKind::WildcardType => Some(Self { syntax: node }),
            _ => None,
        }
    }

    pub fn kind(&self) -> SyntaxKind {
        self.syntax.kind()
    }

    pub fn named_path(&self) -> Option<Path> {
        self.syntax.children().find_map(Path::cast)
    }

    pub fn named_generics(&self) -> Vec<TypeName> {
        self.syntax.children().filter_map(TypeName::cast).collect()
    }

    pub fn ref_inner(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }

    pub fn mut_ref_inner(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }

    pub fn share_inner(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }

    pub fn tuple_elements(&self) -> Vec<TupleElement> {
        self.syntax.children().filter_map(TupleElement::cast).collect()
    }

    pub fn fun_params(&self) -> Vec<TypeName> {
        self.syntax.children().filter_map(TypeName::cast).collect()
    }

    pub fn fun_return(&self) -> Option<TypeName> {
        self.syntax.children().filter_map(TypeName::cast).last()
    }

    pub fn impl_trait(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }

    pub fn typeof_expr(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }

    pub fn is_wildcard(&self) -> bool {
        self.syntax.kind() == SyntaxKind::WildcardType
    }
}

impl AstNode for TypeName {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(kind,
            SyntaxKind::TypeName
            | SyntaxKind::NamedType
            | SyntaxKind::RefType
            | SyntaxKind::MutRefType
            | SyntaxKind::ShareType
            | SyntaxKind::TupleType
            | SyntaxKind::FunType
            | SyntaxKind::ImplType
            | SyntaxKind::TypeofType
            | SyntaxKind::WildcardType
        )
    }
    fn cast(node: SyntaxNode) -> Option<Self> { TypeName::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { TypeName { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { TypeName { syntax: node } }
}

// ===----------------------------
// TupleElement
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TupleElement {
    syntax: SyntaxNode,
}

impl TupleElement {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::TupleElement {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn ty(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }
    pub fn repeat(&self) -> Option<usize> {
        self.syntax.children_with_tokens()
            .filter_map(|e| e.into_token().and_then(|t| t.text().parse().ok()))
            .next()
    }
}
impl AstNode for TupleElement {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::TupleElement }
    fn cast(node: SyntaxNode) -> Option<Self> { TupleElement::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { TupleElement { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { TupleElement { syntax: node } }
}

// ===----------------------------
// Param
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Param {
    syntax: SyntaxNode,
}

impl Param {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::Param {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn type_str(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }
}
impl AstNode for Param {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::Param }
    fn cast(node: SyntaxNode) -> Option<Self> { Param::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { Param { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { Param { syntax: node } }
}

// ===----------------------------
// Field
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Field {
    syntax: SyntaxNode,
}

impl Field {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::Field {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn type_str(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }
}
impl AstNode for Field {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::Field }
    fn cast(node: SyntaxNode) -> Option<Self> { Field::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { Field { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { Field { syntax: node } }
}

// ===----------------------------
// GenericVar
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GenericVar {
    syntax: SyntaxNode,
}

impl GenericVar {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::GenericVar {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn constraint(&self) -> Vec<TypeName> {
        self.syntax.children().filter_map(TypeName::cast).collect()
    }
}
impl AstNode for GenericVar {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::GenericVar }
    fn cast(node: SyntaxNode) -> Option<Self> { GenericVar::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { GenericVar { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { GenericVar { syntax: node } }
}

// ===----------------------------
// MethodDecl
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct MethodDecl {
    syntax: SyntaxNode,
}

impl MethodDecl {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::MethodDecl {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn params(&self) -> impl Iterator<Item = Param> {
        self.syntax.children().filter_map(Param::cast)
    }
    pub fn return_type(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }
    pub fn visibility(&self) -> Visibility {
        if self.syntax.children_with_tokens().any(|t| t.kind() == SyntaxKind::KwPub) {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }
}
impl AstNode for MethodDecl {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::MethodDecl }
    fn cast(node: SyntaxNode) -> Option<Self> { MethodDecl::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { MethodDecl { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { MethodDecl { syntax: node } }
}

// ===----------------------------
// Annotation
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Annotation {
    syntax: SyntaxNode,
}

impl Annotation {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::Annotation {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn name(&self) -> String {
        self.syntax.text().to_string()
    }
    pub fn args(&self) -> Vec<String> {
        self.syntax.children_with_tokens()
            .filter_map(|e| e.into_token().map(|t| t.text().to_string()))
            .collect()
    }
}
impl AstNode for Annotation {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::Annotation }
    fn cast(node: SyntaxNode) -> Option<Self> { Annotation::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { Annotation { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { Annotation { syntax: node } }
}

// ===----------------------------
// Ctor
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Ctor {
    syntax: SyntaxNode,
}

impl Ctor {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::Ctor {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn generic_vars(&self) -> impl Iterator<Item = GenericVar> {
        self.syntax.children().filter_map(GenericVar::cast)
    }
    pub fn from_type(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }
    pub fn return_type(&self) -> Option<TypeName> {
        self.syntax.children().filter_map(TypeName::cast).nth(1)
    }
    pub fn visibility(&self) -> Visibility {
        if self.syntax.children_with_tokens().any(|t| t.kind() == SyntaxKind::KwPub) {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }
}
impl AstNode for Ctor {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::Ctor }
    fn cast(node: SyntaxNode) -> Option<Self> { Ctor::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { Ctor { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { Ctor { syntax: node } }
}

// ===----------------------------
// DeclRedNode
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct DeclRedNode {
    syntax: SyntaxNode,
}

impl DeclRedNode {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::Decl
            | SyntaxKind::ConstDecl
            | SyntaxKind::GlobalDecl
            | SyntaxKind::EffectDecl
            | SyntaxKind::AbstractDecl
            | SyntaxKind::TypeDecl
            | SyntaxKind::TypeAlias
            | SyntaxKind::TypeStruct
            | SyntaxKind::AdtDecl
            | SyntaxKind::FunDecl
            | SyntaxKind::FunDef
            | SyntaxKind::ExternalDecl
            | SyntaxKind::CTypeDecl => Some(Self { syntax: node }),
            _ => None,
        }
    }

    pub fn kind(&self) -> SyntaxKind {
        self.syntax.kind()
    }

    pub fn name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }

    pub fn visibility(&self) -> Visibility {
        if self.syntax.children_with_tokens().any(|t| t.kind() == SyntaxKind::KwPub) {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }

    pub fn annotations(&self) -> impl Iterator<Item = Annotation> {
        self.syntax.children().filter_map(Annotation::cast)
    }

    pub fn fun_params(&self) -> impl Iterator<Item = Param> {
        self.syntax.children().filter_map(Param::cast)
    }
    pub fn fun_return_type(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }
    pub fn fun_generic_vars(&self) -> impl Iterator<Item = GenericVar> {
        self.syntax.children().filter_map(GenericVar::cast)
    }
    pub fn fun_block(&self) -> impl Iterator<Item = ExprRedNode> {
        self.syntax.children().filter_map(ExprRedNode::cast)
    }
    pub fn where_clause(&self) -> Option<WhereClause> {
        self.syntax.children().find_map(WhereClause::cast)
    }
    pub fn is_consteval(&self) -> bool {
        self.syntax.children_with_tokens().any(|t| t.kind() == SyntaxKind::KwConst)
    }

    pub fn type_struct_fields(&self) -> impl Iterator<Item = Field> {
        self.syntax.children().filter_map(Field::cast)
    }
    pub fn type_alias_ref(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }
    pub fn has_abst(&self) -> impl Iterator<Item = IdentName> {
        self.syntax.children_with_tokens().filter_map(cast_ident)
    }

    pub fn abstract_super(&self) -> impl Iterator<Item = IdentName> {
        self.syntax.children_with_tokens().filter_map(cast_ident)
    }
    pub fn abstract_methods(&self) -> impl Iterator<Item = MethodDecl> {
        self.syntax.children().filter_map(MethodDecl::cast)
    }

    pub fn adt_ctors(&self) -> impl Iterator<Item = Ctor> {
        self.syntax.children().filter_map(Ctor::cast)
    }

    pub fn const_expr(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn const_type(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }

    pub fn global_expr(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn global_type(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }

    pub fn effect_controls(&self) -> impl Iterator<Item = EffectControl> {
        self.syntax.children().filter_map(EffectControl::cast)
    }

    pub fn external_sym_name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn external_params(&self) -> impl Iterator<Item = Param> {
        self.syntax.children().filter_map(Param::cast)
    }
    pub fn external_return_type(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }
    pub fn external_is_variadic(&self) -> bool {
        self.syntax.children_with_tokens().any(|t| t.kind() == SyntaxKind::DotDotDot)
    }
}

impl AstNode for DeclRedNode {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(kind,
            SyntaxKind::Decl
            | SyntaxKind::ConstDecl
            | SyntaxKind::GlobalDecl
            | SyntaxKind::EffectDecl
            | SyntaxKind::AbstractDecl
            | SyntaxKind::TypeDecl
            | SyntaxKind::TypeAlias
            | SyntaxKind::TypeStruct
            | SyntaxKind::AdtDecl
            | SyntaxKind::FunDecl
            | SyntaxKind::FunDef
            | SyntaxKind::ExternalDecl
            | SyntaxKind::CTypeDecl
        )
    }
    fn cast(node: SyntaxNode) -> Option<Self> { DeclRedNode::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { DeclRedNode { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { DeclRedNode { syntax: node } }
}

// ===----------------------------
// ExprRedNode
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ExprRedNode {
    syntax: SyntaxNode,
}

impl ExprRedNode {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::Expr
            | SyntaxKind::AtomExpr
            | SyntaxKind::BinaryExpr
            | SyntaxKind::UnaryExpr
            | SyntaxKind::MoveExpr
            | SyntaxKind::CopyExpr
            | SyntaxKind::RefExpr
            | SyntaxKind::MutRefExpr
            | SyntaxKind::ShareExpr
            | SyntaxKind::CallExpr
            | SyntaxKind::UnsafeExternalCallExpr
            | SyntaxKind::StaticPathExpr
            | SyntaxKind::MemberAccessExpr
            | SyntaxKind::MakeStructExpr
            | SyntaxKind::TypeCastExpr
            | SyntaxKind::DoExpr
            | SyntaxKind::LetExpr
            | SyntaxKind::IfExpr
            | SyntaxKind::ReturnExpr
            | SyntaxKind::MatchExpr
            | SyntaxKind::IsExpr
            | SyntaxKind::RaiseExpr
            | SyntaxKind::WithExpr
            | SyntaxKind::ResumeExpr
            | SyntaxKind::TupleIndexExpr
            | SyntaxKind::PipeLineExpr
            | SyntaxKind::ConstEvalExpr => Some(Self { syntax: node }),
            _ => None,
        }
    }

    pub fn span(&self) -> Span {
        let range = self.syntax.text_range();
        Span {
            source_id: crate::id::FileId(0),
            start_off: usize::from(range.start()),
            end_off: usize::from(range.end()),
        }
    }

    pub fn inner(&self) -> &SyntaxNode {
        &self.syntax
    }

    pub fn kind(&self) -> SyntaxKind {
        self.syntax.kind()
    }

    pub fn binary_left(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn binary_op(&self) -> Option<Operator> {
        self.syntax.children_with_tokens().find_map(|e| {
            e.into_token().and_then(|t| syntax_kind_to_operator(t.kind()))
        })
    }
    pub fn binary_right(&self) -> Option<ExprRedNode> {
        self.syntax.children().filter_map(ExprRedNode::cast).nth(1)
    }

    pub fn unary_op(&self) -> Option<Operator> {
        self.syntax.children_with_tokens().find_map(|e| {
            e.into_token().and_then(|t| syntax_kind_to_operator(t.kind()))
        })
    }
    pub fn unary_expr(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }

    pub fn call_callee(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn call_args(&self) -> impl Iterator<Item = CallArg> {
        self.syntax.children().filter_map(CallArg::cast)
    }

    pub fn member_access_left(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn member_access_member(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }

    pub fn make_struct_path(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn make_struct_fields(&self) -> impl Iterator<Item = StructFieldInit> {
        self.syntax.children().filter_map(StructFieldInit::cast)
    }

    pub fn type_cast_expr(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn type_cast_type(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }

    pub fn do_exprs(&self) -> impl Iterator<Item = ExprRedNode> {
        self.syntax.children().filter_map(ExprRedNode::cast)
    }

    pub fn let_name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn let_value(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn let_type(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }

    pub fn if_cond(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn if_then(&self) -> Option<ExprRedNode> {
        self.syntax.children().filter_map(ExprRedNode::cast).nth(1)
    }
    pub fn elifs(&self) -> impl Iterator<Item = ElseIf> {
        self.syntax.children().filter_map(ElseIf::cast)
    }
    pub fn else_expr(&self) -> Option<ExprRedNode> {
        self.syntax.children().filter_map(ExprRedNode::cast).last()
    }

    pub fn return_expr(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }

    pub fn match_scrutinee(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn match_arms(&self) -> impl Iterator<Item = MatchArm> {
        self.syntax.children().filter_map(MatchArm::cast)
    }

    pub fn is_expr(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn is_pattern(&self) -> Option<Pattern> {
        self.syntax.children().find_map(Pattern::cast)
    }

    pub fn raise_effect_path(&self) -> Option<Path> {
        self.syntax.children().find_map(Path::cast)
    }
    pub fn raise_control_name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn raise_args(&self) -> impl Iterator<Item = ExprRedNode> {
        self.syntax.children().filter_map(ExprRedNode::cast)
    }

    pub fn with_handler(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn with_clauses(&self) -> impl Iterator<Item = CatchClause> {
        self.syntax.children().filter_map(CatchClause::cast)
    }

    pub fn resume_expr(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }

    pub fn tuple_index_expr(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn tuple_index_index(&self) -> Option<usize> {
        self.syntax.children_with_tokens()
            .filter_map(|e| e.into_token().and_then(|t| t.text().parse().ok()))
            .next()
    }

    pub fn pipeline_left(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn pipeline_right(&self) -> Option<ExprRedNode> {
        self.syntax.children().filter_map(ExprRedNode::cast).nth(1)
    }

    pub fn const_eval_expr(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
}

impl AstNode for ExprRedNode {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(kind,
            SyntaxKind::Expr
            | SyntaxKind::AtomExpr
            | SyntaxKind::BinaryExpr
            | SyntaxKind::UnaryExpr
            | SyntaxKind::MoveExpr
            | SyntaxKind::CopyExpr
            | SyntaxKind::RefExpr
            | SyntaxKind::MutRefExpr
            | SyntaxKind::ShareExpr
            | SyntaxKind::CallExpr
            | SyntaxKind::UnsafeExternalCallExpr
            | SyntaxKind::StaticPathExpr
            | SyntaxKind::MemberAccessExpr
            | SyntaxKind::MakeStructExpr
            | SyntaxKind::TypeCastExpr
            | SyntaxKind::DoExpr
            | SyntaxKind::LetExpr
            | SyntaxKind::IfExpr
            | SyntaxKind::ReturnExpr
            | SyntaxKind::MatchExpr
            | SyntaxKind::IsExpr
            | SyntaxKind::RaiseExpr
            | SyntaxKind::WithExpr
            | SyntaxKind::ResumeExpr
            | SyntaxKind::TupleIndexExpr
            | SyntaxKind::PipeLineExpr
            | SyntaxKind::ConstEvalExpr
        )
    }
    fn cast(node: SyntaxNode) -> Option<Self> { ExprRedNode::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { ExprRedNode { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { ExprRedNode { syntax: node } }
}

// ===----------------------------
// StructFieldInit
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct StructFieldInit {
    syntax: SyntaxNode,
}

impl StructFieldInit {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::StructFieldInit {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn value(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
}
impl AstNode for StructFieldInit {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::StructFieldInit }
    fn cast(node: SyntaxNode) -> Option<Self> { StructFieldInit::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { StructFieldInit { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { StructFieldInit { syntax: node } }
}

// ===----------------------------
// CallArg
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CallArg {
    syntax: SyntaxNode,
}

impl CallArg {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::CallArg {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn value(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
}
impl AstNode for CallArg {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::CallArg }
    fn cast(node: SyntaxNode) -> Option<Self> { CallArg::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { CallArg { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { CallArg { syntax: node } }
}

// ===----------------------------
// Pattern
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Pattern {
    syntax: SyntaxNode,
}

impl Pattern {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::Pattern
            | SyntaxKind::WildcardPat
            | SyntaxKind::LiteralPat
            | SyntaxKind::BindingPat
            | SyntaxKind::ConstructorPat
            | SyntaxKind::OrPat
            | SyntaxKind::RestPat
            | SyntaxKind::TuplePat
            | SyntaxKind::StructPat
            | SyntaxKind::AliasPat => Some(Self { syntax: node }),
            _ => None,
        }
    }

    pub fn kind(&self) -> SyntaxKind {
        self.syntax.kind()
    }

    pub fn wildcard(&self) -> Option<()> {
        if self.syntax.kind() == SyntaxKind::WildcardPat { Some(()) } else { None }
    }

    pub fn literal(&self) -> Option<AtomExprNode> {
        if self.syntax.kind() == SyntaxKind::LiteralPat {
            self.syntax.children().find_map(AtomExprNode::cast)
        } else {
            None
        }
    }

    pub fn binding(&self) -> Option<IdentName> {
        if self.syntax.kind() == SyntaxKind::BindingPat {
            self.syntax.children_with_tokens().find_map(cast_ident)
        } else {
            None
        }
    }

    pub fn constructor(&self) -> Option<(TypeName, Vec<Pattern>)> {
        if self.syntax.kind() == SyntaxKind::ConstructorPat {
            let type_name = self.syntax.children().find_map(TypeName::cast)?;
            let args = self.syntax.children().filter_map(Pattern::cast).collect();
            Some((type_name, args))
        } else {
            None
        }
    }

    pub fn or_pattern(&self) -> Option<(Pattern, Pattern)> {
        if self.syntax.kind() == SyntaxKind::OrPat {
            let mut children = self.syntax.children().filter_map(Pattern::cast);
            let left = children.next()?;
            let right = children.next()?;
            Some((left, right))
        } else {
            None
        }
    }

    pub fn rest(&self) -> bool {
        self.syntax.kind() == SyntaxKind::RestPat
    }

    pub fn tuple_pattern(&self) -> Vec<Pattern> {
        self.syntax.children().filter_map(Pattern::cast).collect()
    }

    pub fn struct_pattern(&self) -> Option<(Path, Vec<StructPatternField>, bool)> {
        if self.syntax.kind() == SyntaxKind::StructPat {
            let path = self.syntax.children().find_map(Path::cast)?;
            let fields: Vec<_> = self.syntax.children().filter_map(StructPatternField::cast).collect();
            let rest = self.syntax.children_with_tokens().any(|t| t.kind() == SyntaxKind::DotDot);
            Some((path, fields, rest))
        } else {
            None
        }
    }

    pub fn alias(&self) -> Option<(Pattern, IdentName)> {
        if self.syntax.kind() == SyntaxKind::AliasPat {
            let inner = self.syntax.children().find_map(Pattern::cast)?;
            let name = self.syntax.children_with_tokens().find_map(cast_ident)?;
            Some((inner, name))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct StructPatternField {
    syntax: SyntaxNode,
}
impl StructPatternField {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::StructPatternField {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn field_name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn pattern(&self) -> Option<Pattern> {
        self.syntax.children().find_map(Pattern::cast)
    }
}
impl AstNode for StructPatternField {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::StructPatternField }
    fn cast(node: SyntaxNode) -> Option<Self> { StructPatternField::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { StructPatternField { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { StructPatternField { syntax: node } }
}

// ===----------------------------
// MatchArm
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct MatchArm {
    syntax: SyntaxNode,
}
impl MatchArm {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::MatchArm {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn pattern(&self) -> Option<Pattern> {
        self.syntax.children().find_map(Pattern::cast)
    }
    pub fn guard(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn body(&self) -> Option<ExprRedNode> {
        self.syntax.children().filter_map(ExprRedNode::cast).nth(1)
    }
}
impl AstNode for MatchArm {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::MatchArm }
    fn cast(node: SyntaxNode) -> Option<Self> { MatchArm::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { MatchArm { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { MatchArm { syntax: node } }
}

// ===----------------------------
// EffectControl
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct EffectControl {
    syntax: SyntaxNode,
}
impl EffectControl {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::EffectControl {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
    pub fn params(&self) -> impl Iterator<Item = Param> {
        self.syntax.children().filter_map(Param::cast)
    }
    pub fn return_type(&self) -> Option<TypeName> {
        self.syntax.children().find_map(TypeName::cast)
    }
}
impl AstNode for EffectControl {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::EffectControl }
    fn cast(node: SyntaxNode) -> Option<Self> { EffectControl::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { EffectControl { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { EffectControl { syntax: node } }
}

// ===----------------------------
// CatchClause
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CatchClause {
    syntax: SyntaxNode,
}
impl CatchClause {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::CatchClause {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn control_path(&self) -> Option<Path> {
        self.syntax.children().find_map(Path::cast)
    }
    pub fn params(&self) -> Vec<CatchParam> {
        self.syntax.children().filter_map(CatchParam::cast).collect()
    }
    pub fn body(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
}
impl AstNode for CatchClause {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::CatchClause }
    fn cast(node: SyntaxNode) -> Option<Self> { CatchClause::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { CatchClause { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { CatchClause { syntax: node } }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CatchParam {
    syntax: SyntaxNode,
}
impl CatchParam {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::CatchParam {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn is_rest(&self) -> bool {
        self.syntax.children_with_tokens().any(|t| t.kind() == SyntaxKind::DotDot)
    }
    pub fn name(&self) -> Option<IdentName> {
        self.syntax.children_with_tokens().find_map(cast_ident)
    }
}
impl AstNode for CatchParam {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::CatchParam }
    fn cast(node: SyntaxNode) -> Option<Self> { CatchParam::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { CatchParam { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { CatchParam { syntax: node } }
}

// ===----------------------------
// ElseIf
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ElseIf {
    syntax: SyntaxNode,
}
impl ElseIf {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::ElseIf {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn cond(&self) -> Option<ExprRedNode> {
        self.syntax.children().find_map(ExprRedNode::cast)
    }
    pub fn body(&self) -> Option<ExprRedNode> {
        self.syntax.children().filter_map(ExprRedNode::cast).nth(1)
    }
}
impl AstNode for ElseIf {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::ElseIf }
    fn cast(node: SyntaxNode) -> Option<Self> { ElseIf::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { ElseIf { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { ElseIf { syntax: node } }
}

// ===----------------------------
// AtomExprNode
// ===----------------------------

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct AtomExprNode {
    syntax: SyntaxNode,
}
impl AtomExprNode {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        if node.kind() == SyntaxKind::AtomExpr {
            Some(Self { syntax: node })
        } else {
            None
        }
    }
    pub fn text(&self) -> String {
        self.syntax.text().to_string()
    }
    pub fn kind(&self) -> SyntaxKind {
        self.syntax.first_child_or_token()
            .map(|e| e.kind())
            .unwrap_or(SyntaxKind::Error)
    }
}
impl AstNode for AtomExprNode {
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::AtomExpr }
    fn cast(node: SyntaxNode) -> Option<Self> { AtomExprNode::cast(node) }
    fn syntax(&self) -> &SyntaxNode { &self.syntax }
    fn clone_for_update(&self) -> Self { AtomExprNode { syntax: self.syntax.clone_for_update() } }
    fn wrap(node: SyntaxNode) -> Self { AtomExprNode { syntax: node } }
}

// ===----------------------------
// Helper functions (red -> red)
// ===----------------------------

pub fn child_expr_red(_parent: &SyntaxNode, child: &SyntaxNode) -> ExprRedNode {
    ExprRedNode::wrap(child.clone())
}

pub fn child_decl_red(_parent: &SyntaxNode, child: &SyntaxNode) -> DeclRedNode {
    DeclRedNode::wrap(child.clone())
}

pub fn child_span(parent: &SyntaxNode, relative_start: usize, text_len: usize) -> Span {
    let base_start = usize::from(parent.text_range().start());
    Span {
        source_id: crate::id::FileId(0),
        start_off: base_start + relative_start,
        end_off: base_start + relative_start + text_len,
    }
}

pub fn child_span_of(_base: &SyntaxNode, child: &SyntaxNode) -> Span {
    let range = child.text_range();
    Span {
        source_id: crate::id::FileId(0),
        start_off: usize::from(range.start()),
        end_off: usize::from(range.end()),
    }
}