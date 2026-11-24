//! The core of Noema Deck: agent sessions arranged on a universe of topic stars.
//!
//! Sessions stay with the agent that created them. Deck keeps the stars, the
//! links between them and which sessions sit on which stars, and runs an
//! agent's own interface in shell panes.
//!
//! | Module | Holds |
//! | --- | --- |
//! | [`agent`] | The agents and how each lists and runs its sessions |
//! | [`session`], [`star`] | The things on the map |
//! | [`store`] | Deck's own records, in one `SQLite` file |
//! | [`universe`] | The map as a whole, and the requests that change it |
//! | [`pane`] | Shell panes on pseudoterminals |
//! | [`deck`] | One running Deck, shared by every window and connection |

pub mod agent;
pub mod deck;
pub mod folder;
pub mod import;
pub mod pane;
pub mod session;
pub mod star;
pub mod store;
pub mod universe;

pub use agent::{Agent, Launch};
pub use deck::Deck;
pub use session::{Session, SessionKey};
pub use star::{Star, StarId};
pub use universe::{Snapshot, Universe};
