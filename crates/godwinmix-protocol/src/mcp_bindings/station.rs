//! The rest of what the station answers: channels, the governor, the plan,
//! and the project file. None of these is hot; `search_tools` finds them.

use super::Binding;
use crate::method::Tier::Search;

pub const BINDINGS: &[Binding] = &[
    Binding {
        method: "governor.status",
        tool: "governor_status",
        tier: Search,
        description: "What this machine can still encode: the governor's budget, what is \
            admitted and what it costs, and anything shed. Read it before adding re-encodes; \
            add_shows with dry_run prices a whole list against it.",
    },
    Binding {
        method: "governor.calibrate",
        tool: "calibrate_governor",
        tier: Search,
        description: "Measure what this machine's encoders really cost, a few seconds of every \
            core. Refused while something is on air unless confirm is true.",
    },
    Binding {
        method: "rendition.plan",
        tool: "plan_rendition",
        tier: Search,
        description: "What the planner built for every output that asked for a rendition: each \
            encode, which encoder and why, and the totals. scope is `programme` or \
            `channel:<id>`.",
    },
    Binding {
        method: "project.export",
        tool: "export_project",
        tier: Search,
        description: "This mixer as one project file: settings, sources, outputs and \
            renditions. Keys and addresses stay out unless include_secrets is true.",
    },
    Binding {
        method: "project.import",
        tool: "import_project",
        tier: Search,
        description: "Open a project file. Answers with what it would change unless dry_run is \
            false. Destructive: it can replace every source, output and scene at once.",
    },
    Binding {
        method: "channel.add",
        tool: "add_channel",
        tier: Search,
        description: "Make a channel: an address encoders publish to (RTMP by default, SRT, \
            WHIP), and its first key, which is in the answer. A show's input can then be \
            `channel:<app>/<stream>`.",
    },
    Binding {
        method: "channel.get",
        tool: "get_channel",
        tier: Search,
        description: "One channel: its addresses, its keys as hints, who is publishing and its \
            destinations with their state.",
    },
    Binding {
        method: "channel.set",
        tool: "set_channel",
        tier: Search,
        description: "Rename a channel, switch it on or off, or change its application name or \
            protocols. Name only what moves.",
    },
    Binding {
        method: "channel.remove",
        tool: "remove_channel",
        tier: Search,
        description: "Remove a channel and forget its keys. Destructive: a key taken back \
            cannot be given again.",
    },
    Binding {
        method: "channel.destination.add",
        tool: "add_channel_destination",
        tier: Search,
        description: "Send a channel's stream on to a platform (youtube, facebook, twitch, \
            custom, srt) with a key, copied as it arrives or converted with \
            rendition {preset: <id>}. `id` is the channel.",
    },
    Binding {
        method: "channel.destination.set",
        tool: "set_channel_destination",
        tier: Search,
        description: "Change one destination of a channel: on or off, its key, its label or \
            its rendition. `id` is the channel, `destination` the destination.",
    },
    Binding {
        method: "channel.destination.remove",
        tool: "remove_channel_destination",
        tier: Search,
        description: "Stop sending a channel to one destination and forget it. Destructive: \
            the platform sees the stream end.",
    },
];
