#![allow(
    clippy::unnecessary_wraps,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use agentd::cli::{self, Cli, Command, DaemonAction, PluginAction};
use agentd::paths::{self, Paths};

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.quiet);
    match cli.command {
        Command::Daemon { action } => daemon(action)?,
        Command::List => list_via_daemon()?,
        Command::New {
            cwd,
            pick: _,
            recent: _,
            agent,
        } => new_via_daemon(cwd.as_deref(), agent.as_deref())?,
        Command::Jump { id } => jump_via_daemon(&id)?,
        Command::Rename { id, name } => rename_via_daemon(&id, &name)?,
        Command::Kill { id } => kill_via_daemon(&id)?,
        Command::Status { global, pane } => status(global, pane)?,
        Command::Plugin { action } => plugin(action),
        #[allow(clippy::if_not_else)]
        Command::Init { yes } => {
            let paths = paths::Paths::resolve();
            if !cli::init::tmux_version_ok() {
                eprintln!("agentd init: tmux not found or < 2.6. Install tmux and retry.");
                std::process::exit(1);
            }
            if let Err(e) = cli::init::write_default_configs(&paths) {
                eprintln!("agentd init: failed to write configs: {e}");
                std::process::exit(1);
            }
            println!(
                "Wrote {} and {}/plugins.toml",
                paths.config_dir.join("config.toml").display(),
                paths.config_dir.display()
            );
            println!();
            println!("Add these lines to your ~/.tmux.conf:");
            println!();
            println!("{}", cli::init::tmux_conf_fragment());
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            if let Some(home) = home {
                if !cli::init::tmux_conf_has_fragment(&home) {
                    if yes {
                        let path = home.join(".tmux.conf");
                        if !path.exists() {
                            let _ = std::fs::File::create(&path);
                        }
                        let backup = home.join(".tmux.conf.bak");
                        let _ = std::fs::copy(&path, &backup);
                        let mut body = std::fs::read_to_string(&path).unwrap_or_default();
                        body.push('\n');
                        body.push_str(&cli::init::tmux_conf_fragment());
                        let _ = std::fs::write(&path, body);
                        println!("Appended to {}", path.display());
                    } else {
                        println!("(Re-run with --yes to append automatically.)");
                    }
                } else {
                    println!("(Fragment already present in ~/.tmux.conf — skipped.)");
                }
            }
            println!();
            println!("Next: agentd plugin install opencode");
        }
        Command::Doctor => println!("agentd doctor: not yet implemented"),
        Command::Metrics { format } => {
            println!("agentd metrics --format {format}: not yet implemented")
        }
        Command::Debug => println!("agentd debug: not yet implemented"),
        Command::Uninstall { .. } => println!("agentd uninstall: not yet implemented"),
    }
    Ok(())
}

fn init_tracing(quiet: bool) {
    use tracing_subscriber::{EnvFilter, fmt};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if quiet {
            EnvFilter::new("error")
        } else {
            EnvFilter::new("info")
        }
    });
    let _ = fmt().with_env_filter(filter).try_init();
}

fn daemon(action: DaemonAction) -> Result<()> {
    use agentd::daemon::Daemon;
    use agentd::event_bus::EventBus;
    use agentd::plugin_spawner::RealPluginSpawner;
    use agentd::tmux::RealTmux;
    match action {
        DaemonAction::Start { foreground, .. } => {
            eprintln!("agentd daemon starting (foreground={foreground})");
            let paths = Paths::resolve();
            std::fs::create_dir_all(&paths.state_dir)?;
            let db = agentd::db::Db::open(&paths.state_db_path)?;
            agentd::db::migrations::run(&db)?;
            let manifest = load_manifest(&paths)?;
            let bus = EventBus::default();
            let d = Daemon::new(
                paths,
                db,
                bus,
                Arc::new(RealTmux::new()),
                manifest,
                Arc::new(RealPluginSpawner::new()),
            );
            install_signal_handler(d.shutdown_handle());
            if !foreground {
                return detach_via_double_fork();
            }
            // Foreground: park on the daemon.
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(async move { d.run().await })?;
            Ok(())
        }
        DaemonAction::Stop => {
            // RPC: daemon.shutdown, then poll for socket disappearance.
            stop_daemon()
        }
        DaemonAction::Restart => {
            stop_daemon()?;
            daemon(DaemonAction::Start {
                foreground: true,
                detach: false,
            })
        }
        DaemonAction::Status => status_daemon(),
    }
}

fn load_manifest(paths: &Paths) -> Result<agentd::plugins_manifest::PluginsManifest> {
    let path = paths.config_dir.join("plugins.toml");
    if !path.exists() {
        return Ok(agentd::plugins_manifest::PluginsManifest::default());
    }
    let body = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&body)?)
}

fn install_signal_handler(shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    // Best-effort: install SIGINT + SIGTERM handler that flips `shutdown`.
    let _ = ctrlc::set_handler(move || {
        shutdown.store(true, Ordering::SeqCst);
    });
}

#[allow(unsafe_code)]
fn detach_via_double_fork() -> Result<()> {
    use std::os::unix::process::CommandExt;
    // Double-fork: parent forks once, intermediate forks again and exits,
    // grandchild execs `agentd daemon start --foreground` in a new session.
    // Forking happens BEFORE any Tokio runtime is entered, so no other
    // threads are mid-syscall and the fork is async-signal-safe.
    let exe = std::env::current_exe()?;
    let paths = agentd::paths::Paths::resolve();
    let pid_path = paths.daemon_pid_path();
    std::fs::create_dir_all(&paths.runtime_dir)?;

    // SAFETY: between fork() and exec() only async-signal-safe code runs;
    // the Tokio runtime is not yet started in this process, so no other
    // threads are mid-syscall and the child's address space is consistent.
    let intermediate = unsafe { libc::fork() };
    if intermediate < 0 {
        return Err(anyhow::anyhow!("fork failed"));
    }
    if intermediate == 0 {
        // Intermediate child: fork once more, then exit so the grandchild
        // is reparented to init and is no longer a zombie tied to the CLI
        // session.
        // SAFETY: same as above — no threads, async-signal-safe between
        // fork and the immediate exit below.
        let grand = unsafe { libc::fork() };
        if grand < 0 {
            std::process::exit(1);
        }
        if grand == 0 {
            // Grandchild: become a session leader, detach stdio, write the
            // PID file, then exec the same binary in --foreground mode.
            // SAFETY: setsid() is safe to call in a forked child that is
            // not a process group leader (true here — only the original
            // parent was, and this process was just created by fork).
            unsafe {
                libc::setsid();
            }
            // Redirect stdio to /dev/null so the daemon does not hold a
            // reference to the controlling terminal or the parent's pipes.
            let devnull = std::ffi::CString::new("/dev/null").unwrap();
            // SAFETY: between fork() and exec() only async-signal-safe
            // code runs; libc::open, dup2, and close are all POSIX
            // async-signal-safe. /dev/null is a valid constant path.
            unsafe {
                let fd = libc::open(devnull.as_ptr(), libc::O_RDWR);
                if fd >= 0 {
                    libc::dup2(fd, 0);
                    libc::dup2(fd, 1);
                    libc::dup2(fd, 2);
                    if fd > 2 {
                        libc::close(fd);
                    }
                }
            }
            // Write our PID. We are the long-running daemon (the upcoming
            // exec preserves the PID), so this file lets `daemon stop`
            // signal the right process.
            let pid = std::process::id();
            let _ = std::fs::write(&pid_path, pid.to_string());
            // Exec the same binary in foreground mode. After exec, the
            // new process re-runs the daemon's boot sequence and enters
            // the Tokio runtime for the first time.
            let err = std::process::Command::new(&exe)
                .args(["daemon", "start", "--foreground"])
                .exec();
            // If exec returns, it failed.
            eprintln!("agentd: exec failed: {err}");
            std::process::exit(127);
        }
        // Intermediate: exit immediately, orphaning the grandchild to init.
        std::process::exit(0);
    }
    // Parent: reap the intermediate child so we don't leak a zombie, then
    // return so the CLI exits cleanly. The grandchild is owned by init.
    // SAFETY: waitpid() is safe in the parent thread; we block until the
    // intermediate child has exited (either normally or via signal).
    unsafe {
        let mut status = 0;
        libc::waitpid(intermediate, &raw mut status, 0);
    }
    Ok(())
}

fn stop_daemon() -> Result<()> {
    let paths = agentd::paths::Paths::resolve();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let client = agentd::control_client::ControlClient::connect(&paths.control_socket_path)
            .await
            .map_err(|e| anyhow::anyhow!("connect: {e}"))?;
        client
            .call("daemon.shutdown", serde_json::json!({}))
            .await
            .map_err(|e| anyhow::anyhow!("shutdown: {e}"))?;
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if !paths.control_socket_path.exists() {
                return Ok(());
            }
        }
        Err(anyhow::anyhow!("daemon did not stop within 5s"))
    })
}

fn status_daemon() -> Result<()> {
    let paths = agentd::paths::Paths::resolve();
    if !paths.control_socket_path.exists() {
        println!("agentd: not running");
        return Ok(());
    }
    println!(
        "agentd: running (pid file: {})",
        paths.daemon_pid_path().display()
    );
    Ok(())
}

fn list_via_daemon() -> Result<()> {
    let paths = agentd::paths::Paths::resolve();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let client = agentd::daemon::ensure_daemon_running(&paths)
            .await
            .map_err(|e| anyhow::anyhow!("ensure_daemon_running: {e}"))?;
        let v = client
            .call("session.list_active", serde_json::json!({}))
            .await
            .map_err(|e| anyhow::anyhow!("list_active: {e}"))?;
        if let Some(arr) = v.as_array() {
            for s in arr {
                println!("{}\t{:?}\t{}", s["id"], s["status"], s["display_name"]);
            }
        }
        Ok::<(), anyhow::Error>(())
    })
}

fn new_via_daemon(cwd: Option<&str>, agent: Option<&str>) -> Result<()> {
    let paths = agentd::paths::Paths::resolve();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let client = agentd::daemon::ensure_daemon_running(&paths)
            .await
            .map_err(|e| anyhow::anyhow!("ensure_daemon_running: {e}"))?;
        let cwd = cwd.unwrap_or(".");
        let params = serde_json::json!({
            "agent_type": agent.unwrap_or("opencode"),
            "working_dir": cwd,
            "name": std::path::Path::new(cwd)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("session"),
        });
        let v = client
            .call("session.create", params)
            .await
            .map_err(|e| anyhow::anyhow!("session.create: {e}"))?;
        println!("Created session {}", v["id"]);
        Ok::<(), anyhow::Error>(())
    })
}

fn jump_via_daemon(id: &str) -> Result<()> {
    rpc_one_way("session.jump", serde_json::json!({"id": id}), "jump")
}

fn rename_via_daemon(id: &str, name: &str) -> Result<()> {
    rpc_one_way(
        "session.rename",
        serde_json::json!({"id": id, "name": name}),
        "rename",
    )
}

fn kill_via_daemon(id: &str) -> Result<()> {
    rpc_one_way("session.kill", serde_json::json!({"id": id}), "kill")
}

fn rpc_one_way(method: &str, params: serde_json::Value, label: &str) -> Result<()> {
    let paths = agentd::paths::Paths::resolve();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let client = agentd::daemon::ensure_daemon_running(&paths)
            .await
            .map_err(|e| anyhow::anyhow!("ensure_daemon_running: {e}"))?;
        client
            .call(method, params)
            .await
            .map_err(|e| anyhow::anyhow!("{label}: {e}"))?;
        println!("{label} ok");
        Ok::<(), anyhow::Error>(())
    })
}

fn plugin(action: PluginAction) {
    match action {
        PluginAction::List => println!("agentd plugin list: not yet implemented"),
        PluginAction::Install { name } => {
            println!("agentd plugin install {name}: not yet implemented")
        }
        PluginAction::Update => println!("agentd plugin update: not yet implemented"),
        PluginAction::Remove { name } => {
            println!("agentd plugin remove {name}: not yet implemented")
        }
        PluginAction::Start { name } => println!("agentd plugin start {name}: not yet implemented"),
        PluginAction::Stop { name } => println!("agentd plugin stop {name}: not yet implemented"),
    }
}

fn status(global: bool, pane: Option<String>) -> Result<()> {
    use agentd::control_client::ControlClient;
    use agentd::{db, paths, status};
    let paths = paths::Paths::resolve();
    let runtime = futures::executor::block_on(ControlClient::connect(&paths.control_socket_path));
    // We may not have a running daemon; fall back to in-process cache built
    // from a read-only DB.
    match (global, pane) {
        (true, _) => {
            // Try the daemon first; on failure, build cache from DB.
            if let Ok(client) = &runtime {
                if let Ok(snap) = futures::executor::block_on(async {
                    client.call("state.snapshot", serde_json::json!({})).await
                }) {
                    println!("{snap}");
                    return Ok(());
                }
            }
            // Fallback: no daemon. Open DB read-only and rebuild cache.
            let Ok(db) = db::Db::open(&paths.state_db_path) else {
                println!();
                return Ok(());
            };
            let _ = db::migrations::run(&db);
            let cache = status::cache::StatusCache::new();
            let _ = cache.rebuild(&db);
            println!("{}", cache.format_global());
        }
        (false, Some(pane)) => {
            // Same fallback strategy.
            if let Ok(client) = &runtime {
                if let Ok(v) = futures::executor::block_on(async {
                    client
                        .call("session.get", serde_json::json!({"id_lookup": pane}))
                        .await
                }) {
                    println!("{v}");
                    return Ok(());
                }
            }
            let Ok(db) = db::Db::open(&paths.state_db_path) else {
                println!();
                return Ok(());
            };
            let _ = db::migrations::run(&db);
            let cache = status::cache::StatusCache::new();
            let _ = cache.rebuild(&db);
            println!("{}", cache.format_pane(&pane));
        }
        (false, None) => {
            println!("agentd status: specify --global or --pane <id>");
        }
    }
    Ok(())
}
