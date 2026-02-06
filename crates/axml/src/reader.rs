//! AXML byte stream → [`AxmlDocument`] event model.

use crate::document::{Attribute, AxmlDocument, XmlEvent};
use crate::string_pool::StringPool;
use crate::types::{chunk, ResValue};
use crate::Error;
use byteorder::{ByteOrder, LittleEndian as LE};

pub fn parse(bytes: &[u8]) -> Result<AxmlDocument, Error> {
    if bytes.len() < 8 {
        return Err(Error::Truncated("file header"));
    }
    let ty = LE::read_u16(&bytes[0..2]);
    if ty != chunk::RES_XML_TYPE {
        return Err(Error::BadMagic(ty));
    }

    let mut doc = AxmlDocument::default();
    let mut pos = 8usize; // skip the outer RES_XML header

    while pos + 8 <= bytes.len() {
        let ctype = LE::read_u16(&bytes[pos..pos + 2]);
        let header_size = LE::read_u16(&bytes[pos + 2..pos + 4]) as usize;
        let size = LE::read_u32(&bytes[pos + 4..pos + 8]) as usize;
        if size < 8 || pos + size > bytes.len() {
            // Tolerate trailing padding / protect-mode junk.
            break;
        }
        let chunk_bytes = &bytes[pos..pos + size];

        match ctype {
            chunk::RES_STRING_POOL_TYPE => {
                doc.string_pool = StringPool::parse(chunk_bytes)?;
            }
            chunk::RES_XML_RESOURCE_MAP_TYPE => {
                let count = (size - header_size) / 4;
                for i in 0..count {
                    let at = header_size + i * 4;
                    doc.resource_map.push(LE::read_u32(&chunk_bytes[at..at + 4]));
                }
            }
            chunk::RES_XML_START_NAMESPACE_TYPE => {
                let (line, prefix, uri) = read_namespace(chunk_bytes, &doc.string_pool);
                doc.events.push(XmlEvent::StartNamespace { line, prefix, uri });
            }
            chunk::RES_XML_END_NAMESPACE_TYPE => {
                let (line, prefix, uri) = read_namespace(chunk_bytes, &doc.string_pool);
                doc.events.push(XmlEvent::EndNamespace { line, prefix, uri });
            }
            chunk::RES_XML_START_ELEMENT_TYPE => {
                let ev = read_start_element(chunk_bytes, &doc)?;
                doc.events.push(ev);
            }
            chunk::RES_XML_END_ELEMENT_TYPE => {
                let line = LE::read_u32(&chunk_bytes[8..12]);
                let ns = string_ref(&doc.string_pool, LE::read_u32(&chunk_bytes[16..20]));
                let name = string_ref(&doc.string_pool, LE::read_u32(&chunk_bytes[20..24]))
                    .unwrap_or_default();
                doc.events.push(XmlEvent::EndElement { line, ns, name });
            }
            chunk::RES_XML_CDATA_TYPE => {
                let line = LE::read_u32(&chunk_bytes[8..12]);
                let text = string_ref(&doc.string_pool, LE::read_u32(&chunk_bytes[16..20]))
                    .unwrap_or_default();
                doc.events.push(XmlEvent::CData { line, text });
            }
            _ => { /* skip unknown chunk */ }
        }

        pos += size;
    }

    Ok(doc)
}

fn read_namespace(chunk: &[u8], pool: &StringPool) -> (u32, Option<String>, String) {
    let line = LE::read_u32(&chunk[8..12]);
    let prefix = string_ref(pool, LE::read_u32(&chunk[16..20]));
    let uri = string_ref(pool, LE::read_u32(&chunk[20..24])).unwrap_or_default();
    (line, prefix, uri)
}

fn read_start_element(chunk: &[u8], doc: &AxmlDocument) -> Result<XmlEvent, Error> {
    if chunk.len() < 36 {
        return Err(Error::Truncated("start element"));
    }
    let pool = &doc.string_pool;
    let line = LE::read_u32(&chunk[8..12]);
    let ns = string_ref(pool, LE::read_u32(&chunk[16..20]));
    let name = string_ref(pool, LE::read_u32(&chunk[20..24])).unwrap_or_default();
    let attr_start = LE::read_u16(&chunk[24..26]) as usize;
    let attr_size = LE::read_u16(&chunk[26..28]) as usize;
    let attr_count = LE::read_u16(&chunk[28..30]) as usize;

    let mut attributes = Vec::with_capacity(attr_count);
    let base = 16 + attr_start; // attr_start is relative to the element header end
    let stride = if attr_size == 0 { 20 } else { attr_size };
    for i in 0..attr_count {
        let at = base + i * stride;
        if at + 20 > chunk.len() {
            break;
        }
        let ns_idx = LE::read_u32(&chunk[at..at + 4]);
        let name_idx = LE::read_u32(&chunk[at + 4..at + 8]);
        let raw_idx = LE::read_u32(&chunk[at + 8..at + 12]);
        let size = LE::read_u16(&chunk[at + 12..at + 14]);
        let res0 = chunk[at + 14];
        let data_type = chunk[at + 15];
        let data = LE::read_u32(&chunk[at + 16..at + 20]);

        let name_resource_id = doc
            .resource_map
            .get(name_idx as usize)
            .copied()
            .filter(|id| *id != 0);
        // Prefer the pool name. If it's missing/empty, recover the framework
        // attribute name from its resource id (e.g. 0x01010003 -> "name"),
        // falling back to a raw hex id only when the id is unknown.
        let aname = string_ref(pool, name_idx)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                name_resource_id.map(|id| {
                    crate::android_attrs::attr_name(id)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("0x{:08x}", id))
                })
            })
            .unwrap_or_default();

        attributes.push(Attribute {
            ns: string_ref(pool, ns_idx),
            name: aname,
            name_resource_id,
            raw_value: string_ref(pool, raw_idx),
            typed: ResValue {
                size,
                res0,
                data_type,
                data,
            },
        });
    }

    Ok(XmlEvent::StartElement {
        line,
        ns,
        name,
        attributes,
    })
}

fn string_ref(pool: &StringPool, idx: u32) -> Option<String> {
    pool.get(idx).map(|s| s.to_string())
}
