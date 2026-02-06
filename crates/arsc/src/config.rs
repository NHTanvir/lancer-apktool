//! `ResTable_config` — the packed device-configuration struct that qualifies a
//! resource type table (locale, density, orientation, API level, ...).

use byteorder::{ByteOrder, LittleEndian as LE};

/// A decoded resource configuration. Only the fields needed to render the
/// `res/values-<qualifier>/` directory name are interpreted; the rest are
/// preserved as raw bytes so re-encoding is lossless.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResConfig {
    pub raw: Vec<u8>,
    pub mcc: u16,
    pub mnc: u16,
    pub language: [u8; 2],
    pub country: [u8; 2],
    pub orientation: u8,
    pub touchscreen: u8,
    pub density: u16,
    pub keyboard: u8,
    pub navigation: u8,
    pub screen_width: u16,
    pub screen_height: u16,
    pub sdk_version: u16,
}

impl ResConfig {
    /// Parse a `ResTable_config` from its leading `size` (u32) and body.
    pub fn parse(buf: &[u8]) -> ResConfig {
        let mut c = ResConfig {
            raw: buf.to_vec(),
            ..Default::default()
        };
        let rd = |o: usize, n: usize| -> Option<&[u8]> {
            if o + n <= buf.len() {
                Some(&buf[o..o + n])
            } else {
                None
            }
        };
        if let Some(b) = rd(4, 2) {
            c.mcc = LE::read_u16(b);
        }
        if let Some(b) = rd(6, 2) {
            c.mnc = LE::read_u16(b);
        }
        if let Some(b) = rd(8, 2) {
            c.language = [b[0], b[1]];
        }
        if let Some(b) = rd(10, 2) {
            c.country = [b[0], b[1]];
        }
        if let Some(b) = rd(12, 1) {
            c.orientation = b[0];
        }
        if let Some(b) = rd(13, 1) {
            c.touchscreen = b[0];
        }
        if let Some(b) = rd(14, 2) {
            c.density = LE::read_u16(b);
        }
        if let Some(b) = rd(16, 1) {
            c.keyboard = b[0];
        }
        if let Some(b) = rd(17, 1) {
            c.navigation = b[0];
        }
        if let Some(b) = rd(24, 2) {
            c.screen_width = LE::read_u16(b);
        }
        if let Some(b) = rd(26, 2) {
            c.screen_height = LE::read_u16(b);
        }
        if let Some(b) = rd(28, 2) {
            c.sdk_version = LE::read_u16(b);
        }
        c
    }

    /// Is this the "default" (any) configuration?
    pub fn is_default(&self) -> bool {
        self.qualifier().is_empty()
    }

    /// Render the directory qualifier suffix (e.g. `-en-rUS-xxhdpi-v26`).
    pub fn qualifier(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.mcc != 0 {
            parts.push(format!("mcc{}", self.mcc));
            if self.mnc != 0 {
                parts.push(format!("mnc{}", self.mnc));
            }
        }
        if self.language[0] != 0 {
            let lang = String::from_utf8_lossy(&self.language).to_string();
            if self.country[0] != 0 {
                let country = String::from_utf8_lossy(&self.country).to_string();
                parts.push(format!("{}-r{}", lang, country));
            } else {
                parts.push(lang);
            }
        }
        match self.orientation {
            1 => parts.push("port".into()),
            2 => parts.push("land".into()),
            3 => parts.push("square".into()),
            _ => {}
        }
        if let Some(d) = density_name(self.density) {
            parts.push(d);
        }
        if self.sdk_version != 0 {
            parts.push(format!("v{}", self.sdk_version));
        }
        parts.join("-")
    }
}

fn density_name(d: u16) -> Option<String> {
    Some(
        match d {
            0 => return None,
            120 => "ldpi",
            160 => "mdpi",
            213 => "tvdpi",
            240 => "hdpi",
            320 => "xhdpi",
            480 => "xxhdpi",
            640 => "xxxhdpi",
            0xfffe => "anydpi",
            0xffff => "nodpi",
            other => return Some(format!("{}dpi", other)),
        }
        .to_string(),
    )
}
