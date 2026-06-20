#![allow(clippy::expect_used)]

use agentd::tui::{StatusColor, color_for, style_for, symbol_for};
use agentd_protocol::SessionStatus;

#[test]
fn symbol_for_each_status() {
    assert_eq!(symbol_for(SessionStatus::Working), "●");
    assert_eq!(symbol_for(SessionStatus::WaitingForUser), "⚠");
    assert_eq!(symbol_for(SessionStatus::Errored), "✕");
    assert_eq!(symbol_for(SessionStatus::Idle), "◌");
    assert_eq!(symbol_for(SessionStatus::Starting), "◌");
    assert_eq!(symbol_for(SessionStatus::Finished), "◌");
}

#[test]
fn color_for_each_status() {
    assert_eq!(color_for(SessionStatus::Working), StatusColor::Working);
    assert_eq!(
        color_for(SessionStatus::WaitingForUser),
        StatusColor::Waiting
    );
    assert_eq!(color_for(SessionStatus::Errored), StatusColor::Errored);
    assert_eq!(color_for(SessionStatus::Idle), StatusColor::Idle);
    assert_eq!(color_for(SessionStatus::Starting), StatusColor::Idle);
    assert_eq!(color_for(SessionStatus::Finished), StatusColor::Idle);
}

#[test]
fn style_for_returns_distinct_colors() {
    let s_working = style_for(StatusColor::Working);
    let s_waiting = style_for(StatusColor::Waiting);
    let s_errored = style_for(StatusColor::Errored);
    let s_idle = style_for(StatusColor::Idle);
    // Distinct fg colors (ANSI values 71, 178, 167, 244)
    assert_ne!(s_working.fg, s_waiting.fg);
    assert_ne!(s_working.fg, s_errored.fg);
    assert_ne!(s_working.fg, s_idle.fg);
}
