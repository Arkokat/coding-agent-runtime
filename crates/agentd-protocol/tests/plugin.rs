use agentd_protocol::Plugin;

fn sample_plugin() -> Plugin {
    Plugin {
        name: "opencode".into(),
        binary: "agentd-plugin-opencode".into(),
        socket_name: "opencode.sock".into(),
    }
}

#[test]
fn plugin_roundtrips_through_json() {
    let original = sample_plugin();
    let json = serde_json::to_string(&original).unwrap();
    let parsed: Plugin = serde_json::from_str(&json).unwrap();
    assert_eq!(original, parsed);
}

#[test]
fn plugin_deserializes_from_daemon_payload_shape() {
    // The daemon's `plugin.connected` bus event emits exactly this shape
    // (see `handlers/plugin_handlers::plugin_hello`). TUI relies on the
    // roundtrip, so guard it here.
    let payload = serde_json::json!({
        "name": "opencode",
        "binary": "agentd-plugin-opencode",
        "socket_name": "opencode.sock",
    });
    let p: Plugin = serde_json::from_value(payload).unwrap();
    assert_eq!(p.name, "opencode");
    assert_eq!(p.binary, "agentd-plugin-opencode");
    assert_eq!(p.socket_name, "opencode.sock");
}
