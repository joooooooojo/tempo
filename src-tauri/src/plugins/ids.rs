//! Plugin and contribution ID validation (design §4.1).

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
    fn accepts_local_ids() {
        assert!(is_valid_local_id("main"));
        assert!(is_valid_local_id("quick-run"));
        assert!(!is_valid_local_id("1bad"));
        assert!(!is_valid_local_id("has/slash"));
    }
}
