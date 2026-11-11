//! Whether the link to a switcher is up, and why not when it is not.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkStatus {
    /// Sending hellos and waiting for an answer.
    Connecting,
    /// The switcher answered and is sending its state.
    Syncing,
    /// The state has arrived and commands go through.
    Up,
}

/// The link as a caller sees it. `reason` is set whenever the status is not
/// `up` because something went wrong, and says what to check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    pub status: LinkStatus,
    pub reason: Option<String>,
    /// Sessions opened since this client started. Above one means the link
    /// has dropped and come back.
    pub sessions: u32,
}

impl Link {
    pub(crate) fn connecting() -> Link {
        Link { status: LinkStatus::Connecting, reason: None, sessions: 0 }
    }

    pub fn is_up(&self) -> bool {
        self.status == LinkStatus::Up
    }

    /// One line for a log or an error.
    pub fn describe(&self) -> String {
        match (&self.status, &self.reason) {
            (LinkStatus::Up, _) => "up".into(),
            (LinkStatus::Syncing, _) => "receiving the switcher's state".into(),
            (LinkStatus::Connecting, Some(reason)) => format!("down: {reason}"),
            (LinkStatus::Connecting, None) => "connecting".into(),
        }
    }
}

/// Reasons, in one place so the docs can list them.
pub mod reasons {
    pub const NO_ANSWER: &str = "no answer to the hello. Check the switcher is on, that the \
        address is right, and that this computer can reach it (ping it, and check no firewall \
        blocks UDP port 9910). Retrying every second";
    pub const FULL: &str = "the switcher refused the session because every client slot is in \
        use. Close ATEM Software Control, a panel or another program connected to it. Retrying \
        in five seconds";
    pub const SILENT: &str = "the switcher stopped answering for five seconds. Check its power \
        and network cable. Reconnecting";
    pub const UNACKED: &str = "the switcher did not acknowledge a command after ten tries. \
        Reconnecting";
}
