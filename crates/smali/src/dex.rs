//! DEX container reader: header + id sections + class data.

use crate::Error;
use byteorder::{ByteOrder, LittleEndian as LE};

pub const DEX_FILE_MAGIC: &[u8; 4] = b"dex\n";

#[derive(Debug, Clone)]
pub struct ProtoId {
    pub shorty_idx: u32,
    pub return_type_idx: u32,
    pub parameters_off: u32,
}

#[derive(Debug, Clone)]
pub struct FieldId {
    pub class_idx: u16,
    pub type_idx: u16,
    pub name_idx: u32,
}

#[derive(Debug, Clone)]
pub struct MethodId {
    pub class_idx: u16,
    pub proto_idx: u16,
    pub name_idx: u32,
}

#[derive(Debug, Clone)]
pub struct EncodedField {
    pub field_idx: u32,
    pub access_flags: u32,
}

#[derive(Debug, Clone)]
pub struct EncodedMethod {
    pub method_idx: u32,
    pub access_flags: u32,
    pub code_off: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ClassData {
    pub static_fields: Vec<EncodedField>,
    pub instance_fields: Vec<EncodedField>,
    pub direct_methods: Vec<EncodedMethod>,
    pub virtual_methods: Vec<EncodedMethod>,
}

#[derive(Debug, Clone)]
pub struct ClassDef {
    pub class_idx: u32,
    pub access_flags: u32,
    pub superclass_idx: u32,
    pub interfaces_off: u32,
    pub source_file_idx: u32,
    pub annotations_off: u32,
    pub class_data_off: u32,
    pub static_values_off: u32,
    pub class_data: ClassData,
    pub interfaces: Vec<u32>,
}

/// A single `try_item`: a guarded code range plus a pointer into the catch
/// handler list. Addresses/counts are in 16-bit code units.
#[derive(Debug, Clone)]
pub struct TryItem {
    /// First code unit covered by the try (inclusive).
    pub start_addr: u32,
    /// Number of code units covered.
    pub insn_count: u16,
    /// Byte offset (relative to the start of the encoded handler list) of the
    /// `encoded_catch_handler` for this try.
    pub handler_off: u16,
}

/// A single typed catch within an `encoded_catch_handler`.
#[derive(Debug, Clone)]
pub struct CatchHandler {
    /// `type_idx` of the caught exception class.
    pub type_idx: u32,
    /// Target instruction address (in code units).
    pub addr: u32,
}

/// A decoded `encoded_catch_handler`: zero or more typed catches and an
/// optional catch-all address.
#[derive(Debug, Clone, Default)]
pub struct EncodedCatchHandler {
    /// Byte offset of this handler within the handler list (used to match
    /// [`TryItem::handler_off`]).
    pub offset: u16,
    pub handlers: Vec<CatchHandler>,
    /// Catch-all target address (in code units), if present.
    pub catch_all_addr: Option<u32>,
}

/// A decoded instruction stream for a method (raw 16-bit code units).
#[derive(Debug, Clone, Default)]
pub struct CodeItem {
    pub registers_size: u16,
    pub ins_size: u16,
    pub outs_size: u16,
    pub tries_size: u16,
    pub debug_info_off: u32,
    pub insns: Vec<u16>,
    /// Decoded try table (empty when `tries_size == 0`).
    pub tries: Vec<TryItem>,
    /// Decoded catch handler list, keyed by their byte offset.
    pub handlers: Vec<EncodedCatchHandler>,
}

#[derive(Debug, Clone)]
pub struct Dex {
    pub version: u32,
    pub strings: Vec<String>,
    pub type_ids: Vec<u32>,
    pub proto_ids: Vec<ProtoId>,
    pub field_ids: Vec<FieldId>,
    pub method_ids: Vec<MethodId>,
    pub class_defs: Vec<ClassDef>,
    raw: Vec<u8>,
}

impl Dex {
    pub fn version(bytes: &[u8]) -> Result<u32, Error> {
        if bytes.len() < 8 || &bytes[0..4] != DEX_FILE_MAGIC {
            return Err(Error::BadMagic);
        }
        // magic is "dex\n0XY\0"
        let v = std::str::from_utf8(&bytes[4..7])
            .ok()
            .and_then(|s| s.trim_matches('\0').parse::<u32>().ok())
            .ok_or(Error::BadMagic)?;
        Ok(v)
    }

    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    pub fn type_descriptor(&self, type_idx: usize) -> String {
        self.type_ids
            .get(type_idx)
            .and_then(|s| self.strings.get(*s as usize))
            .cloned()
            .unwrap_or_else(|| format!("Ltype{}?;", type_idx))
    }

    pub fn string(&self, idx: usize) -> &str {
        self.strings.get(idx).map(|s| s.as_str()).unwrap_or("")
    }

    /// Build `(retType, paramTypes)` for a proto.
    pub fn proto_descriptor(&self, proto_idx: usize) -> (String, Vec<String>) {
        let Some(proto) = self.proto_ids.get(proto_idx) else {
            return ("V".to_string(), Vec::new());
        };
        let ret = self.type_descriptor(proto.return_type_idx as usize);
        let mut params = Vec::new();
        if proto.parameters_off != 0 {
            let off = proto.parameters_off as usize;
            if off + 4 <= self.raw.len() {
                let size = LE::read_u32(&self.raw[off..off + 4]) as usize;
                for i in 0..size {
                    let at = off + 4 + i * 2;
                    if at + 2 > self.raw.len() {
                        break;
                    }
                    let tidx = LE::read_u16(&self.raw[at..at + 2]) as usize;
                    params.push(self.type_descriptor(tidx));
                }
            }
        }
        (ret, params)
    }

    /// `Lclass;->name(params)ret` for a method ref.
    pub fn method_ref(&self, method_idx: usize) -> String {
        let Some(m) = self.method_ids.get(method_idx) else {
            return format!("method{}?", method_idx);
        };
        let cls = self.type_descriptor(m.class_idx as usize);
        let name = self.string(m.name_idx as usize).to_string();
        let (ret, params) = self.proto_descriptor(m.proto_idx as usize);
        format!("{}->{}({}){}", cls, name, params.join(""), ret)
    }

    /// `Lclass;->name:type` for a field ref.
    pub fn field_ref(&self, field_idx: usize) -> String {
        let Some(f) = self.field_ids.get(field_idx) else {
            return format!("field{}?", field_idx);
        };
        let cls = self.type_descriptor(f.class_idx as usize);
        let name = self.string(f.name_idx as usize).to_string();
        let ty = self.type_descriptor(f.type_idx as usize);
        format!("{}->{}:{}", cls, name, ty)
    }

    pub fn code_item(&self, code_off: u32) -> Option<CodeItem> {
        if code_off == 0 {
            return None;
        }
        let off = code_off as usize;
        if off + 16 > self.raw.len() {
            return None;
        }
        let registers_size = LE::read_u16(&self.raw[off..off + 2]);
        let ins_size = LE::read_u16(&self.raw[off + 2..off + 4]);
        let outs_size = LE::read_u16(&self.raw[off + 4..off + 6]);
        let tries_size = LE::read_u16(&self.raw[off + 6..off + 8]);
        let debug_info_off = LE::read_u32(&self.raw[off + 8..off + 12]);
        let insns_size = LE::read_u32(&self.raw[off + 12..off + 16]) as usize;
        let mut insns = Vec::with_capacity(insns_size);
        for i in 0..insns_size {
            let at = off + 16 + i * 2;
            if at + 2 > self.raw.len() {
                break;
            }
            insns.push(LE::read_u16(&self.raw[at..at + 2]));
        }

        // Tries + catch-handler list follow the insns array. When present, the
        // try table starts at a 4-byte boundary after the insns (a 2-byte
        // padding unit is inserted when `insns_size` is odd).
        let mut tries = Vec::new();
        let mut handlers = Vec::new();
        if tries_size != 0 {
            let mut tries_off = off + 16 + insns_size * 2;
            if insns_size % 2 == 1 {
                tries_off += 2; // padding to 4-byte alignment
            }
            // Each try_item is 8 bytes: start_addr u32, insn_count u16, handler_off u16.
            for i in 0..tries_size as usize {
                let at = tries_off + i * 8;
                if at + 8 > self.raw.len() {
                    break;
                }
                tries.push(TryItem {
                    start_addr: LE::read_u32(&self.raw[at..at + 4]),
                    insn_count: LE::read_u16(&self.raw[at + 4..at + 6]),
                    handler_off: LE::read_u16(&self.raw[at + 6..at + 8]),
                });
            }
            // The encoded_catch_handler_list immediately follows the try table.
            let list_off = tries_off + tries_size as usize * 8;
            handlers = parse_catch_handler_list(&self.raw, list_off);
        }

        Some(CodeItem {
            registers_size,
            ins_size,
            outs_size,
            tries_size,
            debug_info_off,
            insns,
            tries,
            handlers,
        })
    }

    pub fn parse(bytes: &[u8]) -> Result<Dex, Error> {
        let version = Self::version(bytes)?;
        if bytes.len() < 112 {
            return Err(Error::Truncated("header"));
        }
        let rd_u32 = |o: usize| LE::read_u32(&bytes[o..o + 4]);

        let string_ids_size = rd_u32(56) as usize;
        let string_ids_off = rd_u32(60) as usize;
        let type_ids_size = rd_u32(64) as usize;
        let type_ids_off = rd_u32(68) as usize;
        let proto_ids_size = rd_u32(72) as usize;
        let proto_ids_off = rd_u32(76) as usize;
        let field_ids_size = rd_u32(80) as usize;
        let field_ids_off = rd_u32(84) as usize;
        let method_ids_size = rd_u32(88) as usize;
        let method_ids_off = rd_u32(92) as usize;
        let class_defs_size = rd_u32(96) as usize;
        let class_defs_off = rd_u32(100) as usize;

        // Strings.
        let mut strings = Vec::with_capacity(string_ids_size);
        for i in 0..string_ids_size {
            let at = string_ids_off + i * 4;
            if at + 4 > bytes.len() {
                break;
            }
            let data_off = LE::read_u32(&bytes[at..at + 4]) as usize;
            strings.push(read_string_data(bytes, data_off));
        }

        // Type ids.
        let mut type_ids = Vec::with_capacity(type_ids_size);
        for i in 0..type_ids_size {
            let at = type_ids_off + i * 4;
            if at + 4 > bytes.len() {
                break;
            }
            type_ids.push(LE::read_u32(&bytes[at..at + 4]));
        }

        // Proto ids (12 bytes each).
        let mut proto_ids = Vec::with_capacity(proto_ids_size);
        for i in 0..proto_ids_size {
            let at = proto_ids_off + i * 12;
            if at + 12 > bytes.len() {
                break;
            }
            proto_ids.push(ProtoId {
                shorty_idx: LE::read_u32(&bytes[at..at + 4]),
                return_type_idx: LE::read_u32(&bytes[at + 4..at + 8]),
                parameters_off: LE::read_u32(&bytes[at + 8..at + 12]),
            });
        }

        // Field ids (8 bytes each).
        let mut field_ids = Vec::with_capacity(field_ids_size);
        for i in 0..field_ids_size {
            let at = field_ids_off + i * 8;
            if at + 8 > bytes.len() {
                break;
            }
            field_ids.push(FieldId {
                class_idx: LE::read_u16(&bytes[at..at + 2]),
                type_idx: LE::read_u16(&bytes[at + 2..at + 4]),
                name_idx: LE::read_u32(&bytes[at + 4..at + 8]),
            });
        }

        // Method ids (8 bytes each).
        let mut method_ids = Vec::with_capacity(method_ids_size);
        for i in 0..method_ids_size {
            let at = method_ids_off + i * 8;
            if at + 8 > bytes.len() {
                break;
            }
            method_ids.push(MethodId {
                class_idx: LE::read_u16(&bytes[at..at + 2]),
                proto_idx: LE::read_u16(&bytes[at + 2..at + 4]),
                name_idx: LE::read_u32(&bytes[at + 4..at + 8]),
            });
        }

        // Class defs (32 bytes each).
        let mut class_defs = Vec::with_capacity(class_defs_size);
        for i in 0..class_defs_size {
            let at = class_defs_off + i * 32;
            if at + 32 > bytes.len() {
                break;
            }
            let class_data_off = LE::read_u32(&bytes[at + 24..at + 28]);
            let interfaces_off = LE::read_u32(&bytes[at + 12..at + 16]);
            let class_data = if class_data_off != 0 {
                parse_class_data(bytes, class_data_off as usize)
            } else {
                ClassData::default()
            };
            let interfaces = parse_type_list(bytes, interfaces_off as usize);
            class_defs.push(ClassDef {
                class_idx: LE::read_u32(&bytes[at..at + 4]),
                access_flags: LE::read_u32(&bytes[at + 4..at + 8]),
                superclass_idx: LE::read_u32(&bytes[at + 8..at + 12]),
                interfaces_off,
                source_file_idx: LE::read_u32(&bytes[at + 16..at + 20]),
                annotations_off: LE::read_u32(&bytes[at + 20..at + 24]),
                class_data_off,
                static_values_off: LE::read_u32(&bytes[at + 28..at + 32]),
                class_data,
                interfaces,
            });
        }

        Ok(Dex {
            version,
            strings,
            type_ids,
            proto_ids,
            field_ids,
            method_ids,
            class_defs,
            raw: bytes.to_vec(),
        })
    }
}

fn parse_type_list(bytes: &[u8], off: usize) -> Vec<u32> {
    if off == 0 || off + 4 > bytes.len() {
        return Vec::new();
    }
    let size = LE::read_u32(&bytes[off..off + 4]) as usize;
    let mut out = Vec::with_capacity(size);
    for i in 0..size {
        let at = off + 4 + i * 2;
        if at + 2 > bytes.len() {
            break;
        }
        out.push(LE::read_u16(&bytes[at..at + 2]) as u32);
    }
    out
}

fn parse_class_data(bytes: &[u8], off: usize) -> ClassData {
    let mut p = off;
    let static_fields_size = read_uleb128(bytes, &mut p);
    let instance_fields_size = read_uleb128(bytes, &mut p);
    let direct_methods_size = read_uleb128(bytes, &mut p);
    let virtual_methods_size = read_uleb128(bytes, &mut p);

    let read_fields = |bytes: &[u8], p: &mut usize, n: u32| -> Vec<EncodedField> {
        let mut out = Vec::new();
        let mut idx = 0u32;
        for _ in 0..n {
            let delta = read_uleb128(bytes, p);
            idx = idx.wrapping_add(delta);
            let access = read_uleb128(bytes, p);
            out.push(EncodedField {
                field_idx: idx,
                access_flags: access,
            });
        }
        out
    };
    let read_methods = |bytes: &[u8], p: &mut usize, n: u32| -> Vec<EncodedMethod> {
        let mut out = Vec::new();
        let mut idx = 0u32;
        for _ in 0..n {
            let delta = read_uleb128(bytes, p);
            idx = idx.wrapping_add(delta);
            let access = read_uleb128(bytes, p);
            let code_off = read_uleb128(bytes, p);
            out.push(EncodedMethod {
                method_idx: idx,
                access_flags: access,
                code_off,
            });
        }
        out
    };

    let static_fields = read_fields(bytes, &mut p, static_fields_size);
    let instance_fields = read_fields(bytes, &mut p, instance_fields_size);
    let direct_methods = read_methods(bytes, &mut p, direct_methods_size);
    let virtual_methods = read_methods(bytes, &mut p, virtual_methods_size);

    ClassData {
        static_fields,
        instance_fields,
        direct_methods,
        virtual_methods,
    }
}

/// Read a `string_data_item`: ULEB128 utf16 length, then MUTF-8 bytes to NUL.
fn read_string_data(bytes: &[u8], off: usize) -> String {
    if off >= bytes.len() {
        return String::new();
    }
    let mut p = off;
    let _utf16_len = read_uleb128(bytes, &mut p);
    let start = p;
    let mut end = p;
    while end < bytes.len() && bytes[end] != 0 {
        end += 1;
    }
    // MUTF-8 is close enough to UTF-8 for our purposes; lossily decode.
    String::from_utf8_lossy(&bytes[start..end]).into_owned()
}

/// Parse an `encoded_catch_handler_list` at `list_off`.
///
/// Layout: `uleb128 size`, then `size` `encoded_catch_handler` items. Each
/// handler records its byte offset relative to `list_off` so [`TryItem`]'s
/// `handler_off` can be matched against [`EncodedCatchHandler::offset`].
fn parse_catch_handler_list(bytes: &[u8], list_off: usize) -> Vec<EncodedCatchHandler> {
    let mut out = Vec::new();
    if list_off >= bytes.len() {
        return out;
    }
    let mut p = list_off;
    let size = read_uleb128(bytes, &mut p);
    for _ in 0..size {
        // Offset is measured from the start of the handler list.
        let offset = (p - list_off) as u16;
        // `size`: signed. >0 => that many typed catches, no catch-all.
        //         <=0 => |size| typed catches followed by a catch-all.
        let raw_size = read_sleb128(bytes, &mut p);
        let typed = raw_size.unsigned_abs() as usize;
        let mut handlers = Vec::with_capacity(typed);
        for _ in 0..typed {
            let type_idx = read_uleb128(bytes, &mut p);
            let addr = read_uleb128(bytes, &mut p);
            handlers.push(CatchHandler { type_idx, addr });
        }
        let catch_all_addr = if raw_size <= 0 {
            Some(read_uleb128(bytes, &mut p))
        } else {
            None
        };
        out.push(EncodedCatchHandler {
            offset,
            handlers,
            catch_all_addr,
        });
        if p > bytes.len() {
            break;
        }
    }
    out
}

/// Read a signed LEB128 value, advancing `*p`.
pub fn read_sleb128(bytes: &[u8], p: &mut usize) -> i32 {
    let mut result = 0i32;
    let mut shift = 0u32;
    let mut byte;
    loop {
        if *p >= bytes.len() || shift >= 32 {
            return result;
        }
        byte = bytes[*p];
        *p += 1;
        result |= ((byte & 0x7f) as i32) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            break;
        }
    }
    // Sign-extend if the value is negative and we have room.
    if shift < 32 && (byte & 0x40) != 0 {
        result |= -(1i32 << shift);
    }
    result
}

/// Read an unsigned LEB128 value, advancing `*p`.
pub fn read_uleb128(bytes: &[u8], p: &mut usize) -> u32 {
    let mut result = 0u32;
    let mut shift = 0;
    loop {
        if *p >= bytes.len() || shift >= 32 {
            break;
        }
        let b = bytes[*p];
        *p += 1;
        result |= ((b & 0x7f) as u32) << shift;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    result
}

/// Write an unsigned LEB128 value.
pub fn write_uleb128(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleb128_roundtrips_signed_values() {
        // 0 -> 0x00 ; -1 -> 0x7f ; 2 -> 0x02 ; -2 -> 0x7e ; 127 -> 0xff 0x00
        let cases: &[(&[u8], i32)] = &[
            (&[0x00], 0),
            (&[0x7f], -1),
            (&[0x02], 2),
            (&[0x7e], -2),
            (&[0xff, 0x00], 127),
            (&[0x80, 0x7f], -128),
        ];
        for (bytes, expected) in cases {
            let mut p = 0usize;
            assert_eq!(read_sleb128(bytes, &mut p), *expected, "bytes={bytes:?}");
            assert_eq!(p, bytes.len(), "consumed all bytes for {bytes:?}");
        }
    }

    #[test]
    fn catch_handler_list_typed_and_catchall() {
        // size=1 handler.
        // handler: size = 0 (sleb) => catch-all only, addr = 9
        let mut data = Vec::new();
        write_uleb128(&mut data, 1); // list size
        data.push(0x00); // encoded size 0 => catch-all only
        write_uleb128(&mut data, 9); // catch_all_addr
        let hs = parse_catch_handler_list(&data, 0);
        assert_eq!(hs.len(), 1);
        assert!(hs[0].handlers.is_empty());
        assert_eq!(hs[0].catch_all_addr, Some(9));
        assert_eq!(hs[0].offset, 1); // after the 1-byte list-size uleb

        // size=1 handler.
        // handler: size = 2 (sleb) => two typed catches, no catch-all
        let mut data = Vec::new();
        write_uleb128(&mut data, 1);
        data.push(0x02); // two typed handlers
        write_uleb128(&mut data, 3); // type_idx
        write_uleb128(&mut data, 10); // addr
        write_uleb128(&mut data, 4);
        write_uleb128(&mut data, 20);
        let hs = parse_catch_handler_list(&data, 0);
        assert_eq!(hs.len(), 1);
        assert_eq!(hs[0].handlers.len(), 2);
        assert_eq!(hs[0].handlers[0].type_idx, 3);
        assert_eq!(hs[0].handlers[0].addr, 10);
        assert_eq!(hs[0].handlers[1].type_idx, 4);
        assert_eq!(hs[0].handlers[1].addr, 20);
        assert_eq!(hs[0].catch_all_addr, None);
    }
}
