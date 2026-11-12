//! One switcher over a real socket: a tokio task that owns a [`Session`]
//! and a UDP socket, and a handle that sends commands to it and reads its
//! state.
//!
//! The task never ends on its own. A switcher that is off, unplugged or full
//! is retried for as long as the handle lives, and the reason it is not up
//! is in [`Snapshot::link`] the whole time.
//!
//! [`Session`]: crate::session::Session

mod task;

use std::time::Duration;

use serde::Serialize;
use tokio::sync::{mpsc, oneshot, watch};

use crate::command::Command;
use crate::session::{Link, NotUp, Timing};
use crate::state::State;
use crate::Error;

/// What a caller sees of one switcher at one moment.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub id: String,
    pub address: String,
    pub link: Link,
    pub state: State,
}

type Reply = oneshot::Sender<Result<(), NotUp>>;

/// The handle to one switcher. Dropping it closes the session.
pub struct Atem {
    id: String,
    requests: mpsc::UnboundedSender<(Vec<Command>, Reply)>,
    snapshot: watch::Receiver<Snapshot>,
    task: tokio::task::AbortHandle,
}

impl Atem {
    /// Start talking to the switcher at `address` on the current tokio
    /// runtime. Returns at once; the link comes up in the background.
    pub fn connect(id: &str, address: &str) -> Atem {
        Atem::connect_on(&tokio::runtime::Handle::current(), id, address)
    }

    /// The same, on a runtime handle, for a caller outside the runtime.
    pub fn connect_on(runtime: &tokio::runtime::Handle, id: &str, address: &str) -> Atem {
        Atem::connect_with(runtime, id, address, Timing::default())
    }

    /// The same with other waits, which a test on loopback shortens.
    pub fn connect_with(runtime: &tokio::runtime::Handle, id: &str, address: &str, timing: Timing) -> Atem {
        let first = Snapshot {
            id: id.to_string(),
            address: address.to_string(),
            link: Link::connecting(),
            state: State::default(),
        };
        let (publish, snapshot) = watch::channel(first);
        let (requests, inbox) = mpsc::unbounded_channel();
        let task = runtime.spawn(task::run(address.to_string(), timing, publish, inbox));
        Atem { id: id.to_string(), requests, snapshot, task: task.abort_handle() }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// The latest state and link.
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.borrow().clone()
    }

    /// A receiver that wakes on every change.
    pub fn watch(&self) -> watch::Receiver<Snapshot> {
        self.snapshot.clone()
    }

    /// Send commands and wait until they are on the wire. The switcher's
    /// answer shows up in the state, not here.
    pub async fn send(&self, commands: Vec<Command>) -> Result<(), Error> {
        let (reply, answer) = oneshot::channel();
        self.requests.send((commands, reply)).map_err(|_| self.closed())?;
        match answer.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(NotUp(link))) => Err(Error::Down { slug: self.id.clone(), link }),
            Err(_) => Err(self.closed()),
        }
    }

    /// Send from code that is not async. Refuses at once when the link is
    /// not up; otherwise the commands are queued and go out in order.
    pub fn send_now(&self, commands: Vec<Command>) -> Result<(), Error> {
        let link = self.snapshot.borrow().link.clone();
        if !link.is_up() {
            return Err(Error::Down { slug: self.id.clone(), link });
        }
        let (reply, _ignored) = oneshot::channel();
        self.requests.send((commands, reply)).map_err(|_| self.closed())
    }

    /// Wait for the link to be up, at most `limit`.
    pub async fn wait_up(&self, limit: Duration) -> Result<(), Error> {
        let mut watch = self.snapshot.clone();
        let up = tokio::time::timeout(limit, watch.wait_for(|s| s.link.is_up())).await;
        match up {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) => Err(self.closed()),
            Err(_) => Err(Error::Down { slug: self.id.clone(), link: self.snapshot().link }),
        }
    }

    fn closed(&self) -> Error {
        Error::Closed { slug: self.id.clone() }
    }
}

impl Drop for Atem {
    fn drop(&mut self) {
        self.task.abort();
    }
}
