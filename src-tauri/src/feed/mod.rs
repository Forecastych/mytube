//! The feed: the user's rules and the pure `rank` that applies them.
//! Nothing here touches the network, so it is all testable on fixtures.

pub mod commands;
pub mod rank;
pub mod rules;
pub mod store;
pub mod text;
pub mod topics;
