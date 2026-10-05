//! The show tools: one show is one encoder or transcoder, so a headend's 200
//! channels are 200 shows, and these are how an agent makes and watches them.
//!
//! Four are hot in the standard profile (`add_shows`, `show_stats`,
//! `set_show`, `set_show_output`), with `list_shows` from its own definition.
//! Their descriptions are charged on every call, so they say what to send and
//! what comes back and stop.

use super::Binding;
use crate::method::Tier::{Headend, Search};

pub const BINDINGS: &[Binding] = &[
    Binding {
        method: "show.add_many",
        tool: "add_shows",
        tier: Headend,
        description: "Add many shows in one call: a headend's channel list, one show per feed. \
            Each entry is what add_show takes: name, input {uri, program?, backup?}, outputs, and \
            compositing (false: the input straight to its outputs, light, no process per show). \
            Call with dry_run true first and read plan.cost, plan.fits and refused, then again \
            with dry_run false. A refused entry names its index and why; the rest go ahead whole.",
    },
    Binding {
        method: "show.stats",
        tool: "show_stats",
        tier: Headend,
        description: "Health and numbers for many shows in one read: health (ok, warning, \
            alarm, off, with alarms such as no-input, black, freeze, cc-errors), the input's \
            kbps, fps, size and codecs, and each output's state and kbps. Cheap enough to call \
            every few seconds for 200 shows. `ids` narrows it, `fields` trims it.",
    },
    Binding {
        method: "show.set",
        tool: "set_show",
        tier: Headend,
        description: "Change one show: name, input, or compositing. compositing true gives it \
            scenes, transitions and a programme encode; false sends one input straight to its \
            outputs, refused while it holds more than one source or a scene. Outputs keep \
            sending across the switch.",
    },
    Binding {
        method: "show.output.set",
        tool: "set_show_output",
        tier: Headend,
        description: "Change one output of a show without compositing: `id` is the show, \
            `output` the output. On or off, where it goes, or its format: rendition null copies \
            the input's own bytes (almost free); {preset: <id>} from rendition_presets \
            re-encodes, admitted by the governor or refused with what it would cost.",
    },
    Binding {
        method: "show.add",
        tool: "add_show",
        tier: Search,
        description: "Make one show and start it. With compositing false and an input (udp, \
            srt, rtmp, rtsp, hls, rist, file or channel:<app>/<stream>) it is a light transcoder \
            or remux; with compositing true (the default) it is a whole mixer. For more than one, \
            add_shows is one call.",
    },
    Binding {
        method: "show.remove",
        tool: "remove_show",
        tier: Search,
        description: "Stop a show and remove it with its folder. Destructive. Refused for the \
            last show and for main. remove_shows takes a list.",
    },
    Binding {
        method: "show.remove_many",
        tool: "remove_shows",
        tier: Search,
        description: "Stop and remove several shows in one call, by id. Destructive, and \
            dry_run says what would go.",
    },
    Binding {
        method: "show.start",
        tool: "start_show",
        tier: Search,
        description: "Start a show that was stopped or failed. The answer is the show with \
            its state; show_stats says when its input and outputs are flowing.",
    },
    Binding {
        method: "show.stop",
        tool: "stop_show",
        tier: Search,
        description: "Stop a show and its outputs. It keeps its settings and stays stopped \
            across a restart of the station until start_show.",
    },
    Binding {
        method: "show.rename",
        tool: "rename_show",
        tier: Search,
        description: "Give a show another name. Its id stays, so nothing that names it by id \
            has to change.",
    },
    Binding {
        method: "show.output.add",
        tool: "add_show_output",
        tier: Search,
        description: "Send a show without compositing somewhere else as well: `id` is the \
            show, then an address (udp, rtp, rist, srt, rtmp) or a platform and key. No \
            rendition copies the input; a rendition or preset re-encodes, priced first.",
    },
    Binding {
        method: "show.output.remove",
        tool: "remove_show_output",
        tier: Search,
        description: "Stop one output of a show without compositing and forget it: `id` is \
            the show, `output` the output. The show and its other outputs carry on.",
    },
];
