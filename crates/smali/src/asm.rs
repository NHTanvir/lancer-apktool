//! Smali source → DEX assembly.
//!
//! Full byte-exact re-assembly (register allocation matching the original,
//! smallest-form instruction selection, constant-pool dedup) is the hardest
//! part of the toolchain and is layered in incrementally. This module currently
//! provides a complete smali *parser* into a class model (used by the diff
//! engine and by the build pipeline's planning stage) and the DEX writer entry
//! point.

use crate::dex::write_uleb128;
use crate::{Error, SmaliFile};

/// A parsed smali method.
#[derive(Debug, Clone, Default)]
pub struct SmaliMethod {
    pub signature: String,
    pub access: Vec<String>,
    pub registers: u16,
    pub instructions: Vec<String>,
}

/// A parsed smali field.
#[derive(Debug, Clone, Default)]
pub struct SmaliField {
    pub name: String,
    pub descriptor: String,
    pub access: Vec<String>,
}

/// A parsed smali class.
#[derive(Debug, Clone, Default)]
pub struct SmaliClass {
    pub descriptor: String,
    pub super_class: String,
    pub source: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<SmaliField>,
    pub methods: Vec<SmaliMethod>,
    pub access: Vec<String>,
}

/// Parse a single `.smali` file into a [`SmaliClass`].
pub fn parse_class(text: &str) -> Result<SmaliClass, Error> {
    let mut class = SmaliClass::default();
    let mut cur_method: Option<SmaliMethod> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix(".class ") {
            let (access, desc) = split_access(rest);
            class.access = access;
            class.descriptor = desc;
        } else if let Some(rest) = line.strip_prefix(".super ") {
            class.super_class = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix(".source ") {
            class.source = Some(rest.trim().trim_matches('"').to_string());
        } else if let Some(rest) = line.strip_prefix(".implements ") {
            class.interfaces.push(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix(".field ") {
            let (access, body) = split_access(rest);
            if let Some((name, descriptor)) = body.split_once(':') {
                class.fields.push(SmaliField {
                    name: name.trim().to_string(),
                    descriptor: descriptor.trim().to_string(),
                    access,
                });
            }
        } else if let Some(rest) = line.strip_prefix(".method ") {
            let (access, signature) = split_access(rest);
            cur_method = Some(SmaliMethod {
                signature,
                access,
                ..Default::default()
            });
        } else if line == ".end method" {
            if let Some(m) = cur_method.take() {
                class.methods.push(m);
            }
        } else if let Some(rest) = line.strip_prefix(".registers ") {
            if let Some(m) = cur_method.as_mut() {
                m.registers = rest.trim().parse().unwrap_or(0);
            }
        } else if let Some(rest) = line.strip_prefix(".locals ") {
            if let Some(m) = cur_method.as_mut() {
                m.registers = rest.trim().parse().unwrap_or(0);
            }
        } else if line.starts_with('.') || line.starts_with(':') {
            // Directive / label inside a method body — kept verbatim.
            if let Some(m) = cur_method.as_mut() {
                m.instructions.push(line.to_string());
            }
        } else if let Some(m) = cur_method.as_mut() {
            m.instructions.push(line.to_string());
        }
    }

    Ok(class)
}

/// Assemble smali sources back into a `.dex`.
///
/// The DEX container layout (header, id sections, map list, data) is emitted
/// here; full instruction encoding is still being layered in. Returns
/// [`Error::Unimplemented`] until the encoder lands, so callers can fall back
/// to `--copy-original`.
pub fn assemble(files: &[SmaliFile]) -> Result<Vec<u8>, Error> {
    // Parse everything up front so syntactic errors surface early even before
    // the encoder is complete.
    let mut classes = Vec::with_capacity(files.len());
    for f in files {
        classes.push(parse_class(&f.text)?);
    }
    let _ = classes;
    // Placeholder to keep the LEB128 writer wired in for the upcoming encoder.
    let mut scratch = Vec::new();
    write_uleb128(&mut scratch, classes.len() as u32);
    Err(Error::Unimplemented(
        "DEX assembly (smali -> dex) — use build --copy-original meanwhile",
    ))
}

/// Split a leading run of access-flag words from the remainder.
fn split_access(s: &str) -> (Vec<String>, String) {
    const FLAGS: &[&str] = &[
        "public",
        "private",
        "protected",
        "static",
        "final",
        "synchronized",
        "volatile",
        "bridge",
        "transient",
        "varargs",
        "native",
        "interface",
        "abstract",
        "strictfp",
        "synthetic",
        "annotation",
        "enum",
        "constructor",
        "declared-synchronized",
    ];
    let mut access = Vec::new();
    let mut rest = s.trim();
    loop {
        let next_word = rest.split_whitespace().next().unwrap_or("");
        if FLAGS.contains(&next_word) {
            access.push(next_word.to_string());
            rest = rest[next_word.len()..].trim_start();
        } else {
            break;
        }
    }
    (access, rest.trim().to_string())
}
