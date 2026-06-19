//! test-agent fixture binary.
//!
//! Reads a script (TOML) from `--script <path>` or stdin, emits events to
//! stdout as NDJSON, and exits. Used by plugin tests to simulate agent
//! output deterministically.

#![allow(clippy::expect_used)] // CLI fixture; panic on closed stdout is acceptable

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

use agentd_testing::test_agent::{Script, ScriptAction};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let script = match parse_args(&args) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("test-agent: {e}");
            std::process::exit(2);
        }
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();

    for action in script.actions {
        match action {
            ScriptAction::Emit { after_ms, emit } => {
                sleep(Duration::from_millis(after_ms));
                let event = serde_json::json!({
                    "type": emit,
                    "ts": chrono::Utc::now().to_rfc3339(),
                });
                writeln!(out, "{event}").expect("write event");
                out.flush().expect("flush");
            }
            ScriptAction::Exit => {
                writeln!(out, r#"{{"type":"exit"}}"#).ok();
                out.flush().ok();
                return;
            }
        }
    }
    writeln!(out, r#"{{"type":"exit"}}"#).ok();
}

fn parse_args(args: &[String]) -> Result<Script, String> {
    if args.len() == 1 {
        let stdin = io::stdin();
        let mut buf = String::new();
        stdin
            .lock()
            .read_to_string(&mut buf)
            .map_err(|e| e.to_string())?;
        toml::from_str(&buf).map_err(|e| e.to_string())
    } else if args.len() == 3 && args[1] == "--script" {
        let path = PathBuf::from(&args[2]);
        let body = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        toml::from_str(&body).map_err(|e| e.to_string())
    } else {
        Err(format!("usage: {} [--script <path>]", args[0]))
    }
}
