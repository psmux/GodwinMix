//! `gmx-ptz`: PTZ cameras for GodwinMix, as a `service` plugin.
//!
//! The drivers are `godwinmix-ptz`'s and the tracking is `godwinmix-track`'s;
//! this is what joins them to the mixer and to agents: a thread per camera,
//! a joystick that stops by itself, presets with pictures, and a follow loop
//! that reads the camera's picture and steers it.
//!
//! | Module | What it is |
//! |---|---|
//! | [`settings`] | `schemas/ptz.json`, read into structs |
//! | [`worker`] | the thread that owns one camera's driver, and the joystick watchdog |
//! | [`rig`] | every camera, from the settings and from `camera_add` |
//! | [`presets`] | presets' names and thumbnails on disk |
//! | [`core`] | the mixer's address, a still and an MJPEG stream from it |
//! | [`frames`] | pictures for the tracker: the frame bus, or the core's MJPEG |
//! | [`follow`] | the follow loop |
//! | [`tools`] | every tool, and the answers they give |
//! | [`service`] | the plugin protocol's `Service`, and the command line mode |

pub mod core;
pub mod follow;
pub mod frames;
pub mod presets;
pub mod rig;
pub mod service;
pub mod settings;
pub mod tools;
pub mod worker;

pub use settings::{CameraSpec, Settings};
