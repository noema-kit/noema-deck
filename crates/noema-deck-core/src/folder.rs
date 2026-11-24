//! Folders as the interface shows them and as a person types them.

use std::{
    env,
    path::{Path, PathBuf},
};

/// A folder for display, with the home directory written as `~`.
#[must_use]
pub fn label(folder: &Path) -> String {
    match env::home_dir().and_then(|home| folder.strip_prefix(home).ok().map(Path::to_owned)) {
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => folder.display().to_string(),
    }
}

/// The folder a label names, with a leading `~` read as the home directory.
#[must_use]
pub fn from_label(label: &str) -> PathBuf {
    let label = label.trim();
    match (label.strip_prefix('~'), env::home_dir()) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            home.join(rest.trim_start_matches('/'))
        }
        _ => label.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_reads_back_as_the_folder_it_was_made_from() {
        let home = env::home_dir().unwrap_or_default();

        for folder in [home.join("work/notes"), home.clone(), PathBuf::from("/var/data")] {
            assert_eq!(from_label(&label(&folder)), folder);
        }
        assert!(label(&home.join("work")).starts_with('~'));
        assert_eq!(from_label("  ~other/x "), PathBuf::from("~other/x"));
    }
}
