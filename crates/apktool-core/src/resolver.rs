//! Bridges the parsed `arsc` resource table to `axml`'s reference resolver so
//! manifest/layout decoding can render `@string/app_name` instead of raw hex.

use arsc::ResTable;
use axml::RefResolver;

pub struct TableResolver {
    pub app: Option<ResTable>,
    pub framework: Option<ResTable>,
}

impl TableResolver {
    pub fn new(app: Option<ResTable>, framework: Option<ResTable>) -> Self {
        TableResolver { app, framework }
    }
}

impl RefResolver for TableResolver {
    fn resolve(&self, id: u32) -> Option<String> {
        let pkg = (id >> 24) & 0xff;
        if pkg == 0x01 {
            if let Some(fw) = &self.framework {
                if let Some(name) = fw.resolve(id) {
                    return Some(name.replace('@', "@android:"));
                }
            }
            return Some(format!("@android:0x{:08x}", id));
        }
        self.app.as_ref().and_then(|t| t.resolve(id))
    }
}
