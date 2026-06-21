//! `agentd tui` subcommand. See submodules for the actual work.

pub mod color;
pub mod event_source;
pub mod input;
pub mod new_modal;
pub mod rename_modal;
pub mod render;
pub mod state;

pub use color::{StatusColor, color_for, style_for, symbol_for};
pub use event_source::connect_and_subscribe;
pub use render::render;
pub use state::{
    FLASH_DURATION, NewModal, RenameModal, STATUS_MESSAGE_DURATION, StatusCounters, TuiState,
};

/// TUI entry point: connect to the daemon, render the dashboard, handle input.
///
/// Wires up the terminal, opens an event subscription on the daemon's
/// control UDS, fetches an initial state snapshot, then runs the event
/// loop until the user quits (`q`, `Esc`, or close from a modal). If the
/// daemon is not running, prints a helpful message to stderr and
/// returns `Ok(())` so the CLI exits cleanly.
pub async fn run() -> anyhow::Result<()> {
    use crate::paths::Paths;
    use crate::tui::input;
    use crossterm::event::{self, Event as CtEvent};
    use crossterm::execute;
    use crossterm::terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    };
    use ratatui::Terminal;
    use ratatui::backend::CrosstermBackend;
    use std::io::stdout;
    use std::time::{Duration, Instant};

    let paths = Paths::resolve();
    let (client, mut events) =
        match event_source::connect_and_subscribe(&paths.control_socket_path).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("agentd tui: cannot connect to daemon: {e}");
                eprintln!("(is the daemon running? try `agentd daemon start`)");
                return Ok(());
            }
        };

    // Initial state.
    let snap = match client
        .call(
            agentd_protocol::Method::STATE_SNAPSHOT,
            serde_json::json!({}),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("agentd tui: state.snapshot failed: {e}");
            return Ok(());
        }
    };
    let mut state = TuiState::from_snapshot(&snap);

    // Terminal setup.
    enable_raw_mode()?;
    let mut stdout_handle = stdout();
    execute!(stdout_handle, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout_handle);
    let mut terminal = Terminal::new(backend)?;

    let result: anyhow::Result<()> = async {
        let frame_interval = Duration::from_millis(33); // 30fps
        // Initialize `last_draw` one frame in the past so the first frame renders
        // immediately. `checked_sub` guards against the (theoretical) underflow
        // if `frame_interval` exceeded the platform epoch; in practice that
        // cannot happen, and we fall back to `now` so the first frame waits one
        // tick — still correct, just not maximally eager.
        let mut last_draw = Instant::now()
            .checked_sub(frame_interval)
            .unwrap_or_else(Instant::now);
        let mut last_flash_tick = Instant::now();

        loop {
            // Apply pending events.
            while let Ok(event) = events.try_recv() {
                state.apply_event(&event);
            }
            // Periodic flash tick.
            if last_flash_tick.elapsed() > Duration::from_millis(100) {
                state.tick_flash(Instant::now());
                last_flash_tick = Instant::now();
            }

            // Render if dirty or flash window active.
            let need_redraw = state.dirty
                || state
                    .flash_until
                    .values()
                    .any(|t| t.saturating_duration_since(Instant::now()).as_millis() > 0);
            if need_redraw && last_draw.elapsed() >= frame_interval {
                terminal.draw(|f| render::render(f, &state))?;
                state.dirty = false;
                last_draw = Instant::now();
            }

            // Poll for input.
            if event::poll(Duration::from_millis(50))? {
                if let Ok(CtEvent::Key(key)) = event::read() {
                    if input::handle_key(&mut state, key, &client).await {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
    .await;

    // Teardown.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}
