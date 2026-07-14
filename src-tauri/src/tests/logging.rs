use crate::logging::sanitize_log_value;

#[test]
fn console_logging_is_enabled_for_debug_builds() {
    assert_eq!(
        crate::logging::console_logging_enabled(),
        cfg!(debug_assertions)
    );
}

#[test]
fn sanitize_log_value_removes_control_characters_and_truncates() {
    let value = format!("hello\n{}\rworld", "x".repeat(1100));

    let sanitized = sanitize_log_value(&value);

    assert!(!sanitized.contains('\n'));
    assert!(!sanitized.contains('\r'));
    assert!(sanitized.ends_with("..."));
    assert!(sanitized.len() <= 1027);
}
