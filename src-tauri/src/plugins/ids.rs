//! Plugin and contribution ID validation (design §4.1).

const UUID_LEN: usize = 36;
const UUID_HYPHENS: [usize; 4] = [8, 13, 18, 23];

pub fn is_valid_plugin_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 128 {
        return false;
    }
    if id == "builtin" || id == "tempo" || id.starts_with("builtin.") || id.starts_with("tempo.") {
        return false;
    }
    let mut parts = id.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    if !is_dns_label(first, false) {
        return false;
    }
    let mut count = 1;
    for part in parts {
        count += 1;
        if !is_dns_label(part, true) {
            return false;
        }
    }
    count >= 2
}

/// Repository index `id`: a lowercase UUID, or a legacy reverse-DNS plugin ID.
pub fn is_valid_repository_index_id(id: &str) -> bool {
    is_canonical_uuid(id) || is_valid_plugin_id(id)
}

pub fn new_repository_index_id() -> String {
    uuid::Uuid::new_v4().hyphenated().to_string()
}

fn is_canonical_uuid(id: &str) -> bool {
    if id.len() != UUID_LEN {
        return false;
    }
    let bytes = id.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if UUID_HYPHENS.contains(&index) {
            if *byte != b'-' {
                return false;
            }
            continue;
        }
        if !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase() {
            return false;
        }
    }
    uuid::Uuid::parse_str(id).is_ok()
}

pub fn is_valid_local_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 || id.contains('/') || id.contains("..") {
        return false;
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_dns_label(label: &str, allow_hyphen: bool) -> bool {
    if label.is_empty() {
        return false;
    }
    label.chars().all(|c| {
        c.is_ascii_lowercase()
            || c.is_ascii_digit()
            || (allow_hyphen && c == '-')
    })
}

pub fn runtime_id(plugin_id: &str, local_id: &str) -> String {
    format!("{plugin_id}/{local_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_reverse_dns() {
        assert!(is_valid_plugin_id("com.example.hello"));
        assert!(!is_valid_plugin_id("Hello"));
        assert!(!is_valid_plugin_id("builtin"));
        assert!(!is_valid_plugin_id("tempo.official"));
        assert!(!is_valid_plugin_id("single"));
    }

    #[test]
    fn accepts_repository_index_uuid_and_legacy_id() {
        let generated = new_repository_index_id();
        assert!(is_valid_repository_index_id(&generated));
        assert!(is_canonical_uuid(&generated));
        assert!(is_valid_repository_index_id("7c2f1a90-4b3e-4d8a-9c1b-2e5f6a7b8c9d"));
        assert!(is_valid_repository_index_id("com.example.plugins"));
        assert!(!is_valid_repository_index_id("7C2F1A90-4B3E-4D8A-9C1B-2E5F6A7B8C9D"));
        assert!(!is_valid_repository_index_id("not-a-uuid"));
    }

    #[test]
    fn accepts_local_ids() {
        assert!(is_valid_local_id("main"));
        assert!(is_valid_local_id("quick-run"));
        assert!(!is_valid_local_id("1bad"));
        assert!(!is_valid_local_id("has/slash"));
    }
}
