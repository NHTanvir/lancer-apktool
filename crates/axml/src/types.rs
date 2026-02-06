//! Binary XML chunk types and typed-value constants.
//!
//! Reference: AOSP `ResourceTypes.h`.

/// Chunk type identifiers (`ResChunk_header::type`).
///
/// The full set is kept for spec completeness; a few entries are reference-only.
#[allow(dead_code)]
pub mod chunk {
    pub const RES_NULL_TYPE: u16 = 0x0000;
    pub const RES_STRING_POOL_TYPE: u16 = 0x0001;
    pub const RES_TABLE_TYPE: u16 = 0x0002;
    pub const RES_XML_TYPE: u16 = 0x0003;

    // XML sub-chunks.
    pub const RES_XML_FIRST_CHUNK_TYPE: u16 = 0x0100;
    pub const RES_XML_START_NAMESPACE_TYPE: u16 = 0x0100;
    pub const RES_XML_END_NAMESPACE_TYPE: u16 = 0x0101;
    pub const RES_XML_START_ELEMENT_TYPE: u16 = 0x0102;
    pub const RES_XML_END_ELEMENT_TYPE: u16 = 0x0103;
    pub const RES_XML_CDATA_TYPE: u16 = 0x0104;
    pub const RES_XML_LAST_CHUNK_TYPE: u16 = 0x017f;
    pub const RES_XML_RESOURCE_MAP_TYPE: u16 = 0x0180;
}

/// String pool header flags.
pub mod string_pool_flags {
    /// String index is sorted by the string values (uncommon in AXML).
    pub const SORTED_FLAG: u32 = 1 << 0;
    /// Strings are stored as UTF-8 (otherwise UTF-16LE).
    pub const UTF8_FLAG: u32 = 1 << 8;
}

/// `Res_value::dataType` constants.
pub mod value_type {
    pub const TYPE_NULL: u8 = 0x00;
    pub const TYPE_REFERENCE: u8 = 0x01;
    pub const TYPE_ATTRIBUTE: u8 = 0x02;
    pub const TYPE_STRING: u8 = 0x03;
    pub const TYPE_FLOAT: u8 = 0x04;
    pub const TYPE_DIMENSION: u8 = 0x05;
    pub const TYPE_FRACTION: u8 = 0x06;
    pub const TYPE_DYNAMIC_REFERENCE: u8 = 0x07;
    pub const TYPE_DYNAMIC_ATTRIBUTE: u8 = 0x08;
    pub const TYPE_INT_DEC: u8 = 0x10;
    pub const TYPE_INT_HEX: u8 = 0x11;
    pub const TYPE_INT_BOOLEAN: u8 = 0x12;
    pub const TYPE_INT_COLOR_ARGB8: u8 = 0x1c;
    pub const TYPE_INT_COLOR_RGB8: u8 = 0x1d;
    pub const TYPE_INT_COLOR_ARGB4: u8 = 0x1e;
    pub const TYPE_INT_COLOR_RGB4: u8 = 0x1f;

    /// `TYPE_NULL` data meaning "explicitly defined as empty" (vs. absent).
    pub const DATA_NULL_EMPTY: u32 = 1;
}

/// Dimension / fraction unit suffixes, indexed by the low 4 bits of `Res_value::data`.
pub const DIMENSION_UNITS: [&str; 8] = ["px", "dip", "sp", "pt", "in", "mm", "", ""];
pub const FRACTION_UNITS: [&str; 2] = ["%", "%p"];

/// Complex value radix tables used to render dimensions / fractions.
pub const RADIX_MULTS: [f32; 4] = [
    0.003_906_25,
    3.051_757_8e-5,
    2.384_185_8e-7,
    1.862_645_1e-9,
];

/// A decoded `Res_value` (size, res0, dataType, data).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResValue {
    pub size: u16,
    pub res0: u8,
    pub data_type: u8,
    pub data: u32,
}

impl ResValue {
    pub const SIZE: u16 = 8;

    pub fn new(data_type: u8, data: u32) -> Self {
        ResValue {
            size: Self::SIZE,
            res0: 0,
            data_type,
            data,
        }
    }
}

/// Sentinel for "no reference into the string pool".
pub const NO_ENTRY: u32 = 0xffff_ffff;
