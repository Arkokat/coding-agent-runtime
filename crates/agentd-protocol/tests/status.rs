use agentd_protocol::SessionStatus;

#[test]
fn serializes_starting_as_lowercase() {
    assert_eq!(
        serde_json::to_string(&SessionStatus::Starting).unwrap(),
        r#""starting""#
    );
}

#[test]
fn serializes_idle_as_lowercase() {
    assert_eq!(
        serde_json::to_string(&SessionStatus::Idle).unwrap(),
        r#""idle""#
    );
}

#[test]
fn serializes_working_as_lowercase() {
    assert_eq!(
        serde_json::to_string(&SessionStatus::Working).unwrap(),
        r#""working""#
    );
}

#[test]
fn serializes_waiting_for_user_with_underscores() {
    assert_eq!(
        serde_json::to_string(&SessionStatus::WaitingForUser).unwrap(),
        r#""waiting_for_user""#
    );
}

#[test]
fn serializes_errored_as_lowercase() {
    assert_eq!(
        serde_json::to_string(&SessionStatus::Errored).unwrap(),
        r#""errored""#
    );
}

#[test]
fn serializes_finished_as_lowercase() {
    assert_eq!(
        serde_json::to_string(&SessionStatus::Finished).unwrap(),
        r#""finished""#
    );
}

#[test]
fn deserializes_from_lowercase_string() {
    let s: SessionStatus = serde_json::from_str(r#""starting""#).unwrap();
    assert_eq!(s, SessionStatus::Starting);
}

#[test]
fn deserializes_waiting_for_user() {
    let s: SessionStatus = serde_json::from_str(r#""waiting_for_user""#).unwrap();
    assert_eq!(s, SessionStatus::WaitingForUser);
}

#[test]
fn rejects_unknown_status() {
    let result: Result<SessionStatus, _> = serde_json::from_str(r#""bogus""#);
    assert!(result.is_err());
}

#[test]
fn rejects_uppercase_status() {
    let result: Result<SessionStatus, _> = serde_json::from_str(r#""STARTING""#);
    assert!(result.is_err());
}

#[test]
fn display_matches_serde() {
    assert_eq!(SessionStatus::Working.to_string(), "working");
    assert_eq!(
        SessionStatus::WaitingForUser.to_string(),
        "waiting_for_user"
    );
}
