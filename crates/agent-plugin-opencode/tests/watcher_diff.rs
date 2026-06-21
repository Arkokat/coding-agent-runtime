#![allow(clippy::expect_used)]

//! Tests for the watcher's `diff_pane_states` helper.
//!
//! The diff is based on `(pane_key, pane_current_command)`, not just
//! `pane_key`: a pane whose `pane_current_command` is no longer
//! `opencode` (e.g. opencode exited cleanly and the shell prompt
//! returned) must be treated as vanished even though the pane still
//! exists in tmux.

use std::collections::{HashMap, HashSet};

use agent_plugin_opencode::watcher::diff_pane_states;
use agentd_plugin_sdk::uuid::Uuid;

#[test]
fn diff_pane_states_reports_vanished_and_new() {
    // `known` has a, b, c. `current` has b, c, d (all opencode).
    // → a vanished (not in current); d new (in current, not in known).
    let sid_a = Uuid::now_v7();
    let sid_b = Uuid::now_v7();
    let sid_c = Uuid::now_v7();
    let known: HashMap<String, Uuid> = [
        ("a".to_string(), sid_a),
        ("b".to_string(), sid_b),
        ("c".to_string(), sid_c),
    ]
    .into_iter()
    .collect();
    let current: HashMap<String, String> = [
        ("b".to_string(), "opencode".to_string()),
        ("c".to_string(), "opencode".to_string()),
        ("d".to_string(), "opencode".to_string()),
    ]
    .into_iter()
    .collect();

    let (vanished, new) = diff_pane_states(&known, &current);

    let mut vanished_set: HashSet<String> = vanished.into_iter().collect();
    let mut new_set: HashSet<String> = new.into_iter().collect();
    vanished_set.retain(|k| k != "ignored");
    new_set.retain(|k| k != "ignored");
    assert_eq!(vanished_set, HashSet::from(["a".to_string()]));
    assert_eq!(new_set, HashSet::from(["d".to_string()]));
}

#[test]
fn diff_pane_states_treats_opencode_finished_as_vanished() {
    // opencode exited cleanly. The pane is still in `current`, but its
    // `pane_current_command` is now `zsh` (the shell prompt). The pane
    // must be reported as vanished so the watcher emits
    // `session.finished`.
    let sid_a = Uuid::now_v7();
    let known: HashMap<String, Uuid> = [("a".to_string(), sid_a)].into_iter().collect();
    let current: HashMap<String, String> =
        [("a".to_string(), "zsh".to_string())].into_iter().collect();

    let (vanished, new) = diff_pane_states(&known, &current);
    assert_eq!(vanished, vec!["a".to_string()]);
    assert!(new.is_empty(), "expected no new panes, got: {new:?}");
}

#[test]
fn diff_pane_states_treats_bash_wrapper_as_vanished() {
    // opencode was wrapped in `bash -c 'opencode run ...'`. Once
    // opencode exits, the shell command field is now just the bash
    // wrapper. Must be treated as vanished.
    let sid_a = Uuid::now_v7();
    let known: HashMap<String, Uuid> = [("a".to_string(), sid_a)].into_iter().collect();
    let current: HashMap<String, String> =
        [("a".to_string(), "bash -c 'opencode run ...'".to_string())]
            .into_iter()
            .collect();

    let (vanished, new) = diff_pane_states(&known, &current);
    assert_eq!(vanished, vec!["a".to_string()]);
    assert!(new.is_empty());
}

#[test]
fn diff_pane_states_returns_empty_when_unchanged() {
    let sid_a = Uuid::now_v7();
    let known: HashMap<String, Uuid> = [("a".to_string(), sid_a)].into_iter().collect();
    let current: HashMap<String, String> = [("a".to_string(), "opencode".to_string())]
        .into_iter()
        .collect();

    let (vanished, new) = diff_pane_states(&known, &current);
    assert!(vanished.is_empty());
    assert!(new.is_empty());
}

#[test]
fn diff_pane_states_returns_all_new_when_known_empty() {
    let known: HashMap<String, Uuid> = HashMap::new();
    let current: HashMap<String, String> = [
        ("a".to_string(), "opencode".to_string()),
        ("b".to_string(), "opencode".to_string()),
    ]
    .into_iter()
    .collect();

    let (vanished, new) = diff_pane_states(&known, &current);
    assert!(vanished.is_empty());
    let mut new_sorted = new;
    new_sorted.sort();
    assert_eq!(
        new_sorted,
        vec!["a".to_string(), "b".to_string()],
        "expected both a and b to be reported as new, got: {new_sorted:?}"
    );
}
