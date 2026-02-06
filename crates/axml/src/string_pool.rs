//! `RES_STRING_POOL_TYPE` chunk reader/writer.

use crate::types::string_pool_flags::{SORTED_FLAG, UTF8_FLAG};
use crate::Error;
use byteorder::{ByteOrder, LittleEndian as LE, WriteBytesExt};

/// An in-memory string pool: an ordered list of strings plus the encoding flag.
#[derive(Debug, Clone, Default)]
pub struct StringPool {
    pub strings: Vec<String>,
    pub utf8: bool,
    pub sorted: bool,
    /// Style spans are preserved opaquely (rare in AXML, common in ARSC).
    pub styles: Vec<Vec<u8>>,
}

impl StringPool {
    pub fn get(&self, idx: u32) -> Option<&str> {
        if idx == crate::types::NO_ENTRY {
            return None;
        }
        self.strings.get(idx as usize).map(|s| s.as_str())
    }

    /// Returns the index of `s`, appending it if not present.
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(pos) = self.strings.iter().position(|e| e == s) {
            return pos as u32;
        }
        self.strings.push(s.to_string());
        (self.strings.len() - 1) as u32
    }

    /// Parse a string-pool chunk body. `chunk` is the full chunk including its
    /// 8-byte `ResChunk_header`.
    pub fn parse(chunk: &[u8]) -> Result<StringPool, Error> {
        if chunk.len() < 28 {
            return Err(Error::Truncated("string pool header"));
        }
        let header_size = LE::read_u16(&chunk[2..4]) as usize;
        let string_count = LE::read_u32(&chunk[8..12]) as usize;
        let style_count = LE::read_u32(&chunk[12..16]) as usize;
        let flags = LE::read_u32(&chunk[16..20]);
        let strings_start = LE::read_u32(&chunk[20..24]) as usize;
        let styles_start = LE::read_u32(&chunk[24..28]) as usize;

        let utf8 = flags & UTF8_FLAG != 0;
        let sorted = flags & SORTED_FLAG != 0;

        let offsets_base = header_size.max(28);
        let mut string_offsets = Vec::with_capacity(string_count);
        for i in 0..string_count {
            let at = offsets_base + i * 4;
            if at + 4 > chunk.len() {
                return Err(Error::Truncated("string offset table"));
            }
            string_offsets.push(LE::read_u32(&chunk[at..at + 4]) as usize);
        }

        let mut strings = Vec::with_capacity(string_count);
        for off in string_offsets {
            let at = strings_start + off;
            let s = if utf8 {
                read_utf8_string(chunk, at)?
            } else {
                read_utf16_string(chunk, at)?
            };
            strings.push(s);
        }

        // Style spans: read offsets then keep raw bytes up to the next boundary.
        let mut styles = Vec::with_capacity(style_count);
        if style_count > 0 && styles_start != 0 {
            let style_off_base = offsets_base + string_count * 4;
            let mut style_offsets = Vec::with_capacity(style_count);
            for i in 0..style_count {
                let at = style_off_base + i * 4;
                if at + 4 > chunk.len() {
                    break;
                }
                style_offsets.push(LE::read_u32(&chunk[at..at + 4]) as usize);
            }
            for (i, off) in style_offsets.iter().enumerate() {
                let start = styles_start + off;
                let end = style_offsets
                    .get(i + 1)
                    .map(|n| styles_start + n)
                    .unwrap_or(chunk.len());
                if start <= end && end <= chunk.len() {
                    styles.push(chunk[start..end].to_vec());
                }
            }
        }

        Ok(StringPool {
            strings,
            utf8,
            sorted,
            styles,
        })
    }

    /// Serialize this pool into a complete `RES_STRING_POOL_TYPE` chunk.
    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::new();
        let mut offsets = Vec::with_capacity(self.strings.len());

        for s in &self.strings {
            offsets.push(data.len() as u32);
            if self.utf8 {
                write_utf8_string(&mut data, s);
            } else {
                write_utf16_string(&mut data, s);
            }
        }
        // Pad string data to a 4-byte boundary.
        while data.len() % 4 != 0 {
            data.push(0);
        }

        let header_size: u32 = 28;
        let offset_table_len = (self.strings.len() * 4) as u32;
        let strings_start = header_size + offset_table_len;
        let total = strings_start + data.len() as u32;

        let mut flags = 0u32;
        if self.utf8 {
            flags |= UTF8_FLAG;
        }
        if self.sorted {
            flags |= SORTED_FLAG;
        }

        let mut out = Vec::with_capacity(total as usize);
        out.write_u16::<LE>(crate::types::chunk::RES_STRING_POOL_TYPE)
            .unwrap();
        out.write_u16::<LE>(header_size as u16).unwrap();
        out.write_u32::<LE>(total).unwrap();
        out.write_u32::<LE>(self.strings.len() as u32).unwrap();
        out.write_u32::<LE>(0).unwrap(); // style count (styles not re-emitted)
        out.write_u32::<LE>(flags).unwrap();
        out.write_u32::<LE>(strings_start).unwrap();
        out.write_u32::<LE>(0).unwrap(); // styles start
        for o in offsets {
            out.write_u32::<LE>(o).unwrap();
        }
        out.extend_from_slice(&data);
        out
    }
}

/// Read a UTF-8 length-prefixed, NUL-terminated string at `at`.
fn read_utf8_string(buf: &[u8], at: usize) -> Result<String, Error> {
    let (_u16_len, p) = read_utf8_len(buf, at)?;
    let (byte_len, p) = read_utf8_len(buf, p)?;
    let end = p + byte_len;
    if end > buf.len() {
        return Err(Error::Truncated("utf8 string body"));
    }
    Ok(String::from_utf8_lossy(&buf[p..end]).into_owned())
}

/// AXML UTF-8 length: 1 byte if high bit clear, else 2 bytes (big-endian-ish).
fn read_utf8_len(buf: &[u8], at: usize) -> Result<(usize, usize), Error> {
    if at >= buf.len() {
        return Err(Error::Truncated("utf8 length"));
    }
    let b0 = buf[at] as usize;
    if b0 & 0x80 != 0 {
        if at + 1 >= buf.len() {
            return Err(Error::Truncated("utf8 length"));
        }
        let b1 = buf[at + 1] as usize;
        Ok((((b0 & 0x7f) << 8) | b1, at + 2))
    } else {
        Ok((b0, at + 1))
    }
}

fn read_utf16_string(buf: &[u8], at: usize) -> Result<String, Error> {
    let (len, p) = read_utf16_len(buf, at)?;
    let end = p + len * 2;
    if end > buf.len() {
        return Err(Error::Truncated("utf16 string body"));
    }
    let mut units = Vec::with_capacity(len);
    let mut i = p;
    while i + 1 < end {
        units.push(LE::read_u16(&buf[i..i + 2]));
        i += 2;
    }
    Ok(String::from_utf16_lossy(&units))
}

fn read_utf16_len(buf: &[u8], at: usize) -> Result<(usize, usize), Error> {
    if at + 1 >= buf.len() {
        return Err(Error::Truncated("utf16 length"));
    }
    let first = LE::read_u16(&buf[at..at + 2]) as usize;
    if first & 0x8000 != 0 {
        if at + 3 >= buf.len() {
            return Err(Error::Truncated("utf16 length"));
        }
        let second = LE::read_u16(&buf[at + 2..at + 4]) as usize;
        Ok((((first & 0x7fff) << 16) | second, at + 4))
    } else {
        Ok((first, at + 2))
    }
}

fn write_utf8_len(out: &mut Vec<u8>, len: usize) {
    if len > 0x7f {
        out.push((((len >> 8) & 0x7f) | 0x80) as u8);
        out.push((len & 0xff) as u8);
    } else {
        out.push(len as u8);
    }
}

fn write_utf8_string(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let utf16_len = s.encode_utf16().count();
    write_utf8_len(out, utf16_len);
    write_utf8_len(out, bytes.len());
    out.extend_from_slice(bytes);
    out.push(0);
}

fn write_utf16_len(out: &mut Vec<u8>, len: usize) {
    if len > 0x7fff {
        let hi = (((len >> 16) & 0x7fff) | 0x8000) as u16;
        out.write_u16::<LE>(hi).unwrap();
        out.write_u16::<LE>((len & 0xffff) as u16).unwrap();
    } else {
        out.write_u16::<LE>(len as u16).unwrap();
    }
}

fn write_utf16_string(out: &mut Vec<u8>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    write_utf16_len(out, units.len());
    for u in units {
        out.write_u16::<LE>(u).unwrap();
    }
    out.write_u16::<LE>(0).unwrap();
}
