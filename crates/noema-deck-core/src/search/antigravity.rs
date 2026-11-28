//! What was said in an Antigravity conversation: the content of the
//! `USER_INPUT` and `PLANNER_RESPONSE` steps in its transcript, which lies in
//! a folder named by the conversation's ID.

use std::{fs::File, path::Path};

use serde::Deserialize;

use super::Said;
use crate::records::lines;

#[derive(Deserialize)]
struct Step {
    #[serde(rename = "type")]
    kind: String,
    content: Option<String>,
}

pub(super) fn read(path: &Path) -> Said {
    // `<id>/.system_generated/logs/transcript_full.jsonl`.
    let id = path.ancestors().nth(3).and_then(Path::file_name).and_then(|name| name.to_str());
    let mut said = Said { session: id.unwrap_or_default().into(), ..Said::default() };
    let Ok(file) = File::open(path) else { return said };
    lines(file, usize::MAX, &["\"USER_INPUT\"", "\"PLANNER_RESPONSE\""], |text| {
        let Ok(Step { kind, content: Some(content) }) = serde_json::from_str(text) else { return };
        if matches!(&*kind, "USER_INPUT" | "PLANNER_RESPONSE") {
            said.add(&content);
        }
    });
    said
}
