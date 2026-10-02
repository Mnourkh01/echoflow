//! Which models and effort levels the local Claude CLI offers, read from the CLI
//! itself so the Settings picker follows Claude Code updates with no app release.
//!
//! Aliases ("sonnet", "opus"...) and effort levels come from `claude --help`.
//! The exact model an alias points at today comes from the `init` event of a
//! throwaway stream-json session, killed the moment that event arrives (about
//! 0.8 s in, before the model has replied). Results are cached per CLI version
//! for a day, so the probe runs rarely.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::enhance::{cli_exe, hidden, user_message_line};

/// Always offered: `--help` names only some aliases in its example (haiku is
/// missing from it today).
const BASE_ALIASES: [&str; 4] = ["haiku", "sonnet", "opus", "fable"];
const BASE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_AGE_SECS: u64 = 24 * 60 * 60;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CliModel {
    pub alias: String,
    /// Full model id the alias resolves to today. None when the probe failed
    /// (CLI missing, logged out, offline).
    pub id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CliModels {
    pub cli_version: String,
    pub checked_at: u64,
    pub models: Vec<CliModel>,
    pub efforts: Vec<String>,
}

/// A model alias or full id we are willing to pass to `--model`.
pub fn is_model_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-.[]".contains(c))
}

pub fn is_effort(s: &str) -> bool {
    !s.is_empty() && s.len() <= 16 && s.chars().all(|c| c.is_ascii_lowercase())
}

/// The help text of one option: its own line plus the wrapped lines under it.
fn option_help<'a>(help: &'a str, flag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in help.lines() {
        let t = line.trim_start();
        if t.starts_with('-') || t.is_empty() {
            if inside {
                break;
            }
            inside = t.starts_with(&format!("{flag} ")) || t.split(", ").any(|p| p.starts_with(&format!("{flag} ")));
        }
        if inside {
            out.push(line);
        }
    }
    out
}

/// Aliases named in the `--model` help ("e.g. 'fable', 'opus', or 'sonnet'"),
/// merged after the base set so known ones keep their order.
pub fn aliases_from_help(help: &str) -> Vec<String> {
    let mut out: Vec<String> = BASE_ALIASES.iter().map(|s| s.to_string()).collect();
    for line in option_help(help, "--model") {
        for (i, part) in line.split('\'').enumerate() {
            if i % 2 == 1 && is_model_name(part) && !out.iter().any(|a| a == part) {
                out.push(part.to_string());
            }
        }
    }
    out
}

/// Effort levels from the `--effort` help ("(low, medium, high, xhigh, max)").
pub fn efforts_from_help(help: &str) -> Vec<String> {
    let text = option_help(help, "--effort").join(" ");
    let found: Vec<String> = text
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(list, _)| list.split(',').map(|s| s.trim().to_string()).filter(|s| is_effort(s)).collect())
        .unwrap_or_default();
    if found.is_empty() {
        BASE_EFFORTS.iter().map(|s| s.to_string()).collect()
    } else {
        found
    }
}

/// The model id in a stream-json `system/init` event; None for any other line.
pub fn parse_init_model(line: &str) -> Option<String> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if v.get("type")?.as_str()? != "system" || v.get("subtype")?.as_str()? != "init" {
        return None;
    }
    v.get("model")?.as_str().map(str::to_string)
}

fn run_text(exe: &str, arg: &str) -> Result<String> {
    let out = hidden(Command::new(exe).arg(arg).stdin(Stdio::null()))
        .output()
        .map_err(|e| anyhow!("could not start '{exe}': {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// "2.1.287 (Claude Code)" -> "2.1.287".
fn cli_version(exe: &str) -> Result<String> {
    let text = run_text(exe, "--version")?;
    text.split_whitespace()
        .next()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("{exe} --version printed nothing"))
}

/// Which model `alias` resolves to, read from the session's init event. The
/// process is killed as soon as that event arrives.
fn probe(exe: &str, alias: &str) -> Result<String> {
    let mut cmd = Command::new(exe);
    cmd.args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"])
        .args(["--model", alias, "--system-prompt", "Reply with OK."])
        .args(["--effort", "low", "--tools", "", "--safe-mode", "--no-session-persistence"])
        .current_dir(std::env::temp_dir())
        .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
        .env("MAX_THINKING_TOKENS", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = hidden(&mut cmd)
        .spawn()
        .map_err(|e| anyhow!("could not start '{exe}': {e}"))?;
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("{exe}: no stdout"))?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).split(b'\n') {
            let Ok(line) = line else { break };
            if let Some(model) = parse_init_model(&String::from_utf8_lossy(&line)) {
                let _ = tx.send(model);
                return;
            }
        }
    });
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{}\n", user_message_line("OK")).as_bytes());
    }
    let got = rx.recv_timeout(PROBE_TIMEOUT);
    let _ = child.kill();
    let _ = child.wait();
    got.map_err(|_| anyhow!("{exe} --model {alias}: no init event"))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn read_cache(path: &Path) -> Option<CliModels> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// The CLI's models and effort levels. Served from `cache_path` while the CLI
/// version is unchanged and the entry is under a day old, unless `refresh`.
pub fn list(command: &str, cache_path: &Path, refresh: bool) -> Result<CliModels> {
    let exe = cli_exe(command).to_string();
    let version = cli_version(&exe)?;
    if !refresh {
        if let Some(c) = read_cache(cache_path) {
            let fresh = now_secs().saturating_sub(c.checked_at) < MAX_AGE_SECS;
            // Partial results count: an alias the plan can't use stays unresolved
            // until Refresh, instead of re-probing (and spending quota) on every open.
            if c.cli_version == version && fresh {
                return Ok(c);
            }
        }
    }

    let help = run_text(&exe, "--help").unwrap_or_default();
    let probes: Vec<_> = aliases_from_help(&help)
        .into_iter()
        .map(|alias| {
            let exe = exe.clone();
            std::thread::spawn(move || {
                let id = probe(&exe, &alias)
                    .map_err(|e| log::warn!("cli model probe: {e}"))
                    .ok();
                CliModel { alias, id }
            })
        })
        .collect();
    let models: Vec<CliModel> = probes.into_iter().filter_map(|h| h.join().ok()).collect();

    let out = CliModels {
        cli_version: version,
        checked_at: now_secs(),
        models,
        efforts: efforts_from_help(&help),
    };
    if out.models.iter().any(|m| m.id.is_some()) {
        if let Ok(bytes) = serde_json::to_vec_pretty(&out) {
            let _ = std::fs::write(cache_path, bytes);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELP: &str = "Options:
  --effort <level>                      Effort level for the current session
                                        (low, medium, high, xhigh, max)
  --fallback-model <model>              Enable automatic fallback to 'zeta'
  --model <model>                       Model for the current session. Provide
                                        an alias for the latest model (e.g.
                                        'fable', 'opus', or 'sonnet') or a
                                        model's full name.
  -n, --name <name>                     Set a display name, see 'agent'
";

    #[test]
    fn aliases_come_from_the_model_option_only() {
        assert_eq!(aliases_from_help(HELP), ["haiku", "sonnet", "opus", "fable"]);
        let newer = HELP.replace("'fable', ", "'fable', 'nova', ");
        assert_eq!(aliases_from_help(&newer).last().unwrap(), "nova");
        assert_eq!(aliases_from_help("").len(), 4);
    }

    #[test]
    fn efforts_parse_with_fallback() {
        assert_eq!(efforts_from_help(HELP), ["low", "medium", "high", "xhigh", "max"]);
        let fewer = HELP.replace("(low, medium, high, xhigh, max)", "(low, high)");
        assert_eq!(efforts_from_help(&fewer), ["low", "high"]);
        assert_eq!(efforts_from_help("").len(), 5);
    }

    #[test]
    fn init_model_only_from_init_events() {
        let init = r#"{"type":"system","subtype":"init","model":"claude-sonnet-5-5","tools":[]}"#;
        assert_eq!(parse_init_model(init).as_deref(), Some("claude-sonnet-5-5"));
        assert!(parse_init_model(r#"{"type":"assistant","model":"x"}"#).is_none());
        assert!(parse_init_model("not json").is_none());
    }

    /// Against the real CLI (installed and logged in):
    /// `cargo test --lib live_cli_models -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_cli_models() {
        let cache = std::env::temp_dir().join("echoflow_cli_models_test.json");
        let started = std::time::Instant::now();
        let got = list("claude", &cache, true).expect("list");
        eprintln!("{} ms -> {got:#?}", started.elapsed().as_millis());
        let sonnet = got.models.iter().find(|m| m.alias == "sonnet").expect("sonnet listed");
        assert!(sonnet.id.as_deref().is_some_and(|id| id.starts_with("claude-sonnet")));
        let cached = list("claude", &cache, false).expect("cached");
        assert_eq!(cached.checked_at, got.checked_at, "second call should come from the cache");
        let _ = std::fs::remove_file(cache);
    }

    #[test]
    fn model_names_are_validated() {
        assert!(is_model_name("sonnet"));
        assert!(is_model_name("claude-haiku-4-5-20251001"));
        assert!(is_model_name("opus[1m]"));
        assert!(!is_model_name(""));
        assert!(!is_model_name("Sonnet"));
        assert!(!is_model_name("sonnet --tools x"));
        assert!(!is_model_name("-x"));
    }
}
