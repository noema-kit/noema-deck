//! The core of Noema Deck: agent sessions arranged on a map of domains.
//!
//! Sessions stay with the agent that created them. Deck keeps the domains, the
//! links between them and which sessions belong to which domains, and runs an
//! agent's own interface in shell panes.
//!
//! | Module | Holds |
//! | --- | --- |
//! | [`agent`] | The agents and how each lists and runs its sessions |
//! | [`session`], [`domain`] | The things on the map |
//! | [`store`] | Deck's own records, in one `SQLite` file |
//! | [`map`] | The map as a whole, and the requests that change it |
//! | [`pane`] | Shell panes on pseudoterminals |
//! | [`usage`] | The tokens the agents used |
//! | [`monitor`] | What the machine is doing, and each pane's part in it |
//! | [`fonts`] | The typefaces installed on this machine |
//! | [`deck`] | One running Deck, shared by every window and connection |

pub mod agent;
pub mod deck;
pub mod domain;
pub mod folder;
pub mod fonts;
pub mod import;
pub mod map;
pub mod monitor;
pub mod pane;
pub mod session;
pub mod store;
pub mod usage;

pub use agent::{Agent, Launch};
pub use deck::Deck;
pub use domain::{Domain, DomainId};
pub use map::{Map, Snapshot};
pub use session::{Session, SessionKey};
