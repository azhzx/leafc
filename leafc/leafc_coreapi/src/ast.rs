use std::sync::Arc;
use crate::source::{SourceId, Span};

pub type TextLen = usize;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenChild<T> {
    pub relative_start: usize,
    pub node: Arc<T>,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TypeNameString {
    pub name: String,
    pub generics: Vec<TypeNameString>,
    pub text_len: TextLen,
}


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
        right: GreenChild<String>,
    },
    TypeCast {
        expr: GreenChild<GreenExpr>,
        into_type: GreenChild<GreenExpr>,
    },
    Do {
        exprs: Vec<GreenChild<GreenExpr>>,
    },
    Let {
        name: GreenChild<String>,
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


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ExprRedNode {
    pub span: Span,
    pub inner: Arc<GreenExpr>,
}

impl ExprRedNode {
    fn child_red(&self, child: &GreenChild<GreenExpr>) -> ExprRedNode {
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

    fn operator_red(&self, child: &GreenChild<Operator>) -> OperatorRedNode {
        let start = self.span.start_off + child.relative_start;
        let len = child.node.text_len();
        OperatorRedNode {
            span: Span {
                source_id: self.span.source_id,
                start_off: start,
                end_off: start + len,
            },
            op: child.node.as_ref().clone(),
        }
    }

    fn type_name_red(&self, child: &GreenChild<TypeNameString>) -> TypeNameRedNode {
        let start = self.span.start_off + child.relative_start;
        let len = child.node.text_len;
        TypeNameRedNode {
            span: Span {
                source_id: self.span.source_id,
                start_off: start,
                end_off: start + len,
            },
            green: Arc::clone(&child.node),
        }
    }

    fn string_red(&self, child: &GreenChild<String>, text_len: usize) -> StringRedNode {
        let start = self.span.start_off + child.relative_start;
        StringRedNode {
            span: Span {
                source_id: self.span.source_id,
                start_off: start,
                end_off: start + text_len,
            },
            text: child.node.as_ref().clone(),
        }
    }

    pub fn kind(&self) -> &GreenExprKind {
        &self.inner.kind
    }

    // --- 便捷导航方法 ---

    pub fn cond(&self) -> Option<ExprRedNode> {
        match &self.inner.kind {
            GreenExprKind::If { cond, .. } => Some(self.child_red(cond)),
            _ => None,
        }
    }

    pub fn then_expr(&self) -> Option<ExprRedNode> {
        match &self.inner.kind {
            GreenExprKind::If { then_expr, .. } => Some(self.child_red(then_expr)),
            _ => None,
        }
    }

    pub fn else_expr(&self) -> Option<ExprRedNode> {
        match &self.inner.kind {
            GreenExprKind::If { else_expr, .. } => else_expr.as_ref().map(|e| self.child_red(e)),
            _ => None,
        }
    }

    pub fn elifs(&self) -> Vec<ExprRedNode> {
        match &self.inner.kind {
            GreenExprKind::If { elifs, .. } => elifs
                .iter()
                .flat_map(|elif| {
                    // 对于 elif，我们可以只返回条件或 body，这里按常见需求返回条件
                    Some(self.child_red(&elif.cond))
                })
                .collect(),
            _ => vec![],
        }
    }

    pub fn left(&self) -> Option<ExprRedNode> {
        match &self.inner.kind {
            GreenExprKind::Binary { left, .. } => Some(self.child_red(left)),
            _ => None,
        }
    }

    pub fn right(&self) -> Option<ExprRedNode> {
        match &self.inner.kind {
            GreenExprKind::Binary { right, .. } => Some(self.child_red(right)),
            _ => None,
        }
    }

    pub fn op(&self) -> Option<OperatorRedNode> {
        match &self.inner.kind {
            GreenExprKind::Binary { op, .. } | GreenExprKind::Unary { op, .. } => {
                Some(self.operator_red(op))
            }
            _ => None,
        }
    }

    pub fn target(&self) -> Option<ExprRedNode> {
        match &self.inner.kind {
            GreenExprKind::Move { target }
            | GreenExprKind::Copy { target }
            | GreenExprKind::Ref { target }
            | GreenExprKind::MutRef { target }
            | GreenExprKind::Share { target } => Some(self.child_red(target)),
            _ => None,
        }
    }

    pub fn callee(&self) -> Option<ExprRedNode> {
        match &self.inner.kind {
            GreenExprKind::Call { callee, .. } | GreenExprKind::UnsafeExternalCall { callee, .. } => {
                Some(self.child_red(callee))
            }
            _ => None,
        }
    }

    pub fn args(&self) -> Vec<ExprRedNode> {
        match &self.inner.kind {
            GreenExprKind::Call { args, .. } | GreenExprKind::UnsafeExternalCall { args, .. } => {
                args.iter().map(|a| self.child_red(a)).collect()
            }
            _ => vec![],
        }
    }

    pub fn member_name(&self) -> Option<StringRedNode> {
        match &self.inner.kind {
            GreenExprKind::Member { right, .. } => {
                let len = right.node.len(); // String 的字节长度
                Some(self.string_red(right, len))
            }
            _ => None,
        }
    }

    pub fn let_name(&self) -> Option<StringRedNode> {
        match &self.inner.kind {
            GreenExprKind::Let { name, .. } => {
                let len = name.node.len();
                Some(self.string_red(name, len))
            }
            _ => None,
        }
    }

    pub fn let_type(&self) -> Option<TypeNameRedNode> {
        match &self.inner.kind {
            GreenExprKind::Let { type_str, .. } => type_str.as_ref().map(|t| self.type_name_red(t)),
            _ => None,
        }
    }

    pub fn children(&self) -> Vec<ExprRedNode> {
        match &self.inner.kind {
            GreenExprKind::Atom { .. } | GreenExprKind::Return { expr: None } => vec![],
            GreenExprKind::Binary { left, right, .. } => vec![self.child_red(left), self.child_red(right)],
            GreenExprKind::Unary { right, .. } => vec![self.child_red(right)],
            GreenExprKind::Move { target }
            | GreenExprKind::Copy { target }
            | GreenExprKind::Ref { target }
            | GreenExprKind::MutRef { target }
            | GreenExprKind::Share { target } => vec![self.child_red(target)],
            GreenExprKind::Call { callee, args } | GreenExprKind::UnsafeExternalCall { callee, args } => {
                let mut children = vec![self.child_red(callee)];
                children.extend(args.iter().map(|a| self.child_red(a)));
                children
            }
            GreenExprKind::Member { left, .. } | GreenExprKind::TypeCast { expr: left, .. } => {
                vec![self.child_red(left)]
            }
            GreenExprKind::Do { exprs } => exprs.iter().map(|e| self.child_red(e)).collect(),
            GreenExprKind::Let { expr, .. } => vec![self.child_red(expr)],
            GreenExprKind::If { cond, then_expr, elifs, else_expr } => {
                let mut children = vec![self.child_red(cond), self.child_red(then_expr)];
                for elif in elifs {
                    children.push(self.child_red(&elif.cond));
                    children.push(self.child_red(&elif.body));
                }
                if let Some(else_e) = else_expr {
                    children.push(self.child_red(else_e));
                }
                children
            }
            GreenExprKind::Return { expr: Some(e) } => vec![self.child_red(e)],
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct OperatorRedNode {
    pub span: Span,
    pub op: Operator,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TypeNameRedNode {
    pub span: Span,
    pub green: Arc<TypeNameString>,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenParam {
    pub name: GreenChild<String>,
    pub type_str: GreenChild<TypeNameString>,
    pub text_len: TextLen,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenField {
    pub name: GreenChild<String>,
    pub type_str: GreenChild<TypeNameString>,
    pub text_len: TextLen,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenGenericVar {
    pub name: GreenChild<String>,
    pub constraint: Vec<GreenChild<TypeNameString>>,
    pub text_len: TextLen,
}



#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenCtor {
    pub name: GreenChild<String>,
    pub generic_vars: Vec<GreenChild<GreenGenericVar>>,
    pub from_type_str: GreenChild<TypeNameString>,
    pub return_type_str: GreenChild<TypeNameString>,
    pub visibility: Visibility,
    pub text_len: TextLen,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenMethodDecl {
    pub name: GreenChild<String>,
    pub params: Vec<GreenChild<GreenParam>>,
    pub return_type_str: GreenChild<TypeNameString>,
    pub visibility: Visibility,
    pub text_len: TextLen,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenAnnotation {
    pub name: String,
    pub args: Vec<String>,
    pub text_len: TextLen,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenDecl {
    pub name: GreenChild<String>,
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
        has_abst: Vec<GreenChild<String>>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
        methods: Vec<GreenChild<GreenMethodDecl>>,
    },
    TypeStruct {
        fields: Vec<GreenChild<GreenField>>,
        has_abst: Vec<GreenChild<String>>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
    },
    TypeAlias {
        ref_to: GreenChild<TypeNameString>,
        has_abst: Vec<GreenChild<String>>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
    },
    TypeDecl,
    ADT {
        has_abst: Vec<GreenChild<String>>,
        generic_vars: Vec<GreenChild<GreenGenericVar>>,
        ctors: Vec<GreenChild<GreenCtor>>,
    },
    CType,
    External {
        sym_name: GreenChild<String>,
        params: Vec<GreenChild<GreenParam>>,
        return_type_str: GreenChild<TypeNameString>,
    },
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct DeclRedNode {
    pub span: Span,
    pub inner: Arc<GreenDecl>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FieldRedNode {
    pub span: Span,
    pub green: Arc<GreenField>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CtorRedNode {
    pub span: Span,
    pub green: Arc<GreenCtor>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct MethodRedNode {
    pub span: Span,
    pub green: Arc<GreenMethodDecl>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GenericVarRedNode {
    pub span: Span,
    pub green: Arc<GreenGenericVar>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct AnnotationRedNode {
    pub span: Span,
    pub green: Arc<GreenAnnotation>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct StringRedNode {
    pub span: Span,
    pub text: String,
}

trait HasTextLen {
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

impl DeclRedNode {

    fn child_span<T: HasTextLen>(&self, child: &GreenChild<T>) -> Span {
        let start = self.span.start_off + child.relative_start;
        Span {
            source_id: self.span.source_id,
            start_off: start,
            end_off: start + child.node.text_len(),
        }
    }

    fn child_red(&self, child: &GreenChild<GreenExpr>) -> ExprRedNode {
        let span = self.child_span(child);
        ExprRedNode {
            span,
            inner: Arc::clone(&child.node),
        }
    }

    fn child_decl_red(&self, child: &GreenChild<GreenDecl>) -> DeclRedNode {
        let span = self.child_span(child);
        DeclRedNode {
            span,
            inner: Arc::clone(&child.node),
        }
    }

    fn type_name_red(&self, child: &GreenChild<TypeNameString>) -> TypeNameRedNode {
        let span = self.child_span(child);
        TypeNameRedNode {
            span,
            green: Arc::clone(&child.node),
        }
    }

    fn string_red(&self, child: &GreenChild<String>, text_len: usize) -> StringRedNode {
        let start = self.span.start_off + child.relative_start;
        StringRedNode {
            span: Span {
                source_id: self.span.source_id,
                start_off: start,
                end_off: start + text_len,
            },
            text: child.node.as_ref().clone(),
        }
    }

    pub fn name(&self) -> StringRedNode {
        let len = self.inner.name.node.len();
        self.string_red(&self.inner.name, len)
    }

    pub fn visibility(&self) -> Visibility {
        self.inner.visibility.clone()
    }

    pub fn kind(&self) -> &GreenDeclKind {
        &self.inner.kind
    }

    pub fn params(&self) -> Vec<ParamRedNode> {
        match &self.inner.kind {
            GreenDeclKind::Fun { params, .. }
            | GreenDeclKind::FunDecl { params, .. }
            | GreenDeclKind::External { params, .. } => params
                .iter()
                .map(|p| ParamRedNode {
                    span: self.child_span(p),
                    green: Arc::clone(&p.node),
                })
                .collect(),
            _ => vec![],
        }
    }

    pub fn return_type(&self) -> Option<TypeNameRedNode> {
        match &self.inner.kind {
            GreenDeclKind::Fun { return_type_str, .. }
            | GreenDeclKind::FunDecl { return_type_str, .. }
            | GreenDeclKind::External { return_type_str, .. } => {
                Some(self.type_name_red(return_type_str))
            }
            _ => None,
        }
    }

    pub fn block(&self) -> Vec<ExprRedNode> {
        match &self.inner.kind {
            GreenDeclKind::Fun { block, .. } => block.iter().map(|e| self.child_red(e)).collect(),
            _ => vec![],
        }
    }

    pub fn fields(&self) -> Vec<FieldRedNode> {
        match &self.inner.kind {
            GreenDeclKind::TypeStruct { fields, .. } => fields
                .iter()
                .map(|f| FieldRedNode {
                    span: self.child_span(f),
                    green: Arc::clone(&f.node),
                })
                .collect(),
            _ => vec![],
        }
    }

    pub fn ctors(&self) -> Vec<CtorRedNode> {
        match &self.inner.kind {
            GreenDeclKind::ADT { ctors, .. } => ctors
                .iter()
                .map(|c| CtorRedNode {
                    span: self.child_span(c),
                    green: Arc::clone(&c.node),
                })
                .collect(),
            _ => vec![],
        }
    }

    pub fn methods(&self) -> Vec<MethodRedNode> {
        match &self.inner.kind {
            GreenDeclKind::Abstract { methods, .. } => methods
                .iter()
                .map(|m| MethodRedNode {
                    span: self.child_span(m),
                    green: Arc::clone(&m.node),
                })
                .collect(),
            _ => vec![],
        }
    }

    pub fn generic_vars(&self) -> Vec<GenericVarRedNode> {
        let vars: Option<&Vec<GreenChild<GreenGenericVar>>> = match &self.inner.kind {
            GreenDeclKind::Abstract { generic_vars, .. }
            | GreenDeclKind::TypeStruct { generic_vars, .. }
            | GreenDeclKind::TypeAlias { generic_vars, .. }
            | GreenDeclKind::ADT { generic_vars, .. } => Some(generic_vars),
            _ => None,
        };
        vars.map(|v| {
            v.iter()
                .map(|gv| GenericVarRedNode {
                    span: self.child_span(gv),
                    green: Arc::clone(&gv.node),
                })
                .collect()
        })
            .unwrap_or_default()
    }

    pub fn annotations(&self) -> Vec<AnnotationRedNode> {
        self.inner
            .annotations
            .iter()
            .map(|a| AnnotationRedNode {
                span: self.child_span(a),
                green: Arc::clone(&a.node),
            })
            .collect()
    }

}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ParamRedNode {
    pub span: Span,
    pub green: Arc<GreenParam>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Visibility {
    Private,
    Public,
    PublicExternal,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenRequire {
    pub path: Vec<GreenChild<String>>,
    pub only: Vec<GreenChild<String>>,
    pub is_open: bool, // 将被导入模块的顶层声明塞入当前模块的中(不递归展开)
    pub text_len: TextLen,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct RequireRedNode {
    pub span: Span,
    pub green: Arc<GreenRequire>,
}


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GreenFileUnit {
    pub name: GreenChild<String>,
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
pub struct CrateAst {
    pub external_requires: Vec<RequireRedNode>,
    pub file_units: Vec<FileRedUnit>,
}