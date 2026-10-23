//! The headend's machine and its Flussonic import. The GPU and the machine
//! are hot in the `headend` profile, because a box of fifty channels on one
//! GPU is run by reading them. The import is done once per box, so it waits
//! behind `search_tools` ("flussonic") like the rest of the station's tools.

use super::Binding;
use crate::method::Tier::{Headend, Search};

pub const BINDINGS: &[Binding] = &[
    Binding {
        method: "gpu.status",
        tool: "gpu_status",
        tier: Headend,
        description: "Each NVIDIA GPU now and for two minutes: encoder_pct, decoder_pct, \
            memory, encoder_sessions, temperature, power. Read before adding conversions.",
    },
    Binding {
        method: "machine.status",
        tool: "machine_status",
        tier: Headend,
        description: "This machine: cores, cpu_pct, memory total and available, uptime, \
            load average. One small read.",
    },
    Binding {
        method: "headend.import",
        tool: "import_flussonic",
        tier: Search,
        description: "Make a Flussonic config into shows: each stream one show without \
            compositing, with its input, one HLS output (TS and CMAF, open access) and the \
            transcoder line as its rendition. Send text (the config) or path (a file on the \
            station's machine). dry_run true (the default) answers streams, notes (lines not \
            carried) and shows.refused and makes nothing; then send dry_run false.",
    },
    Binding {
        method: "headend.status",
        tool: "headend_status",
        tier: Search,
        description: "Whether this station follows a Flussonic config (godwinmix headend), \
            which file, where Flussonic's URLs and API answer, and how many of its streams \
            run as shows.",
    },
];
