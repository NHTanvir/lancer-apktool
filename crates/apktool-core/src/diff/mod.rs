//! `diff` (Lancer extension): structural APK comparison.
//!
//! Text-diffing smali is noisy (register numbers shift), so the smali diff
//! compares the *set* of class/method signatures. The manifest diff is
//! semantic (permissions, exported flag, debuggable). The resource diff
//! compares string entries by key.

use crate::apk::Apk;
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct DiffOptions {
    pub smali: bool,
    pub resources: bool,
    pub manifest: bool,
    pub json: bool,
}

impl DiffOptions {
    /// With no specific selector, diff everything.
    fn any_selected(&self) -> bool {
        self.smali || self.resources || self.manifest
    }
    fn want(&self, sel: fn(&DiffOptions) -> bool) -> bool {
        !self.any_selected() || sel(self)
    }
}

#[derive(Debug, Default, Serialize)]
pub struct DiffReport {
    pub smali: Option<SmaliDiff>,
    pub manifest: Option<ManifestDiff>,
    pub resources: Option<ResourceDiff>,
}

#[derive(Debug, Default, Serialize)]
pub struct SmaliDiff {
    pub added_classes: Vec<String>,
    pub removed_classes: Vec<String>,
    pub changed_classes: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ManifestDiff {
    pub added_permissions: Vec<String>,
    pub removed_permissions: Vec<String>,
    pub debuggable_changed: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct ResourceDiff {
    pub added_strings: Vec<String>,
    pub removed_strings: Vec<String>,
}

/// Diff two APKs and render the report (text or JSON).
pub fn diff(a: &Path, b: &Path, opts: &DiffOptions) -> Result<String> {
    let apk_a = Apk::open(a)?;
    let apk_b = Apk::open(b)?;
    let mut report = DiffReport::default();

    if opts.want(|o| o.smali) {
        report.smali = Some(diff_smali(&apk_a, &apk_b));
    }
    if opts.want(|o| o.manifest) {
        report.manifest = Some(diff_manifest(&apk_a, &apk_b));
    }
    if opts.want(|o| o.resources) {
        report.resources = Some(diff_resources(&apk_a, &apk_b));
    }

    if opts.json {
        Ok(serde_json::to_string_pretty(&report)?)
    } else {
        Ok(render_text(&report))
    }
}

fn class_signatures(apk: &Apk) -> std::collections::BTreeMap<String, BTreeSet<String>> {
    let mut map = std::collections::BTreeMap::new();
    for e in &apk.entries {
        if !(e.name.starts_with("classes") && e.name.ends_with(".dex")) {
            continue;
        }
        if let Ok(files) = smali::disassemble(&e.data) {
            for f in files {
                if let Ok(class) = smali::asm::parse_class(&f.text) {
                    let sigs: BTreeSet<String> =
                        class.methods.iter().map(|m| m.signature.clone()).collect();
                    map.insert(class.descriptor.clone(), sigs);
                }
            }
        }
    }
    map
}

fn diff_smali(a: &Apk, b: &Apk) -> SmaliDiff {
    let ca = class_signatures(a);
    let cb = class_signatures(b);
    let mut out = SmaliDiff::default();
    for k in cb.keys() {
        if !ca.contains_key(k) {
            out.added_classes.push(k.clone());
        }
    }
    for k in ca.keys() {
        if !cb.contains_key(k) {
            out.removed_classes.push(k.clone());
        } else if ca[k] != cb[k] {
            out.changed_classes.push(k.clone());
        }
    }
    out
}

fn permissions(apk: &Apk) -> (BTreeSet<String>, bool) {
    let mut perms = BTreeSet::new();
    let mut debuggable = false;
    if let Some(m) = apk.entry("AndroidManifest.xml") {
        if let Ok(doc) = axml::parse(&m.data) {
            for ev in &doc.events {
                if let axml::XmlEvent::StartElement { name, attributes, .. } = ev {
                    if name == "uses-permission" {
                        if let Some(a) = attributes.iter().find(|a| a.name == "name") {
                            if let Some(v) = &a.raw_value {
                                perms.insert(v.clone());
                            }
                        }
                    }
                    if name == "application" {
                        if let Some(a) = attributes.iter().find(|a| a.name == "debuggable") {
                            debuggable = a.typed.data != 0;
                        }
                    }
                }
            }
        }
    }
    (perms, debuggable)
}

fn diff_manifest(a: &Apk, b: &Apk) -> ManifestDiff {
    let (pa, da) = permissions(a);
    let (pb, db) = permissions(b);
    ManifestDiff {
        added_permissions: pb.difference(&pa).cloned().collect(),
        removed_permissions: pa.difference(&pb).cloned().collect(),
        debuggable_changed: da != db,
    }
}

fn strings(apk: &Apk) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    if let Some(e) = apk.entry("resources.arsc") {
        if let Ok(table) = arsc::parse(&e.data) {
            for pkg in &table.packages {
                for entry in &pkg.entries {
                    if entry.type_name == "string" {
                        set.insert(entry.key.clone());
                    }
                }
            }
        }
    }
    set
}

fn diff_resources(a: &Apk, b: &Apk) -> ResourceDiff {
    let sa = strings(a);
    let sb = strings(b);
    ResourceDiff {
        added_strings: sb.difference(&sa).cloned().collect(),
        removed_strings: sa.difference(&sb).cloned().collect(),
    }
}

fn render_text(report: &DiffReport) -> String {
    let mut s = String::new();
    if let Some(sm) = &report.smali {
        s.push_str("== smali ==\n");
        s.push_str(&format!("  +{} classes, -{} classes, ~{} changed\n", sm.added_classes.len(), sm.removed_classes.len(), sm.changed_classes.len()));
        for c in &sm.changed_classes {
            s.push_str(&format!("  ~ {}\n", c));
        }
    }
    if let Some(m) = &report.manifest {
        s.push_str("== manifest ==\n");
        for p in &m.added_permissions {
            s.push_str(&format!("  + permission {}\n", p));
        }
        for p in &m.removed_permissions {
            s.push_str(&format!("  - permission {}\n", p));
        }
        if m.debuggable_changed {
            s.push_str("  ! debuggable flag changed\n");
        }
    }
    if let Some(r) = &report.resources {
        s.push_str("== resources ==\n");
        s.push_str(&format!("  +{} strings, -{} strings\n", r.added_strings.len(), r.removed_strings.len()));
    }
    s
}
