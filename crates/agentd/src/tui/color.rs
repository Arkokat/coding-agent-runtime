//! ANSI 256 color palette per spec §7. Working=71, Waiting=178, Errored=167, Idle=244.

use agentd_protocol::SessionStatus;
use ratatui::style::{Color, Style};

/// Color group for a session status. `Idle` covers `Starting` and `Finished`
/// too — there's no distinct visual signal for them in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusColor {
    /// Working (`●` cyan-green).
    Working,
    /// Waiting for user input (`⚠` amber).
    Waiting,
    /// Errored (`✕` red).
    Errored,
    /// Idle, starting, or finished (`◌` dim grey).
    Idle,
}

/// Build a `ratatui::style::Style` for the given status group.
pub fn style_for(c: StatusColor) -> Style {
    let fg = match c {
        StatusColor::Working => Color::Indexed(71),
        StatusColor::Waiting => Color::Indexed(178),
        StatusColor::Errored => Color::Indexed(167),
        StatusColor::Idle => Color::Indexed(244),
    };
    Style::default().fg(fg)
}

/// Returns the symbol for a session status. Always present (color-independent).
#[allow(clippy::enum_glob_use)]
pub fn symbol_for(status: SessionStatus) -> &'static str {
    use SessionStatus::*;
    match status {
        Working => "●",
        WaitingForUser => "⚠",
        Errored => "✕",
        Idle | Starting | Finished => "◌",
    }
}

/// Map a `SessionStatus` to a `StatusColor`. `Starting` and `Finished` share
/// the `Idle` color in v1.
#[allow(clippy::enum_glob_use)]
pub fn color_for(status: SessionStatus) -> StatusColor {
    use SessionStatus::*;
    match status {
        Working => StatusColor::Working,
        WaitingForUser => StatusColor::Waiting,
        Errored => StatusColor::Errored,
        _ => StatusColor::Idle,
    }
}
