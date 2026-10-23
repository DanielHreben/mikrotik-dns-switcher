pub const CLIENT_STATUS: &[&str] = &["DEFAULT", "CUSTOM", "UNMANAGED"];
pub const DEFAULT: &str = "DEFAULT";
pub const CUSTOM: &str = "CUSTOM";
pub const UNMANAGED: &str = "UNMANAGED";

use std::collections::HashMap;

pub fn get_lease_status(lease: Option<&HashMap<String, String>>, comment: &str) -> &'static str {
    match lease {
        None => DEFAULT,
        Some(l) => {
            let c = l.get("comment").cloned().unwrap_or_default();
            let dynamic = l.get("dynamic").cloned().unwrap_or_default();
            if c == comment {
                return CUSTOM;
            }
            if dynamic == "true" || dynamic == "yes" {
                return DEFAULT;
            }
            UNMANAGED
        }
    }
}
