//! How many malloc arenas the process keeps, on Linux with glibc.
//!
//! glibc gives a thread that finds the main heap's lock taken an arena of its
//! own, up to eight per core, and an arena never hands its free space back to
//! another thread or, mostly, to the system. A mixer is the worst case for
//! that: seventy odd threads, most of them GStreamer streaming threads moving
//! frame sized blocks, and a mosaic, an audio branch and a source pipeline
//! whose threads start and stop every few seconds while an operator works.
//! Each new thread lands on whichever arena is free and leaves its high water
//! mark there.
//!
//! The nightly soak showed what that costs. The show process grew from 475 MB
//! to 527 MB over an hour (482 MB to 538 MB in a twenty minute run), more
//! slowly as the hour went on, while the number of GStreamer elements and pads
//! in every pipeline stayed the same round after round. The same soak with
//! `MALLOC_ARENA_MAX=2` sat at 464 MB from the second minute to the thirtieth
//! and ended at 457 MB, with no programme stall over the bar. So the growth was
//! the allocator spreading itself across arenas, not something the mixer kept,
//! and on a Raspberry Pi with a few hundred megabytes to spare it is memory a
//! show cannot afford.
//!
//! Two, because that is what was measured. A caller who sets
//! `MALLOC_ARENA_MAX` keeps their own number: glibc has already read it, and
//! this leaves it alone.

/// The arena cap. See the module comment for where the number comes from.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
const ARENAS: libc::c_int = 2;

/// Cap the arenas. Called first thing in `main`, before any other thread
/// exists, so every thread the process starts is covered by it.
pub fn cap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        if std::env::var_os("MALLOC_ARENA_MAX").is_some() {
            return;
        }
        // SAFETY: `mallopt` sets one allocator tunable and touches no memory
        // of ours. It is called while the process has one thread.
        unsafe {
            libc::mallopt(libc::M_ARENA_MAX, ARENAS);
        }
    }
}
