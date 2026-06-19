use agentd_protocol::Method;

#[test]
fn control_methods_have_expected_names() {
    assert_eq!(Method::STATE_SNAPSHOT, "state.snapshot");
    assert_eq!(Method::SESSION_CREATE, "session.create");
    assert_eq!(Method::SESSION_RENAME, "session.rename");
    assert_eq!(Method::SESSION_JUMP, "session.jump");
    assert_eq!(Method::SESSION_KILL, "session.kill");
    assert_eq!(Method::SESSION_DISMISS_ERROR, "session.dismiss_error");
    assert_eq!(Method::SESSION_GET, "session.get");
    assert_eq!(Method::SESSION_EVENTS, "session.events");
    assert_eq!(Method::DAEMON_STATUS, "daemon.status");
    assert_eq!(Method::DAEMON_SHUTDOWN, "daemon.shutdown");
    assert_eq!(Method::PLUGIN_LIST, "plugin.list");
    assert_eq!(Method::PLUGIN_START, "plugin.start");
    assert_eq!(Method::PLUGIN_STOP, "plugin.stop");
    assert_eq!(Method::PLUGIN_INSTALL, "plugin.install");
    assert_eq!(Method::PLUGIN_UPDATE, "plugin.update");
    assert_eq!(Method::PLUGIN_REMOVE, "plugin.remove");
    assert_eq!(Method::SUBSCRIBE, "subscribe");
    assert_eq!(Method::UNSUBSCRIBE, "unsubscribe");
    assert_eq!(Method::METRICS, "metrics");
}

#[test]
fn plugin_methods_have_expected_names() {
    assert_eq!(Method::PLUGIN_HELLO, "plugin.hello");
    assert_eq!(Method::SESSION_REPORT_EVENT, "session.report_event");
    assert_eq!(Method::SESSION_DISCOVER, "session.discover");
    assert_eq!(Method::PLUGIN_HEARTBEAT, "plugin.heartbeat");
    assert_eq!(Method::PLUGIN_BYE, "plugin.bye");
}

#[test]
fn event_method_name() {
    assert_eq!(Method::EVENT, "event");
}

#[test]
fn all_method_constants_are_distinct() {
    let all = [
        Method::STATE_SNAPSHOT,
        Method::SESSION_CREATE,
        Method::SESSION_RENAME,
        Method::SESSION_JUMP,
        Method::SESSION_KILL,
        Method::SESSION_DISMISS_ERROR,
        Method::SESSION_GET,
        Method::SESSION_EVENTS,
        Method::DAEMON_STATUS,
        Method::DAEMON_SHUTDOWN,
        Method::PLUGIN_LIST,
        Method::PLUGIN_START,
        Method::PLUGIN_STOP,
        Method::PLUGIN_INSTALL,
        Method::PLUGIN_UPDATE,
        Method::PLUGIN_REMOVE,
        Method::SUBSCRIBE,
        Method::UNSUBSCRIBE,
        Method::METRICS,
        Method::PLUGIN_HELLO,
        Method::SESSION_REPORT_EVENT,
        Method::SESSION_DISCOVER,
        Method::PLUGIN_HEARTBEAT,
        Method::PLUGIN_BYE,
        Method::EVENT,
    ];
    let unique: std::collections::HashSet<_> = all.iter().copied().collect();
    assert_eq!(unique.len(), all.len(), "duplicate method names found");
}
