use agentd_protocol::ProtocolError;

#[test]
fn parse_error_has_code_minus_32700() {
    assert_eq!(ProtocolError::ParseError.code(), -32700);
}

#[test]
fn invalid_request_has_code_minus_32600() {
    assert_eq!(ProtocolError::InvalidRequest.code(), -32600);
}

#[test]
fn method_not_found_has_code_minus_32601() {
    assert_eq!(ProtocolError::MethodNotFound.code(), -32601);
}

#[test]
fn invalid_params_has_code_minus_32602() {
    assert_eq!(ProtocolError::InvalidParams.code(), -32602);
}

#[test]
fn internal_error_has_code_minus_32603() {
    assert_eq!(ProtocolError::InternalError.code(), -32603);
}

#[test]
fn session_not_found_has_code_minus_32001() {
    assert_eq!(ProtocolError::SessionNotFound.code(), -32001);
}

#[test]
fn plugin_not_allowed_has_code_minus_32002() {
    assert_eq!(ProtocolError::PluginNotAllowed.code(), -32002);
}

#[test]
fn permission_denied_has_code_minus_32003() {
    assert_eq!(ProtocolError::PermissionDenied.code(), -32003);
}

#[test]
fn plugin_not_authoritative_has_code_minus_32004() {
    assert_eq!(ProtocolError::PluginNotAuthoritative.code(), -32004);
}

#[test]
fn daemon_shutting_down_has_code_minus_32005() {
    assert_eq!(ProtocolError::DaemonShuttingDown.code(), -32005);
}

#[test]
fn custom_message_overrides_default() {
    let err = ProtocolError::SessionNotFound.with_message("session 01HXYZ not found");
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], -32001);
    assert_eq!(json["message"], "session 01HXYZ not found");
}

#[test]
fn error_implements_display() {
    let err = ProtocolError::InternalError;
    let s = err.to_string();
    assert!(s.contains("internal"));
}
