//! Claude Code's usage: every reply in a transcript carries the `usage` the
//! API reported. A reply in several parts is written once per part, and a
//! session resumed in a new transcript copies the replies before it, so a
//! reply counts once by its message ID: its last part, which has the most
//! output, is kept. Sub-agents keep transcripts of their own inside the
//! session's folder, under the session's ID.

use std::{
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::Deserialize;

use super::{Records, Reply, Tokens, seconds};
use crate::records::{lines, walk};

/// Every transcript under `projects`, sub-agents' included.
pub(super) fn files(projects: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    walk(projects, ".jsonl", 4, &mut found)?;
    Ok(found)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    #[serde(rename = "type")]
    kind: String,
    session_id: Option<String>,
    timestamp: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    stop_reason: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
#[expect(clippy::struct_field_names, reason = "The names Claude Code writes")]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

pub(super) fn read(path: &Path) -> Records {
    let mut records = Records::default();
    let Ok(file) = File::open(path) else { return records };
    let own = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default().to_owned();
    lines(file, usize::MAX, &["\"usage\""], |text| {
        let Ok(line) = serde_json::from_str::<Line>(text) else { return };
        let Some(message) = line.message.filter(|_| line.kind == "assistant") else { return };
        let (Some(id), Some(usage)) = (message.id, message.usage) else { return };
        let tokens = Tokens {
            input: usage.input_tokens,
            cached: usage.cache_read_input_tokens,
            written: usage.cache_creation_input_tokens,
            output: usage.output_tokens,
            replies: 1,
        };
        let model = message.model.unwrap_or_default();
        if tokens.is_empty() || model.starts_with('<') {
            return;
        }
        records.replies.push(Reply {
            key: id,
            // A finished reply ranks above its parts, then the one with more output.
            rank: u64::from(message.stop_reason.is_some()) << 48 | usage.output_tokens,
            session: line.session_id.unwrap_or_else(|| own.clone()),
            model,
            at: line.timestamp.as_deref().and_then(seconds).unwrap_or(0),
            tokens,
        });
    });
    records
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn counts_each_reply_once_by_its_last_part() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("s.jsonl");
        let line = |output: u64, stop: &str| {
            format!(
                r#"{{"type":"assistant","sessionId":"s","timestamp":"2026-11-27T10:00:00Z","message":{{"id":"m","model":"claude-opus-5-5","stop_reason":{stop},"usage":{{"input_tokens":3,"cache_read_input_tokens":100,"cache_creation_input_tokens":20,"output_tokens":{output}}}}}}}"#
            )
        };
        let synthetic = r#"{"type":"assistant","message":{"id":"x","model":"<synthetic>","usage":{"input_tokens":0,"output_tokens":0}}}"#;
        fs::write(&path, [line(1, "null"), line(40, "\"end_turn\""), synthetic.into()].join("\n"))?;
        let mut replies = read(&path).replies;
        replies.sort_by_key(|reply| reply.rank);
        let last = replies.last().expect("a reply");
        assert_eq!(replies.len(), 2, "both parts are kept until every file is read");
        assert_eq!((last.key.as_str(), last.session.as_str()), ("m", "s"));
        assert_eq!(
            last.tokens,
            Tokens { input: 3, cached: 100, written: 20, output: 40, replies: 1 }
        );
        Ok(())
    }
}
