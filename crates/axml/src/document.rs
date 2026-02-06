//! In-memory AXML model plus text-XML rendering and (functional) re-encoding.

use crate::string_pool::StringPool;
use crate::types::{chunk, value_type, ResValue, DIMENSION_UNITS, FRACTION_UNITS, RADIX_MULTS};
use crate::{Error, RefResolver};
use byteorder::{LittleEndian as LE, WriteBytesExt};
use std::collections::BTreeMap;

/// One AXML attribute on a start-element.
#[derive(Debug, Clone)]
pub struct Attribute {
    pub ns: Option<String>,
    pub name: String,
    /// Resource id of a well-known (framework) attribute name, if any.
    pub name_resource_id: Option<u32>,
    /// The original raw string value, when the attribute carried one.
    pub raw_value: Option<String>,
    pub typed: ResValue,
}

/// A SAX-style AXML event.
#[derive(Debug, Clone)]
pub enum XmlEvent {
    StartNamespace {
        line: u32,
        prefix: Option<String>,
        uri: String,
    },
    EndNamespace {
        line: u32,
        prefix: Option<String>,
        uri: String,
    },
    StartElement {
        line: u32,
        ns: Option<String>,
        name: String,
        attributes: Vec<Attribute>,
    },
    EndElement {
        line: u32,
        ns: Option<String>,
        name: String,
    },
    CData {
        line: u32,
        text: String,
    },
}

/// A parsed binary XML document.
#[derive(Debug, Clone, Default)]
pub struct AxmlDocument {
    pub string_pool: StringPool,
    /// Resource-id map: parallel to the first N string-pool entries.
    pub resource_map: Vec<u32>,
    pub events: Vec<XmlEvent>,
}

impl AxmlDocument {
    /// Render the document as indented text XML.
    pub fn to_xml(&self, resolver: &dyn RefResolver) -> String {
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
        // prefix -> uri, captured so attributes/elements can render `android:`.
        let mut ns_map: BTreeMap<String, String> = BTreeMap::new();
        // Namespaces declared on the very next element get emitted as xmlns:* attrs.
        let mut pending_ns: Vec<(Option<String>, String)> = Vec::new();
        let mut depth: usize = 0;

        for ev in &self.events {
            match ev {
                XmlEvent::StartNamespace { prefix, uri, .. } => {
                    if let Some(p) = prefix {
                        ns_map.insert(uri.clone(), p.clone());
                    }
                    pending_ns.push((prefix.clone(), uri.clone()));
                }
                XmlEvent::EndNamespace { .. } => {}
                XmlEvent::StartElement {
                    ns,
                    name,
                    attributes,
                    ..
                } => {
                    let indent = "    ".repeat(depth);
                    let tag = qualify(ns.as_deref(), name, &ns_map);
                    out.push_str(&indent);
                    out.push('<');
                    out.push_str(&tag);

                    for (prefix, uri) in pending_ns.drain(..) {
                        match prefix {
                            Some(p) => out.push_str(&format!("\n{}    xmlns:{}=\"{}\"", indent, p, uri)),
                            None => out.push_str(&format!("\n{}    xmlns=\"{}\"", indent, uri)),
                        }
                    }

                    for attr in attributes {
                        let aname = qualify(attr.ns.as_deref(), &attr.name, &ns_map);
                        let value = self.format_value(attr, resolver);
                        out.push_str(&format!(" {}=\"{}\"", aname, xml_escape(&value)));
                    }
                    out.push_str(">\n");
                    depth += 1;
                }
                XmlEvent::EndElement { ns, name, .. } => {
                    depth = depth.saturating_sub(1);
                    let indent = "    ".repeat(depth);
                    let tag = qualify(ns.as_deref(), name, &ns_map);
                    out.push_str(&format!("{}</{}>\n", indent, tag));
                }
                XmlEvent::CData { text, .. } => {
                    let indent = "    ".repeat(depth);
                    out.push_str(&format!("{}{}\n", indent, xml_escape(text)));
                }
            }
        }
        out
    }

    /// Format an attribute's value as readable text.
    fn format_value(&self, attr: &Attribute, resolver: &dyn RefResolver) -> String {
        // A raw string value present in the pool wins for TYPE_STRING.
        if attr.typed.data_type == value_type::TYPE_STRING {
            if let Some(s) = &attr.raw_value {
                return s.clone();
            }
            if let Some(s) = self.string_pool.get(attr.typed.data) {
                return s.to_string();
            }
        }
        format_typed(&attr.typed, resolver)
    }

    /// Re-encode this document into AXML bytes (functional, not byte-identical).
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        // 1. Build a fresh string pool: attribute-name strings (with resource ids)
        //    first so the resource map lines up, then everything else.
        let mut pool = StringPool {
            utf8: true,
            ..Default::default()
        };
        let mut res_map: Vec<u32> = Vec::new();

        // Collect attribute names with their resource ids, de-duplicated by name.
        let mut name_to_id: BTreeMap<String, u32> = BTreeMap::new();
        for ev in &self.events {
            if let XmlEvent::StartElement { attributes, .. } = ev {
                for a in attributes {
                    if let Some(id) = a.name_resource_id {
                        name_to_id.entry(a.name.clone()).or_insert(id);
                    }
                }
            }
        }
        for (name, id) in &name_to_id {
            pool.intern(name);
            res_map.push(*id);
        }

        // Helper to intern a string and return its index, or NO_ENTRY for None.
        let intern_opt = |pool: &mut StringPool, s: &Option<String>| -> u32 {
            match s {
                Some(v) => pool.intern(v),
                None => crate::types::NO_ENTRY,
            }
        };

        // First pass: intern every string the events reference.
        for ev in &self.events {
            match ev {
                XmlEvent::StartNamespace { prefix, uri, .. }
                | XmlEvent::EndNamespace { prefix, uri, .. } => {
                    if let Some(p) = prefix {
                        pool.intern(p);
                    }
                    pool.intern(uri);
                }
                XmlEvent::StartElement {
                    ns,
                    name,
                    attributes,
                    ..
                } => {
                    if let Some(n) = ns {
                        pool.intern(n);
                    }
                    pool.intern(name);
                    for a in attributes {
                        if let Some(n) = &a.ns {
                            pool.intern(n);
                        }
                        pool.intern(&a.name);
                        if let Some(rv) = &a.raw_value {
                            pool.intern(rv);
                        }
                    }
                }
                XmlEvent::EndElement { ns, name, .. } => {
                    if let Some(n) = ns {
                        pool.intern(n);
                    }
                    pool.intern(name);
                }
                XmlEvent::CData { text, .. } => {
                    pool.intern(text);
                }
            }
        }

        // 2. Emit the body chunks.
        let mut body: Vec<u8> = Vec::new();

        // Resource map chunk.
        if !res_map.is_empty() {
            let size = 8 + res_map.len() * 4;
            body.write_u16::<LE>(chunk::RES_XML_RESOURCE_MAP_TYPE).unwrap();
            body.write_u16::<LE>(8).unwrap();
            body.write_u32::<LE>(size as u32).unwrap();
            for id in &res_map {
                body.write_u32::<LE>(*id).unwrap();
            }
        }

        for ev in &self.events {
            match ev {
                XmlEvent::StartNamespace { line, prefix, uri } => {
                    write_namespace(
                        &mut body,
                        chunk::RES_XML_START_NAMESPACE_TYPE,
                        *line,
                        intern_opt(&mut pool, prefix),
                        pool.intern(uri),
                    );
                }
                XmlEvent::EndNamespace { line, prefix, uri } => {
                    write_namespace(
                        &mut body,
                        chunk::RES_XML_END_NAMESPACE_TYPE,
                        *line,
                        intern_opt(&mut pool, prefix),
                        pool.intern(uri),
                    );
                }
                XmlEvent::StartElement {
                    line,
                    ns,
                    name,
                    attributes,
                } => {
                    let ns_idx = intern_opt(&mut pool, ns);
                    let name_idx = pool.intern(name);
                    write_start_element(&mut body, &mut pool, *line, ns_idx, name_idx, attributes);
                }
                XmlEvent::EndElement { line, ns, name } => {
                    let ns_idx = intern_opt(&mut pool, ns);
                    let name_idx = pool.intern(name);
                    write_end_element(&mut body, *line, ns_idx, name_idx);
                }
                XmlEvent::CData { line, text } => {
                    let idx = pool.intern(text);
                    write_cdata(&mut body, *line, idx);
                }
            }
        }

        // 3. Assemble: outer RES_XML header + string pool + body.
        let pool_bytes = pool.encode();
        let total = 8 + pool_bytes.len() + body.len();
        let mut out = Vec::with_capacity(total);
        out.write_u16::<LE>(chunk::RES_XML_TYPE).unwrap();
        out.write_u16::<LE>(8).unwrap();
        out.write_u32::<LE>(total as u32).unwrap();
        out.extend_from_slice(&pool_bytes);
        out.extend_from_slice(&body);
        Ok(out)
    }
}

fn write_namespace(out: &mut Vec<u8>, ty: u16, line: u32, prefix: u32, uri: u32) {
    out.write_u16::<LE>(ty).unwrap();
    out.write_u16::<LE>(16).unwrap();
    out.write_u32::<LE>(24).unwrap();
    out.write_u32::<LE>(line).unwrap();
    out.write_u32::<LE>(crate::types::NO_ENTRY).unwrap(); // comment
    out.write_u32::<LE>(prefix).unwrap();
    out.write_u32::<LE>(uri).unwrap();
}

fn write_start_element(
    out: &mut Vec<u8>,
    pool: &mut StringPool,
    line: u32,
    ns: u32,
    name: u32,
    attributes: &[Attribute],
) {
    let attr_count = attributes.len();
    let size = 16 + 20 + attr_count * 20;
    out.write_u16::<LE>(chunk::RES_XML_START_ELEMENT_TYPE).unwrap();
    out.write_u16::<LE>(16).unwrap();
    out.write_u32::<LE>(size as u32).unwrap();
    out.write_u32::<LE>(line).unwrap();
    out.write_u32::<LE>(crate::types::NO_ENTRY).unwrap(); // comment
    out.write_u32::<LE>(ns).unwrap();
    out.write_u32::<LE>(name).unwrap();
    out.write_u16::<LE>(20).unwrap(); // attributeStart
    out.write_u16::<LE>(20).unwrap(); // attributeSize
    out.write_u16::<LE>(attr_count as u16).unwrap();
    out.write_u16::<LE>(0).unwrap(); // idIndex
    out.write_u16::<LE>(0).unwrap(); // classIndex
    out.write_u16::<LE>(0).unwrap(); // styleIndex

    for a in attributes {
        let ns_idx = a.ns.as_ref().map(|s| pool.intern(s)).unwrap_or(crate::types::NO_ENTRY);
        let name_idx = pool.intern(&a.name);
        let raw_idx = if a.typed.data_type == value_type::TYPE_STRING {
            a.raw_value
                .as_ref()
                .map(|s| pool.intern(s))
                .unwrap_or(a.typed.data)
        } else {
            a.raw_value
                .as_ref()
                .map(|s| pool.intern(s))
                .unwrap_or(crate::types::NO_ENTRY)
        };
        out.write_u32::<LE>(ns_idx).unwrap();
        out.write_u32::<LE>(name_idx).unwrap();
        out.write_u32::<LE>(raw_idx).unwrap();
        out.write_u16::<LE>(a.typed.size).unwrap();
        out.write_u8(a.typed.res0).unwrap();
        out.write_u8(a.typed.data_type).unwrap();
        out.write_u32::<LE>(a.typed.data).unwrap();
    }
}

fn write_end_element(out: &mut Vec<u8>, line: u32, ns: u32, name: u32) {
    out.write_u16::<LE>(chunk::RES_XML_END_ELEMENT_TYPE).unwrap();
    out.write_u16::<LE>(16).unwrap();
    out.write_u32::<LE>(24).unwrap();
    out.write_u32::<LE>(line).unwrap();
    out.write_u32::<LE>(crate::types::NO_ENTRY).unwrap();
    out.write_u32::<LE>(ns).unwrap();
    out.write_u32::<LE>(name).unwrap();
}

fn write_cdata(out: &mut Vec<u8>, line: u32, data: u32) {
    out.write_u16::<LE>(chunk::RES_XML_CDATA_TYPE).unwrap();
    out.write_u16::<LE>(16).unwrap();
    out.write_u32::<LE>(28).unwrap();
    out.write_u32::<LE>(line).unwrap();
    out.write_u32::<LE>(crate::types::NO_ENTRY).unwrap();
    out.write_u32::<LE>(data).unwrap();
    // typed value (TYPE_STRING pointing at same index)
    out.write_u16::<LE>(ResValue::SIZE).unwrap();
    out.write_u8(0).unwrap();
    out.write_u8(value_type::TYPE_STRING).unwrap();
    out.write_u32::<LE>(data).unwrap();
}

/// Qualify a local name with its namespace prefix (e.g. `android:label`).
fn qualify(ns: Option<&str>, name: &str, ns_map: &BTreeMap<String, String>) -> String {
    match ns.and_then(|uri| ns_map.get(uri)) {
        Some(prefix) => format!("{}:{}", prefix, name),
        None => name.to_string(),
    }
}

/// Format a typed `Res_value` per its data type.
pub fn format_typed(v: &ResValue, resolver: &dyn RefResolver) -> String {
    use value_type::*;
    match v.data_type {
        TYPE_NULL => {
            if v.data == crate::types::value_type::DATA_NULL_EMPTY {
                String::new()
            } else {
                "@null".to_string()
            }
        }
        TYPE_REFERENCE | TYPE_DYNAMIC_REFERENCE => {
            if v.data == 0 {
                "@null".to_string()
            } else if let Some(name) = resolver.resolve(v.data) {
                name
            } else {
                format!("@0x{:08x}", v.data)
            }
        }
        TYPE_ATTRIBUTE | TYPE_DYNAMIC_ATTRIBUTE => format!("?0x{:08x}", v.data),
        TYPE_STRING => String::new(),
        TYPE_FLOAT => format!("{}", f32::from_bits(v.data)),
        TYPE_DIMENSION => format_complex(v.data, false),
        TYPE_FRACTION => format_complex(v.data, true),
        TYPE_INT_DEC => format!("{}", v.data as i32),
        TYPE_INT_HEX => format!("0x{:x}", v.data),
        TYPE_INT_BOOLEAN => {
            if v.data != 0 {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        TYPE_INT_COLOR_ARGB8 => format!("#{:08x}", v.data),
        TYPE_INT_COLOR_RGB8 => format!("#{:06x}", v.data & 0x00ff_ffff),
        TYPE_INT_COLOR_ARGB4 => format!("#{:08x}", v.data),
        TYPE_INT_COLOR_RGB4 => format!("#{:06x}", v.data & 0x00ff_ffff),
        _ => format!("0x{:08x}", v.data),
    }
}

/// Format a complex (dimension/fraction) value.
fn format_complex(data: u32, fraction: bool) -> String {
    const COMPLEX_MANTISSA_SHIFT: u32 = 8;
    const COMPLEX_MANTISSA_MASK: u32 = 0xffffff;
    const COMPLEX_RADIX_SHIFT: u32 = 4;
    const COMPLEX_RADIX_MASK: u32 = 0x3;
    const COMPLEX_UNIT_MASK: u32 = 0xf;

    let mantissa = (data >> COMPLEX_MANTISSA_SHIFT) & COMPLEX_MANTISSA_MASK;
    let radix = (data >> COMPLEX_RADIX_SHIFT) & COMPLEX_RADIX_MASK;
    let unit = data & COMPLEX_UNIT_MASK;
    let value = (mantissa as f32) * RADIX_MULTS[radix as usize];

    let suffix = if fraction {
        FRACTION_UNITS
            .get(unit as usize)
            .copied()
            .unwrap_or("")
    } else {
        let s = DIMENSION_UNITS.get(unit as usize).copied().unwrap_or("");
        // Modern apktool emits `dp` instead of legacy `dip`.
        if s == "dip" {
            "dp"
        } else {
            s
        }
    };
    format!("{}{}", trim_float(value), suffix)
}

fn trim_float(v: f32) -> String {
    let s = format!("{:.4}", v);
    let t = s.trim_end_matches('0').trim_end_matches('.');
    t.to_string()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
