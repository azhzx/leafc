#[derive(Debug, PartialEq, Eq, Clone, Hash, Copy)]
pub struct CrateId(pub usize);

#[derive(Debug, PartialEq, Eq, Clone, Hash, Copy)]
pub struct FileId(pub usize);

#[derive(Debug, PartialEq, Eq, Clone, Hash, Copy)]
pub struct VersionId(pub usize);

#[derive(Debug, PartialEq, Eq, Clone, Hash, Copy)]
pub struct HirDeclId(pub usize);

#[derive(Debug, PartialEq, Eq, Clone, Hash, Copy)]
pub struct HirExprId(pub usize);

#[derive(Debug, PartialEq, Eq, Clone, Hash, Copy)]
pub struct ScopeId(pub usize);

#[derive(Debug, PartialEq, Eq, Clone, Hash, Copy)]
pub struct SymId(pub usize);
