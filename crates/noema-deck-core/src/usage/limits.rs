//! Plan limits: how much of each agent's allowance is used, as the agent
//! itself reports it. Deck never reads an agent's login.
//!
//! | Agent | Asked |
//! | --- | --- |
//! | Claude Code | `claude -p /usage`, which Claude Code answers itself without a model, keeping the answer in its state file as `cachedUsageUtilization`. Deck reads that |
//! | Codex | Nothing: every `token_count` event in a rollout carries `rate_limits` |
//! | Antigravity | `agy --print /usage --output-format json`, which `agy` answers without a model |

use std::{
    fs,
    io::Read,
    panic,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use chrono::Local;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Window, seconds};
use crate::{
    agent::{Agent, Homes, antigravity, claude},
    pane::login_environment,
};

/// An agent is asked again only after this long, however often the page asks.
const AGAIN: Duration = Duration::from_mins(2);
/// How long an agent may take to answer.
const WAIT: Duration = Duration::from_secs(45);

/// A request for the plan limits: every agent's, or with `agent` only its
/// own. With `ask`, the agents are asked first.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct LimitsRequest {
    #[serde(default)]
    pub ask: bool,
    pub agent: Option<Agent>,
}

/// An agent's plan limits, as it last reported them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub agent: Agent,
    pub limits: Vec<Limit>,
    /// When the agent reported them, in Unix seconds. `None` when it never has.
    pub checked: Option<i64>,
}

/// How much of one allowance is used.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Limit {
    /// The window, and what it covers when the plan has several, such as
    /// `Weekly · Fable`.
    pub name: String,
    /// The share used, in percent.
    pub used: f64,
    /// When the window starts again, in Unix seconds. `None` for one that has
    /// not started.
    pub resets: Option<i64>,
}

/// Asks Claude Code and Antigravity for their limits, each at most once in
/// [`AGAIN`], and keeps Antigravity's answer, which it does not keep itself.
/// An agent being asked never holds up reading what is already known.
#[derive(Default)]
pub struct Asker {
    claude: Asking,
    antigravity: Asking,
    /// Antigravity's last answer.
    antigravity_plan: Mutex<Option<Plan>>,
    /// Where Antigravity's last answer is kept between runs, as it takes
    /// `agy` some seconds to give another.
    kept: Option<PathBuf>,
}

/// One agent's turn to be asked: held while it answers, so it is asked once
/// at a time, and when it was last asked.
#[derive(Default)]
struct Asking(Mutex<Option<Instant>>);

impl Asking {
    /// Runs `ask` unless the agent was asked lately, waiting for an answer
    /// already on its way rather than asking twice.
    fn ask(&self, ask: impl FnOnce()) {
        let mut last = self.0.lock();
        if last.is_none_or(|at| at.elapsed() > AGAIN) {
            ask();
            *last = Some(Instant::now());
        }
    }
}

impl Asker {
    /// An asker that keeps Antigravity's last answer in `file`, and starts
    /// from the one kept there.
    #[must_use]
    pub fn keeping(file: PathBuf) -> Self {
        let plan = fs::read(&file).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok());
        Self { antigravity_plan: Mutex::new(plan), kept: Some(file), ..Self::default() }
    }

    /// Claude Code's limits, then Antigravity's, the two asked side by side.
    pub fn plans(&self, homes: &Homes, ask: bool) -> [Plan; 2] {
        thread::scope(|scope| {
            let claude = scope.spawn(|| self.claude(ask));
            let antigravity = scope.spawn(|| self.antigravity(homes, ask));
            [claude, antigravity]
                .map(|asked| asked.join().unwrap_or_else(|panic| panic::resume_unwind(panic)))
        })
    }

    /// Claude Code's limits. With `ask`, unless it was lately, Claude Code
    /// answers its `/usage` first, which takes it a second or two.
    pub fn claude(&self, ask: bool) -> Plan {
        if ask {
            self.claude.ask(|| {
                // Settings, tools and servers are left out, so only Claude Code
                // itself answers, and no session is kept.
                let args = [
                    "-p",
                    "/usage",
                    "--output-format",
                    "json",
                    "--tools",
                    "",
                    "--strict-mcp-config",
                ];
                let args =
                    [&args[..], &["--setting-sources", "", "--no-session-persistence"]].concat();
                let quiet = [
                    ("CLAUDE_CODE_DISABLE_AUTO_MEMORY", Some("1")),
                    ("DISABLE_TELEMETRY", Some("1")),
                    ("DISABLE_ERROR_REPORTING", Some("1")),
                    ("DISABLE_AUTOUPDATER", Some("1")),
                    // With this set, `/usage` would only repeat what it last heard.
                    ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", None),
                ];
                let _ = run(&claude::program(), &args, &quiet);
            });
        }
        claude::state_file().ok().map_or_else(|| empty(Agent::Claude), |state| claude_plan(&state))
    }

    /// Antigravity's limits. With `ask`, unless it was lately, `agy` answers
    /// its `/usage` first, which takes it about five seconds.
    pub fn antigravity(&self, homes: &Homes, ask: bool) -> Plan {
        // A machine where `agy` never ran has nothing to ask.
        if ask && homes.antigravity.exists() {
            self.antigravity.ask(|| {
                let args = ["--print", "/usage", "--output-format", "json"];
                let answer = run(&antigravity::program(), &args, &[]);
                if let Some(limits) = answer.as_deref().and_then(antigravity_limits) {
                    let checked = Some(Local::now().timestamp());
                    let plan = Plan { agent: Agent::Antigravity, limits, checked };
                    if let (Some(file), Ok(text)) = (&self.kept, serde_json::to_vec(&plan)) {
                        // Not kept is only slower to show next time.
                        let _ = fs::write(file, text);
                    }
                    *self.antigravity_plan.lock() = Some(plan);
                }
            });
        }
        self.antigravity_plan.lock().clone().unwrap_or_else(|| empty(Agent::Antigravity))
    }
}

fn empty(agent: Agent) -> Plan {
    Plan { agent, ..Plan::default() }
}

/// Codex's limits from the windows its rollouts recorded: those of the latest
/// record, as older ones may be for another plan. A window that has started
/// again since was not used since, or Codex would have recorded it, so none of
/// it is used.
pub(super) fn codex_plan<'a>(windows: impl IntoIterator<Item = &'a Window>) -> Plan {
    let windows: Vec<&Window> = windows.into_iter().collect();
    let last = windows.iter().map(|window| window.seen).max();
    let mut latest: Vec<_> =
        windows.into_iter().filter(|window| Some(window.seen) == last).collect();
    latest.sort_by_key(|window| window.minutes);
    latest.dedup_by_key(|window| window.minutes);
    let now = Local::now().timestamp();
    let limits = latest
        .iter()
        .map(|window| {
            let over = window.resets <= now;
            Limit {
                name: window_name(window.minutes),
                used: if over { 0.0 } else { window.used },
                resets: (!over).then_some(window.resets),
            }
        })
        .collect();
    Plan { agent: Agent::Codex, limits, checked: last }
}

/// A window's length in words, to the nearest hour.
fn window_name(minutes: u64) -> String {
    match (minutes + 30) / 60 {
        168 => "Weekly".into(),
        hours => format!("{hours}-hour"),
    }
}

/// Claude Code's last answer to `/usage`, kept in its state file, if it was
/// for the account signed in now.
fn claude_plan(state: &Path) -> Plan {
    let state: Value = fs::read(state)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let cached = &state["cachedUsageUtilization"];
    let account = state["oauthAccount"]["accountUuid"].as_str();
    if cached.is_null() || account.is_some() && cached["accountUuid"].as_str() != account {
        return empty(Agent::Claude);
    }
    let listed = cached["utilization"]["limits"].as_array().into_iter().flatten();
    let limits = listed.filter_map(|item| {
        let name = match item["kind"].as_str()? {
            "session" => "5-hour".into(),
            "weekly_all" => "Weekly · All models".into(),
            "weekly_scoped" => {
                format!("Weekly · {}", item["scope"]["model"]["display_name"].as_str()?)
            }
            _ => return None,
        };
        let resets = item["resets_at"].as_str().and_then(seconds);
        Some(Limit { name, used: item["percent"].as_f64()?, resets })
    });
    let checked = cached["fetchedAtMs"].as_i64().map(|at| at / 1000);
    Plan { agent: Agent::Claude, limits: limits.collect(), checked }
}

/// The limits in `agy`'s answer to `/usage`: for each group of models, the
/// five-hour window, then the weekly one.
fn antigravity_limits(answer: &[u8]) -> Option<Vec<Limit>> {
    let answer: Value = serde_json::from_slice(answer).ok()?;
    if answer["status"].as_str() != Some("SUCCESS") {
        return None;
    }
    let mut limits = Vec::new();
    for group in answer["command"]["data"]["groups"].as_array()? {
        let name = group["name"].as_str().unwrap_or("Models");
        let name = name.trim_end_matches(" Models").trim_end_matches(" models");
        let mut buckets: Vec<_> = group["buckets"].as_array().into_iter().flatten().collect();
        buckets.sort_by_key(|bucket| bucket["window"].as_str() != Some("5h"));
        for bucket in buckets {
            let Some(left) = bucket["remaining_fraction"].as_f64() else { continue };
            let window = match bucket["window"].as_str() {
                Some("5h") => "5-hour",
                Some("weekly") => "Weekly",
                Some(other) => other,
                None => continue,
            };
            // A window that has not started gives the first moment there is.
            let resets = bucket["reset_time"].as_str().and_then(seconds).filter(|at| *at > 0);
            limits.push(Limit {
                name: format!("{window} · {name}"),
                used: (1.0 - left).clamp(0.0, 1.0) * 100.0,
                resets,
            });
        }
    }
    Some(limits)
}

/// Runs `program` with `args` in the home folder, in the login shell's
/// environment as a pane is, with each of `env` set or, when `None`, removed,
/// and answers with what it printed, or `None` when
/// it failed or took longer than [`WAIT`]. A desktop app starts without the
/// `PATH` that finds the agent, and without the proxy it may need: `agy`, cut
/// off from its proxy, takes thirty seconds to answer rather than five.
fn run(program: &str, args: &[&str], env: &[(&str, Option<&str>)]) -> Option<Vec<u8>> {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(home) = std::env::home_dir() {
        command.current_dir(home);
    }
    if let Some(login) = login_environment() {
        command.envs(login.iter().map(|(name, value)| (name, value)));
    }
    for (name, value) in env {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    // What the agent starts in the background can hold the pipe open after it
    // exits, so what it printed is passed on as it comes, not at the pipe's end.
    let (pass, chunks) = mpsc::channel();
    thread::spawn(move || {
        let mut chunk = [0; 8192];
        while let Ok(count @ 1..) = stdout.read(&mut chunk) {
            if pass.send(chunk[..count].to_vec()).is_err() {
                return;
            }
        }
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                // Something it left running may go on printing, so this stops in time too.
                let mut printed = Vec::new();
                while started.elapsed() < WAIT
                    && let Ok(chunk) = chunks.recv_timeout(Duration::from_millis(300))
                {
                    printed.extend(chunk);
                }
                return Some(printed);
            }
            Ok(None) if started.elapsed() < WAIT => thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use serde_json::json;

    use super::*;

    #[test]
    fn codex_keeps_the_latest_records_windows_and_empties_one_that_started_again() {
        let now = Local::now().timestamp();
        let window = |minutes, used, resets, seen| Window { minutes, used, resets, seen };
        let windows = [
            window(10_079, 5.0, now + 60, 1),
            window(240, 9.0, now + 60, 1),
            window(299, 10.0, now - 1, 3),
            window(10_080, 7.0, now + 60, 3),
        ];
        let plan = codex_plan(&windows);
        let limits: Vec<_> = plan
            .limits
            .iter()
            .map(|limit| (limit.name.as_str(), limit.used, limit.resets))
            .collect();
        assert_eq!(limits, [("5-hour", 0.0, None), ("Weekly", 7.0, Some(now + 60))]);
        assert_eq!(plan.checked, Some(3));
    }

    #[test]
    fn claude_reads_its_own_answer_for_the_account_signed_in() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let state = dir.path().join(".claude.json");
        let cache = |account: &str| {
            json!({
                "oauthAccount": { "accountUuid": "a" },
                "cachedUsageUtilization": {
                    "accountUuid": account,
                    "fetchedAtMs": 1_800_000_000_000_i64,
                    "utilization": { "limits": [
                        { "kind": "session", "percent": 8, "resets_at": "2026-11-27T20:10:00+00:00" },
                        { "kind": "weekly_scoped", "percent": 39, "resets_at": null,
                          "scope": { "model": { "display_name": "Fable" } } },
                    ] },
                },
            })
        };
        fs::write(&state, cache("a").to_string())?;
        let plan = claude_plan(&state);
        let names: Vec<_> =
            plan.limits.iter().map(|limit| (limit.name.as_str(), limit.used)).collect();
        assert_eq!(names, [("5-hour", 8.0), ("Weekly · Fable", 39.0)]);
        assert_eq!(plan.checked, Some(1_800_000_000));
        fs::write(&state, cache("someone else").to_string())?;
        assert!(claude_plan(&state).limits.is_empty());
        Ok(())
    }

    #[test]
    fn antigravity_starts_from_the_answer_kept_last_run() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("limits.json");
        let homes = Homes {
            claude: dir.path().join("projects"),
            codex: dir.path().join("codex"),
            antigravity: dir.path().join("none"),
        };
        assert_eq!(Asker::keeping(file.clone()).antigravity(&homes, true).limits, []);
        let limit = Limit { name: "Weekly · Gemini".into(), used: 25.0, resets: None };
        let plan = Plan { agent: Agent::Antigravity, limits: vec![limit], checked: Some(1) };
        fs::write(&file, serde_json::to_vec(&plan)?)?;
        assert_eq!(Asker::keeping(file).antigravity(&homes, false), plan);
        Ok(())
    }

    #[test]
    fn antigravity_gives_each_group_five_hours_then_a_week() {
        let answer = json!({ "status": "SUCCESS", "command": { "data": { "groups": [{
            "name": "Gemini Models",
            "buckets": [
                { "window": "weekly", "remaining_fraction": 0.75, "reset_time": "2026-12-04T13:43:11Z" },
                { "window": "5h", "remaining_fraction": 1, "reset_time": "1970-01-01T00:00:00Z" },
            ],
        }] } } });
        let limits = antigravity_limits(answer.to_string().as_bytes()).expect("limits");
        let limits: Vec<_> = limits
            .iter()
            .map(|limit| (limit.name.as_str(), limit.used, limit.resets.is_some()))
            .collect();
        assert_eq!(limits, [("5-hour · Gemini", 0.0, false), ("Weekly · Gemini", 25.0, true)]);
        assert!(antigravity_limits(br#"{"status":"ERROR"}"#).is_none());
    }
}
