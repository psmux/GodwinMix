//! Every refusal says what state things are in and what to do next.

use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// A name that is not `camera:<id>` or `channel:<app>/<stream>`.
    BadName(String),
    /// A size or format the bus cannot carry.
    BadLayout(String),
    /// Nobody publishes this name.
    NotFound { name: String, path: String },
    /// Somebody already publishes this name, and is alive.
    NameTaken { name: String, pid: u32 },
    /// The owner went away and did not come back within the reader's wait.
    OwnerGone(String),
    /// The other side sent something this version does not understand.
    Protocol(String),
    /// The operating system refused a call.
    Os(String),
    /// This platform has no cross process transport yet.
    Unsupported(String),
}

impl Error {
    /// A slug naming the kind, for a caller that turns this into a protocol
    /// error with `data`.
    pub fn code(&self) -> &'static str {
        match self {
            Error::BadName(_) => "bad-name",
            Error::BadLayout(_) => "bad-layout",
            Error::NotFound { .. } => "not-found",
            Error::NameTaken { .. } => "name-taken",
            Error::OwnerGone(_) => "owner-gone",
            Error::Protocol(_) => "protocol",
            Error::Os(_) => "os",
            Error::Unsupported(_) => "unsupported",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadName(m) | Error::BadLayout(m) | Error::OwnerGone(m) => f.write_str(m),
            Error::NotFound { name, path } => write!(
                f,
                "nothing publishes {name} on the frame bus (looked for {path}). \
                 Start the source that owns it, then subscribe again"
            ),
            Error::NameTaken { name, pid } => write!(
                f,
                "{name} is already published by process {pid}, which is running. \
                 Read it with a Subscriber, or stop that process first"
            ),
            Error::Protocol(m) => write!(f, "frame bus protocol: {m}"),
            Error::Os(m) => write!(f, "frame bus: the operating system refused {m}"),
            Error::Unsupported(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Os(e.to_string())
    }
}
