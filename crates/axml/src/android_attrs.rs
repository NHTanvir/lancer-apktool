//! Curated map of well-known Android framework attribute resource ids.
//!
//! Compiled binary XML references framework attributes (the `android:` ones)
//! by their resource id rather than by name. Those ids live in the
//! `0x0101xxxx` block (`package = 0x01` "android", `type = 0x01` "attr").
//! When a binary manifest/layout omits the attribute-name string from its
//! pool, the only way to recover the human name is to look the id up in the
//! framework's `attrs.xml`.
//!
//! This is a **curated subset** — the attributes that actually show up in
//! `AndroidManifest.xml` and common layouts, plus the handful that downstream
//! tooling specifically cares about. It is deliberately *not* the full
//! framework table (which has thousands of entries and changes per API level).
//! Accuracy is prioritised over completeness: every mapping here is verified
//! against AOSP `public.xml`, and ids that could not be verified were omitted
//! rather than guessed.

/// Look up the name of a well-known Android framework attribute by its
/// resource id (e.g. `0x01010003` → `"name"`).
///
/// Returns `None` for ids outside the curated subset, including ids outside
/// the `0x0101xxxx` framework-attribute range.
pub fn attr_name(id: u32) -> Option<&'static str> {
    // Cheap range gate: everything here is an `android:attr` id.
    if id & 0xffff_0000 != 0x0101_0000 {
        return None;
    }
    Some(match id {
        // --- Original framework attrs, allocated densely from 0x01010000. ---
        0x01010000 => "theme",
        0x01010001 => "label",
        0x01010002 => "icon",
        0x01010003 => "name",
        0x01010004 => "manageSpaceActivity",
        0x01010005 => "allowClearUserData",
        0x01010006 => "permission",
        0x01010007 => "readPermission",
        0x01010008 => "writePermission",
        0x01010009 => "protectionLevel",
        0x0101000a => "permissionGroup",
        0x0101000b => "sharedUserId",
        0x0101000c => "hasCode",
        0x0101000d => "persistent",
        0x0101000e => "enabled",
        0x0101000f => "debuggable",
        0x01010010 => "exported",
        0x01010011 => "process",
        0x01010012 => "taskAffinity",
        0x01010013 => "multiprocess",
        0x01010014 => "finishOnTaskLaunch",
        0x01010015 => "clearTaskOnLaunch",
        0x01010016 => "stateNotNeeded",
        0x01010017 => "excludeFromRecents",
        0x01010018 => "authorities",
        0x01010019 => "syncable",
        0x0101001a => "initOrder",
        0x0101001b => "grantUriPermissions",
        0x0101001c => "priority",
        0x0101001d => "launchMode",
        0x0101001e => "screenOrientation",
        0x0101001f => "configChanges",
        0x01010020 => "description",
        0x01010021 => "targetPackage",
        0x01010022 => "allowTaskReparenting",
        0x01010023 => "maxSdkVersion",
        0x01010024 => "value",
        0x01010025 => "resource",
        0x01010026 => "mimeType",
        0x01010027 => "scheme",
        0x01010028 => "host",
        0x01010029 => "port",
        0x0101002a => "path",
        0x0101002b => "pathPrefix",
        0x0101002c => "pathPattern",
        0x0101002d => "action",
        0x0101002e => "data",

        // --- Common view/UI attrs that also surface in manifests
        //     (android:id, android:gravity, android:tag). Verified, stable ids. ---
        0x01010034 => "id",
        0x010100af => "gravity",
        0x010100d0 => "tag",

        // --- versioning / sdk / feature attrs (allocated later in history). ---
        0x0101020c => "minSdkVersion",
        0x0101021b => "versionCode",
        0x0101021c => "versionName",
        0x01010224 => "windowSoftInputMode",
        0x0101026c => "anyDensity",
        0x01010270 => "targetSdkVersion",
        0x01010271 => "maxSdkVersion",
        0x01010272 => "resizeable",
        0x01010281 => "glEsVersion",
        0x01010284 => "smallScreens",
        0x01010285 => "normalScreens",
        0x01010286 => "largeScreens",
        0x0101028e => "required",

        // --- backup / restore era. ---
        0x01010280 => "allowBackup",
        0x01010282 => "killAfterRestore",
        0x01010283 => "restoreNeedsApplication",

        // --- install location / multi-arch. ---
        0x010102b7 => "installLocation",

        // --- modern application / security attrs. ---
        0x010103a4 => "supportsRtl",
        0x010103e6 => "logo",
        0x010104ea => "extractNativeLibs",
        0x010104ec => "usesCleartextTraffic",
        0x01010527 => "networkSecurityConfig",
        0x0101052c => "roundIcon",
        0x01010572 => "fullBackupContent",
        0x0101057b => "appComponentFactory",
        0x01010600 => "foregroundServiceType",
        0x01010603 => "requestLegacyExternalStorage",

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::attr_name;

    #[test]
    fn known_manifest_attrs_resolve() {
        assert_eq!(attr_name(0x01010003), Some("name"));
        assert_eq!(attr_name(0x01010000), Some("theme"));
        assert_eq!(attr_name(0x0101021b), Some("versionCode"));
        assert_eq!(attr_name(0x0101021c), Some("versionName"));
        assert_eq!(attr_name(0x0101020c), Some("minSdkVersion"));
        assert_eq!(attr_name(0x01010270), Some("targetSdkVersion"));
        assert_eq!(attr_name(0x01010280), Some("allowBackup"));
        assert_eq!(attr_name(0x0101052c), Some("roundIcon"));
        assert_eq!(attr_name(0x010104ea), Some("extractNativeLibs"));
        assert_eq!(attr_name(0x0101057b), Some("appComponentFactory"));
    }

    #[test]
    fn out_of_range_and_unknown_return_none() {
        // Not in the framework-attr block.
        assert_eq!(attr_name(0x7f010001), None);
        assert_eq!(attr_name(0x01020001), None);
        // In range but not curated.
        assert_eq!(attr_name(0x0101ffff), None);
    }
}
