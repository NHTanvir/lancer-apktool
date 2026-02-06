//! DEX → smali source emission.

use crate::dex::{ClassDef, CodeItem, Dex, EncodedField, EncodedMethod};
use crate::{descriptor_to_path, opcodes, Error, SmaliFile};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Disassemble every class in `dex`.
pub fn disassemble(dex: &Dex) -> Result<Vec<SmaliFile>, Error> {
    let mut out = Vec::with_capacity(dex.class_defs.len());
    for cd in &dex.class_defs {
        let descriptor = dex.type_descriptor(cd.class_idx as usize);
        let text = disassemble_class(dex, cd, &descriptor);
        out.push(SmaliFile {
            path: descriptor_to_path(&descriptor),
            class_descriptor: descriptor,
            text,
        });
    }
    Ok(out)
}

fn disassemble_class(dex: &Dex, cd: &ClassDef, descriptor: &str) -> String {
    let mut s = String::new();
    let _ = writeln!(s, ".class {}{};", class_access(cd.access_flags), strip_semicolon(descriptor));
    let _ = writeln!(s, ".super {}", dex.type_descriptor(cd.superclass_idx as usize));
    if cd.source_file_idx != 0xffff_ffff {
        let src = dex.string(cd.source_file_idx as usize);
        if !src.is_empty() {
            let _ = writeln!(s, ".source \"{}\"", src);
        }
    }
    for iface in &cd.interfaces {
        let _ = writeln!(s, ".implements {}", dex.type_descriptor(*iface as usize));
    }
    s.push('\n');

    if !cd.class_data.static_fields.is_empty() {
        let _ = writeln!(s, "# static fields");
        for f in &cd.class_data.static_fields {
            emit_field(&mut s, dex, f);
        }
        s.push('\n');
    }
    if !cd.class_data.instance_fields.is_empty() {
        let _ = writeln!(s, "# instance fields");
        for f in &cd.class_data.instance_fields {
            emit_field(&mut s, dex, f);
        }
        s.push('\n');
    }
    if !cd.class_data.direct_methods.is_empty() {
        let _ = writeln!(s, "# direct methods");
        for m in &cd.class_data.direct_methods {
            emit_method(&mut s, dex, m);
        }
    }
    if !cd.class_data.virtual_methods.is_empty() {
        let _ = writeln!(s, "# virtual methods");
        for m in &cd.class_data.virtual_methods {
            emit_method(&mut s, dex, m);
        }
    }
    s
}

fn emit_field(s: &mut String, dex: &Dex, f: &EncodedField) {
    let Some(fid) = dex.field_ids.get(f.field_idx as usize) else {
        return;
    };
    let name = dex.string(fid.name_idx as usize);
    let ty = dex.type_descriptor(fid.type_idx as usize);
    let _ = writeln!(s, ".field {}{}:{}", field_access(f.access_flags), name, ty);
}

fn emit_method(s: &mut String, dex: &Dex, m: &EncodedMethod) {
    let Some(mid) = dex.method_ids.get(m.method_idx as usize) else {
        return;
    };
    let name = dex.string(mid.name_idx as usize).to_string();
    let (ret, params) = dex.proto_descriptor(mid.proto_idx as usize);
    let _ = writeln!(
        s,
        ".method {}{}({}){}",
        method_access(m.access_flags),
        name,
        params.join(""),
        ret
    );

    if let Some(code) = dex.code_item(m.code_off) {
        let _ = writeln!(s, "    .registers {}", code.registers_size);
        emit_instructions(s, dex, &code);
    }
    let _ = writeln!(s, ".end method\n");
}

/// The kind of label that targets a given code offset. Ordering matters for
/// disambiguating when one offset is targeted multiple ways; baksmali prefers
/// the most specific name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LabelKind {
    Goto,
    Cond,
    SwitchCase, // generic case target (named :sswitch_N / :pswitch_N)
    SwitchData, // payload position for a packed/sparse switch
    ArrayData,  // payload position for fill-array-data
}

/// Resolved label names keyed by code-unit offset.
#[derive(Default)]
struct Labels {
    /// offset -> list of label names that must be emitted before that offset.
    at: BTreeMap<usize, Vec<String>>,
    /// offset -> primary branch-target name (for goto/if/switch operands).
    target: BTreeMap<usize, String>,
}

impl Labels {
    fn add(&mut self, off: usize, name: String) {
        let bucket = self.at.entry(off).or_default();
        if !bucket.contains(&name) {
            bucket.push(name.clone());
        }
        // First writer wins as the canonical branch target name.
        self.target.entry(off).or_insert(name);
    }

    fn target_name(&self, off: usize) -> String {
        self.target
            .get(&off)
            .cloned()
            .unwrap_or_else(|| format!(":label_{:x}", off))
    }

    /// The `:catch_N` label declared at `off`, falling back to the canonical
    /// target name if none was recorded.
    fn catch_name(&self, off: usize) -> String {
        if let Some(names) = self.at.get(&off) {
            if let Some(n) = names.iter().find(|n| n.starts_with(":catch_")) {
                return n.clone();
            }
        }
        self.target_name(off)
    }
}

/// Disassemble a method body with baksmali-style labels, payload directives and
/// try/catch annotations.
fn emit_instructions(s: &mut String, dex: &Dex, code: &CodeItem) {
    let insns = &code.insns;

    // ---- Pass 1: walk instructions, collect branch targets and payloads ----
    let labels = collect_labels(dex, code);

    // ---- Emit ----
    let mut pc = 0usize;
    while pc < insns.len() {
        // Emit any labels declared at this offset (in a deterministic order).
        if let Some(names) = labels.at.get(&pc) {
            for name in names {
                let _ = writeln!(s, "    {}", name);
            }
        }

        let width = opcodes::width_at(insns, pc).max(1);
        let op = (insns[pc] & 0xff) as u8;

        if op == 0x00 && (insns[pc] >> 8) != 0 {
            // Payload pseudo-instruction.
            emit_payload(s, insns, pc, &labels);
        } else {
            let line = decode(dex, insns, pc, &labels);
            let _ = writeln!(s, "    {}", line);
        }

        pc += width;
    }
    // Emit any labels that land exactly at the end of the instruction stream
    // (e.g. a :try_end_N that ends at the final instruction's tail).
    if let Some(names) = labels.at.get(&insns.len()) {
        for name in names {
            let _ = writeln!(s, "    {}", name);
        }
    }

    // ---- try/catch directives ----
    emit_tries(s, dex, code, &labels);
}

#[derive(Default)]
struct LabelCounter {
    goto: usize,
    cond: usize,
    sswitch: usize,
    pswitch: usize,
    array: usize,
    catch: usize,
}

/// Pass 1: compute every label target offset and assign names.
fn collect_labels(dex: &Dex, code: &CodeItem) -> Labels {
    let insns = &code.insns;
    let _ = dex;

    // First, classify each branch target offset by kind, gathering raw offsets
    // so names can be assigned in ascending-offset order (matching baksmali's
    // deterministic numbering).
    let mut kinds: BTreeMap<usize, LabelKind> = BTreeMap::new();
    let mut switch_is_packed: BTreeMap<usize, bool> = BTreeMap::new();

    let note = |kinds: &mut BTreeMap<usize, LabelKind>, off: usize, kind: LabelKind| {
        // More specific kinds take precedence (payload/try over plain goto).
        let promote = |existing: &LabelKind, incoming: LabelKind| -> LabelKind {
            use LabelKind::*;
            match (existing, incoming) {
                (Goto, k) => k,
                (Cond, SwitchCase) | (Cond, SwitchData) | (Cond, ArrayData) => incoming,
                (SwitchCase, SwitchData) | (SwitchCase, ArrayData) => incoming,
                _ => *existing,
            }
        };
        kinds
            .entry(off)
            .and_modify(|e| *e = promote(e, kind))
            .or_insert(kind);
    };

    let mut pc = 0usize;
    while pc < insns.len() {
        let width = opcodes::width_at(insns, pc).max(1);
        let op = (insns[pc] & 0xff) as u8;
        let unit = insns[pc];
        let next = |i: usize| -> u16 { *insns.get(pc + i).unwrap_or(&0) };

        match op {
            // goto (10t)
            0x28 => {
                let off = (unit >> 8) as i8 as i32;
                add_target(&mut kinds, pc, off, LabelKind::Goto, note);
            }
            // goto/16 (20t)
            0x29 => {
                let off = next(1) as i16 as i32;
                add_target(&mut kinds, pc, off, LabelKind::Goto, note);
            }
            // goto/32 (30t)
            0x2a => {
                let off = (next(1) as i32) | ((next(2) as i32) << 16);
                add_target(&mut kinds, pc, off, LabelKind::Goto, note);
            }
            // packed-switch / sparse-switch (31t): operand is rel off to payload
            0x2b | 0x2c => {
                let off = (next(1) as i32) | ((next(2) as i32) << 16);
                let payload_pc = (pc as i64 + off as i64).max(0) as usize;
                let packed = op == 0x2b;
                switch_is_packed.insert(payload_pc, packed);
                note(&mut kinds, payload_pc, LabelKind::SwitchData);
                // Case targets are encoded relative to *this* switch's pc.
                collect_switch_cases(insns, pc, payload_pc, packed, &mut kinds, &note);
            }
            // fill-array-data (31t): operand is rel off to payload
            0x26 => {
                let off = (next(1) as i32) | ((next(2) as i32) << 16);
                let payload_pc = (pc as i64 + off as i64) as usize;
                note(&mut kinds, payload_pc, LabelKind::ArrayData);
            }
            // if-test (22t)
            0x32..=0x37 => {
                let off = next(1) as i16 as i32;
                add_target(&mut kinds, pc, off, LabelKind::Cond, note);
            }
            // if-testz (21t)
            0x38..=0x3d => {
                let off = next(1) as i16 as i32;
                add_target(&mut kinds, pc, off, LabelKind::Cond, note);
            }
            _ => {}
        }
        pc += width;
    }

    // Assign names in ascending offset order so numbering is deterministic.
    // Try/catch labels are handled separately below (with per-try numbering)
    // so a try's start/end/catch labels share a consistent index.
    let mut labels = Labels::default();
    let mut counter = LabelCounter::default();
    for (off, kind) in &kinds {
        let name = match kind {
            LabelKind::Goto => {
                let n = counter.goto;
                counter.goto += 1;
                format!(":goto_{}", n)
            }
            LabelKind::Cond => {
                let n = counter.cond;
                counter.cond += 1;
                format!(":cond_{}", n)
            }
            LabelKind::SwitchCase => {
                let packed = switch_case_is_packed(*off, &switch_is_packed, insns);
                if packed {
                    let n = counter.pswitch;
                    counter.pswitch += 1;
                    format!(":pswitch_{}", n)
                } else {
                    let n = counter.sswitch;
                    counter.sswitch += 1;
                    format!(":sswitch_{}", n)
                }
            }
            LabelKind::SwitchData => {
                let packed = *switch_is_packed.get(off).unwrap_or(&false);
                if packed {
                    let n = counter.pswitch;
                    counter.pswitch += 1;
                    format!(":pswitch_data_{}", n)
                } else {
                    let n = counter.sswitch;
                    counter.sswitch += 1;
                    format!(":sswitch_data_{}", n)
                }
            }
            LabelKind::ArrayData => {
                let n = counter.array;
                counter.array += 1;
                format!(":array_{}", n)
            }
        };
        labels.add(*off, name);
    }

    // Try/catch labels: each try gets a matching start/end index; catch targets
    // are numbered globally and deduplicated by offset.
    let mut catch_names: BTreeMap<usize, String> = BTreeMap::new();
    for (i, t) in code.tries.iter().enumerate() {
        let start = t.start_addr as usize;
        let end = start + t.insn_count as usize;
        labels.add(start, format!(":try_start_{}", i));
        labels.add(end, format!(":try_end_{}", i));
        if let Some(h) = code.handlers.iter().find(|h| h.offset == t.handler_off) {
            for c in &h.handlers {
                let addr = c.addr as usize;
                let name = catch_names.entry(addr).or_insert_with(|| {
                    let n = counter.catch;
                    counter.catch += 1;
                    format!(":catch_{}", n)
                });
                labels.add(addr, name.clone());
            }
            if let Some(addr) = h.catch_all_addr {
                let addr = addr as usize;
                let name = catch_names.entry(addr).or_insert_with(|| {
                    let n = counter.catch;
                    counter.catch += 1;
                    format!(":catch_{}", n)
                });
                labels.add(addr, name.clone());
            }
        }
    }

    labels
}

/// Whether a switch-case target belongs to a packed switch. Case targets are
/// not directly tagged with their parent switch, so this is a best-effort:
/// default to sparse unless a packed switch dominates. Since both kinds simply
/// produce `:pswitch_N` / `:sswitch_N` and either is accepted by smali, we
/// resolve via the nearest known switch payload kind.
fn switch_case_is_packed(
    _off: usize,
    switch_is_packed: &BTreeMap<usize, bool>,
    _insns: &[u16],
) -> bool {
    // If any packed switch exists in the method, prefer pswitch naming; this
    // keeps output consistent for the common single-switch case.
    switch_is_packed.values().any(|p| *p)
        && !switch_is_packed.values().any(|p| !*p)
}

/// Resolve a branch offset relative to `pc` and register a target label.
fn add_target<F>(
    kinds: &mut BTreeMap<usize, LabelKind>,
    pc: usize,
    rel: i32,
    kind: LabelKind,
    note: F,
) where
    F: Fn(&mut BTreeMap<usize, LabelKind>, usize, LabelKind),
{
    let target = pc as i64 + rel as i64;
    if target >= 0 {
        note(kinds, target as usize, kind);
    }
}

/// Decode a packed/sparse-switch payload and register each case target offset.
/// Case branch offsets are stored relative to `switch_pc` (the address of the
/// switch instruction), per the Dalvik format.
fn collect_switch_cases<F>(
    insns: &[u16],
    switch_pc: usize,
    payload_pc: usize,
    packed: bool,
    kinds: &mut BTreeMap<usize, LabelKind>,
    note: &F,
) where
    F: Fn(&mut BTreeMap<usize, LabelKind>, usize, LabelKind),
{
    let rd = |i: usize| -> u16 { *insns.get(payload_pc + i).unwrap_or(&0) };
    if payload_pc + 2 > insns.len() {
        return;
    }
    let size = rd(1) as usize;
    // For packed: targets start after ident(1) + size(1) + first_key(2) = 4 units.
    // For sparse: targets start after ident(1) + size(1) + keys(size*2) units.
    let targets_base = if packed { 4 } else { 2 + size * 2 };
    for i in 0..size {
        let lo = rd(targets_base + i * 2) as i32;
        let hi = rd(targets_base + i * 2 + 1) as i32;
        let rel = lo | (hi << 16);
        let target = (switch_pc as i64 + rel as i64).max(0) as usize;
        note(kinds, target, LabelKind::SwitchCase);
    }
}

/// Emit a switch / array-data payload as a real smali directive.
fn emit_payload(s: &mut String, insns: &[u16], pc: usize, labels: &Labels) {
    let ident = insns[pc] >> 8;
    match ident {
        0x01 => emit_packed_switch(s, insns, pc, labels),
        0x02 => emit_sparse_switch(s, insns, pc, labels),
        0x03 => emit_array_data(s, insns, pc),
        _ => {
            let width = opcodes::width_at(insns, pc).max(1);
            let _ = writeln!(s, "    nop # payload ({} units)", width);
        }
    }
}

/// `packed-switch-payload`: ident(0x0100), size u16, first_key i32, targets[size] i32.
/// Targets are relative to the address of the switch instruction.
fn emit_packed_switch(s: &mut String, insns: &[u16], pc: usize, labels: &Labels) {
    let rd = |i: usize| -> u16 { *insns.get(pc + i).unwrap_or(&0) };
    let size = rd(1) as usize;
    let first_key = (rd(2) as i32) | ((rd(3) as i32) << 16);
    let switch_pc = owning_switch_pc(insns, pc);
    let _ = writeln!(s, "    .packed-switch {}", fmt_lit(first_key));
    for i in 0..size {
        let lo = rd(4 + i * 2) as i32;
        let hi = rd(4 + i * 2 + 1) as i32;
        let rel = lo | (hi << 16);
        let target = (switch_pc as i64 + rel as i64).max(0) as usize;
        let _ = writeln!(s, "        {}", labels.target_name(target));
    }
    let _ = writeln!(s, "    .end packed-switch");
}

/// `sparse-switch-payload`: ident(0x0200), size u16, keys[size] i32, targets[size] i32.
fn emit_sparse_switch(s: &mut String, insns: &[u16], pc: usize, labels: &Labels) {
    let rd = |i: usize| -> u16 { *insns.get(pc + i).unwrap_or(&0) };
    let size = rd(1) as usize;
    let switch_pc = owning_switch_pc(insns, pc);
    let _ = writeln!(s, "    .sparse-switch");
    let keys_base = 2;
    let targets_base = keys_base + size * 2;
    for i in 0..size {
        let klo = rd(keys_base + i * 2) as i32;
        let khi = rd(keys_base + i * 2 + 1) as i32;
        let key = klo | (khi << 16);
        let tlo = rd(targets_base + i * 2) as i32;
        let thi = rd(targets_base + i * 2 + 1) as i32;
        let rel = tlo | (thi << 16);
        let target = (switch_pc as i64 + rel as i64).max(0) as usize;
        let _ = writeln!(s, "        {} -> {}", fmt_lit(key), labels.target_name(target));
    }
    let _ = writeln!(s, "    .end sparse-switch");
}

/// `fill-array-data-payload`: ident(0x0300), element_width u16, size u32, data[].
fn emit_array_data(s: &mut String, insns: &[u16], pc: usize) {
    let rd = |i: usize| -> u16 { *insns.get(pc + i).unwrap_or(&0) };
    let element_width = rd(1) as usize;
    let size = (rd(2) as usize) | ((rd(3) as usize) << 16);
    let _ = writeln!(s, "    .array-data {}", element_width.max(1));

    // Data bytes begin right after the 4-unit (8-byte) header.
    let data_unit_start = pc + 4;
    let total_bytes = size * element_width;
    let bytes = read_payload_bytes(insns, data_unit_start, total_bytes);
    if element_width == 0 {
        // Defensive: avoid divide-by-zero; emit raw bytes.
        for b in &bytes {
            let _ = writeln!(s, "        0x{:x}t", b);
        }
    } else {
        for chunk in bytes.chunks(element_width) {
            let mut val: u64 = 0;
            for (i, b) in chunk.iter().enumerate() {
                val |= (*b as u64) << (8 * i);
            }
            let suffix = match element_width {
                1 => "t",
                2 => "s",
                4 => "",
                8 => "L",
                _ => "",
            };
            let _ = writeln!(s, "        0x{:x}{}", val, suffix);
        }
    }
    let _ = writeln!(s, "    .end array-data");
}

/// Read `n` raw bytes from the code-unit array starting at unit `unit_start`.
fn read_payload_bytes(insns: &[u16], unit_start: usize, n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n);
    let mut i = 0usize;
    while out.len() < n {
        let unit = match insns.get(unit_start + i) {
            Some(u) => *u,
            None => break,
        };
        out.push((unit & 0xff) as u8);
        if out.len() < n {
            out.push((unit >> 8) as u8);
        }
        i += 1;
    }
    out
}

/// Find the address of the switch/fill-array-data instruction that references
/// the payload at `payload_pc`. Targets inside switch payloads are encoded
/// relative to the *switch* instruction, so we must scan back to find it.
fn owning_switch_pc(insns: &[u16], payload_pc: usize) -> usize {
    let mut pc = 0usize;
    while pc < insns.len() {
        let width = opcodes::width_at(insns, pc).max(1);
        let op = (insns[pc] & 0xff) as u8;
        if matches!(op, 0x2b | 0x2c | 0x26) {
            let lo = *insns.get(pc + 1).unwrap_or(&0) as i32;
            let hi = *insns.get(pc + 2).unwrap_or(&0) as i32;
            let off = lo | (hi << 16);
            let tgt = (pc as i64 + off as i64).max(0) as usize;
            if tgt == payload_pc {
                return pc;
            }
        }
        pc += width;
    }
    payload_pc
}

/// Emit `.catch` / `.catchall` directives for the method's try table.
///
/// Uses the per-try `:try_start_N`/`:try_end_N` names (matching the indices
/// assigned in [`collect_labels`]) and the deduplicated catch labels resolved
/// from the offset map.
fn emit_tries(s: &mut String, dex: &Dex, code: &CodeItem, labels: &Labels) {
    if code.tries.is_empty() {
        return;
    }
    for (i, t) in code.tries.iter().enumerate() {
        let start_lbl = format!(":try_start_{}", i);
        let end_lbl = format!(":try_end_{}", i);
        let Some(h) = code.handlers.iter().find(|h| h.offset == t.handler_off) else {
            continue;
        };
        for c in &h.handlers {
            let ty = dex.type_descriptor(c.type_idx as usize);
            let catch_lbl = labels.catch_name(c.addr as usize);
            let _ = writeln!(
                s,
                "    .catch {} {{{} .. {}}} {}",
                ty, start_lbl, end_lbl, catch_lbl
            );
        }
        if let Some(addr) = h.catch_all_addr {
            let catch_lbl = labels.catch_name(addr as usize);
            let _ = writeln!(
                s,
                "    .catchall {{{} .. {}}} {}",
                start_lbl, end_lbl, catch_lbl
            );
        }
    }
}

/// Format an integer literal the way smali does (decimal, hex for large).
fn fmt_lit(v: i32) -> String {
    if v.unsigned_abs() < 0x1_0000 {
        format!("{}", v)
    } else if v < 0 {
        format!("-0x{:x}", (v as i64).unsigned_abs())
    } else {
        format!("0x{:x}", v)
    }
}

#[derive(Clone, Copy)]
enum Ref {
    Str,
    Type,
    Field,
    Method,
    Proto,
    Handle,
}

/// Decode a single instruction at `pc` to a smali line. Branch operands are
/// rendered as resolved labels via `labels`.
fn decode(dex: &Dex, insns: &[u16], pc: usize, labels: &Labels) -> String {
    let unit = insns[pc];
    let op = (unit & 0xff) as u8;
    let name = opcodes::name(op);
    let aa = (unit >> 8) as u8;
    let a = ((unit >> 8) & 0xf) as u8;
    let b = ((unit >> 12) & 0xf) as u8;
    let next = |i: usize| -> u16 { *insns.get(pc + i).unwrap_or(&0) };

    let ref_for = |kind: Ref, idx: usize| -> String {
        match kind {
            Ref::Str => format!("\"{}\"", dex.string(idx)),
            Ref::Type => dex.type_descriptor(idx),
            Ref::Field => dex.field_ref(idx),
            Ref::Method => dex.method_ref(idx),
            Ref::Proto => {
                let (ret, params) = dex.proto_descriptor(idx);
                format!("({}){}", params.join(""), ret)
            }
            Ref::Handle => format!("handle@{}", idx),
        }
    };

    match op {
        0x00 => "nop".to_string(),
        0x0e => "return-void".to_string(),
        // 12x
        0x01 | 0x04 | 0x07 | 0x21 | 0x7b..=0x8f | 0xb0..=0xcf => {
            format!("{} v{}, v{}", name, a, b)
        }
        // 11x
        0x0a..=0x0d | 0x0f..=0x11 | 0x1d | 0x1e | 0x27 => format!("{} v{}", name, aa),
        // 11n
        0x12 => format!("{} v{}, #{}", name, a, sign4(b)),
        // 21s / 21h / 21t / 21c handled below by exact opcode
        0x13 | 0x16 => format!("{} v{}, #{}", name, aa, next(1) as i16),
        0x15 | 0x19 => format!("{} v{}, #0x{:x}0000", name, aa, next(1)),
        0x14 | 0x17 => {
            let val = (next(1) as u32) | ((next(2) as u32) << 16);
            format!("{} v{}, #0x{:x}", name, aa, val)
        }
        0x18 => {
            let val = (next(1) as u64)
                | ((next(2) as u64) << 16)
                | ((next(3) as u64) << 32)
                | ((next(4) as u64) << 48);
            format!("{} v{}, #0x{:x}L", name, aa, val)
        }
        0x1a => format!("{} v{}, {}", name, aa, ref_for(Ref::Str, next(1) as usize)),
        0x1b => {
            let idx = (next(1) as usize) | ((next(2) as usize) << 16);
            format!("{} v{}, {}", name, aa, ref_for(Ref::Str, idx))
        }
        0x1c | 0x1f | 0x22 => {
            format!("{} v{}, {}", name, aa, ref_for(Ref::Type, next(1) as usize))
        }
        0x20 | 0x23 => format!(
            "{} v{}, v{}, {}",
            name,
            a,
            b,
            ref_for(Ref::Type, next(1) as usize)
        ),
        0x60..=0x6d => format!("{} v{}, {}", name, aa, ref_for(Ref::Field, next(1) as usize)),
        0x52..=0x5f => format!(
            "{} v{}, v{}, {}",
            name,
            a,
            b,
            ref_for(Ref::Field, next(1) as usize)
        ),
        // branches → resolved labels
        0x28 => {
            let off = (aa as i8) as i32;
            format!("{} {}", name, branch_label(labels, pc, off))
        }
        0x29 => {
            let off = next(1) as i16 as i32;
            format!("{} {}", name, branch_label(labels, pc, off))
        }
        0x2a => {
            let off = (next(1) as i32) | ((next(2) as i32) << 16);
            format!("{} {}", name, branch_label(labels, pc, off))
        }
        // packed-switch / sparse-switch / fill-array-data: operand points at a payload
        0x2b | 0x2c | 0x26 => {
            let off = raw_branch32(insns, pc);
            let payload = (pc as i64 + off as i64).max(0) as usize;
            format!("{} v{}, {}", name, aa, labels.target_name(payload))
        }
        0x32..=0x37 => {
            let off = next(1) as i16 as i32;
            format!("{} v{}, v{}, {}", name, a, b, branch_label(labels, pc, off))
        }
        0x38..=0x3d => {
            let off = next(1) as i16 as i32;
            format!("{} v{}, {}", name, aa, branch_label(labels, pc, off))
        }
        // 22x / 23x
        0x02 | 0x05 | 0x08 => format!("{} v{}, v{}", name, aa, next(1)),
        0x03 | 0x06 | 0x09 => format!("{} v{}, v{}", name, next(1), next(2)),
        0x2d..=0x31 | 0x44..=0x51 | 0x90..=0xaf => {
            let bb = (next(1) & 0xff) as u8;
            let cc = (next(1) >> 8) as u8;
            format!("{} v{}, v{}, v{}", name, aa, bb, cc)
        }
        0xd0..=0xd7 => format!("{} v{}, v{}, #{}", name, a, b, next(1) as i16),
        0xd8..=0xe2 => {
            let bb = (next(1) & 0xff) as u8;
            let cc = (next(1) >> 8) as i8;
            format!("{} v{}, v{}, #{}", name, aa, bb, cc)
        }
        // invoke
        0x6e..=0x72 | 0xfc => {
            let count = (unit >> 12) as usize;
            let g = ((unit >> 8) & 0xf) as u8;
            let idx = next(1) as usize;
            let regs = next(2);
            let mut v = Vec::new();
            for i in 0..count.min(5) {
                let r = if i == 4 {
                    g
                } else {
                    ((regs >> (4 * i)) & 0xf) as u8
                };
                v.push(format!("v{}", r));
            }
            let kind = if op == 0xfc { Ref::Method } else { Ref::Method };
            format!("{} {{{}}}, {}", name, v.join(", "), ref_for(kind, idx))
        }
        0x74..=0x78 | 0xfd => {
            let count = aa as usize;
            let idx = next(1) as usize;
            let start = next(2);
            format!(
                "{} {{v{} .. v{}}}, {}",
                name,
                start,
                start as usize + count.saturating_sub(1),
                ref_for(Ref::Method, idx)
            )
        }
        0x24 => {
            let idx = next(1) as usize;
            format!("{} {{...}}, {}", name, ref_for(Ref::Type, idx))
        }
        0x25 => format!("{} {{...}}, {}", name, ref_for(Ref::Type, next(1) as usize)),
        0xfe => format!("{} v{}, {}", name, aa, ref_for(Ref::Handle, next(1) as usize)),
        0xff => format!("{} v{}, {}", name, aa, ref_for(Ref::Proto, next(1) as usize)),
        _ => format!("{} # 0x{:04x}", name, unit),
    }
}

fn raw_branch32(insns: &[u16], pc: usize) -> i32 {
    let lo = *insns.get(pc + 1).unwrap_or(&0) as i32;
    let hi = *insns.get(pc + 2).unwrap_or(&0) as i32;
    lo | (hi << 16)
}

/// Resolve a relative branch (offset in code units from `pc`) to its label name.
fn branch_label(labels: &Labels, pc: usize, rel: i32) -> String {
    let target = (pc as i64 + rel as i64).max(0) as usize;
    labels.target_name(target)
}

fn sign4(n: u8) -> i32 {
    if n & 0x8 != 0 {
        (n as i32) - 16
    } else {
        n as i32
    }
}

fn strip_semicolon(d: &str) -> &str {
    d.trim_end_matches(';')
}

// --- access flag formatting -------------------------------------------------

fn flag_words(flags: u32, table: &[(u32, &str)]) -> String {
    let mut out = String::new();
    for (bit, word) in table {
        if flags & bit != 0 {
            out.push_str(word);
            out.push(' ');
        }
    }
    out
}

fn class_access(flags: u32) -> String {
    flag_words(
        flags,
        &[
            (0x1, "public"),
            (0x2, "private"),
            (0x4, "protected"),
            (0x10, "final"),
            (0x200, "interface"),
            (0x400, "abstract"),
            (0x1000, "synthetic"),
            (0x2000, "annotation"),
            (0x4000, "enum"),
        ],
    )
}

fn field_access(flags: u32) -> String {
    flag_words(
        flags,
        &[
            (0x1, "public"),
            (0x2, "private"),
            (0x4, "protected"),
            (0x8, "static"),
            (0x10, "final"),
            (0x40, "volatile"),
            (0x80, "transient"),
            (0x1000, "synthetic"),
            (0x4000, "enum"),
        ],
    )
}

fn method_access(flags: u32) -> String {
    flag_words(
        flags,
        &[
            (0x1, "public"),
            (0x2, "private"),
            (0x4, "protected"),
            (0x8, "static"),
            (0x10, "final"),
            (0x20, "synchronized"),
            (0x40, "bridge"),
            (0x80, "varargs"),
            (0x100, "native"),
            (0x400, "abstract"),
            (0x800, "strictfp"),
            (0x1000, "synthetic"),
            (0x10000, "constructor"),
            (0x20000, "declared-synchronized"),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_switch_targets_relative_to_switch() {
        // Layout (code units):
        //  0: packed-switch v0, +3   (op 0x2b, lo=3, hi=0)  width 3
        //  3: packed-switch-payload  ident 0x0100, size=2, first_key=0,
        //     targets[0] = +5 (rel to switch pc 0 => unit 5)
        //     targets[1] = +6 (=> unit 6)
        let insns: Vec<u16> = vec![
            0x002b, 3, 0, // packed-switch v0, +3
            0x0100, 2, 0, 0, // payload: ident, size=2, first_key lo, hi
            5, 0, // target 0 = +5
            6, 0, // target 1 = +6
        ];
        let switch_pc = owning_switch_pc(&insns, 3);
        assert_eq!(switch_pc, 0);

        let mut labels = Labels::default();
        labels.add(5, ":pswitch_0".into());
        labels.add(6, ":pswitch_1".into());
        let mut s = String::new();
        emit_packed_switch(&mut s, &insns, 3, &labels);
        assert!(s.contains(".packed-switch 0"), "got: {s}");
        assert!(s.contains(":pswitch_0"), "got: {s}");
        assert!(s.contains(":pswitch_1"), "got: {s}");
        assert!(s.contains(".end packed-switch"), "got: {s}");
    }

    #[test]
    fn array_data_decodes_bytes() {
        // fill-array-data-payload: ident 0x0300, element_width=1, size=3, data 01 02 03
        let insns: Vec<u16> = vec![
            0x0300, 1, 3, 0, // header
            0x0201, // bytes 01, 02
            0x0003, // byte 03, pad
        ];
        let mut s = String::new();
        emit_array_data(&mut s, &insns, 0);
        assert!(s.contains(".array-data 1"), "got: {s}");
        assert!(s.contains("0x1t"), "got: {s}");
        assert!(s.contains("0x2t"), "got: {s}");
        assert!(s.contains("0x3t"), "got: {s}");
        assert!(s.contains(".end array-data"), "got: {s}");
    }

    #[test]
    fn payload_bytes_pack_low_then_high() {
        let insns = [0x2211u16, 0x4433u16];
        let b = read_payload_bytes(&insns, 0, 4);
        assert_eq!(b, vec![0x11, 0x22, 0x33, 0x44]);
        // Truncated request stops at requested length.
        let b2 = read_payload_bytes(&insns, 0, 3);
        assert_eq!(b2, vec![0x11, 0x22, 0x33]);
    }

    #[test]
    fn fmt_lit_decimal_and_hex() {
        assert_eq!(fmt_lit(0), "0");
        assert_eq!(fmt_lit(-1), "-1");
        assert_eq!(fmt_lit(0x10000), "0x10000");
        assert_eq!(fmt_lit(-0x20000), "-0x20000");
    }
}
