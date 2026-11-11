//! One error type for every driver. Each message says what happened and what
//! to do next, and `code` is a slug a caller can branch on.

use std::fmt;

/// What kind of failure, as a slug: `unreachable`, `timeout`, `refused`,
/// `unauthorized`, `unsupported`, `bad-reply`, `bad-argument`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// Nothing answered at the address, or the connection broke.
    Unreachable,
    /// The camera took the connection but did not answer in time.
    Timeout,
    /// The camera answered with an error for this command.
    Refused,
    /// The camera wants a login, or refused the one given.
    Unauthorized,
    /// This protocol or this camera has no such control.
    Unsupported,
    /// The camera said something this driver cannot read.
    BadReply,
    /// The caller asked for something out of range.
    BadArgument,
}

impl Code {
    pub fn slug(self) -> &'static str {
        match self {
            Code::Unreachable => "unreachable",
            Code::Timeout => "timeout",
            Code::Refused => "refused",
            Code::Unauthorized => "unauthorized",
            Code::Unsupported => "unsupported",
            Code::BadReply => "bad-reply",
            Code::BadArgument => "bad-argument",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub code: Code,
    pub message: String,
}

impl Error {
    pub fn new(code: Code, message: impl Into<String>) -> Error {
        Error { code, message: message.into() }
    }

    pub fn unsupported(protocol: &str, what: &str) -> Error {
        Error::new(
            Code::Unsupported,
            format!("{protocol} has no {what} control on this camera. Use the camera's own menu for it, or pick a protocol that has it."),
        )
    }

    pub fn io(to: &str, e: &std::io::Error) -> Error {
        use std::io::ErrorKind::*;
        match e.kind() {
            TimedOut | WouldBlock => Error::new(
                Code::Timeout,
                format!("{to} took the command but did not answer in time. Check the camera is on and that its control port is the one configured."),
            ),
            _ => Error::new(
                Code::Unreachable,
                format!("cannot reach {to}: {e}. Check the address, the port and that the camera's IP control is switched on."),
            ),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.code.slug())
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
