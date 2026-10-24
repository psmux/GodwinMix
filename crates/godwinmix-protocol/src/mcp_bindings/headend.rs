//! The headend's machine, its channels and its Flussonic import. The GPU
//! and the machine are hot in the `headend` profile, because that is how a box of fifty channels
//! is run by reading them. Setting channels up is done once per box, so
//! those tools, the import among them, wait behind `search_tools`
//! ("headend channel", "scan", "flussonic"). The hot list has no room left:
//! a channel's schema alone is larger than what remains of its budget.

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
        description: "Make a Flussonic config into headend channels: each stream one show without \
            compositing, kept by the station and editable afterwards, with its input, one HLS output (TS and CMAF, open access) and the \
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
    Binding {
        method: "headend.channel.list",
        tool: "headend_channels",
        tier: Search,
        description: "Every headend channel with its input, profile, HLS settings, outputs and \
            the show it runs as (follows: a stream of the followed Flussonic file), every \
            transcode profile, and the encoders this machine has (on: gpu or cpu).",
    },
    Binding {
        method: "headend.channel.add",
        tool: "add_headend_channels",
        tier: Search,
        description: "Add channels, each {name, title?, enabled?, input: {uri, program?, interface?, \
            params?, backup?}, profile (copy, aac-sound, h264-480p-800, h264-720p-1500, h264-1080p-4000, a saved id from \
            headend_channels, or custom with custom: {video, audio}), \
            hls?, outputs?: [{uri, copy?}]}. name is the public path /<name>/index.m3u8. dry_run \
            true checks and prices them and makes nothing. Answers done and refused with why.",
    },
    Binding {
        method: "headend.scan",
        tool: "scan_feed",
        tier: Search,
        description: "List the programs of a multiplex: uri (udp://@239.0.0.1:5000), interface. Takes \
            up to wait_ms (8000). Each program has number, name, provider, streams and tracks \
            (video size and rate, sound codec, channels, language). Then add_headend_channels \
            with input.program set to each number wanted.",
    },
    Binding {
        method: "headend.channel.update",
        tool: "update_headend_channel",
        tier: Search,
        description: "Replace one headend channel whole: {name, channel}. Only an input, profile or \
            output change makes its show again. A stream of the followed Flussonic file is taken \
            over by the station from then on.",
    },
    Binding {
        method: "headend.channel.bulk",
        tool: "bulk_edit_headend_channels",
        tier: Search,
        description: "One change to many headend channels: names, and profile (or custom), \
            enabled, or interface.",
    },
    Binding {
        method: "headend.channel.remove",
        tool: "remove_headend_channels",
        tier: Search,
        description: "Remove headend channels by name, and their shows. A followed file's stream \
            is hidden from the file, which is not edited.",
    },
    Binding {
        method: "headend.channel.follow",
        tool: "follow_flussonic_again",
        tier: Search,
        description: "Give channels taken over from the followed Flussonic file back to it, by name: \
            the station stops keeping them and runs them as the file says.",
    },
    Binding {
        method: "headend.profile.save",
        tool: "save_transcode_profile",
        tier: Search,
        description: "Save a transcode profile {id, name, video, audio}: video and audio are a \
            rendition's wants (codec, width, height, fit, bitrate_kbps, keyframe_frames, \
            deinterlace, encoder gpu or cpu; audio codec, bitrate_kbps, tracks). Every channel \
            that takes it follows.",
    },
    Binding {
        method: "headend.profile.remove",
        tool: "remove_transcode_profile",
        tier: Search,
        description: "Remove a saved transcode profile by id; move_to names the profile its \
            channels take instead.",
    },
    Binding {
        method: "headend.export",
        tool: "export_flussonic",
        tier: Search,
        description: "The headend channels this station keeps, as a Flussonic config, with notes \
            for what Flussonic cannot say.",
    },
];
