pub const LANG_BUILTIN_TYPES: [&'static str; 13] = [
    "builtin_i8_type",
    "builtin_i16_type",
    "builtin_i32_type",
    "builtin_i64_type",

    "builtin_u8_type",
    "builtin_u16_type",
    "builtin_u32_type",
    "builtin_u64_type",
    
    "builtin_f32_type",
    "builtin_f64_type",

    "builtin_bool_type",
    "builtin_never_type",
    "builtin_ptr_type",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinType {
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

    Bool,
    Never,
    Pointer,
}