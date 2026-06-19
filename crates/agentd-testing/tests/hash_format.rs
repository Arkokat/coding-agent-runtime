use agentd_testing::http_mock::hash_body;
use serde_json::json;

#[test]
fn hash_of_empty_body_is_known_sha256() {
    assert_eq!(
        hash_body(b""),
        "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn hash_of_hello_world_is_known_sha256() {
    assert_eq!(
        hash_body(b"hello world"),
        "sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

#[test]
fn hash_is_64_hex_chars_after_prefix() {
    let h = hash_body(b"x");
    assert!(h.starts_with("sha256:"));
    assert_eq!(h.len(), "sha256:".len() + 64);
    let hex = &h["sha256:".len()..];
    assert!(
        hex.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
}

#[test]
fn hash_distinguishes_different_bodies() {
    assert_ne!(hash_body(b"a"), hash_body(b"b"));
}

#[test]
fn http_mock_response_uses_sha256_in_error_body() {
    let body = serde_json::to_vec(&json!({"x": 1})).unwrap();
    let h = hash_body(&body);
    assert!(h.starts_with("sha256:"));
    assert_eq!(h.len(), 71);
}
