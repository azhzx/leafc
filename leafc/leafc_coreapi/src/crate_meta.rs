use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct CrateManifest {
    /// [crate]
    #[serde(rename = "crate")]
    pub crate_info: CrateInfo,

    /// [dependencies]
    #[serde(default)]
    pub dependencies: HashMap<String, Dependency>,

    /// [build]
    pub build: BuildConfig,

    /// [operator.xxx]
    #[serde(default)]
    pub operator: HashMap<String, OperatorDef>,
}

/// [crate]
#[derive(Debug, Deserialize)]
pub struct CrateInfo {
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub license: Option<String>,
    pub description: Option<String>,
    pub repo: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Simple(String),
    Detailed {
        version: String,
        path: Option<String>,
    },
}

/// [build]
#[derive(Debug, Deserialize)]
pub struct BuildConfig {
    pub target: BuildTarget,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BuildTarget {
    Lib,
    Bin,
}

/// 允许作为优先级参照的内置运算符
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinOperator {
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
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OperatorKind {
    Prefix,
    Infix,
    Postfix,
}

/// [operator.xxx]
#[derive(Debug, Deserialize)]
pub struct OperatorDef {
    pub text: String,
    pub is_pub_external: bool,
    pub high_than: Option<BuiltinOperator>,
    pub less_than: Option<BuiltinOperator>,
    /// prefix/infix/postfix
    pub kind: OperatorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorityRelation {
    HigherThan(BuiltinOperator),
    LowerThan(BuiltinOperator),
}

impl OperatorDef {
    /// 互斥
    pub fn validate(&self) -> Result<(), String> {
        match (&self.high_than, &self.less_than) {
            (Some(_), Some(_)) => {
                Err("high_than and less_than are mutually exclusive; only one may be specified".to_string())
            }
            (None, None) => {
                Err("exactly one of high_than or less_than must be specified".to_string())
            }
            _ => Ok(()),
        }
    }

    pub fn priority_relation(&self) -> PriorityRelation {
        match (&self.high_than, &self.less_than) {
            (Some(op), _) => PriorityRelation::HigherThan(*op),
            (_, Some(op)) => PriorityRelation::LowerThan(*op),
            _ => unreachable!("priority configuration is not validated"),
        }
    }
}


impl CrateManifest {
    pub fn from_str(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let manifest = Self::from_str(&content)?;
        Ok(manifest)
    }
}

impl Dependency {
    pub fn version(&self) -> &str {
        match self {
            Dependency::Simple(v) => v,
            Dependency::Detailed { version, .. } => version,
        }
    }

    pub fn path(&self) -> Option<&str> {
        match self {
            Dependency::Simple(_) => None,
            Dependency::Detailed { path, .. } => path.as_deref(),
        }
    }
}