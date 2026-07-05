use std::mem;

pub struct SymName (String);

const HEADER: &'static str = "_m_";

pub enum SymNameType {
    ModuleName,
    TypeName,
    LocalName,
    GlobalName,
    FunName,
    AbstName
}

impl SymName {
    fn new() -> SymName {
        let header = HEADER.to_string();
        SymName(header)
    }
    fn add(&mut self, name_ty: SymNameType, s: String) {
        let prefix = match name_ty {
            SymNameType::ModuleName => "$m_",
            SymNameType::TypeName => "$t_",
            SymNameType::LocalName => "$l_",
            SymNameType::GlobalName => "$g_",
            SymNameType::FunName => "$f_",
            SymNameType::AbstName => "$a_"
        };
        self.0 += prefix;
        self.0 += s.as_str();
    }
    fn as_string(&self) -> &String {
        &self.0
    }
}