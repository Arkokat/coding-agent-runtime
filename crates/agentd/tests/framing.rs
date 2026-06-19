#![allow(clippy::expect_used)]

use serde_json::json;

#[test]
fn roundtrips_single_message() {
    let mut buf: Vec<u8> = Vec::new();
    agentd::ipc::framing::write_message(&mut buf, &json!({"id":1,"method":"ping"})).expect("write");
    let mut cur = std::io::Cursor::new(buf);
    let msg = agentd::ipc::framing::read_message(&mut cur)
        .expect("some")
        .expect("ok");
    assert_eq!(msg["method"], "ping");
}

#[test]
fn reads_multiple_messages_from_same_buffer() {
    let mut buf: Vec<u8> = Vec::new();
    agentd::ipc::framing::write_message(&mut buf, &json!({"i": 1})).expect("w");
    agentd::ipc::framing::write_message(&mut buf, &json!({"i": 2})).expect("w");
    agentd::ipc::framing::write_message(&mut buf, &json!({"i": 3})).expect("w");
    let mut cur = std::io::Cursor::new(buf);
    for i in 1..=3 {
        let m = agentd::ipc::framing::read_message(&mut cur)
            .expect("some")
            .expect("ok");
        assert_eq!(m["i"], i);
    }
    assert!(agentd::ipc::framing::read_message(&mut cur).is_none());
}

#[test]
fn read_returns_none_on_eof() {
    let mut cur = std::io::Cursor::new(Vec::<u8>::new());
    assert!(agentd::ipc::framing::read_message(&mut cur).is_none());
}

#[test]
fn read_rejects_oversized_line() {
    let big = "x".repeat(agentd::ipc::framing::MAX_LINE_BYTES + 1);
    let mut cur = std::io::Cursor::new(big.into_bytes());
    let r = agentd::ipc::framing::read_message(&mut cur).expect("some");
    assert!(r.is_err());
}

#[test]
fn read_rejects_invalid_json() {
    let mut cur = std::io::Cursor::new(b"not json\n".to_vec());
    let r = agentd::ipc::framing::read_message(&mut cur).expect("some");
    assert!(r.is_err());
}

#[test]
fn written_messages_are_newline_terminated() {
    let mut buf: Vec<u8> = Vec::new();
    agentd::ipc::framing::write_message(&mut buf, &json!({"a": 1})).expect("w");
    agentd::ipc::framing::write_message(&mut buf, &json!({"a": 2})).expect("w");
    let s = String::from_utf8(buf).expect("utf8");
    assert_eq!(
        s.matches('\n').count(),
        2,
        "expected 2 newlines, got: {s:?}"
    );
}
