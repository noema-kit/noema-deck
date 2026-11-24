//! The agents whose sessions Deck arranges, and how each one is run.

pub mod claude;

use std::{fmt, path::PathBuf, str::FromStr};

use anyhow::{Error, bail};
use serde::{Deserialize, Serialize};

/// An agent whose sessions Deck can assign to domains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
}

impl Agent {
    /// The name the store and the interface know the agent by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
        }
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl FromStr for Agent {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "claude" => Ok(Self::Claude),
            other => bail!("Unknown agent `{other}`"),
        }
    }
}

/// A command to run in a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub folder: PathBuf,
    pub program: String,
    pub args: Vec<String>,
}
