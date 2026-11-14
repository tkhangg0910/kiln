//! One sidebar row's worth of transcript data.

use crate::agent::AgentKind;
use crate::status::SessionStatus;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Where a Codex turn stands, from the last task event in its transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnState {
    Working,
    Done,
    Stopped,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionInfo {
    pub session_id: String,
    pub agent: AgentKind,
    /// The session this one was branched from, when the transcript says so (Codex).
    pub parent_id: Option<String>,
    pub file: Option<PathBuf>,
    /// Working directory from the transcript (or a display fallback).
    pub project_dir: String,
    /// First human prompt; [`NO_PROMPT`] when there is none.
    pub preview: String,
    pub ai_title: Option<String>,
    pub last_prompt: Option<String>,
    pub last_activity: DateTime<Utc>,
    /// When the session began (Codex session_meta). For Claude, same as last_activity.
    pub started: DateTime<Utc>,
    pub turn: Option<TurnState>,
    pub status: Option<SessionStatus>,
}

impl SessionInfo {
    /// Last path segment of the project folder, else the folder string itself.
    pub fn project_name(&self) -> String {
        crate::paths::leaf(&self.project_dir)
    }
}

/// Preview placeholder for a transcript with no human prompt yet.
pub const NO_PROMPT: &str = "(no prompt yet)";
