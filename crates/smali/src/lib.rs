//! `smali` — DEX ↔ Smali assembler/disassembler.
//!
//! A from-scratch port of JesusFreke's `smali`/`baksmali`. The disassembler
//! reads a `classes*.dex` and emits one `.smali` source file per class; the
//! assembler reverses that back into DEX bytecode.
//!
//! The current implementation provides a complete DEX container reader
//! (header, string/type/proto/field/method ids, class defs) and a structural
//! disassembler (class/super/interface/field/method directives). Full
//! per-instruction round-trip is layered on top in [`disasm`] / [`asm`].

pub mod asm;
pub mod disasm;
pub mod dex;
pub mod opcodes;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not a DEX file (bad magic)")]
    BadMagic,
    #[error("unsupported DEX version {0}")]
    UnsupportedVersion(u32),
    #[error("truncated DEX: {0}")]
    Truncated(&'static str),
    #[error("smali syntax error: {0}")]
    Syntax(String),
    #[error("unimplemented: {0}")]
    Unimplemented(&'static str),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// A disassembled class destined for a `.smali` file on disk.
#[derive(Debug, Clone)]
pub struct SmaliFile {
    /// Dalvik type descriptor, e.g. `Lcom/example/Foo;`.
    pub class_descriptor: String,
    /// Relative path under the smali tree, e.g. `com/example/Foo.smali`.
    pub path: String,
    /// The smali source text.
    pub text: String,
}

/// Read the DEX format version (e.g. `35`, `38`, `39`).
pub fn dex_version(dex: &[u8]) -> Result<u32, Error> {
    dex::Dex::version(dex)
}

/// Disassemble a `.dex` into one [`SmaliFile`] per class.
pub fn disassemble(dex_bytes: &[u8]) -> Result<Vec<SmaliFile>, Error> {
    let dex = dex::Dex::parse(dex_bytes)?;
    disasm::disassemble(&dex)
}

/// Assemble `.smali` sources back into a `.dex`.
pub fn assemble(files: &[SmaliFile]) -> Result<Vec<u8>, Error> {
    asm::assemble(files)
}

/// Convert a Dalvik type descriptor (`Lcom/example/Foo;`) to a smali path.
pub fn descriptor_to_path(descriptor: &str) -> String {
    let inner = descriptor
        .trim_start_matches('L')
        .trim_end_matches(';');
    format!("{}.smali", inner)
}
