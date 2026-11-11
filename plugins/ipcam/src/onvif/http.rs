//! A SOAP POST over plain HTTP. The code lives in `godwinmix-ptz`, which
//! drives PTZ cameras over the same ONVIF services; this module keeps the
//! names discovery has always used.

pub use godwinmix_ptz::http::{post, split, Failure};
