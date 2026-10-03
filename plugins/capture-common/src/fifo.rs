//! Opening the FIFO an output plugin receives the programme on.
//!
//! An output's media travels the opposite way from a source's, and stdin is
//! already the control channel, so the core makes a FIFO beside the instance's
//! sockets and puts its path in `GMX_MEDIA`. That is written down in
//! `docs/reference/plugin-lifecycle.md`.
//!
//! There is one trap in it, and it is the reason this module exists. Opening a
//! FIFO for reading blocks until somebody opens the other end for writing, and
//! opening it for writing blocks until somebody opens it for reading. The core
//! builds its `filesink` on the FIFO and only then calls `start` on the
//! plugin, so a plugin that waited for `start` before opening its end would
//! deadlock with the core, each waiting for the other.
//!
//! `O_NONBLOCK` breaks it. A read only open with `O_NONBLOCK` returns at once
//! whether or not a writer exists, and clearing the flag afterwards leaves an
//! ordinary blocking descriptor. So the plugin opens its end as soon as it
//! knows the path, which is at `initialize`, and the core's own open then
//! succeeds.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;

/// A file descriptor this process owns, closed when it is dropped.
#[derive(Debug)]
pub struct Fifo {
    fd: i32,
    /// The pipe this process made, on Windows.
    #[cfg(windows)]
    handle: isize,
    /// Whether the core has its end open, on Windows: a pipe is connected to
    /// once per writer.
    #[cfg(windows)]
    connected: bool,
    path: String,
}

impl Fifo {
    /// The number to give `fdsrc`.
    pub fn fd(&self) -> i32 {
        self.fd
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

#[cfg(windows)]
impl Drop for Fifo {
    fn drop(&mut self) {
        win::close(self.handle);
    }
}

#[cfg(unix)]
impl Drop for Fifo {
    fn drop(&mut self) {
        // SAFETY: the descriptor was opened by `open_read` and nothing else
        // has closed it; `Fifo` is not `Copy` and hands out only the number.
        unsafe {
            libc::close(self.fd);
        }
    }
}

/// Open a FIFO for reading without waiting for a writer.
#[cfg(unix)]
pub fn open_read(path: &Path) -> Result<Fifo, String> {
    use std::os::unix::ffi::OsStrExt;
    let display = path.display().to_string();
    let c = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| format!("the media path {display} has a nul byte in it"))?;
    // SAFETY: the path is NUL terminated and outlives both calls, and the
    // descriptor is checked before it is used.
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
    if fd < 0 {
        return Err(format!(
            "could not open the programme FIFO at {display}: {}. The core makes it before it \
             starts this plugin, so this usually means the instance was removed.",
            std::io::Error::last_os_error()
        ));
    }
    // Back to a blocking descriptor now that the open has happened. `fdsrc`
    // reads it, and a non blocking read would come back as an error rather
    // than as a wait.
    // SAFETY: `fd` is a descriptor this function just opened.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags >= 0 {
        // SAFETY: as above; clearing one flag on a descriptor we own.
        unsafe {
            libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
        }
    }
    Ok(Fifo { fd, path: display })
}

/// Windows: the core names a pipe (`\\.\pipe\...`) rather than a file, and
/// this end makes it. A named pipe has a server that makes it and a client
/// that opens it by name, and the core's `filesink` can only open, so this is
/// the server: made here at `initialize`, the same moment a Unix plugin opens
/// its FIFO, and connected to in the pump once the core's sink opens its end.
#[cfg(windows)]
pub fn open_read(path: &Path) -> Result<Fifo, String> {
    let display = path.display().to_string();
    let handle = win::serve(&display)
        .map_err(|e| format!("could not make the programme pipe at {display}: {e}. The core names it when it starts this plugin, so this usually means another copy of this output is still running."))?;
    Ok(Fifo { fd: 0, handle, connected: false, path: display })
}

/// How much to ask for in one read. Big enough that a megabyte of programme is
/// a handful of syscalls, small enough that it is not a page of memory per
/// instance sitting idle.
const CHUNK: usize = 256 * 1024;

/// How long to wait before looking again, while nobody is writing yet.
const WAIT_FOR_WRITER: std::time::Duration = std::time::Duration::from_millis(20);

/// A thread that reads the FIFO and pushes what it finds into an `appsrc`.
///
/// `fdsrc` would be the obvious element and it does not work here. It waits on
/// the descriptor with `poll`, and `poll` on a FIFO does not report readable on
/// macOS however much is written into it: the reader sits in the poll for as
/// long as you leave it while the writer blocks on a full pipe. The core's own
/// Windows path already reads a pipe on a thread and feeds an `appsrc` for its
/// own reasons, so this is the same shape, and it works the same on every
/// platform.
///
/// The pump blocks when the `appsrc` is full, which is the backpressure that
/// makes a slow disk slow the recording rather than losing the middle of it.
pub struct Pump {
    stop: Arc<AtomicBool>,
    bytes: Arc<AtomicU64>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Pump {
    /// Start reading `fifo` into `src`, which must be an `appsrc`.
    ///
    /// The pump owns the descriptor from here: it is closed when the pump
    /// stops, which is what lets the core see the far end go away.
    pub fn start(fifo: Fifo, src: gst::Element) -> Pump {
        let stop = Arc::new(AtomicBool::new(false));
        let bytes = Arc::new(AtomicU64::new(0));
        let thread_stop = Arc::clone(&stop);
        let thread_bytes = Arc::clone(&bytes);
        let handle = std::thread::Builder::new()
            .name("gmx-fifo-pump".into())
            .spawn(move || {
                let mut fifo = fifo;
                let mut chunk = vec![0u8; CHUNK];
                while !thread_stop.load(Ordering::Relaxed) {
                    let read = fifo.read_some(&mut chunk);
                    match read {
                        // A read of nothing means no writer has the other end.
                        // Before the first byte that is the core not having
                        // opened it yet, and waiting is right; after it, the
                        // core has closed and the programme is over. Getting
                        // this the wrong way round ends the recording before
                        // it starts, because the pump is running by the time
                        // the core opens its end.
                        Ok(0) if thread_bytes.load(Ordering::Relaxed) == 0 => {
                            std::thread::sleep(WAIT_FOR_WRITER);
                        }
                        Ok(0) => break,
                        Ok(n) => {
                            thread_bytes.fetch_add(n as u64, Ordering::Relaxed);
                            let buffer = gst::Buffer::from_slice(chunk[..n].to_vec());
                            let flow: gst::FlowReturn = src.emit_by_name("push-buffer", &[&buffer]);
                            if flow != gst::FlowReturn::Ok {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let _: gst::FlowReturn = src.emit_by_name("end-of-stream", &[]);
                drop(fifo);
            })
            .expect("could not start the FIFO reader");
        Pump {
            stop,
            bytes,
            handle: Some(handle),
        }
    }

    /// How much programme has been read. `health` reports it.
    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    /// Ask the pump to stop after the read it is in.
    ///
    /// It does not wait: the thread may be inside a `read` that only returns
    /// when the core writes again, and a `stop` that blocked on the core would
    /// be the wrong way round. The descriptor closes when the thread ends.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for Pump {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Fifo {
    /// One read. `Ok(0)` means no writer has the other end: not yet, or not
    /// any more, which the pump tells apart by whether anything came first.
    #[cfg(unix)]
    fn read_some(&mut self, into: &mut [u8]) -> Result<usize, std::io::Error> {
        loop {
            // SAFETY: reading into a buffer we own, from a descriptor this
            // `Fifo` owns and keeps alive for the length of this call.
            let n = unsafe { libc::read(self.fd, into.as_mut_ptr().cast(), into.len()) };
            if n >= 0 {
                return Ok(n as usize);
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }

    /// The same on Windows: wait for the core's sink to open the pipe, then
    /// read. The core closing its end reads as `Ok(0)`, and the pipe is made
    /// ready for the next writer in case it was the first open of two.
    #[cfg(windows)]
    fn read_some(&mut self, into: &mut [u8]) -> Result<usize, std::io::Error> {
        if !self.connected {
            win::accept(self.handle)?;
            self.connected = true;
        }
        match win::read(self.handle, into)? {
            0 => {
                self.connected = false;
                win::disconnect(self.handle);
                Ok(0)
            }
            n => Ok(n),
        }
    }
}

/// The named pipe calls, declared here rather than pulled in with a bindings
/// crate for five of them.
#[cfg(windows)]
mod win {
    use std::io::Error;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateNamedPipeW(
            name: *const u16,
            open_mode: u32,
            pipe_mode: u32,
            max_instances: u32,
            out_buffer: u32,
            in_buffer: u32,
            timeout_ms: u32,
            security: *const core::ffi::c_void,
        ) -> isize;
        fn ConnectNamedPipe(pipe: isize, overlapped: *mut core::ffi::c_void) -> i32;
        fn DisconnectNamedPipe(pipe: isize) -> i32;
        fn ReadFile(file: isize, buffer: *mut u8, len: u32, read: *mut u32, overlapped: *mut core::ffi::c_void) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }

    const PIPE_ACCESS_INBOUND: u32 = 0x1;
    const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
    /// Byte mode, blocking, and nobody from another machine.
    const PIPE_MODE: u32 = 0x8;
    const ERROR_PIPE_CONNECTED: i32 = 535;
    const ERROR_BROKEN_PIPE: i32 = 109;
    const ERROR_NO_DATA: i32 = 232;
    /// A megabyte each way, which is a few frames of an HD programme.
    const BUFFER: u32 = 1 << 20;

    pub fn serve(name: &str) -> Result<isize, Error> {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: a NUL terminated name that outlives the call; the handle is
        // checked before it is used.
        let h = unsafe {
            CreateNamedPipeW(wide.as_ptr(), PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_MODE, 1, BUFFER, BUFFER, 0, std::ptr::null())
        };
        if h == -1 {
            return Err(Error::last_os_error());
        }
        Ok(h)
    }

    /// Block until a writer opens the pipe. One that opened it before this
    /// was called is already connected, which is not an error.
    pub fn accept(pipe: isize) -> Result<(), Error> {
        // SAFETY: a pipe handle this module made.
        if unsafe { ConnectNamedPipe(pipe, std::ptr::null_mut()) } != 0 {
            return Ok(());
        }
        let e = Error::last_os_error();
        match e.raw_os_error() {
            Some(ERROR_PIPE_CONNECTED) => Ok(()),
            _ => Err(e),
        }
    }

    pub fn read(pipe: isize, into: &mut [u8]) -> Result<usize, Error> {
        let mut got = 0u32;
        let len = u32::try_from(into.len()).unwrap_or(u32::MAX);
        // SAFETY: reading into a buffer we own, from a handle this module made.
        if unsafe { ReadFile(pipe, into.as_mut_ptr(), len, &mut got, std::ptr::null_mut()) } != 0 {
            return Ok(got as usize);
        }
        let e = Error::last_os_error();
        match e.raw_os_error() {
            // The writer closed its end: the same as a FIFO's end of file.
            Some(ERROR_BROKEN_PIPE) | Some(ERROR_NO_DATA) => Ok(0),
            _ => Err(e),
        }
    }

    pub fn disconnect(pipe: isize) {
        // SAFETY: a pipe handle this module made.
        unsafe { DisconnectNamedPipe(pipe) };
    }

    pub fn close(pipe: isize) {
        // SAFETY: closed once, from `Fifo::drop`.
        unsafe { CloseHandle(pipe) };
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_fifo_opens_with_nobody_writing_yet_and_reads_what_arrives_later() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("gmx-fifo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("media.programme");
        let c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        // SAFETY: a NUL terminated path this test owns.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);

        // This is the whole point: it returns rather than waiting for a writer.
        let fifo = open_read(&path).expect("the read end opens with no writer");
        assert!(fifo.fd() >= 0);
        assert!(fifo.path().ends_with("media.programme"));

        // And the descriptor is a blocking one that really carries bytes.
        let mut writer = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        writer.write_all(b"programme").unwrap();
        drop(writer);
        let mut buffer = [0u8; 9];
        // SAFETY: reading into a buffer we own from a descriptor we own.
        let read = unsafe { libc::read(fifo.fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
        assert_eq!(read, 9);
        assert_eq!(&buffer, b"programme");

        drop(fifo);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_pump_carries_what_the_writer_wrote_into_an_appsrc() {
        use std::io::Write;
        gst::init().unwrap();
        let dir = std::env::temp_dir().join(format!("gmx-pump-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("media.programme");
        let c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        // SAFETY: a NUL terminated path this test owns.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);

        let pipeline = gst::parse::launch(
            "appsrc name=in format=bytes is-live=false ! fakesink name=out sync=false",
        )
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
        let src = pipeline.by_name("in").unwrap();
        let sink = pipeline.by_name("out").unwrap();
        let seen = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&seen);
        sink.static_pad("sink")
            .unwrap()
            .add_probe(gst::PadProbeType::BUFFER, move |_, info| {
                if let Some(gst::PadProbeData::Buffer(b)) = &info.data {
                    counter.fetch_add(b.size() as u64, Ordering::Relaxed);
                }
                gst::PadProbeReturn::Ok
            })
            .unwrap();

        // The pump starts before anything is writing, which is exactly how
        // the core does it, and is the case that used to end the recording
        // before it began.
        let fifo = open_read(&path).expect("the read end opens");
        let mut pump = Pump::start(fifo, src);
        pipeline.set_state(gst::State::Playing).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let mut writer = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        writer.write_all(&vec![7u8; 300_000]).unwrap();
        writer.flush().unwrap();
        drop(writer);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while seen.load(Ordering::Relaxed) < 300_000 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(seen.load(Ordering::Relaxed), 300_000, "the pump lost bytes");
        assert_eq!(pump.bytes(), 300_000);

        pump.stop();
        let _ = pipeline.set_state(gst::State::Null);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Windows shape of the test above: the pump starts on a pipe nobody
    /// has opened, and the writer opens it by name as the core's `filesink`
    /// does.
    #[cfg(windows)]
    #[test]
    fn the_pump_carries_what_a_writer_wrote_down_a_named_pipe() {
        use std::io::Write;
        gst::init().unwrap();
        let path = format!(r"\\.\pipe\gmx-pump-test-{}", std::process::id());
        let pipeline = gst::parse::launch(
            "appsrc name=in format=bytes is-live=false ! fakesink name=out sync=false",
        )
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
        let src = pipeline.by_name("in").unwrap();
        let sink = pipeline.by_name("out").unwrap();
        let seen = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&seen);
        sink.static_pad("sink")
            .unwrap()
            .add_probe(gst::PadProbeType::BUFFER, move |_, info| {
                if let Some(gst::PadProbeData::Buffer(b)) = &info.data {
                    counter.fetch_add(b.size() as u64, Ordering::Relaxed);
                }
                gst::PadProbeReturn::Ok
            })
            .unwrap();
        let fifo = open_read(Path::new(&path)).expect("the pipe is made");
        let mut pump = Pump::start(fifo, src);
        pipeline.set_state(gst::State::Playing).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let mut writer = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        writer.write_all(&vec![7u8; 300_000]).unwrap();
        writer.flush().unwrap();
        drop(writer);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while seen.load(Ordering::Relaxed) < 300_000 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(seen.load(Ordering::Relaxed), 300_000, "the pump lost bytes");
        assert_eq!(pump.bytes(), 300_000);
        pump.stop();
        let _ = pipeline.set_state(gst::State::Null);
    }

    #[test]
    fn a_path_that_is_not_there_says_what_makes_it() {
        let err = open_read(Path::new("/no/such/place/media.programme")).unwrap_err();
        assert!(err.contains("core"), "{err}");
    }
}
