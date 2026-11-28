//! What was said in a Codex session: the text of the `user` and `assistant`
//! messages in its rollout. Codex sends its context and instructions as
//! messages from the person too, which are left out, as are the rollouts of
//! sub-agents and runs nobody can resume.

use std::path::Path;

use serde::Deserialize;

use super::Said;
use crate::{agent::codex, records::lines};

/// How far into a line its kind is looked for. Codex writes a record's kind
/// first and a message's own next, before anything long. A rollout is mostly
/// tools' output, megabytes of it, which need not be searched for them.
const HEAD: usize = 200;

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: String,
    payload: Payload,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: Option<String>,
    /// The session's ID and where it was started from, in its first record.
    id: Option<String>,
    source: Option<serde_json::Value>,
    role: Option<String>,
    #[serde(default)]
    content: Vec<Content>,
}

#[derive(Deserialize)]
struct Content {
    text: Option<String>,
}

pub(super) fn read(path: &Path) -> Said {
    let mut said = Said::default();
    let Some(source) = codex::open_rollout(path) else { return said };
    let mut resumable = false;
    let marks = ["\"session_meta\"", "\"type\":\"message\""];
    lines(source, HEAD, &marks, |text| {
        let Ok(Line { kind, payload }) = serde_json::from_str(text) else { return };
        if kind == "session_meta" && said.session.is_empty() {
            let source = payload.source.as_ref().and_then(|source| source.as_str());
            resumable = source.is_some_and(|source| codex::INTERACTIVE.contains(&source));
            said.session = payload.id.unwrap_or_default();
        }
        let role = payload.role.as_deref().unwrap_or_default();
        let message = kind == "response_item" && payload.kind.as_deref() == Some("message");
        if !resumable || !message || !matches!(role, "user" | "assistant") {
            return;
        }
        let texts = payload.content.into_iter().filter_map(|content| content.text);
        texts
            .filter(|text| role != "user" || codex::is_typed(text))
            .for_each(|text| said.add(&text));
    });
    said
}
