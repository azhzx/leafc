use phf::phf_map;
use crate::scope::SymId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BuiltinType {
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F32, F64,
    Bool, Never, Ptr,
}

const BUILTIN_COUNT: usize = 13;

pub static STR_TO_BUILTIN: phf::Map<&'static str, BuiltinType> = phf_map! {
    "builtin_i8_type"    => BuiltinType::I8,
    "builtin_i16_type"   => BuiltinType::I16,
    "builtin_i32_type"   => BuiltinType::I32,
    "builtin_i64_type"   => BuiltinType::I64,
    "builtin_u8_type"    => BuiltinType::U8,
    "builtin_u16_type"   => BuiltinType::U16,
    "builtin_u32_type"   => BuiltinType::U32,
    "builtin_u64_type"   => BuiltinType::U64,
    "builtin_f32_type"   => BuiltinType::F32,
    "builtin_f64_type"   => BuiltinType::F64,
    "builtin_bool_type"  => BuiltinType::Bool,
    "builtin_never_type" => BuiltinType::Never,
    "builtin_ptr_type"   => BuiltinType::Ptr,
};


#[derive(Debug, Clone)]
pub struct LangItems {
    symbols: [Option<SymId>; BUILTIN_COUNT],
}

impl LangItems {
    pub fn new() -> Self {
        Self {
            symbols: [None; BUILTIN_COUNT],
        }
    }

    pub fn is_lang_item(&self, name: &String) -> bool {
        STR_TO_BUILTIN.contains_key(&name)
    }

    /// register builtin symbol by name
    pub fn register_builtin_symbol(&mut self, name: String, sym: SymId) {
        let ty = STR_TO_BUILTIN
            .get(&name)
            .expect("Unknown builtin type name");
        self.symbols[*ty as usize] = Some(sym);
    }

    /// register builtin symbol by enum
    pub fn register_builtin_sym_by_type(&mut self, ty: BuiltinType, sym: SymId) {
        self.symbols[ty as usize] = Some(sym);
    }

    /// get symbol by name
    pub fn get_symbol_of_lang_item(&self, name: String) -> Option<SymId> {
        let ty = STR_TO_BUILTIN.get(&name)?;
        self.symbols[*ty as usize]
    }

    /// get symbol by enum
    pub fn get_symbol_by_type(&self, ty: BuiltinType) -> Option<SymId> {
        self.symbols[ty as usize]
    }

    pub fn get_builtin_type_by_sym(&self, sym_id: SymId) -> Option<BuiltinType> {
        for (idx, &opt_sym) in self.symbols.iter().enumerate() {
            if opt_sym == Some(sym_id) {
                return Some(unsafe { std::mem::transmute(idx as u8) });
            }
        }
        None
    }
}