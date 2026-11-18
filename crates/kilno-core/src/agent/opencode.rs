//! OpenCode's command lines. Like Codex it picks its own session ids, so a new pane or a fork is
//! bound to its session once that appears ([`crate::sessions::opencode`]). No folder argument:
//! the pane's shell already starts in it.

use super::AgentKind;
use crate::launch::Agent;
use crate::paths::Env;
use crate::text::ps_quote;

/// OpenCode as one pane launches it. Build a fresh one for every launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenCodeAgent {
    /// Full path where found; "opencode" when nothing was found.
    pub exe: String,
}

impl OpenCodeAgent {
    pub fn new(exe: impl Into<String>) -> Self {
        Self { exe: exe.into() }
    }

    pub fn for_this_user(env: &dyn Env, saved_path: Option<&str>) -> Self {
        let exe = super::detect::locate(env, AgentKind::OpenCode, saved_path);
        Self::new(exe.unwrap_or_else(|| "opencode".to_string()))
    }

    fn exe(&self) -> String {
        format!("& {}", ps_quote(&self.exe))
    }
}

impl Agent for OpenCodeAgent {
    fn kind(&self) -> AgentKind {
        AgentKind::OpenCode
    }

    fn knows_id_upfront(&self) -> bool {
        false
    }

    fn new_command(&self, _new_id: Option<&str>) -> String {
        self.exe()
    }

    // An id is read from OpenCode's database, so it is quoted like any other text.
    fn resume_command(&self, id: &str) -> String {
        format!("{} --session {}", self.exe(), ps_quote(id))
    }

    fn fork_command(&self, parent_id: &str, _new_id: Option<&str>) -> String {
        format!("{} --session {} --fork", self.exe(), ps_quote(parent_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines() {
        let a = OpenCodeAgent::new(r"C:\Users\x\AppData\Roaming\npm\opencode.cmd");
        assert_eq!(a.new_command(None), r"& 'C:\Users\x\AppData\Roaming\npm\opencode.cmd'");
        assert_eq!(a.resume_command("ses_abc"), r"& 'C:\Users\x\AppData\Roaming\npm\opencode.cmd' --session 'ses_abc'");
        assert_eq!(a.fork_command("ses_abc", None), r"& 'C:\Users\x\AppData\Roaming\npm\opencode.cmd' --session 'ses_abc' --fork");
        assert_eq!(a.resume_command("x'; rm"), r"& 'C:\Users\x\AppData\Roaming\npm\opencode.cmd' --session 'x''; rm'", "an id stays text");
        assert!(!a.knows_id_upfront());
    }
}
