//! Codex's usage: after each reply a rollout records a `token_count` event
//! with the session's running total and the reply's own share. An event that
//! repeats the total only brings news of the plan's limits, of which the
//! newest are kept. A forked session's
//! rollout begins with a copy of the events before the fork, so an event
//! counts once by its time and total.
//!
//! Codex counts cached input within its input, and reasoning within output.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

use super::{Records, Reply, Tokens, Window, lines, seconds};
use crate::agent::codex;

/// Every rollout under the Codex folder `home`, archived ones included.
pub(super) fn files(home: &Path) -> Result<Vec<PathBuf>> {
    let mut found = codex::rollouts(&home.join("sessions"))?;
    found.extend(codex::archived(home)?);
    Ok(found)
}

#[derive(Deserialize)]
struct Line {
    #[serde(default)]
    timestamp: String,
    #[serde(rename = "type")]
    kind: String,
    payload: Payload,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: Option<String>,
    /// The session's ID, in its first record.
    id: Option<String>,
    /// The model each turn runs, in a turn's first record.
    model: Option<String>,
    info: Option<Info>,
    /// Read apart, so an odd limits block, such as one with no reset
    /// time, costs only the limits and not the reply's tokens.
    rate_limits: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Info {
    total_token_usage: Usage,
    last_token_usage: Usage,
}

#[derive(Deserialize, PartialEq)]
#[expect(clippy::struct_field_names, reason = "The names Codex writes")]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    cache_write_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

#[derive(Deserialize)]
struct RateLimits {
    primary: Option<Allowance>,
    secondary: Option<Allowance>,
}

#[derive(Deserialize)]
struct Allowance {
    used_percent: f64,
    window_minutes: u64,
    resets_at: Option<i64>,
}

pub(super) fn read(path: &Path) -> Records {
    let mut records = Records::default();
    let Some(source) = codex::open_rollout(path) else { return records };
    let (mut session, mut model, mut total) = (None, String::new(), None);
    lines(source, 400, &["\"session_meta\"", "\"turn_context\"", "\"token_count\""], |text| {
        let Ok(line) = serde_json::from_str::<Line>(text) else { return };
        let payload = line.payload;
        match (line.kind.as_str(), payload.kind.as_deref()) {
            ("session_meta", _) => session = session.take().or(payload.id),
            ("turn_context", _) => model = payload.model.unwrap_or_default(),
            ("event_msg", Some("token_count")) => {
                let at = seconds(&line.timestamp).unwrap_or(0);
                // Each event gives the windows anew; only the newest are kept.
                let limits = payload
                    .rate_limits
                    .and_then(|value| serde_json::from_value::<RateLimits>(value).ok());
                if let Some(limits) = limits {
                    records.windows = [limits.primary, limits.secondary]
                        .into_iter()
                        .flatten()
                        .filter_map(|window| {
                            let resets = window.resets_at?;
                            Some(Window {
                                minutes: window.window_minutes,
                                used: window.used_percent,
                                resets,
                                seen: at,
                            })
                        })
                        .collect();
                }
                let Some(info) = payload.info else { return };
                if total.as_ref() == Some(&info.total_token_usage) {
                    return;
                }
                let last = info.last_token_usage;
                let tokens = Tokens {
                    input: last.input_tokens.saturating_sub(last.cached_input_tokens),
                    cached: last.cached_input_tokens,
                    written: last.cache_write_input_tokens,
                    output: last.output_tokens,
                    replies: 1,
                };
                let key = format!("{}/{}", line.timestamp, info.total_token_usage.total_tokens);
                total = Some(info.total_token_usage);
                if tokens.is_empty() {
                    return;
                }
                records.replies.push(Reply {
                    key,
                    rank: 0,
                    session: session.clone().unwrap_or_default(),
                    model: model.clone(),
                    at,
                    tokens,
                });
            }
            _ => {}
        }
    });
    records
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn counts_each_reply_once_and_keeps_the_limits() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("rollout.jsonl");
        let count = |at: &str, total: u64, last: u64| {
            format!(
                r#"{{"timestamp":"2026-11-27T10:00:{at}Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{total},"total_tokens":{total}}},"last_token_usage":{{"input_tokens":{last},"cached_input_tokens":4,"output_tokens":2,"total_tokens":{last}}}}},"rate_limits":{{"primary":{{"used_percent":8.0,"window_minutes":10080,"resets_at":1796197238}},"secondary":null}}}}}}"#
            )
        };
        let lines = [
            r#"{"timestamp":"2026-11-27T10:00:00Z","type":"session_meta","payload":{"id":"s","cwd":"/"}}"#.into(),
            r#"{"timestamp":"2026-11-27T10:00:00Z","type":"turn_context","payload":{"model":"gpt-6.2"}}"#.into(),
            count("01", 10, 10),
            count("02", 10, 10),
            // A limits block without a reset time still leaves the reply counted.
            count("03", 20, 10).replace("1796197238", "null"),
            count("04", 40, 20),
        ];
        fs::write(&path, lines.join("\n"))?;
        let records = read(&path);
        let replies: Vec<_> = records
            .replies
            .iter()
            .map(|reply| (reply.session.as_str(), reply.model.as_str(), reply.tokens.input))
            .collect();
        assert_eq!(replies, [("s", "gpt-6.2", 6), ("s", "gpt-6.2", 6), ("s", "gpt-6.2", 16)]);
        assert_eq!(records.replies[0].tokens.cached, 4);
        assert_eq!(records.windows.len(), 1);
        assert_eq!((records.windows[0].minutes, records.windows[0].used), (10_080, 8.0));
        Ok(())
    }
}
