//! `resources.arsc` chunk walker and in-memory model.

use crate::config::ResConfig;
use crate::Error;
use byteorder::{ByteOrder, LittleEndian as LE};
use std::collections::HashMap;

pub mod chunk {
    pub const RES_STRING_POOL_TYPE: u16 = 0x0001;
    pub const RES_TABLE_TYPE: u16 = 0x0002;
    pub const RES_TABLE_PACKAGE_TYPE: u16 = 0x0200;
    pub const RES_TABLE_TYPE_TYPE: u16 = 0x0201;
    pub const RES_TABLE_TYPE_SPEC_TYPE: u16 = 0x0202;
    pub const RES_TABLE_LIBRARY_TYPE: u16 = 0x0203;
    pub const RES_TABLE_OVERLAYABLE_TYPE: u16 = 0x0204;
    pub const RES_TABLE_OVERLAYABLE_POLICY_TYPE: u16 = 0x0205;
    pub const RES_TABLE_STAGED_ALIAS_TYPE: u16 = 0x0206;
}

/// Entry flags (`ResTable_entry::flags`).
pub mod entry_flags {
    pub const FLAG_COMPLEX: u16 = 0x0001;
    pub const FLAG_PUBLIC: u16 = 0x0002;
    pub const FLAG_WEAK: u16 = 0x0004;
    pub const FLAG_COMPACT: u16 = 0x0008;
}

/// Type-table flags (`ResTable_type::flags`).
pub mod type_flags {
    pub const FLAG_SPARSE: u8 = 0x01;
    pub const FLAG_OFFSET16: u8 = 0x02;
}

#[derive(Debug, Clone, Copy)]
pub struct ResValueLite {
    pub data_type: u8,
    pub data: u32,
}

#[derive(Debug, Clone)]
pub enum EntryValue {
    Simple(ResValueLite),
    Complex {
        parent: u32,
        items: Vec<(u32, ResValueLite)>,
    },
}

#[derive(Debug, Clone)]
pub struct ResEntry {
    pub res_id: u32,
    pub type_id: u8,
    pub type_name: String,
    pub entry_id: u16,
    pub key: String,
    pub config: ResConfig,
    pub value: EntryValue,
}

#[derive(Debug, Clone, Default)]
pub struct ResTablePackage {
    pub id: u32,
    pub name: String,
    pub type_names: Vec<String>,
    pub key_names: Vec<String>,
    pub entries: Vec<ResEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct ResTable {
    /// Global value string pool (shared by all packages).
    pub value_pool: Vec<String>,
    pub packages: Vec<ResTablePackage>,
    /// res_id -> "type/name" lookup, built after parse.
    pub(crate) index: HashMap<u32, (String, String, u32)>,
}

impl ResTable {
    pub fn value_string(&self, idx: u32) -> Option<&str> {
        self.value_pool.get(idx as usize).map(|s| s.as_str())
    }

    /// Resolve a resource id (`0xPPTTEEEE`) into `@[pkg:]type/name`.
    pub fn resolve(&self, id: u32) -> Option<String> {
        let pkg_id = (id >> 24) & 0xff;
        if let Some((ty, name, _)) = self.index.get(&id) {
            // Own-package refs render without the package prefix.
            let own = self.packages.first().map(|p| p.id).unwrap_or(0x7f);
            if pkg_id == own {
                Some(format!("@{}/{}", ty, name))
            } else {
                Some(format!("@0x{:02x}:{}/{}", pkg_id, ty, name))
            }
        } else if pkg_id == 0x01 {
            // Framework refs require the framework table; surface a hint.
            Some(format!("@android:0x{:08x}", id))
        } else {
            None
        }
    }

    /// Build the id → name index from parsed entries.
    pub fn build_index(&mut self) {
        let mut index = HashMap::new();
        for pkg in &self.packages {
            for e in &pkg.entries {
                index
                    .entry(e.res_id)
                    .or_insert_with(|| (e.type_name.clone(), e.key.clone(), e.res_id));
            }
        }
        self.index = index;
    }
}

/// Walk an ARSC byte buffer into a [`ResTable`].
pub fn parse(bytes: &[u8]) -> Result<ResTable, Error> {
    if bytes.len() < 12 {
        return Err(Error::Truncated("table header"));
    }
    let ty = LE::read_u16(&bytes[0..2]);
    if ty != chunk::RES_TABLE_TYPE {
        return Err(Error::BadChunk(ty));
    }
    let header_size = LE::read_u16(&bytes[2..4]) as usize;

    let mut table = ResTable::default();
    let mut pos = header_size.max(12);

    while pos + 8 <= bytes.len() {
        let ctype = LE::read_u16(&bytes[pos..pos + 2]);
        let chsize = LE::read_u32(&bytes[pos + 4..pos + 8]) as usize;
        if chsize < 8 || pos + chsize > bytes.len() {
            break;
        }
        let chunk = &bytes[pos..pos + chsize];
        match ctype {
            chunk::RES_STRING_POOL_TYPE => {
                table.value_pool = parse_string_pool(chunk);
            }
            chunk::RES_TABLE_PACKAGE_TYPE => {
                if let Some(pkg) = parse_package(chunk) {
                    table.packages.push(pkg);
                }
            }
            _ => {}
        }
        pos += chsize;
    }

    table.build_index();
    Ok(table)
}

fn parse_package(chunk: &[u8]) -> Option<ResTablePackage> {
    if chunk.len() < 288 {
        return None;
    }
    let id = LE::read_u32(&chunk[8..12]);
    // name: 128 UTF-16 code units at offset 12.
    let mut name_units = Vec::new();
    let mut o = 12;
    while o + 1 < 12 + 256 {
        let u = LE::read_u16(&chunk[o..o + 2]);
        if u == 0 {
            break;
        }
        name_units.push(u);
        o += 2;
    }
    let name = String::from_utf16_lossy(&name_units);

    let type_strings_off = LE::read_u32(&chunk[268..272]) as usize;
    let key_strings_off = LE::read_u32(&chunk[276..280]) as usize;

    let type_names = if type_strings_off != 0 && type_strings_off < chunk.len() {
        parse_string_pool(&chunk[type_strings_off..])
    } else {
        Vec::new()
    };
    let key_names = if key_strings_off != 0 && key_strings_off < chunk.len() {
        parse_string_pool(&chunk[key_strings_off..])
    } else {
        Vec::new()
    };

    let mut pkg = ResTablePackage {
        id,
        name,
        type_names,
        key_names,
        entries: Vec::new(),
    };

    // Walk the package body for type chunks. Start after the key string pool.
    let body_start = first_chunk_after(chunk, key_strings_off).unwrap_or(288);
    let mut pos = body_start;
    while pos + 8 <= chunk.len() {
        let ctype = LE::read_u16(&chunk[pos..pos + 2]);
        let chsize = LE::read_u32(&chunk[pos + 4..pos + 8]) as usize;
        if chsize < 8 || pos + chsize > chunk.len() {
            break;
        }
        if ctype == chunk::RES_TABLE_TYPE_TYPE {
            parse_type_chunk(&chunk[pos..pos + chsize], &mut pkg);
        }
        pos += chsize;
    }

    Some(pkg)
}

fn first_chunk_after(chunk: &[u8], key_strings_off: usize) -> Option<usize> {
    if key_strings_off == 0 || key_strings_off + 8 > chunk.len() {
        return None;
    }
    let chsize = LE::read_u32(&chunk[key_strings_off + 4..key_strings_off + 8]) as usize;
    Some(key_strings_off + chsize)
}

fn parse_type_chunk(chunk: &[u8], pkg: &mut ResTablePackage) {
    if chunk.len() < 20 {
        return;
    }
    let type_id = chunk[8];
    let flags = chunk[9];
    let entry_count = LE::read_u32(&chunk[12..16]) as usize;
    let entries_start = LE::read_u32(&chunk[16..20]) as usize;

    // ResTable_config begins at offset 20; its first u32 is its own size.
    if chunk.len() < 24 {
        return;
    }
    let config_size = LE::read_u32(&chunk[20..24]) as usize;
    let config = ResConfig::parse(&chunk[20..(20 + config_size).min(chunk.len())]);

    let type_name = pkg
        .type_names
        .get((type_id as usize).wrapping_sub(1))
        .cloned()
        .unwrap_or_else(|| format!("type{}", type_id));

    let off_table = 20 + config_size;
    let sparse = flags & type_flags::FLAG_SPARSE != 0;
    let offset16 = flags & type_flags::FLAG_OFFSET16 != 0;

    for i in 0..entry_count {
        let (entry_idx, entry_off) = if sparse {
            let at = off_table + i * 4;
            if at + 4 > chunk.len() {
                break;
            }
            let idx = LE::read_u16(&chunk[at..at + 2]) as usize;
            let off = LE::read_u16(&chunk[at + 2..at + 4]) as usize * 4;
            (idx, off)
        } else if offset16 {
            let at = off_table + i * 2;
            if at + 2 > chunk.len() {
                break;
            }
            let off = LE::read_u16(&chunk[at..at + 2]) as usize;
            if off == 0xffff {
                continue;
            }
            (i, off * 4)
        } else {
            let at = off_table + i * 4;
            if at + 4 > chunk.len() {
                break;
            }
            let off = LE::read_u32(&chunk[at..at + 4]) as usize;
            if off == 0xffff_ffff {
                continue;
            }
            (i, off)
        };

        let eat = entries_start + entry_off;
        if let Some(entry) = parse_entry(chunk, eat, pkg, type_id, entry_idx as u16, &type_name, &config) {
            pkg.entries.push(entry);
        }
    }
}

fn parse_entry(
    chunk: &[u8],
    at: usize,
    pkg: &ResTablePackage,
    type_id: u8,
    entry_idx: u16,
    type_name: &str,
    config: &ResConfig,
) -> Option<ResEntry> {
    if at + 8 > chunk.len() {
        return None;
    }
    let flags = LE::read_u16(&chunk[at + 2..at + 4]);
    let key_idx = LE::read_u32(&chunk[at + 4..at + 8]);
    let key = pkg
        .key_names
        .get(key_idx as usize)
        .cloned()
        .unwrap_or_else(|| format!("key{}", key_idx));

    let res_id = ((pkg.id & 0xff) << 24) | ((type_id as u32) << 16) | entry_idx as u32;

    let value = if flags & entry_flags::FLAG_COMPLEX != 0 {
        let map_at = at + 8;
        if map_at + 8 > chunk.len() {
            return None;
        }
        let parent = LE::read_u32(&chunk[map_at..map_at + 4]);
        let count = LE::read_u32(&chunk[map_at + 4..map_at + 8]) as usize;
        let mut items = Vec::with_capacity(count);
        let mut p = map_at + 8;
        for _ in 0..count {
            if p + 12 > chunk.len() {
                break;
            }
            let name = LE::read_u32(&chunk[p..p + 4]);
            let v = read_res_value(chunk, p + 4);
            items.push((name, v));
            p += 12;
        }
        EntryValue::Complex { parent, items }
    } else {
        let v = read_res_value(chunk, at + 8);
        EntryValue::Simple(v)
    };

    Some(ResEntry {
        res_id,
        type_id,
        type_name: type_name.to_string(),
        entry_id: entry_idx,
        key,
        config: config.clone(),
        value,
    })
}

fn read_res_value(chunk: &[u8], at: usize) -> ResValueLite {
    if at + 8 > chunk.len() {
        return ResValueLite {
            data_type: 0,
            data: 0,
        };
    }
    let data_type = chunk[at + 3];
    let data = LE::read_u32(&chunk[at + 4..at + 8]);
    ResValueLite { data_type, data }
}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// Header sizes for the fixed chunk headers we emit.
mod hdr {
    /// `ResTable_header`: type+headerSize+size + packageCount.
    pub const TABLE: usize = 12;
    /// `ResStringPool_header`: chunk header (8) + 5 u32 fields.
    pub const STRING_POOL: usize = 28;
    /// `ResTable_package`: chunk header (8) + id + name[128 u16] + 5 u32.
    pub const PACKAGE: usize = 288;
    /// `ResTable_typeSpec`: chunk header (8) + id/res0/res1 (4) + entryCount (4).
    pub const TYPE_SPEC: usize = 16;
    /// `ResTable_type`: chunk header (8) + id/flags/reserved (4) + entryCount (4)
    /// + entriesStart (4); the `ResTable_config` follows immediately.
    pub const TYPE: usize = 20;
}

/// Sentinel for "no entry at this index" in a dense `ResTable_type` offset array.
const NO_ENTRY: u32 = 0xffff_ffff;

/// Encode a parsed [`ResTable`] back into a valid `resources.arsc` byte buffer.
///
/// The output is a real, re-parseable table: an outer `RES_TABLE_TYPE` header,
/// the global (value) string pool, then one `RES_TABLE_PACKAGE_TYPE` chunk per
/// package. Each package carries its type-strings and key-strings pools and,
/// for every type id, a `RES_TABLE_TYPE_SPEC_TYPE` chunk followed by one dense
/// `RES_TABLE_TYPE_TYPE` chunk per distinct config.
///
/// Byte-for-byte fidelity with the original is **not** a goal; producing a
/// buffer that [`parse`] accepts and that preserves keys/values **is**.
pub fn encode(table: &ResTable) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();

    // Global value string pool (shared by all packages).
    body.extend_from_slice(&encode_string_pool(&table.value_pool, true));

    // One package chunk per package.
    for pkg in &table.packages {
        body.extend_from_slice(&encode_package(pkg)?);
    }

    // Outer RES_TABLE_TYPE header wrapping the body.
    let total = hdr::TABLE + body.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&u16::to_le_bytes(chunk::RES_TABLE_TYPE));
    out.extend_from_slice(&u16::to_le_bytes(hdr::TABLE as u16));
    out.extend_from_slice(&u32::to_le_bytes(total as u32));
    out.extend_from_slice(&u32::to_le_bytes(table.packages.len() as u32));
    out.extend_from_slice(&body);
    Ok(out)
}

/// Encode a single `RES_TABLE_PACKAGE_TYPE` chunk for `pkg`.
fn encode_package(pkg: &ResTablePackage) -> Result<Vec<u8>, Error> {
    // The two per-package string pools, emitted right after the package header.
    let type_pool = encode_string_pool(&pkg.type_names, true);
    let key_pool = encode_string_pool(&pkg.key_names, true);

    let type_strings_off = hdr::PACKAGE;
    let key_strings_off = type_strings_off + type_pool.len();

    // Encode the type bodies (spec + type chunks) grouped by type id.
    let types_body = encode_types(pkg)?;

    let total = hdr::PACKAGE + type_pool.len() + key_pool.len() + types_body.len();
    let mut out = Vec::with_capacity(total);

    // ResChunk_header.
    out.extend_from_slice(&u16::to_le_bytes(chunk::RES_TABLE_PACKAGE_TYPE));
    out.extend_from_slice(&u16::to_le_bytes(hdr::PACKAGE as u16));
    out.extend_from_slice(&u32::to_le_bytes(total as u32));
    // id.
    out.extend_from_slice(&u32::to_le_bytes(pkg.id));
    // name: 128 UTF-16 code units, NUL-terminated/padded.
    let mut name_units: Vec<u16> = pkg.name.encode_utf16().take(127).collect();
    name_units.resize(128, 0);
    for u in name_units {
        out.extend_from_slice(&u16::to_le_bytes(u));
    }
    // typeStrings, lastPublicType, keyStrings, lastPublicKey, typeIdOffset.
    out.extend_from_slice(&u32::to_le_bytes(type_strings_off as u32));
    out.extend_from_slice(&u32::to_le_bytes(0));
    out.extend_from_slice(&u32::to_le_bytes(key_strings_off as u32));
    out.extend_from_slice(&u32::to_le_bytes(0));
    out.extend_from_slice(&u32::to_le_bytes(0));
    debug_assert_eq!(out.len(), hdr::PACKAGE);

    out.extend_from_slice(&type_pool);
    out.extend_from_slice(&key_pool);
    out.extend_from_slice(&types_body);
    Ok(out)
}

/// Encode every type's `typeSpec` + `type` chunks for a package.
fn encode_types(pkg: &ResTablePackage) -> Result<Vec<u8>, Error> {
    use std::collections::BTreeMap;

    // Resolve key strings back to their pool index (parser stores the string).
    let mut key_index: HashMap<&str, u32> = HashMap::new();
    for (i, k) in pkg.key_names.iter().enumerate() {
        key_index.entry(k.as_str()).or_insert(i as u32);
    }

    // Group entries by type id, then within a type by config.
    // type_id -> (config_raw -> (entry_index -> &ResEntry))
    let mut by_type: BTreeMap<u8, BTreeMap<Vec<u8>, BTreeMap<u16, &ResEntry>>> = BTreeMap::new();
    let mut max_entry: BTreeMap<u8, u16> = BTreeMap::new();

    for e in &pkg.entries {
        let cfg = config_bytes(&e.config);
        by_type
            .entry(e.type_id)
            .or_default()
            .entry(cfg)
            .or_default()
            .insert(e.entry_id, e);
        let m = max_entry.entry(e.type_id).or_insert(0);
        *m = (*m).max(e.entry_id);
    }

    let mut out = Vec::new();
    for (&type_id, configs) in &by_type {
        let entry_count = (*max_entry.get(&type_id).unwrap_or(&0) as usize) + 1;

        // ---- typeSpec chunk ----
        let spec_size = hdr::TYPE_SPEC + entry_count * 4;
        out.extend_from_slice(&u16::to_le_bytes(chunk::RES_TABLE_TYPE_SPEC_TYPE));
        out.extend_from_slice(&u16::to_le_bytes(hdr::TYPE_SPEC as u16));
        out.extend_from_slice(&u32::to_le_bytes(spec_size as u32));
        out.push(type_id);
        out.push(0); // res0
        out.extend_from_slice(&u16::to_le_bytes(0)); // res1
        out.extend_from_slice(&u32::to_le_bytes(entry_count as u32));
        for _ in 0..entry_count {
            out.extend_from_slice(&u32::to_le_bytes(0)); // per-entry spec flags
        }

        // ---- one type chunk per config ----
        for (cfg_raw, entries) in configs {
            out.extend_from_slice(&encode_type_chunk(
                type_id,
                entry_count,
                cfg_raw,
                entries,
                &key_index,
            ));
        }
    }
    Ok(out)
}

/// Encode one dense `RES_TABLE_TYPE_TYPE` chunk for a single config.
fn encode_type_chunk(
    type_id: u8,
    entry_count: usize,
    cfg_raw: &[u8],
    entries: &std::collections::BTreeMap<u16, &ResEntry>,
    key_index: &HashMap<&str, u32>,
) -> Vec<u8> {
    // Serialize entry blobs and remember each one's start offset.
    let mut entry_blob = Vec::new();
    let mut offsets = vec![NO_ENTRY; entry_count];
    for (&idx, e) in entries {
        if (idx as usize) < entry_count {
            offsets[idx as usize] = entry_blob.len() as u32;
            let key_idx = key_index.get(e.key.as_str()).copied().unwrap_or(0);
            encode_entry(&mut entry_blob, e, key_idx);
        }
    }

    let header_size = hdr::TYPE + cfg_raw.len();
    let offsets_size = entry_count * 4;
    let entries_start = header_size + offsets_size;
    let total = entries_start + entry_blob.len();

    let mut out = Vec::with_capacity(total);
    // ResChunk_header.
    out.extend_from_slice(&u16::to_le_bytes(chunk::RES_TABLE_TYPE_TYPE));
    out.extend_from_slice(&u16::to_le_bytes(header_size as u16));
    out.extend_from_slice(&u32::to_le_bytes(total as u32));
    // id / flags / reserved (dense, non-sparse → flags = 0).
    out.push(type_id);
    out.push(0);
    out.extend_from_slice(&u16::to_le_bytes(0));
    // entryCount / entriesStart.
    out.extend_from_slice(&u32::to_le_bytes(entry_count as u32));
    out.extend_from_slice(&u32::to_le_bytes(entries_start as u32));
    // ResTable_config.
    out.extend_from_slice(cfg_raw);
    debug_assert_eq!(out.len(), header_size);
    // Dense offset table.
    for off in &offsets {
        out.extend_from_slice(&u32::to_le_bytes(*off));
    }
    // Entry blobs.
    out.extend_from_slice(&entry_blob);
    out
}

/// Append a single entry (`ResTable_entry` or `ResTable_map_entry`) to `buf`,
/// using `key_idx` as the already-resolved key-string-pool index.
fn encode_entry(buf: &mut Vec<u8>, e: &ResEntry, key_idx: u32) {
    match &e.value {
        EntryValue::Simple(v) => {
            // ResTable_entry: size(2)=8, flags(2), key(4).
            buf.extend_from_slice(&u16::to_le_bytes(8));
            buf.extend_from_slice(&u16::to_le_bytes(0)); // flags: simple
            buf.extend_from_slice(&u32::to_le_bytes(key_idx));
            // Res_value: size(2)=8, res0(1)=0, dataType(1), data(4).
            encode_res_value(buf, v);
        }
        EntryValue::Complex { parent, items } => {
            // ResTable_map_entry: ResTable_entry(8) + parent(4) + count(4).
            buf.extend_from_slice(&u16::to_le_bytes(8));
            buf.extend_from_slice(&u16::to_le_bytes(entry_flags::FLAG_COMPLEX));
            buf.extend_from_slice(&u32::to_le_bytes(key_idx));
            buf.extend_from_slice(&u32::to_le_bytes(*parent));
            buf.extend_from_slice(&u32::to_le_bytes(items.len() as u32));
            // ResTable_map[]: name(4) + Res_value(8).
            for (name, v) in items {
                buf.extend_from_slice(&u32::to_le_bytes(*name));
                encode_res_value(buf, v);
            }
        }
    }
}

/// Append an 8-byte `Res_value` to `buf`.
fn encode_res_value(buf: &mut Vec<u8>, v: &ResValueLite) {
    buf.extend_from_slice(&u16::to_le_bytes(8)); // size
    buf.push(0); // res0
    buf.push(v.data_type); // dataType
    buf.extend_from_slice(&u32::to_le_bytes(v.data)); // data
}

/// Produce a self-describing `ResTable_config` blob (leading u32 size).
///
/// Prefers the raw bytes preserved at parse time; otherwise synthesizes the
/// minimal 28-byte "default" config used by the rest of the crate.
fn config_bytes(cfg: &ResConfig) -> Vec<u8> {
    if cfg.raw.len() >= 4 {
        let declared = LE::read_u32(&cfg.raw[0..4]) as usize;
        // Trust the embedded size when it matches the buffer; pad if short.
        if declared >= 4 {
            let mut out = cfg.raw.clone();
            if out.len() < declared {
                out.resize(declared, 0);
            } else if out.len() > declared {
                out.truncate(declared);
            }
            // Configs must be 4-byte aligned in length.
            while out.len() % 4 != 0 {
                out.push(0);
            }
            // Keep the size field in sync after any padding.
            let n = out.len() as u32;
            out[0..4].copy_from_slice(&u32::to_le_bytes(n));
            return out;
        }
    }
    // Fallback: minimal default config (size = 28, all-zero fields).
    let mut out = vec![0u8; 28];
    out[0..4].copy_from_slice(&u32::to_le_bytes(28));
    out
}

/// Encode a `&[String]` into a complete `RES_STRING_POOL_TYPE` chunk (UTF-8 when
/// `utf8` is set, otherwise UTF-16). Mirrors [`parse_string_pool`].
pub fn encode_string_pool(strings: &[String], utf8: bool) -> Vec<u8> {
    const FLAG_UTF8: u32 = 0x100;

    let count = strings.len();
    let offsets_size = count * 4;
    let strings_start = hdr::STRING_POOL + offsets_size; // styleCount == 0

    // Serialize each string, recording its offset within the data block.
    let mut data = Vec::new();
    let mut offsets = Vec::with_capacity(count);
    for s in strings {
        offsets.push(data.len() as u32);
        if utf8 {
            encode_utf8_string(&mut data, s);
        } else {
            encode_utf16_string(&mut data, s);
        }
    }
    // String data is 4-byte aligned as a block.
    while data.len() % 4 != 0 {
        data.push(0);
    }

    let total = strings_start + data.len();
    let mut out = Vec::with_capacity(total);
    // ResChunk_header.
    out.extend_from_slice(&u16::to_le_bytes(chunk::RES_STRING_POOL_TYPE));
    out.extend_from_slice(&u16::to_le_bytes(hdr::STRING_POOL as u16));
    out.extend_from_slice(&u32::to_le_bytes(total as u32));
    // stringCount / styleCount / flags / stringsStart / stylesStart.
    out.extend_from_slice(&u32::to_le_bytes(count as u32));
    out.extend_from_slice(&u32::to_le_bytes(0));
    out.extend_from_slice(&u32::to_le_bytes(if utf8 { FLAG_UTF8 } else { 0 }));
    out.extend_from_slice(&u32::to_le_bytes(if count == 0 { 0 } else { strings_start as u32 }));
    out.extend_from_slice(&u32::to_le_bytes(0)); // stylesStart
    debug_assert_eq!(out.len(), hdr::STRING_POOL);
    // Offset array.
    for off in &offsets {
        out.extend_from_slice(&u32::to_le_bytes(*off));
    }
    out.extend_from_slice(&data);
    out
}

/// Write one UTF-8 pool string: char-len, byte-len (both length-prefixed),
/// the UTF-8 bytes, then a NUL terminator. Matches `read_utf8`.
fn encode_utf8_string(buf: &mut Vec<u8>, s: &str) {
    let nchars = s.chars().count();
    let bytes = s.as_bytes();
    write_len8(buf, nchars);
    write_len8(buf, bytes.len());
    buf.extend_from_slice(bytes);
    buf.push(0); // NUL terminator
}

/// Write a UTF-8 pool length using aapt's high-bit continuation scheme.
fn write_len8(buf: &mut Vec<u8>, len: usize) {
    if len > 0x7f {
        buf.push((0x80 | ((len >> 8) & 0x7f)) as u8);
        buf.push((len & 0xff) as u8);
    } else {
        buf.push((len & 0x7f) as u8);
    }
}

/// Write one UTF-16 pool string: u16 char-len then UTF-16 code units and a NUL.
/// Matches `read_utf16` (single-word length path; we cap to < 0x8000).
fn encode_utf16_string(buf: &mut Vec<u8>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    let len = units.len();
    if len > 0x7fff {
        // Two-word length form: high word first with continuation bit.
        buf.extend_from_slice(&u16::to_le_bytes((0x8000 | ((len >> 16) & 0x7fff)) as u16));
        buf.extend_from_slice(&u16::to_le_bytes((len & 0xffff) as u16));
    } else {
        buf.extend_from_slice(&u16::to_le_bytes(len as u16));
    }
    for u in units {
        buf.extend_from_slice(&u16::to_le_bytes(u));
    }
    buf.extend_from_slice(&u16::to_le_bytes(0)); // NUL terminator
}

/// Parse a `RES_STRING_POOL_TYPE` chunk into a list of strings.
fn parse_string_pool(chunk: &[u8]) -> Vec<String> {
    if chunk.len() < 28 {
        return Vec::new();
    }
    let header_size = LE::read_u16(&chunk[2..4]) as usize;
    let count = LE::read_u32(&chunk[8..12]) as usize;
    let flags = LE::read_u32(&chunk[16..20]);
    let strings_start = LE::read_u32(&chunk[20..24]) as usize;
    let utf8 = flags & 0x100 != 0;

    let offsets_base = header_size.max(28);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let at = offsets_base + i * 4;
        if at + 4 > chunk.len() {
            break;
        }
        let off = LE::read_u32(&chunk[at..at + 4]) as usize;
        let sat = strings_start + off;
        let s = if utf8 {
            read_utf8(chunk, sat)
        } else {
            read_utf16(chunk, sat)
        };
        out.push(s);
    }
    out
}

fn read_utf8(buf: &[u8], at: usize) -> String {
    let (_, p) = read_len8(buf, at);
    let (blen, p) = read_len8(buf, p);
    let end = (p + blen).min(buf.len());
    String::from_utf8_lossy(&buf[p..end]).into_owned()
}

fn read_len8(buf: &[u8], at: usize) -> (usize, usize) {
    if at >= buf.len() {
        return (0, at);
    }
    let b0 = buf[at] as usize;
    if b0 & 0x80 != 0 && at + 1 < buf.len() {
        (((b0 & 0x7f) << 8) | buf[at + 1] as usize, at + 2)
    } else {
        (b0, at + 1)
    }
}

fn read_utf16(buf: &[u8], at: usize) -> String {
    if at + 1 >= buf.len() {
        return String::new();
    }
    let first = LE::read_u16(&buf[at..at + 2]) as usize;
    let (len, p) = if first & 0x8000 != 0 && at + 3 < buf.len() {
        ((((first & 0x7fff) << 16) | LE::read_u16(&buf[at + 2..at + 4]) as usize), at + 4)
    } else {
        (first, at + 2)
    };
    let end = (p + len * 2).min(buf.len());
    let mut units = Vec::new();
    let mut i = p;
    while i + 1 < end {
        units.push(LE::read_u16(&buf[i..i + 2]));
        i += 2;
    }
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a tiny table, encode it, parse it back, and check survival.
    #[test]
    fn round_trip_simple_entries() {
        // value_pool: index 0 = "Hello", index 1 = "World".
        let value_pool = vec!["Hello".to_string(), "World".to_string()];

        // String value type (0x03 == TYPE_STRING) whose `data` indexes value_pool.
        let mk_string_entry = |entry_id: u16, key: &str, val_idx: u32| ResEntry {
            res_id: (0x7f << 24) | (0x01 << 16) | entry_id as u32,
            type_id: 1,
            type_name: "string".to_string(),
            entry_id,
            key: key.to_string(),
            config: ResConfig::default(),
            value: EntryValue::Simple(ResValueLite {
                data_type: 0x03,
                data: val_idx,
            }),
        };

        let pkg = ResTablePackage {
            id: 0x7f,
            name: "com.example".to_string(),
            type_names: vec!["string".to_string()],
            key_names: vec!["greeting".to_string(), "place".to_string()],
            entries: vec![
                mk_string_entry(0, "greeting", 0),
                mk_string_entry(1, "place", 1),
            ],
        };

        let mut table = ResTable {
            value_pool,
            packages: vec![pkg],
            index: Default::default(),
        };
        table.build_index();

        // Encode → parse round-trip.
        let bytes = encode(&table).expect("encode");
        let back = parse(&bytes).expect("parse");

        // Global value pool survives.
        assert_eq!(back.value_pool, vec!["Hello", "World"]);

        // One package with id 0x7f.
        assert_eq!(back.packages.len(), 1);
        let p = &back.packages[0];
        assert_eq!(p.id, 0x7f);
        assert_eq!(p.type_names, vec!["string"]);

        // Both entries survive with correct keys, types, and value indices.
        let mut got: Vec<(String, u32)> = p
            .entries
            .iter()
            .map(|e| {
                assert_eq!(e.type_name, "string");
                match &e.value {
                    EntryValue::Simple(v) => {
                        assert_eq!(v.data_type, 0x03);
                        (e.key.clone(), v.data)
                    }
                    _ => panic!("expected simple entry"),
                }
            })
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec![("greeting".to_string(), 0), ("place".to_string(), 1)]
        );

        // The decoded value strings resolve through the value pool.
        assert_eq!(back.value_string(0), Some("Hello"));
        assert_eq!(back.value_string(1), Some("World"));
    }

    /// A complex (map) entry should also re-parse.
    #[test]
    fn round_trip_complex_entry() {
        let pkg = ResTablePackage {
            id: 0x7f,
            name: "com.example".to_string(),
            type_names: vec!["style".to_string()],
            key_names: vec!["MyStyle".to_string()],
            entries: vec![ResEntry {
                res_id: (0x7f << 24) | (0x01 << 16),
                type_id: 1,
                type_name: "style".to_string(),
                entry_id: 0,
                key: "MyStyle".to_string(),
                config: ResConfig::default(),
                value: EntryValue::Complex {
                    parent: 0,
                    items: vec![
                        (0x0101_0095, ResValueLite { data_type: 0x1d, data: 0xff00_00ff }),
                        (0x0101_0098, ResValueLite { data_type: 0x05, data: 42 }),
                    ],
                },
            }],
        };
        let mut table = ResTable {
            value_pool: Vec::new(),
            packages: vec![pkg],
            index: Default::default(),
        };
        table.build_index();

        let bytes = encode(&table).expect("encode");
        let back = parse(&bytes).expect("parse");

        let e = &back.packages[0].entries[0];
        assert_eq!(e.key, "MyStyle");
        match &e.value {
            EntryValue::Complex { parent, items } => {
                assert_eq!(*parent, 0);
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].0, 0x0101_0095);
                assert_eq!(items[0].1.data, 0xff00_00ff);
                assert_eq!(items[1].1.data, 42);
            }
            _ => panic!("expected complex entry"),
        }
    }
}
