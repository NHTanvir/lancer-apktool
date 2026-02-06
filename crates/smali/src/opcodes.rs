//! Dalvik opcode names and instruction widths.
//!
//! Widths are in 16-bit code units. Payload pseudo-instructions
//! (`packed-switch`, `sparse-switch`, `fill-array-data`) are variable and are
//! sized by the disassembler from their bodies; everything else is a fixed
//! width determined solely by the opcode.

/// Opcode mnemonics indexed by the low byte of the first code unit.
pub const NAMES: [&str; 256] = [
    "nop", "move", "move/from16", "move/16", "move-wide", "move-wide/from16", "move-wide/16",
    "move-object", "move-object/from16", "move-object/16", "move-result", "move-result-wide",
    "move-result-object", "move-exception", "return-void", "return", "return-wide",
    "return-object", "const/4", "const/16", "const", "const/high16", "const-wide/16",
    "const-wide/32", "const-wide", "const-wide/high16", "const-string", "const-string/jumbo",
    "const-class", "monitor-enter", "monitor-exit", "check-cast", "instance-of", "array-length",
    "new-instance", "new-array", "filled-new-array", "filled-new-array/range", "fill-array-data",
    "throw", "goto", "goto/16", "goto/32", "packed-switch", "sparse-switch", "cmpl-float",
    "cmpg-float", "cmpl-double", "cmpg-double", "cmp-long", "if-eq", "if-ne", "if-lt", "if-ge",
    "if-gt", "if-le", "if-eqz", "if-nez", "if-ltz", "if-gez", "if-gtz", "if-lez",
    "unused-3e", "unused-3f", "unused-40", "unused-41", "unused-42", "unused-43",
    "aget", "aget-wide", "aget-object", "aget-boolean", "aget-byte", "aget-char", "aget-short",
    "aput", "aput-wide", "aput-object", "aput-boolean", "aput-byte", "aput-char", "aput-short",
    "iget", "iget-wide", "iget-object", "iget-boolean", "iget-byte", "iget-char", "iget-short",
    "iput", "iput-wide", "iput-object", "iput-boolean", "iput-byte", "iput-char", "iput-short",
    "sget", "sget-wide", "sget-object", "sget-boolean", "sget-byte", "sget-char", "sget-short",
    "sput", "sput-wide", "sput-object", "sput-boolean", "sput-byte", "sput-char", "sput-short",
    "invoke-virtual", "invoke-super", "invoke-direct", "invoke-static", "invoke-interface",
    "unused-73", "invoke-virtual/range", "invoke-super/range", "invoke-direct/range",
    "invoke-static/range", "invoke-interface/range", "unused-79", "unused-7a", "neg-int",
    "not-int", "neg-long", "not-long", "neg-float", "neg-double", "int-to-long", "int-to-float",
    "int-to-double", "long-to-int", "long-to-float", "long-to-double", "float-to-int",
    "float-to-long", "float-to-double", "double-to-int", "double-to-long", "double-to-float",
    "int-to-byte", "int-to-char", "int-to-short", "add-int", "sub-int", "mul-int", "div-int",
    "rem-int", "and-int", "or-int", "xor-int", "shl-int", "shr-int", "ushr-int", "add-long",
    "sub-long", "mul-long", "div-long", "rem-long", "and-long", "or-long", "xor-long", "shl-long",
    "shr-long", "ushr-long", "add-float", "sub-float", "mul-float", "div-float", "rem-float",
    "add-double", "sub-double", "mul-double", "div-double", "rem-double", "add-int/2addr",
    "sub-int/2addr", "mul-int/2addr", "div-int/2addr", "rem-int/2addr", "and-int/2addr",
    "or-int/2addr", "xor-int/2addr", "shl-int/2addr", "shr-int/2addr", "ushr-int/2addr",
    "add-long/2addr", "sub-long/2addr", "mul-long/2addr", "div-long/2addr", "rem-long/2addr",
    "and-long/2addr", "or-long/2addr", "xor-long/2addr", "shl-long/2addr", "shr-long/2addr",
    "ushr-long/2addr", "add-float/2addr", "sub-float/2addr", "mul-float/2addr", "div-float/2addr",
    "rem-float/2addr", "add-double/2addr", "sub-double/2addr", "mul-double/2addr",
    "div-double/2addr", "rem-double/2addr", "add-int/lit16", "rsub-int", "mul-int/lit16",
    "div-int/lit16", "rem-int/lit16", "and-int/lit16", "or-int/lit16", "xor-int/lit16",
    "add-int/lit8", "rsub-int/lit8", "mul-int/lit8", "div-int/lit8", "rem-int/lit8",
    "and-int/lit8", "or-int/lit8", "xor-int/lit8", "shl-int/lit8", "shr-int/lit8",
    "ushr-int/lit8", "unused-e3", "unused-e4", "unused-e5", "unused-e6", "unused-e7", "unused-e8",
    "unused-e9", "unused-ea", "unused-eb", "unused-ec", "unused-ed", "unused-ee", "unused-ef",
    "unused-f0", "unused-f1", "unused-f2", "unused-f3", "unused-f4", "unused-f5", "unused-f6",
    "unused-f7", "unused-f8", "unused-f9", "invoke-polymorphic", "invoke-polymorphic/range",
    "invoke-custom", "invoke-custom/range", "const-method-handle", "const-method-type",
];

pub fn name(op: u8) -> &'static str {
    NAMES[op as usize]
}

/// Fixed instruction width (in 16-bit code units) for `op`.
///
/// Payload pseudo-ops (opcode `0x00` with a non-zero second unit) are sized by
/// the caller; here `nop` reports width 1.
pub fn instr_width(op: u8) -> usize {
    match op {
        // Width 1 (one code unit).
        0x00..=0x01 | 0x04 | 0x07 | 0x0a..=0x12 | 0x1d..=0x1e | 0x21 | 0x27 | 0x28 => 1,
        0x2d..=0x31 => 2,
        // move/from16, move-object/from16, etc.
        0x02 | 0x05 | 0x08 => 2,
        0x03 | 0x06 | 0x09 => 3,
        0x13 | 0x15 | 0x16 | 0x19 => 2,
        0x14 | 0x17 => 3,
        0x18 => 5,
        0x1a | 0x1c | 0x1f | 0x20 | 0x22 | 0x23 => 2,
        0x1b => 3,
        0x24..=0x26 => 3,
        0x29 => 2,
        0x2a..=0x2c => 3,
        0x32..=0x3d => 2,
        0x3e..=0x43 => 1,
        0x44..=0x6d => 2,
        0x6e..=0x72 => 3,
        0x73 => 1,
        0x74..=0x78 => 3,
        0x79..=0x7a => 1,
        0x7b..=0x8f => 1,
        0x90..=0xaf => 2,
        0xb0..=0xcf => 1,
        0xd0..=0xd7 => 2,
        0xd8..=0xe2 => 2,
        0xe3..=0xf9 => 1,
        0xfa..=0xfb => 4,
        0xfc..=0xfd => 3,
        0xfe..=0xff => 2,
    }
}

/// Compute the width of the instruction at `pc`, accounting for switch/array
/// payloads.
pub fn width_at(insns: &[u16], pc: usize) -> usize {
    let unit = insns[pc];
    let op = (unit & 0xff) as u8;
    if op == 0x00 {
        let ident = unit >> 8;
        match ident {
            0x01 => {
                // packed-switch payload: size at [pc+1]
                let size = *insns.get(pc + 1).unwrap_or(&0) as usize;
                (size * 2) + 4
            }
            0x02 => {
                // sparse-switch payload
                let size = *insns.get(pc + 1).unwrap_or(&0) as usize;
                (size * 4) + 2
            }
            0x03 => {
                // fill-array-data payload
                let element_width = *insns.get(pc + 1).unwrap_or(&0) as usize;
                let size = ((*insns.get(pc + 2).unwrap_or(&0) as usize)
                    | ((*insns.get(pc + 3).unwrap_or(&0) as usize) << 16))
                    as usize;
                let data_units = (size * element_width + 1) / 2;
                data_units + 4
            }
            _ => 1,
        }
    } else {
        instr_width(op).max(1)
    }
}
