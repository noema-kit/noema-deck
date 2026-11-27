//! Antigravity's usage: each conversation's transcript, at
//! `brain/<id>/.system_generated/logs/transcript_full.jsonl`, holds a step
//! per line, and a finished `PLANNER_RESPONSE` step carries the tokens of its
//! reply. A step may name its model; otherwise it runs the model last chosen,
//! which the transcript records as a change to `Model Selection`. The CLI and
//! the editor, which keeps its folder beside the CLI's, are both read.
//!
//! Antigravity counts input apart from cached input.

use std::{
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::Deserialize;

use super::{Records, Reply, Tokens, lines, seconds};
use crate::agent::entries;

const TRANSCRIPT: &str = ".system_generated/logs/transcript_full.jsonl";

/// Every transcript of the CLI whose folder is `home`, and of the editor.
pub(super) fn files(home: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let editor = home.with_file_name("antigravity");
    let apps = if editor == home { vec![editor] } else { vec![home.to_owned(), editor] };
    for app in apps {
        for conversation in entries(&app.join("brain"))? {
            let transcript = conversation.join(TRANSCRIPT);
            if transcript.is_file() {
                found.push(transcript);
            }
        }
    }
    Ok(found)
}

#[derive(Deserialize)]
#[expect(clippy::struct_field_names, reason = "The names Antigravity writes")]
struct Step {
    step_index: Option<u64>,
    #[serde(rename = "type")]
    kind: Option<String>,
    status: Option<String>,
    created_at: Option<String>,
    content: Option<String>,
    model: Option<String>,
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cache_read_tokens: u64,
    #[serde(default)]
    cache_write_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

pub(super) fn read(path: &Path) -> Records {
    let mut records = Records::default();
    // The conversation's folder, three up from the transcript, is its ID.
    let session = path.ancestors().nth(3).and_then(Path::file_name).and_then(|name| name.to_str());
    let (Some(session), Ok(file)) = (session, File::open(path)) else { return records };
    let mut chosen = String::from("unknown");
    lines(file, usize::MAX, &["\"PLANNER_RESPONSE\"", "Model Selection"], |text| {
        let Ok(step) = serde_json::from_str::<Step>(text) else { return };
        if let Some(model) = step.content.as_deref().and_then(model_chosen) {
            chosen = model;
        }
        let done = step.kind.as_deref() == Some("PLANNER_RESPONSE")
            && step.status.as_deref() == Some("DONE");
        let tokens = Tokens {
            input: step.input_tokens,
            cached: step.cache_read_tokens,
            written: step.cache_write_tokens,
            output: step.output_tokens,
            replies: 1,
        };
        let Some(index) = step.step_index.filter(|_| done && !tokens.is_empty()) else { return };
        records.replies.push(Reply {
            key: format!("{session}/{index}"),
            rank: 0,
            session: session.into(),
            model: step.model.filter(|model| !model.is_empty()).unwrap_or_else(|| chosen.clone()),
            at: step.created_at.as_deref().and_then(seconds).unwrap_or(0),
            tokens,
        });
    });
    records
}

/// The model a note such as "changed setting `Model Selection` from None to
/// Gemini 4 Neon (High). …" chose. A model's name holds dots of its own, so
/// only a dot and a space end it.
fn model_chosen(content: &str) -> Option<String> {
    let rest = &content[content.find("Model Selection")?..];
    let rest = &rest[rest.find(" to ")? + 4..];
    let end = rest.find(". ").or_else(|| rest.find('\n')).unwrap_or(rest.len());
    let model = rest[..end].trim().trim_end_matches('.');
    (!model.is_empty() && !model.eq_ignore_ascii_case("none")).then(|| model.to_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn counts_finished_replies_with_the_model_chosen() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let logs = dir.path().join("brain/c1").join(TRANSCRIPT);
        fs::create_dir_all(logs.parent().expect("a folder"))?;
        let lines = [
            r#"{"step_index":0,"type":"USER_INPUT","status":"DONE","content":"The user changed setting `Model Selection` from None to Gemini 4.5 Neon (High). No need"}"#,
            r#"{"step_index":1,"type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-11-27T13:43:10Z","input_tokens":100,"cache_read_tokens":50,"output_tokens":7}"#,
            r#"{"step_index":2,"type":"PLANNER_RESPONSE","status":"RUNNING","input_tokens":9}"#,
            r#"{"step_index":3,"type":"PLANNER_RESPONSE","status":"DONE","model":"gemini-4-argon","output_tokens":3}"#,
        ];
        fs::write(&logs, lines.join("\n"))?;
        assert_eq!(files(dir.path())?, std::slice::from_ref(&logs));
        let replies: Vec<_> = read(&logs)
            .replies
            .into_iter()
            .map(|reply| (reply.key, reply.model, reply.tokens.input))
            .collect();
        assert_eq!(
            replies,
            [
                ("c1/1".into(), "Gemini 4.5 Neon (High)".into(), 100),
                ("c1/3".into(), "gemini-4-argon".into(), 0)
            ]
        );
        Ok(())
    }
}
