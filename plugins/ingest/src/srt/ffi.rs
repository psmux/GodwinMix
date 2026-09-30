//! libsrt, loaded when the first SRT channel needs it.
//!
//! Loaded at run time with `libloading` rather than linked, for the reason
//! the RTSP server is not in this plugin: linking would make libsrt's
//! headers a build requirement on every platform, for people who only ever
//! use RTMP. libsrt is on every machine that has GStreamer's SRT elements,
//! because they are built on it, so it is found where they found it. A
//! machine without it gets a sentence saying so, and RTMP carries on.
//!
//! Only the dozen calls a listener needs are here, each wrapped so nothing
//! outside this file touches a raw pointer.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::OnceLock;

pub type Socket = i32;
pub const INVALID: Socket = -1;

/// `int (*)(void* opaque, SRTSOCKET ns, int hsversion, const struct sockaddr* peer, const char* streamid)`.
pub type ListenHook = unsafe extern "C" fn(*mut c_void, Socket, c_int, *const c_void, *const c_char) -> c_int;

/// Socket options used here, from `srt.h`.
pub const RCVTIMEO: c_int = 14;
pub const PASSPHRASE: c_int = 26;

/// "No data yet" on a blocking socket with a timeout, from `srt.h`.
pub const E_ASYNC_RCV: c_int = 6002;

pub struct Lib {
    _lib: libloading::Library,
    startup: unsafe extern "C" fn() -> c_int,
    create_socket: unsafe extern "C" fn() -> Socket,
    bind: unsafe extern "C" fn(Socket, *const c_void, c_int) -> c_int,
    listen: unsafe extern "C" fn(Socket, c_int) -> c_int,
    accept: unsafe extern "C" fn(Socket, *mut c_void, *mut c_int) -> Socket,
    listen_callback: unsafe extern "C" fn(Socket, Option<ListenHook>, *mut c_void) -> c_int,
    close: unsafe extern "C" fn(Socket) -> c_int,
    setsockflag: unsafe extern "C" fn(Socket, c_int, *const c_void, c_int) -> c_int,
    recvmsg: unsafe extern "C" fn(Socket, *mut c_char, c_int) -> c_int,
    getlasterror: unsafe extern "C" fn(*mut c_int) -> c_int,
    getlasterror_str: unsafe extern "C" fn() -> *const c_char,
    setrejectreason: unsafe extern "C" fn(Socket, c_int) -> c_int,
}

/// Where libsrt is looked for, after `GMX_LIBSRT` if that is set.
fn candidates() -> Vec<String> {
    let mut out: Vec<String> = std::env::var("GMX_LIBSRT").ok().into_iter().collect();
    let names: &[&str] = if cfg!(target_os = "macos") {
        &[
            "libsrt.1.5.dylib",
            "libsrt.dylib",
            "/opt/homebrew/lib/libsrt.1.5.dylib",
            "/usr/local/lib/libsrt.1.5.dylib",
            "/Library/Frameworks/GStreamer.framework/Versions/1.0/lib/libsrt.1.5.dylib",
            "/Library/Frameworks/GStreamer.framework/Versions/1.0/lib/libsrt.dylib",
        ]
    } else if cfg!(windows) {
        &["srt.dll", "libsrt.dll", "srt-1.5.dll"]
    } else {
        &["libsrt.so.1.5", "libsrt-gnutls.so.1.5", "libsrt-openssl.so.1.5", "libsrt.so.1", "libsrt.so"]
    };
    out.extend(names.iter().map(|s| s.to_string()));
    out
}

/// libsrt, loaded and started once for the life of the process.
pub fn lib() -> Result<&'static Lib, String> {
    static LIB: OnceLock<Result<Lib, String>> = OnceLock::new();
    LIB.get_or_init(load).as_ref().map_err(Clone::clone)
}

fn load() -> Result<Lib, String> {
    let tried = candidates();
    // SAFETY: loading libsrt runs its initialisers, which have no
    // preconditions; nothing else is called before `srt_startup`.
    let lib = tried.iter().find_map(|name| unsafe { libloading::Library::new(name) }.ok()).ok_or_else(|| {
        "SRT needs libsrt, which comes with GStreamer's SRT elements, and it is not on this \
         machine. Install GStreamer's bad plugins (they carry srtsrc), or set GMX_LIBSRT to \
         the library's path, and switch SRT on again."
            .to_string()
    })?;
    macro_rules! get {
        ($name:literal) => {
            // SAFETY: each symbol is declared above with the signature
            // `srt.h` gives it for libsrt 1.4 and 1.5.
            *unsafe { lib.get(concat!($name, "\0").as_bytes()) }
                .map_err(|e| format!("this libsrt has no {}: {e}. SRT needs libsrt 1.4 or later.", $name))?
        };
    }
    let loaded = Lib {
        startup: get!("srt_startup"),
        create_socket: get!("srt_create_socket"),
        bind: get!("srt_bind"),
        listen: get!("srt_listen"),
        accept: get!("srt_accept"),
        listen_callback: get!("srt_listen_callback"),
        close: get!("srt_close"),
        setsockflag: get!("srt_setsockflag"),
        recvmsg: get!("srt_recvmsg"),
        getlasterror: get!("srt_getlasterror"),
        getlasterror_str: get!("srt_getlasterror_str"),
        setrejectreason: get!("srt_setrejectreason"),
        _lib: lib,
    };
    // SAFETY: no arguments; safe to call more than once.
    if unsafe { (loaded.startup)() } < 0 {
        return Err(format!("libsrt would not start: {}", loaded.last_error()));
    }
    Ok(loaded)
}

impl Lib {
    pub fn last_error(&self) -> String {
        // SAFETY: returns a pointer to a thread local string owned by libsrt.
        let text = unsafe { (self.getlasterror_str)() };
        if text.is_null() {
            return "unknown error".into();
        }
        // SAFETY: non null, nul terminated, valid until the next SRT call on this thread.
        unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
    }

    pub fn last_code(&self) -> c_int {
        // SAFETY: a null errno pointer is allowed.
        unsafe { (self.getlasterror)(std::ptr::null_mut()) }
    }

    /// A listening socket on `addr`, with `hook` asked about every caller.
    pub fn listener(&self, addr: std::net::SocketAddr, hook: ListenHook, opaque: *mut c_void) -> Result<Socket, String> {
        // SAFETY: no arguments.
        let sock = unsafe { (self.create_socket)() };
        if sock == INVALID {
            return Err(format!("could not make an SRT socket: {}", self.last_error()));
        }
        let raw = super::addr::encode(addr);
        // SAFETY: `raw` is a sockaddr of the length given, alive for the call.
        let bound = unsafe { (self.bind)(sock, raw.as_ptr().cast(), raw.len() as c_int) };
        // SAFETY: `opaque` outlives the socket; the caller closes the socket first.
        let hooked = bound >= 0 && unsafe { (self.listen_callback)(sock, Some(hook), opaque) } >= 0;
        // SAFETY: a bound socket.
        if !hooked || unsafe { (self.listen)(sock, 16) } < 0 {
            let why = self.last_error();
            self.close(sock);
            return Err(format!(
                "could not listen for SRT on UDP {addr}: {why}. Another program may hold the port; \
                 pick another SRT port in the ingest plugin's settings."
            ));
        }
        Ok(sock)
    }

    /// Wait for a caller. `None` when the listener was closed.
    pub fn accept(&self, listener: Socket) -> Option<(Socket, String)> {
        let mut raw = [0u8; 128];
        let mut len = raw.len() as c_int;
        // SAFETY: `raw` is large enough for any sockaddr and `len` says so.
        let sock = unsafe { (self.accept)(listener, raw.as_mut_ptr().cast(), &mut len) };
        (sock != INVALID).then(|| (sock, super::addr::decode(&raw[..len.max(0) as usize])))
    }

    pub fn close(&self, sock: Socket) {
        // SAFETY: closing an id that is already closed is an error, not undefined.
        unsafe { (self.close)(sock) };
    }

    pub fn set_text(&self, sock: Socket, option: c_int, value: &str) -> bool {
        let Ok(text) = CString::new(value) else { return false };
        let bytes = text.as_bytes();
        // SAFETY: a string option takes a pointer and its length without the nul.
        unsafe { (self.setsockflag)(sock, option, bytes.as_ptr().cast(), bytes.len() as c_int) >= 0 }
    }

    pub fn set_int(&self, sock: Socket, option: c_int, value: i32) -> bool {
        // SAFETY: an int option takes a pointer to an int and its size.
        unsafe { (self.setsockflag)(sock, option, (&value as *const i32).cast(), 4) >= 0 }
    }

    pub fn reject(&self, sock: Socket, code: i32) {
        // SAFETY: a socket id handed to the listen callback.
        unsafe { (self.setrejectreason)(sock, code) };
    }

    /// One message, up to 1316 bytes in live mode. `Ok(0)` is a timeout.
    pub fn recv(&self, sock: Socket, buf: &mut [u8]) -> Result<usize, String> {
        // SAFETY: `buf` is writable for its whole length.
        let n = unsafe { (self.recvmsg)(sock, buf.as_mut_ptr().cast(), buf.len() as c_int) };
        if n >= 0 {
            return Ok(n as usize);
        }
        match self.last_code() {
            E_ASYNC_RCV => Ok(0),
            _ => Err(self.last_error()),
        }
    }
}
