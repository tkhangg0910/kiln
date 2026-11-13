//! Kiln's logic with no UI: sessions, transcripts, status, settings, rules, the island's
//! state machine. Nothing in here writes under the Claude or Codex data folders; see
//! [`paths::ReadOnlyDir`] and `tests/invariants.rs`.

pub mod agent;
pub mod cli;
pub mod crash;
pub mod doc;
pub mod git;
pub mod images;
pub mod island;
pub mod launch;
pub mod pages;
pub mod pane_status;
pub mod paths;
pub mod screenshots;
pub mod security;
pub mod sessions;
pub mod settings;
pub mod settings_view;
pub mod setup;
pub mod sidebar;
pub mod status;
pub mod store;
pub mod system;
pub mod text;
pub mod time;
