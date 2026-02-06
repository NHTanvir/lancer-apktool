//! Minimal text-XML → [`AxmlDocument`] parser.
//!
//! This is the inverse of [`AxmlDocument::to_xml`] for the common manifest
//! shape. It is deliberately small: it understands declarations, elements,
//! attributes (with `prefix:` namespaces), self-closing tags, and text nodes,
//! and infers a typed `Res_value` for booleans / ints / hex / colors, falling
//! back to `TYPE_STRING`. References (`@id/...`) are kept as strings unless a
//! numeric id is supplied; the full resolution path runs through `apktool-core`.

use crate::document::{Attribute, AxmlDocument, XmlEvent};
use crate::types::{value_type, ResValue};
use crate::Error;
use std::collections::BTreeMap;

pub fn parse_text(xml: &str) -> Result<AxmlDocument, Error> {
    let mut doc = AxmlDocument::default();
    doc.string_pool.utf8 = true;

    // prefix -> uri, so we can split `android:name` back into (uri, name).
    let mut ns_uris: BTreeMap<String, String> = BTreeMap::new();
    let bytes = xml.as_bytes();
    let mut i = 0usize;
    let line = 0u32;

    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Comment / declaration / processing instruction.
            if xml[i..].starts_with("<?") {
                i = find(xml, i, "?>").map(|p| p + 2).unwrap_or(bytes.len());
                continue;
            }
            if xml[i..].starts_with("<!--") {
                i = find(xml, i, "-->").map(|p| p + 3).unwrap_or(bytes.len());
                continue;
            }
            let end = find(xml, i, ">").ok_or_else(|| Error::BadXml("unterminated tag".into()))?;
            let inner = &xml[i + 1..end];
            i = end + 1;

            if let Some(rest) = inner.strip_prefix('/') {
                // End element.
                let (ns, name) = split_ns(rest.trim(), &ns_uris);
                doc.events.push(XmlEvent::EndElement { line, ns, name });
                continue;
            }

            let self_closing = inner.trim_end().ends_with('/');
            let inner = inner.trim_end().trim_end_matches('/').trim();
            let (tag, attrs) = split_tag(inner);
            let (ns, name) = split_ns(tag, &ns_uris);

            // First, harvest xmlns declarations so attributes can be qualified.
            let mut pending_ns: Vec<(Option<String>, String)> = Vec::new();
            let mut attributes = Vec::new();
            for (k, v) in parse_attrs(attrs) {
                if k == "xmlns" {
                    ns_uris.insert(String::new(), v.clone());
                    pending_ns.push((None, v));
                } else if let Some(prefix) = k.strip_prefix("xmlns:") {
                    ns_uris.insert(prefix.to_string(), v.clone());
                    pending_ns.push((Some(prefix.to_string()), v));
                } else {
                    let (ans, aname) = split_ns(&k, &ns_uris);
                    attributes.push(Attribute {
                        ns: ans,
                        name: aname,
                        name_resource_id: None,
                        raw_value: Some(v.clone()),
                        typed: infer_value(&v, &mut doc),
                    });
                }
            }

            for (prefix, uri) in pending_ns {
                doc.events.push(XmlEvent::StartNamespace {
                    line,
                    prefix,
                    uri,
                });
            }
            doc.events.push(XmlEvent::StartElement {
                line,
                ns,
                name: name.clone(),
                attributes,
            });
            if self_closing {
                doc.events.push(XmlEvent::EndElement {
                    line,
                    ns: None,
                    name,
                });
            }
        } else {
            // Text node.
            let end = find(xml, i, "<").unwrap_or(bytes.len());
            let text = xml[i..end].trim();
            if !text.is_empty() {
                doc.events.push(XmlEvent::CData {
                    line,
                    text: unescape(text),
                });
            }
            i = end;
        }
    }

    Ok(doc)
}

fn infer_value(v: &str, doc: &mut AxmlDocument) -> ResValue {
    let t = v.trim();
    if t == "true" {
        return ResValue::new(value_type::TYPE_INT_BOOLEAN, 0xffff_ffff);
    }
    if t == "false" {
        return ResValue::new(value_type::TYPE_INT_BOOLEAN, 0);
    }
    if let Some(hex) = t.strip_prefix("0x") {
        if let Ok(n) = u32::from_str_radix(hex, 16) {
            return ResValue::new(value_type::TYPE_INT_HEX, n);
        }
    }
    if let Some(color) = t.strip_prefix('#') {
        if let Ok(n) = u32::from_str_radix(color, 16) {
            let dt = if color.len() <= 6 {
                value_type::TYPE_INT_COLOR_RGB8
            } else {
                value_type::TYPE_INT_COLOR_ARGB8
            };
            return ResValue::new(dt, n);
        }
    }
    if let Ok(n) = t.parse::<i32>() {
        return ResValue::new(value_type::TYPE_INT_DEC, n as u32);
    }
    // Fall back to a string value pointing at the interned raw text.
    let idx = doc.string_pool.intern(t);
    ResValue::new(value_type::TYPE_STRING, idx)
}

fn split_tag(inner: &str) -> (&str, &str) {
    match inner.find(char::is_whitespace) {
        Some(p) => (&inner[..p], &inner[p..]),
        None => (inner, ""),
    }
}

fn split_ns(qname: &str, ns_uris: &BTreeMap<String, String>) -> (Option<String>, String) {
    match qname.split_once(':') {
        Some((prefix, local)) => {
            let uri = ns_uris.get(prefix).cloned();
            (uri, local.to_string())
        }
        None => (None, qname.to_string()),
    }
}

fn parse_attrs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if start == i {
            break;
        }
        let key = s[start..i].to_string();
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'=') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let quote = bytes[i];
        if quote == b'"' || quote == b'\'' {
            i += 1;
            let vstart = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            let val = unescape(&s[vstart..i]);
            i += 1;
            out.push((key, val));
        }
    }
    out
}

fn find(hay: &str, from: usize, needle: &str) -> Option<usize> {
    hay[from..].find(needle).map(|p| p + from)
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
