//! `resources.arsc` + compiled `res/**/*.xml` → readable `res/` tree.

use crate::apk::Apk;
use crate::resolver::TableResolver;
use anyhow::{Context, Result};
use arsc::table::{EntryValue, ResValueLite};
use arsc::ResTable;
use axml::{value_type as vt, RefResolver};
use std::collections::BTreeMap;
use std::path::Path;

/// Emit `res/values[-qualifier]/*.xml` from the parsed resource table.
pub fn emit_values(table: &ResTable, resolver: &TableResolver, out: &Path) -> Result<()> {
    // (qualifier, target file) -> body lines
    let mut files: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();

    for pkg in &table.packages {
        for entry in &pkg.entries {
            let qual = entry.config.qualifier();
            let dir = if qual.is_empty() {
                "values".to_string()
            } else {
                format!("values-{}", qual)
            };
            let (file, line) = match render_entry(table, resolver, &entry.type_name, &entry.key, &entry.value) {
                Some(v) => v,
                None => continue,
            };
            files
                .entry((dir, file))
                .or_default()
                .push(line);
        }
    }

    for ((dir, file), lines) in files {
        let dirpath = out.join("res").join(&dir);
        std::fs::create_dir_all(&dirpath)
            .with_context(|| format!("creating {}", dirpath.display()))?;
        let mut body = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n");
        for l in lines {
            body.push_str("    ");
            body.push_str(&l);
            body.push('\n');
        }
        body.push_str("</resources>\n");
        std::fs::write(dirpath.join(&file), body)?;
    }
    Ok(())
}

/// Render one entry into (target file, xml line). Returns None to skip.
fn render_entry(
    table: &ResTable,
    resolver: &TableResolver,
    type_name: &str,
    key: &str,
    value: &EntryValue,
) -> Option<(String, String)> {
    match value {
        EntryValue::Simple(v) => {
            // File-backed resources (drawable/layout/...) point at a path string;
            // those are extracted as files, not emitted into values XML.
            if v.data_type == vt::TYPE_STRING {
                if let Some(s) = table.value_string(v.data) {
                    if s.starts_with("res/") {
                        return None;
                    }
                }
            }
            let text = format_value(table, resolver, *v);
            let (file, line) = match type_name {
                "string" => ("strings.xml", format!("<string name=\"{}\">{}</string>", key, esc(&text))),
                "color" => ("colors.xml", format!("<color name=\"{}\">{}</color>", key, esc(&text))),
                "dimen" => ("dimens.xml", format!("<dimen name=\"{}\">{}</dimen>", key, esc(&text))),
                "bool" => ("bools.xml", format!("<bool name=\"{}\">{}</bool>", key, esc(&text))),
                "integer" => ("integers.xml", format!("<integer name=\"{}\">{}</integer>", key, esc(&text))),
                "id" => ("public.xml", format!("<item type=\"id\" name=\"{}\"/>", key)),
                other => ("values.xml".into(), format!("<item type=\"{}\" name=\"{}\">{}</item>", other, key, esc(&text))),
            };
            Some((file.to_string(), line))
        }
        EntryValue::Complex { items, .. } => {
            let (file, open, close) = match type_name {
                "style" => ("styles.xml", format!("<style name=\"{}\">", key), "</style>"),
                "array" => ("arrays.xml", format!("<array name=\"{}\">", key), "</array>"),
                "plurals" => ("plurals.xml", format!("<plurals name=\"{}\">", key), "</plurals>"),
                "attr" => return None,
                other => (
                    "values.xml",
                    format!("<{} name=\"{}\">", other, key),
                    "",
                ),
            };
            let mut line = open;
            for (name_id, v) in items {
                let text = format_value(table, resolver, *v);
                if type_name == "style" {
                    let attr = resolver
                        .resolve(*name_id)
                        .unwrap_or_else(|| format!("@0x{:08x}", name_id));
                    line.push_str(&format!("<item name=\"{}\">{}</item>", attr.trim_start_matches('@'), esc(&text)));
                } else {
                    line.push_str(&format!("<item>{}</item>", esc(&text)));
                }
            }
            line.push_str(close);
            Some((file.to_string(), line))
        }
    }
}

/// Format an arsc `ResValueLite` to readable text.
fn format_value(table: &ResTable, resolver: &TableResolver, v: ResValueLite) -> String {
    match v.data_type {
        vt::TYPE_NULL => String::new(),
        vt::TYPE_STRING => table.value_string(v.data).unwrap_or("").to_string(),
        vt::TYPE_REFERENCE | vt::TYPE_DYNAMIC_REFERENCE => resolver
            .resolve(v.data)
            .unwrap_or_else(|| format!("@0x{:08x}", v.data)),
        vt::TYPE_ATTRIBUTE => format!("?0x{:08x}", v.data),
        vt::TYPE_INT_DEC => format!("{}", v.data as i32),
        vt::TYPE_INT_HEX => format!("0x{:x}", v.data),
        vt::TYPE_INT_BOOLEAN => if v.data != 0 { "true" } else { "false" }.to_string(),
        vt::TYPE_FLOAT => format!("{}", f32::from_bits(v.data)),
        vt::TYPE_INT_COLOR_ARGB8 => format!("#{:08x}", v.data),
        vt::TYPE_INT_COLOR_RGB8 => format!("#{:06x}", v.data & 0xff_ffff),
        vt::TYPE_INT_COLOR_ARGB4 => format!("#{:08x}", v.data),
        vt::TYPE_INT_COLOR_RGB4 => format!("#{:06x}", v.data & 0xff_ffff),
        _ => format!("0x{:08x}", v.data),
    }
}

/// Decode compiled `res/**/*.xml` files and copy other resources verbatim.
pub fn decode_res_files(apk: &Apk, resolver: &TableResolver, out: &Path) -> Result<()> {
    for e in &apk.entries {
        if !e.name.starts_with("res/") {
            continue;
        }
        let dest = out.join(&e.name);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if e.name.ends_with(".xml") {
            match axml::decode_with(&e.data, resolver) {
                Ok(text) => std::fs::write(&dest, text)?,
                // Not actually AXML (already-plain or odd) — copy raw.
                Err(_) => std::fs::write(&dest, &e.data)?,
            }
        } else {
            std::fs::write(&dest, &e.data)?;
        }
    }
    Ok(())
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
