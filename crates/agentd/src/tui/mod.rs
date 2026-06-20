//! `agentd tui` subcommand. See submodules for the actual work.

pub mod color;
pub mod event_source;
pub mod input;
pub mod render;
pub mod state;

pub use color::{StatusColor, color_for, style_for, symbol_for};
pub use event_source::connect_and_subscribe;
pub use render::render;
pub use state::{
    FLASH_DURATION, NewModal, RenameModal, STATUS_MESSAGE_DURATION, StatusCounters, TuiState,
};

/// TUI entry point. Wired in Task 9.
#[allow(clippy::unused_async)]
pub async fn run() -> anyhow::Result<()> {
    Ok(())
}
