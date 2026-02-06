//! `axml` — binary `AndroidManifest.xml` (AXML) encoder/decoder.
//!
//! Android stores `AndroidManifest.xml` (and every compiled `res/**/*.xml`)
//! in a binary XML format: a `ResChunk_header`-framed stream of a string pool,
//! an optional resource-id map, and namespace / element / cdata events.
//!
//! This crate streams that format both ways:
//!
//! * [`decode`] / [`parse`] — AXML bytes → readable text XML.
//! * [`encode_xml`] — text XML → AXML bytes (functional, not byte-identical).
//! * [`AxmlDocument`] — the in-memory event model used by both directions.

pub mod android_attrs;
mod document;
mod reader;
mod string_pool;
mod types;
mod xml_parse;

pub use android_attrs::attr_name;
pub use document::{Attribute, AxmlDocument, XmlEvent};
pub use string_pool::StringPool;
pub use types::{value_type, ResValue};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not a binary XML document (bad magic 0x{0:04x})")]
    BadMagic(u16),
    #[error("truncated AXML: {0}")]
    Truncated(&'static str),
    #[error("unexpected chunk type 0x{0:04x}")]
    UnexpectedChunk(u16),
    #[error("malformed text XML: {0}")]
    BadXml(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// A resolver maps a resource id (e.g. `0x7f0a0001`) to a human label
/// (e.g. `@string/app_name` or `@android:string/ok`). Supplied by the caller
/// (the `arsc` crate + framework cache); decoding works without one.
pub trait RefResolver {
    fn resolve(&self, id: u32) -> Option<String>;
    /// Resolve a well-known framework attribute id to its name (`android:label`).
    fn attribute_name(&self, _id: u32) -> Option<String> {
        None
    }
}

/// A no-op resolver: references render as raw `@0x........` hex.
pub struct NullResolver;
impl RefResolver for NullResolver {
    fn resolve(&self, _id: u32) -> Option<String> {
        None
    }
}

/// Decode AXML bytes into readable, indented text XML.
pub fn decode(bytes: &[u8]) -> Result<String, Error> {
    let doc = parse(bytes)?;
    Ok(doc.to_xml(&NullResolver))
}

/// Decode AXML bytes into readable text XML, resolving references via `resolver`.
pub fn decode_with(bytes: &[u8], resolver: &dyn RefResolver) -> Result<String, Error> {
    let doc = parse(bytes)?;
    Ok(doc.to_xml(resolver))
}

/// Parse AXML bytes into the [`AxmlDocument`] event model.
pub fn parse(bytes: &[u8]) -> Result<AxmlDocument, Error> {
    reader::parse(bytes)
}

/// Encode text XML into AXML bytes.
pub fn encode_xml(xml: &str) -> Result<Vec<u8>, Error> {
    let doc = xml_parse::parse_text(xml)?;
    doc.encode()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_pool_round_trips() {
        let mut pool = StringPool {
            utf8: true,
            ..Default::default()
        };
        pool.intern("android");
        pool.intern("http://schemas.android.com/apk/res/android");
        let bytes = pool.encode();
        let back = StringPool::parse(&bytes).unwrap();
        assert_eq!(back.strings, pool.strings);
    }

    #[test]
    fn document_encode_parse_round_trips() {
        const ANDROID_URI: &str = "http://schemas.android.com/apk/res/android";

        // Build a tiny document by hand:
        //   <manifest xmlns:android="..." android:versionName="1.0">
        //   </manifest>
        let attr = Attribute {
            ns: Some(ANDROID_URI.to_string()),
            name: "versionName".to_string(),
            name_resource_id: Some(0x0101021c),
            raw_value: Some("1.0".to_string()),
            typed: ResValue::new(value_type::TYPE_STRING, 0),
        };

        let mut doc = AxmlDocument::default();
        doc.events.push(XmlEvent::StartNamespace {
            line: 1,
            prefix: Some("android".to_string()),
            uri: ANDROID_URI.to_string(),
        });
        doc.events.push(XmlEvent::StartElement {
            line: 1,
            ns: None,
            name: "manifest".to_string(),
            attributes: vec![attr],
        });
        doc.events.push(XmlEvent::EndElement {
            line: 1,
            ns: None,
            name: "manifest".to_string(),
        });
        doc.events.push(XmlEvent::EndNamespace {
            line: 1,
            prefix: Some("android".to_string()),
            uri: ANDROID_URI.to_string(),
        });

        // Encode to AXML bytes, then parse them back.
        let bytes = doc.encode().expect("encode");
        let back = parse(&bytes).expect("parse");

        // The element name and the single attribute must survive the trip.
        let start = back
            .events
            .iter()
            .find_map(|e| match e {
                XmlEvent::StartElement {
                    name, attributes, ..
                } => Some((name.clone(), attributes.clone())),
                _ => None,
            })
            .expect("a start element");

        assert_eq!(start.0, "manifest");
        assert_eq!(start.1.len(), 1);
        let a = &start.1[0];
        assert_eq!(a.name, "versionName");
        assert_eq!(a.ns.as_deref(), Some(ANDROID_URI));
        assert_eq!(a.raw_value.as_deref(), Some("1.0"));
    }
}
