//! The headend's machine and its Flussonic import. Three are hot in the
//! `headend` profile: a box of fifty channels on one GPU is run by reading
//! the GPU and the machine, and is usually moved from a Flussonic config.

use super::Binding;
use crate::method::Tier::{Headend, Search};

pub const BINDINGS: &[Binding] = &[
    Binding {
        method: "gpu.status",
        tool: "gpu_status",
        tier: Headend,
        description: "Every NVIDIA GPU on this machine now and over the last two minutes: \
            encoder_pct, decoder_pct, memory, encoder_sessions (every process's), temperature \
            and power. Read it before adding conversions and when channels fail on the GPU. \
            since_ms returns only newer history. No GPUs and a note on a machine without one.",
    },
    Binding {
        method: "machine.status",
        tool: "machine_status",
        tier: Headend,
        description: "The machine itself: host name, cores, cpu_pct of every process, \
            memory total and available, uptime of the machine and of this station, load \
            average. One small read; call it beside gpu_status.",
    },
    Binding {
        method: "headend.import",
        tool: "import_flussonic",
        tier: Headend,
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
