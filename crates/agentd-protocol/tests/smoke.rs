use agentd_protocol::version;

#[test]
fn smoke_version_returns_crate_version() {
    assert!(!version().is_empty());
    // `char::is_ascii_digit` takes `&char` but `str::starts_with` needs
    // `fn(char) -> bool`, so wrap in a closure.
    assert!(version().starts_with(|c: char| c.is_ascii_digit()));
}

#[test]
fn smoke_protocol_version_is_positive() {
    assert!(agentd_protocol::PROTOCOL_VERSION >= 1);
}
