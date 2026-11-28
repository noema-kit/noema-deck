//! What was said in a Claude Code session: the text of its `user` and
//! `assistant` records. A person's message is a string, or blocks of text
//! and pictures; an agent's is blocks, of which only the text is what it
//! said. Records Claude Code wrote for itself are marked `isMeta`.

use std::{
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::Deserialize;

use super::Said;
use crate::records::{lines, walk};

/// Every session's transcript under `projects`, a folder per project.
/// Sub-agents' transcripts lie deeper, and are not sessions.
pub(super) fn files(projects: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    walk(projects, ".jsonl", 1, &mut found)?;
    Ok(found)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    #[serde(default)]
    is_meta: bool,
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: Content,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<Block>),
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

pub(super) fn read(path: &Path) -> Said {
    let id = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default();
    let mut said = Said { session: id.into(), ..Said::default() };
    let Ok(file) = File::open(path) else { return said };
    lines(file, usize::MAX, &["\"type\":\"user\"", "\"type\":\"assistant\""], |text| {
        let Ok(Line { is_meta: false, message }) = serde_json::from_str(text) else { return };
        match message.content {
            Content::Text(text) => said.add(&text),
            Content::Blocks(blocks) => {
                let texts = blocks.into_iter().filter(|block| block.kind == "text");
                texts.filter_map(|block| block.text).for_each(|text| said.add(&text));
            }
        }
    });
    said
}
