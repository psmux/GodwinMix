"""Generated from protocol.json by clients/gen/generate.py. Do not edit.

Every type, method and event the core describes in `core.api`, as Python. A
method added to the core reaches this file by running:

    python3 clients/gen/generate.py

The drift test in tests/test_generated.py fails if this file and protocol.json
have parted company.

The types here are TypedDicts, which is to say they are the shape of the JSON
on the wire and nothing more: no validation happens at runtime and a dict with
extra keys is still the right type. `total=False` throughout, because a core
one api_level ahead may leave out a field this build thinks is required.
"""

from __future__ import annotations

from typing import Any, Dict, List, Literal, Optional, TypedDict, Union

API_LEVEL = 1
API_COMPATIBLE = 1

class AdBreakRequest(TypedDict, total=False):
    """`adbreak.start`."""

    at_running_time_ms: Optional[int]
    # Programme running time to open the break on. Omit to roll now.
    return_to: Optional[str]
    # Source to rejoin afterwards. Omit to return to whatever is on air.
    uri: str
    # File path or URI of the clip to roll.

class AdStatus(TypedDict, total=False):
    """An ad break, either armed for a future cue or currently on air."""

    on_air: bool
    # False while it is prerolled and waiting for its cue.
    return_to: Optional[str]
    # Source to return to when the ad ends. None returns to the slate.
    uri: str

class AddDestinationRequest(TypedDict, total=False):
    """`channel.destination.add`. Send a channel's stream on to a platform."""

    enabled: Optional[bool]
    # On by default.
    id: str
    # The channel.
    key: Optional[str]
    # The stream key. Write only: no method reads it back.
    label: Optional[str]
    # What the list calls it. The platform's name when left out.
    platform: str
    # youtube, facebook, twitch, custom or srt.
    rendition: Union[RenditionChoice, None]
    # Convert the stream before sending it: `{"preset": "youtube-720p30"}` or a rendition request written out. Left out, or one the stream already matches, the stream is sent as it arrives.
    server: Optional[str]
    # The ingest address. Left out, the platform's own; custom and srt need one.
    stream: Optional[str]
    # Which of the channel's streams to send. `*`, the default, is the first one live.

class AddFilterRequest(TypedDict, total=False):
    """`filter.add`."""

    id: str
    # The name this filter answers to afterwards. A slug, and yours to pick.
    params: Dict[str, Any]
    # The filter's own settings. What goes in here is the filter's business, not the core's.
    programme: bool
    # Filter the programme rather than one source.
    side: str
    # `input` puts it before the proxy boundary, where the thumbnail sees it too; `programme` puts it on this source's programme branch only.
    source: Optional[str]
    # The source to hang it on. Leave it out and set `programme` to filter everything that goes out.
    type: str
    # A filter type id, as `plugin.list` and `core.api` `kinds` report them.

class AddItemFilterRequest(TypedDict, total=False):
    """`scene.item.filter.add`."""

    draft: Optional[str]
    item: str
    name: Optional[str]
    params: Dict[str, Any]
    scene: str
    type: str
    # A filter type id, as `plugin.list` reports them.

class AddItemRequest(TypedDict, total=False):
    """`scene.item.add`."""

    content: Any
    # What the item shows: `{"source": "cam1"}`, `{"ref": "<scene id>"}` or `{"graphic": "plugin/id"}`.
    draft: Optional[str]
    # A draft id from `scene.edit.begin`, to change a working copy instead of the live document.
    name: Optional[str]
    # What to call it. Left out, a source item is named after its source, because a model reasons about words.
    scene: str
    transform: Any
    # Where it goes. Left out, the next free cell of a grid over what is already there, so a drop on a scene never needs a dialog.

class AddOutputRequest(TypedDict, total=False):
    """`output.add`. The id and the URL are the whole of it for an RTMP destination; anything else a kind understands rides in `params`."""

    id: str
    # Stable id for this destination.
    policy: Optional[str]
    # Reconnect policy: "own" retries quickly, for servers you run; "cdn" backs off harder, for platforms that penalise hammering.
    rendition: Optional[Dict[str, Any]]
    # What to make from the programme for this destination: a rendition request or `{"preset": "youtube-720p30"}` (`rendition.presets` lists them). Absent means the programme encoder, at no extra cost.
    uri: str
    # rtmp:// or rtmps:// URL including the stream key. Left out for a kind with no address of its own, such as `hls/output`, which is served from the control port.

class AddPluginRequest(TypedDict, total=False):
    """`plugin.add`."""

    source: str
    # Where the plugin comes from. One of: `owner/repo` (a GitHub release, optionally `@version`), a git URL ending in `.git`, `cargo:name`, `npm:@scope/name`, `pypi:name`, `oci:ref`, an absolute path to a directory, or a bare plugin name to look up in the marketplaces.

class AddSceneRequest(TypedDict, total=False):
    color: Optional[str]
    # A colour for every client, the tally and the Stream Deck to agree on.
    name: str

class AddSourceRequest(TypedDict, total=False):
    """`source.add`."""

    id: Optional[str]
    # Stable id used by `program.take` and `source.remove`. Lowercase letters, digits and dashes. Derived from the name or the host when omitted, with a numeric suffix if that is taken.
    kind: Optional[str]
    # "web" renders the URL as a page in the browser sidecar, the same as writing `web+` in front of it. "auto" or omitted works the protocol out from the URL.
    name: Optional[str]
    # Name shown to an operator. Defaults to the host of the URL.
    superimpose: Optional[str]
    # Websites only: "auto" lets the mixer decode the page's own video outside the browser and draw the page over the top, which saves about a CPU core. "off" is the default.
    uri: str
    # Stream URL, file path, or with kind "web" the address of a page.

class AgentStateRequest(TypedDict, total=False):
    response_format: ResponseFormat

class Alarm(TypedDict, total=False):
    """One condition that holds now."""

    detail: str
    # One sentence for a person: what was measured, and against what.
    kind: AlarmKind
    since_ms: int
    # Unix milliseconds when the condition began. For black, freeze and silence that is when the picture or sound first measured so, not when the alarm's duration ran out.

class AlarmSettings(TypedDict, total=False):
    """A show's alarms, as a person sets them from the page. Left out fields keep the measuring side's defaults; a duration of 0 switches that check off."""

    black_ms: Optional[int]
    enabled: Optional[bool]
    # Whether black, freeze and silence are watched at all. Left out: on for a show without compositing, off for one that composites.
    freeze_ms: Optional[int]
    silence_dbfs: Optional[float]
    # The peak level under which sound counts as quiet.
    silence_ms: Optional[int]

class ApplyGraphicRequest(TypedDict, total=False):
    """`scene.apply_graphic`."""

    frame: bool
    # Answer with a still of the armed scene as well as the records.
    graphic: str
    # The graphic to fill, `ograf/lower-third`.
    item: Optional[str]
    # Which placement, by the name you gave the item or by its id. Left out, every placement of this graphic is filled.
    play: bool
    # Bring it on after filling it in.
    stop: bool
    # Take it off.
    values: Dict[str, Any]
    # The fields, by name. `scene.item.schema` says which there are.

class ApplyLayoutRequest(TypedDict, total=False):
    """`scene.apply_layout`."""

    duration_ms: Optional[int]
    # How long the change takes, in milliseconds. 0 is a cut.
    easing: Optional[str]
    layout: str
    # A layout name from `scene.layout.list`.
    name: Optional[str]
    scene: Optional[str]
    # The scene to apply it to. Left out, a new one is made. Applying onto an existing scene keeps the item ids, so an animated layout change is a property ramp rather than a cut.
    values: Dict[str, Any]
    # The layout's parameters by name: its source slots and its numbers.

class ApplyRequest(TypedDict, total=False):
    dry_run: bool
    # Work out the plan and write nothing.
    force: bool
    # Take the preset's value wherever the operator already has one.
    keep_sources: bool
    # Leave the operator's sources and outputs alone.
    name: str
    # A preset name, or a path to a directory holding `gmx-plugin.toml`.

class ApplyResult(TypedDict, total=False):
    """What `preset.apply` answers with."""

    applied: Any
    # Present when the preset was actually applied.
    dry_run: bool
    # True when nothing was written because `dry_run` was set.
    live: List[str]
    # The sources and outputs this core picked up without a restart.
    needs_restart: List[str]
    # What still needs a restart, in plain words. Empty is the good case.
    plan: Any
    # The whole plan, as JSON. The same object `preset.list` rows point at.

class Asset(TypedDict, total=False):
    """A file the collection carries with it."""

    path: str
    # Relative to the collection root, always.
    sha256: Optional[str]
    size: Optional[int]

class AudioSetParams(TypedDict, total=False):
    """`source.audio.set` takes an id as well as the levels: the id comes off the path on REST and out of the params on `/rpc`, and both land in one object."""

    gain: Optional[float]
    # The operator's fader for the whole source, 0.0 to 10.0. Omit to leave it where it is.
    id: str
    # Source id.
    media: List[Optional[float]]
    # Gain per video underneath, by position. A null entry, or a list shorter than the number of videos, leaves those alone: `[null, 0.0]` silences the second video and touches nothing else.
    muted: Optional[bool]
    # Mute the whole source. Held apart from the fader, so unmuting comes back to the level that was set.
    page: Optional[float]
    # Gain on a superimposed page's own sound.

class AudioShape(TypedDict, total=False):
    """A sound as it is."""

    bitrate_kbps: int
    # 0 when unknown.
    channels: int
    codec: AudioCodec
    sample_rate: int

class AudioWant(TypedDict, total=False):
    """The audio an output wants. Every field left out is taken from the source."""

    bitrate_kbps: Optional[int]
    channels: Optional[int]
    codec: Union[AudioCodec, None]
    sample_rate: Optional[int]

class BackendInfo(TypedDict, total=False):
    audio_decoder: str
    audio_encoder: str
    hardware_accelerated: bool
    video_decoder: str
    video_encoder: str

class BackupInput(TypedDict, total=False):
    """An input's backup: an input with no backup of its own."""

    params: Optional[Dict[str, Any]]
    program: Optional[int]
    uri: str

class BindRequest(TypedDict, total=False):
    """`scene.item.bind`."""

    draft: Optional[str]
    item: str
    param: str
    # An expression over the collection's params and `W`, `H`. An empty string takes the binding off.
    prop: str
    # A geometry path such as `frame.w` or `position.x`.
    scene: str

class BulkPlan(TypedDict, total=False):
    """What a batch costs, priced by the governor without taking anything."""

    assumed_input: str
    # The input every rendition was priced against, because an input's shape is known only once it arrives.
    cost: Cost
    # Every rendition of the shows that fit, summed. Copies cost nothing.
    fits: bool
    # Whether every show of the batch fits.
    have: Cost
    # What the machine has free now.

class Bundle(TypedDict, total=False):
    """What an importer is told before it reads the document."""

    assets: List[BundleAsset]
    # Every file carried, by the path inside the bundle.
    bundle_version: int
    # The envelope version. See [`BUNDLE_VERSION`].
    canvas: Canvas
    id: Id
    # The collection's own stable id, repeated here so a listing can be read without unpacking the document.
    name: str
    requires: List[Requirement]
    # Every plugin this collection needs, with the version range that will do. An importer that has none of them still gets the geometry.
    skipped: List[str]
    # What could not be carried, one line each, so a partial export is visible rather than silent.
    written_by: str
    # The build that wrote it, for a bug report.

class BundleAsset(TypedDict, total=False):
    """One file carried in the bundle."""

    id: Id
    # The asset id in the document.
    path: str
    # Relative to the bundle root, forward slashes. Never absolute: see the head of this module.
    sha256: str
    size: int

class CalibrateRequest(TypedDict, total=False):
    """`governor.calibrate`."""

    confirm: bool
    # Measure even though something is on air. The measurement takes a few seconds of every core and can cost what is on air frames.

class CalibrateResult(TypedDict, total=False):
    started: bool

class Canvas(TypedDict, total=False):
    """The output raster. One per collection in this release; 11 section 1 leaves room for several."""

    fps: int
    height: int
    width: int

class CanvasInfo(TypedDict, total=False):
    """The canvas every source is scaled onto and every output leaves by."""

    fps: int
    height: int
    width: int

class CellAssignment(TypedDict, total=False):
    h: int
    index: int
    source: Optional[str]
    # None means this cell is the program return.
    w: int
    x: int
    y: int

class CertificateGenerateRequest(TypedDict, total=False):
    """`channel.certificate.generate`: a self signed certificate."""

    names: List[str]
    # Host names and addresses it is for. Defaults to this machine's address, `localhost` and `127.0.0.1`.

class CertificateInfo(TypedDict, total=False):
    """The certificate RTMPS answers with. The private key never leaves the core."""

    created: str
    # When it was set, RFC 3339 in UTC.
    fingerprint: str
    # SHA-256 of the certificate, as colon separated hex, to compare with what an encoder shows.
    names: List[str]
    # The names it was made for, for a self signed one.
    source: str
    # `uploaded`, or `self_signed` for one the mixer made.

class CertificateSetRequest(TypedDict, total=False):
    """`channel.certificate.set`: a certificate and its private key, as PEM."""

    cert: str
    # The certificate, and any chain after it, as PEM.
    key: str
    # Its private key, as PEM.

class Change(TypedDict, total=False):
    """One thing an import does or would do."""

    action: str
    # `add`, `replace`, `remove`, `keep`, `rename`, `set`, `wait`, `skip` or `missing`.
    id: str
    # The id, key, scene name or clip name, as the file has it.
    note: Optional[str]
    part: str
    # `setting`, `source`, `output`, `channel`, `scene` or `media`.
    to: Optional[str]
    # The new id or name, for a rename.

class Channel(TypedDict, total=False):
    """A named place encoders publish to, over every protocol it has switched on."""

    app: str
    # The RTMP application name: the path segment after the port.
    auto_source: bool
    # A stream that goes live becomes a mixer source by itself.
    destinations: List[Destination]
    # Where the channel's streams are sent on to, with what each is doing. Changed by `channel.destination.*`.
    enabled: bool
    # Off turns every publisher away with a sentence saying so.
    id: str
    # A slug, and never changes once the channel exists.
    key_mode: KeyMode
    keys: List[ChannelKey]
    # The keys as hints, never the key itself: a read token sees only these.
    name: str
    # What a person calls it.
    protocols: List[ChannelProtocol]
    # The protocols it takes publishers over, besides RTMPS.
    publish: ChannelPublish
    rtmps: Rtmps
    # RTMPS, on a port of its own, when a person has turned it on.
    streams: List[ChannelStream]
    # Live streams, and streams that left while a scene still holds their source.

class ChannelAddRequest(TypedDict, total=False):
    """`channel.add`."""

    app: Optional[str]
    # Defaults to a slug of the name.
    auto_source: Optional[bool]
    key_mode: Union[KeyMode, None]
    name: str
    protocols: Optional[List[ChannelProtocol]]
    # Defaults to RTMP alone.

class ChannelAdded(TypedDict, total=False):
    """What `channel.add` answers: the channel and its first key."""

    channel: Channel
    key: NewKey

class ChannelKey(TypedDict, total=False):
    """One key, as a list shows it."""

    created: str
    # When it was made, RFC 3339 in UTC.
    hint: str
    # The last four characters, so a person can tell two keys apart.
    id: str
    label: str

class ChannelKeyAddRequest(TypedDict, total=False):
    """`channel.key.add`."""

    id: str
    label: Optional[str]

class ChannelKeyRemoveRequest(TypedDict, total=False):
    """`channel.key.remove`."""

    id: str
    key: str

class ChannelKeyRevealRequest(TypedDict, total=False):
    """`channel.key.reveal`: one key of one channel."""

    id: str
    key: str

class ChannelList(TypedDict, total=False):
    """`channel.list`."""

    certificate: Union[CertificateInfo, None]
    # The certificate RTMPS answers with, when there is one.
    channels: List[Channel]
    hosts: List[str]
    # The addresses an encoder can reach this machine at, first one first.
    listeners: List[Listener]
    # Every listener a channel needs, open or not, and why: the ports this mixer has open for ingest, and the channels each is open for.
    rtmp: RtmpInfo

class ChannelPublish(TypedDict, total=False):
    """Where an encoder is pointed."""

    addresses: List[PublishAddress]
    # The same for every protocol the channel has on, RTMP first.
    example: str
    # `<server>/main?psk=<key>`, with `<key>` left for the person to fill.
    server: str
    # `rtmp://<first address>:<port>/<app>`.

class ChannelRemoved(TypedDict, total=False):
    """What `channel.remove` answers."""

    removed: str

class ChannelSetRequest(TypedDict, total=False):
    """`channel.set`: only what is named moves."""

    app: Optional[str]
    auto_source: Optional[bool]
    enabled: Optional[bool]
    id: str
    key_mode: Union[KeyMode, None]
    name: Optional[str]
    protocols: Optional[List[ChannelProtocol]]
    # Which protocols it takes, as a whole list: `["rtmp", "srt"]`.
    rtmps: Union[Rtmps, None]
    # RTMPS on or off, and its port.

ChannelStream = TypedDict("ChannelStream", {
    "audio": Union[StreamAudio, None],
    "dropped_gops": int,
    "from": str,
    "key": Optional[str],
    "name": str,
    "protocol": Optional[str],
    "relay": Optional[str],
    "since_ms": int,
    "source": Optional[str],
    "state": str,
    "video": Union[StreamVideo, None],
}, total=False)

class ConfigChanged(TypedDict, total=False):
    """One key this call changed, and when the change takes effect."""

    applies: Applies
    key: str
    note: Optional[str]
    # Said when something outside the file wins over it, such as `--bind`.

class ConfigGetRequest(TypedDict, total=False):
    keys: List[str]
    # Only these dotted keys. Empty or absent is every key.

class ConfigGetResult(TypedDict, total=False):
    keys: List[ConfigKey]
    needs_restart: List[str]
    # Every key whose new value waits for a restart, whichever keys were asked for.
    path: str
    # The config file these values are read from and written to.

class ConfigKey(TypedDict, total=False):
    """One setting as `config.get` reports it."""

    applies: Applies
    default: Any
    key: str
    # Dotted, as in `program.video_bitrate_kbps`.
    overridden_by: Optional[str]
    # What wins over the file for this key, when something does: `--bind`, or `GODWINMIX_TOKEN` in the core's environment.
    pending: bool
    # True when the file differs from what the running core uses and only a restart will close the gap.
    secret: bool
    set: Optional[bool]
    # For a secret: whether one is set. The value itself is never sent.
    source: str
    # `file` when the key is written in the config file, `default` when not.
    value: Any
    # What the config file says, or the default when it says nothing. Always null for a secret.

class ConfigResetRequest(TypedDict, total=False):
    dry_run: bool
    keys: List[str]
    # Dotted keys to take out of the config file, so their defaults apply.

class ConfigSetRequest(TypedDict, total=False):
    dry_run: bool
    # Check everything and write nothing.
    values: Dict[str, Any]
    # Dotted key to new value: `{"program.video_bitrate_kbps": 4500}`. Null puts a key back to its default. For a secret, the sentinel `"__secret__"` means leave it as it is, and an empty string clears it.

class ConfigSetResult(TypedDict, total=False):
    """What `config.set` and `config.reset` answer with."""

    applied: List[str]
    # Keys from this call in force now.
    changed: List[ConfigChanged]
    # Every key this call changed, each with its `applies`.
    dry_run: bool
    # True when nothing was written because `dry_run` was set.
    needs_restart: List[str]
    # Every key, from this call or an earlier one, whose new value waits for a restart. Empty is the good case.
    next_source: List[str]
    # Keys from this call every source added or rebuilt from now on uses.
    path: str
    # The config file written to.
    unchanged: List[str]
    # Secrets sent back as the sentinel, so left as they were.

class ConversionState(TypedDict, total=False):
    """One conversion, in flight or remembered after it finished."""

    error: Optional[str]
    # Set only on `failed`, shown to the operator verbatim: "no AAC encoder available" is worth more than "conversion failed".
    output: Optional[str]
    # The `.web.mp4` name, on `done`.
    progress: float
    # 0.0 to 1.0, position over duration, both read off the pipeline.
    state: ConversionPhase

class CoreInfo(TypedDict, total=False):
    """`core.info`: what this core is, what it can do, and where its edges are."""

    api_compatible: int
    api_level: int
    canvas: CanvasInfo
    core: str
    # Always "godwinmix".
    features: List[str]
    # Feature strings a client can branch on: multiview, snapshot, uploads, mcp, browser, exec-sources, rehearsal, tokens.
    limits: Limits
    rehearsal: bool
    # True when the core was started with `--rehearsal`, which refuses `output.add` and accepts rehearsal tokens.
    restart: RestartInfo
    # Whether `core.restart` brings this core back, so a page can decide between a Restart button and a sentence.
    supervised: bool
    # True when the core was started with `--supervised` (or `GODWINMIX_SUPERVISED=1`): a service manager, a container runtime or the desktop app starts it again after it exits.
    token: Union[TokenInfo, None]
    # Present when the request carried a token the core recognises.
    ui: Union[UiDefaults, None]
    # What a surface should start with, when a preset chose it. Absent on a core no preset has been applied to, which is what puts the welcome panel up in the reference UI.
    version: str
    # The build's own version, as in Cargo.toml.

class Cost(TypedDict, total=False):
    """What running one piece of work costs, in units the governor adds up."""

    cpu_millicores: int
    # Thousandths of one CPU core. 1000 is one whole core.
    device_millis: int
    # Share of one hardware device, in thousandths of what it can do, when the work runs on one.
    device_sessions: int
    # Hardware encoder sessions held (consumer NVIDIA cards cap these).
    egress_kbps: int
    # Bytes per second out of the machine, in kbit/s.
    memory_mib: int
    # Resident memory the work adds, in MiB.

class CpuUse(TypedDict, total=False):
    cores: int
    measured_millicores: Optional[int]
    # What a station's processes cost now, read when asked: its own, every show process and every plugin it started. Left out by a single process core and where another process's CPU cannot be read.
    room_millicores: int
    used_millicores: int

class CreateFromRequest(TypedDict, total=False):
    layout: Optional[str]
    # A layout name from `scene.layout.list`. Left out, the number of sources picks one.
    name: Optional[str]
    sources: List[str]
    # Source ids, in the order they should be laid out.

class Crop(TypedDict, total=False):
    """How much of the content's own pixels to trim, normalised 0 to 1 so it survives a canvas change. vMix and CasparCG do this; OBS crops in pixels, which is why an OBS collection moved from 1080p to 720p loses its crops."""

    bottom: float
    left: float
    right: float
    top: float

class Destination(TypedDict, total=False):
    """One destination as a client sees it. The key never appears: `has_key` says whether there is one."""

    enabled: bool
    error: Optional[str]
    # What went wrong last, in words a person can act on.
    has_key: bool
    id: str
    # A slug, unique within its channel: `youtube`, `youtube-2`.
    kbps: int
    # What is going out, over the last second.
    label: str
    plan: Union[DestinationPlan, None]
    # What the plan gave it, while its stream is live.
    platform: str
    # A platform id from the table: youtube, facebook, twitch, custom, srt.
    playback: Union[Playback, None]
    # Where a player opens it, for an output this machine serves as HLS.
    reconnects: int
    # Connections lost and made again since it was switched on.
    refused: Union[DestinationRefusal, None]
    # Why it is not sending what it asked for, and what would fit.
    rendition: Union[RenditionChoice, None]
    # What it asked to be converted to. Absent: sent as it arrives.
    since_ms: int
    # Milliseconds since `state` last changed.
    state: DestinationState
    stream: str
    # Which of the channel's streams to send. `*` is the first live one.
    uri_host: str
    # The scheme, host and port, and nothing that could carry a key.

class DestinationPlan(TypedDict, total=False):
    """The plan's answer for one destination."""

    audio: Union[AudioShape, None]
    encoder: Optional[str]
    # The video encoder, `h264-videotoolbox`, and why that one. Absent for a copy.
    encoder_reason: Optional[str]
    mode: DestinationMode
    nodes: List[str]
    # The plan's nodes this destination reads, so a page can show which work it shares with the channel's other destinations.
    reason: str
    # One sentence: "copied: the source's video goes out as it is", "encoded because the source is 1920x1080 and this output wants 1280x720".
    stream: str
    # The stream it was planned against, when the destination names `*`.
    video: Union[VideoShape, None]
    # What goes out.

class DestinationRefusal(TypedDict, total=False):
    """Why a destination that asked for a rendition is not sending, and what would. `error` on the destination carries the same sentence."""

    advice: List[RenditionAdvice]
    # Renditions that would fit now, largest first.
    code: str
    # `governor` (the machine has no room), `plan` (nothing here can make it), `shed` (it ran and was stopped to keep what is on air).
    have: Union[Cost, None]
    message: str
    need: Union[Cost, None]

class DeviceTotal(TypedDict, total=False):
    """Use of one hardware device by a plan."""

    millis: int
    sessions: int

class DeviceUse(TypedDict, total=False):
    id: str
    kind: str
    # `videotoolbox`, `nvidia`, `va`.
    room_millis: int
    sessions_max: Optional[int]
    # Absent when the device showed no limit.
    sessions_used: int
    used_millis: int

class DiscoverAnswer(TypedDict, total=False):
    found: List[Found]

class DiscoverRequest(TypedDict, total=False):
    """`device.discover`."""

    timeout_ms: Optional[int]
    # How long to look, shared between the devices. Two seconds by default, four and a half at most, because no method blocks for five.

class DiscoverRequest2(TypedDict, total=False):
    timeout_ms: Optional[int]
    # How long to listen. Capped at 4.5 seconds, so the call stays inside the five second ceiling every method is held to.

class DraftRecord(TypedDict, total=False):
    """`scene.edit.begin`."""

    draft: str
    # Pass this as `draft` on any `scene.item.*` call to edit the copy.
    live: bool
    # True when the client asked to edit on air.
    scene: str
    # The scene it was taken from.
    view: Union[SceneView, None]
    # The scene as it stands, so the client has something to draw at once.

class DraftRequest(TypedDict, total=False):
    """`scene.edit.apply` and `discard`."""

    draft: str

class DuplicateSceneRequest(TypedDict, total=False):
    name: Optional[str]
    # What to call the copy. A name already in use gets a number after it.
    scene: str

class DuplicateSourceRequest(TypedDict, total=False):
    """`source.duplicate`."""

    id: str
    # The source to copy.
    name: Optional[str]
    # Name for the copy. The original's name and " copy" when omitted.
    new_id: Optional[str]
    # Id for the copy. Derived from its name when omitted, with a numeric suffix if that is taken.

class EditBeginRequest(TypedDict, total=False):
    """`scene.edit.begin`."""

    live: bool
    # True to edit the scene that is on air as you go. The default is off air: the draft is applied on the next take or on an explicit apply.
    scene: str

class EnrolRequest(TypedDict, total=False):
    address: Optional[str]
    # Where the node is, for the record. The node always dials the core, so this is what `node.list` shows before it has.
    name: str
    # What the node will call itself. A slug: it goes in `place` and in the node's certificate.
    ttl_secs: Optional[int]
    # How long the token is good for. Default one hour.

class ErrorAction(TypedDict, total=False):
    """One thing a client can offer as a button. `label` is the button's text, `kind` says what pressing it does, and the other fields are the ones that kind uses. Flat rather than an enum with data, so every generated client reads every field."""

    after_ms: Optional[int]
    # `retry`: how long to wait first.
    applies: Optional[str]
    # `set-config`: what `config.get` says about the key: `live`, `next_source` or `restart`.
    dialog: Optional[str]
    # `open`: a dialog, such as `settings`.
    key: Optional[str]
    # `set-config`: the dotted key. `open`: the setting to show.
    kind: ActionKind
    label: str
    # Short, in the imperative, for a person: "Turn the multiview on".
    name: Optional[str]
    # `install-plugin` and `enable-plugin`: the plugin.
    panel: Optional[str]
    # `open`: a panel by id.
    value: Any
    # `set-config`: the value to send.

class ExportRequest(TypedDict, total=False):
    include_media: bool
    # Put the clips themselves in, as base64, rather than their names and sizes. Refused past 256 MB: copy the media folder instead.
    include_secrets: bool
    # Put stream keys, channel keys, destination addresses and the control token in the file. Off unless asked; admin scope either way.
    name: Optional[str]
    # What to call the project. Defaults to "GodwinMix project".
    page: Any
    # Whatever the page wants back when the file is opened: its layout and its settings. Carried as it is.

class ExportRequest2(TypedDict, total=False):
    """`scene.export`."""

    collection: Optional[str]
    format: Optional[str]
    # `json` for the document alone, `zip` for a bundle with its assets, or `dir` for the same bundle unpacked.
    path: Optional[str]
    # Where to write it, on the machine the mixer is running on. Required for `dir`. For `zip`, leaving it out hands the bytes back as base64.

class Ext(TypedDict, total=False):
    """The `ext` table from 03 section 6. Every key is off by default. A terminal UI takes meters and tally and declines multiview; a Stream Deck takes tally only; an agent takes nothing."""

    agent: Union[AgentExt, None]
    # `event/agent.state` when a threshold crosses or a state flips, with a snapshot URL. `true` takes the defaults from 09 section 5 item 12.
    meters: bool
    # `event/meters` at 10 per second.
    multiview: Union[MultiviewExt, None]
    # The mosaic: binary frames and `event/multiview.layout`. `false` or omitted builds nothing.
    positions: bool
    # `event/source.position` for seekable sources.
    preview: Union[PreviewExt, None]
    # The preview scene, composited in the multiview pipeline at mosaic size, or `"full"` for a full resolution preview compositor built while subscribed. See 11 section 3.
    tally: bool
    # `event/tally`.
    telemetry: Union[TelemetryExt, None]
    # `event/telemetry`: a line of numbers per tick, at 1 to 10 per second. This is what turns the probes on; nothing measures until it is here.

class Filter(TypedDict, total=False):
    """One filter in an item's chain."""

    enabled: bool
    name: Optional[str]
    params: Any
    type: str
    # The plugin qualified provide id, for example `chroma/filter`.

class FilterIdRequest(TypedDict, total=False):
    """`filter.remove`, and anything else that names one filter."""

    id: str

class FilterListing(TypedDict, total=False):
    filters: List[FilterRecord]

class FilterRecord(TypedDict, total=False):
    """One filter as the core reports it."""

    id: str
    side: str
    source: Optional[str]
    # Absent on a programme filter.
    type: str

class FilterRemoved(TypedDict, total=False):
    removed: str

class FilterReport(TypedDict, total=False):
    """A source filter that had to be copied onto each placement."""

    filter: str
    obs_type: str
    placements: List[str]
    # The items it was copied onto, by their path in the document.
    source: str

class Finding(TypedDict, total=False):
    """One thing the validator found."""

    code: str
    # A stable machine readable code, so a client can filter or translate.
    detail: Any
    # The numbers behind the message, for a client that draws them.
    items: List[Id]
    # The items involved, in the order the message names them.
    message: str
    # One sentence naming the state and the next step.
    scene: Union[Id, None]
    # The scene it is in, when it is about one.
    severity: Severity

class Flush(TypedDict, total=False):
    """`event/flush`: the end of a batch. A client renders here and not before."""

    seq: int
    # The sequence number of the last event in the batch.

class Found(TypedDict, total=False):
    """One thing found on the network."""

    address: str
    # `host:port`, ready to hand to `godwinmix node --core`.
    api: int
    # The bridge version it speaks.
    name: str
    # The instance name, which is the node's name.
    role: str
    # `node` or `core`.

class Fps(TypedDict, total=False):
    """A frame rate as a fraction, so 29.97 is exact."""

    den: int
    num: int

class Frame(TypedDict, total=False):
    """The rectangle an item is fitted into."""

    h: float
    w: float

class Geometry(TypedDict, total=False):
    """One item's derived box."""

    height: float
    item: Id
    opacity: float
    # The item's own opacity multiplied by every group's above it.
    path: str
    # The names from the top item down, so a message can say `corner / pulpit` rather than an id.
    source: Optional[str]
    # Present for an item whose content is a source.
    source_height: float
    source_width: float
    # The canvas, which is what the document knows about a source's own size until the mixer says otherwise. Named so a client that does know can tell the two apart.
    width: float
    x: float
    y: float

class GoLiveRequest(TypedDict, total=False):
    """`program.golive`: add the page, add the destination, take the page."""

    id: Optional[str]
    # Source id. Derived from the host when omitted.
    rtmp: Optional[str]
    # Where to send the programme. Added as an output unless one already sends there. Omit to leave the outputs alone.
    superimpose: Optional[str]
    # "auto" (the default) or "off". See `source.add`.
    url: str
    # The page to put on air. Plain http(s); `web+` is added here.

class GoLiveResult(TypedDict, total=False):
    """What `program.golive` answers with."""

    output: Optional[str]
    # The output that was created or reused, if one was asked for.
    source: str
    # The source that was created or reused.
    state: SourceState
    # Where the source is now. It goes to programme as soon as it is live.

class GovernorStatus(TypedDict, total=False):
    """`governor.status`."""

    calibrated_at: Optional[int]
    # Unix seconds of the calibration in use; absent before the first.
    calibrating: bool
    # True while a calibration is running.
    cpu: CpuUse
    devices: List[DeviceUse]
    egress_kbps: int
    fingerprint: Optional[str]
    # The key the calibration is stored under.
    ingress_kbps: int
    # What arrives: every channel stream and every direct show's input, as last counted. Zero on a core with no station.
    shed: List[ShedNote]

class GraphicListing(TypedDict, total=False):
    """`scene.graphic.list`."""

    graphics: List[GraphicType]

class GraphicType(TypedDict, total=False):
    """One graphic this core can place, as the catalogue has it."""

    designer: Any
    # The `[provides.designer]` block, when the plugin wrote one: the icon for the add gallery, the UI schema, the default frame and the gizmos.
    manifest: str
    # The OGraf manifest's path inside the plugin, so a client can fetch it.
    ograf: Ograf
    plugin: str
    provide: str
    type_id: str
    # The plugin qualified id an item's `content.graphic` names, `ograf/lower-third`.

class GroupSourcesRequest(TypedDict, total=False):
    """`source.group`."""

    name: Optional[str]
    # The tray folder to put them in. Null takes them out of the one they are in.
    sources: List[str]

class Header(TypedDict, total=False):
    """Everything in the document that is not a scene or an item: the name, the canvas, the collection's parameters, the transitions it carries, the assets and the source labels. It is not a record and it has no id, so it cannot be diffed the way the tree is. It is carried whole, because it is small and because the alternative is that a command touching only the header produces an empty patch and is thrown away by `edit`, which is exactly what used to happen to `scene.params.set`, `source.set` and `source.group`: all three answered with the change and none of them kept it."""

    assets: Dict[str, Any]
    canvas: Canvas
    name: str
    params: Any
    sources: Dict[str, Any]
    transitions: List[Transition2]

class HeaderChange(TypedDict, total=False):
    """The header as it was and as it is."""

    after: Header
    before: Header

class Health(TypedDict, total=False):
    """A show's health."""

    alarms: List[Alarm]
    # Every alarm that holds now, oldest first.
    state: HealthState

class HealthEvent(TypedDict, total=False):
    """`event/health`, from a show that composites, about itself. The station sends it on to clients as `event/show.health` with the show's id."""

    health: Health

class HistoryRequest(TypedDict, total=False):
    """`program.history`."""

    limit: Optional[int]
    # How many takes to return, newest first. At most 100.

class HistoryStep(TypedDict, total=False):
    """`scene.undo` and `scene.redo`."""

    patch: Patch
    redo: int
    undo: int
    # How many steps are still on each stack, so a UI greys out a button.

class HlsOutputParams(TypedDict, total=False):
    """An `hls://` output's params, as an `hls/output` takes them."""

    low_latency: Optional[bool]
    # true: parts of 333 ms.
    part_ms: Optional[int]
    # LL-HLS part, 0 for none.
    segment_ms: Optional[int]
    # 500 to 10000, default 2000.
    viewer_key: Optional[str]
    # 16 characters or more.
    window: Optional[int]
    # Seconds kept, default 30.

class IdRequest(TypedDict, total=False):
    """An id on its own: `source.get`, `source.remove`, `output.remove`, `output.reconnect`, `media.remove`."""

    id: str

class ImportObsRequest(TypedDict, total=False):
    add_sources: bool
    # Add the sources the scenes draw, each through `source.add`. Left out, only the scenes are added and the answer carries a `[[sources]]` block in `config_toml` instead.
    content: Optional[str]
    # The collection JSON itself, as text: what a page reads from the file the person picked. Give this or `path`.
    path: Optional[str]
    # The collection JSON exported from OBS (Scene Collection, Export), as a path on the machine the core is running on. Give this or `content`.

class ImportReport(TypedDict, total=False):
    config_toml: Optional[str]
    # The `[[sources]]` block for a config file. Only for an import that did not add the sources itself, which is what the command line wants.
    filters_duplicated: List[FilterReport]
    # OBS attaches a filter to a source, so a camera keyed in one scene is keyed in all of them. Here filters belong to the item, so a source filter is copied onto each placement and each copy is named here. This is the one thing an import changes the meaning of, so it is reported rather than left for somebody to find on air.
    items: int
    scenes: List[str]
    # The scenes that were added, by the names they ended up with.
    skipped: List[str]
    # What could not be brought across, and why, one line each.
    source_report: List[SourceReport]
    # Every OBS source and what became of it: carried across, needing a plugin that is not installed, or skipped with the reason.
    sources: List[str]
    # The sources the collection needs, by id. Without `add_sources` they have to be added separately.
    sources_added: Optional[List[str]]
    # With `add_sources`: the sources added to the mixer, by id.
    sources_not_added: Optional[List[SourceNotAdded]]
    # With `add_sources`: the sources that were not added, each with why.

class ImportRequest(TypedDict, total=False):
    dry_run: Optional[bool]
    # Answer with what would change and change nothing. True unless false is sent.
    file: Any
    # The project: the object `project.export` answered with, or its text.
    machine: bool
    # Also write the file's machine settings: addresses, folders, hardware.
    mode: Mode

class ImportRequest2(TypedDict, total=False):
    """`scene.import`."""

    path: str
    # The bundle: a `.zip` or the directory it unpacks to, as a path on the machine the core is running on.

class ImportedReport(TypedDict, total=False):
    """What `scene.import` answers with."""

    assets_at: Optional[str]
    # Where the assets were written.
    bundle: Bundle
    # What the bundle said about itself.
    items: int
    missing_plugins: List[str]
    # Plugins the collection needs that this core has not got. The scenes still came across; those items will draw nothing until it does.
    relink: List[Relink]
    # Assets that did not come across, with the items that draw them. Empty when everything landed.
    scenes: List[str]
    # The scenes that were added, by the names they ended up with.

class InputSpec(TypedDict, total=False):
    """What a show without compositing takes in. A show that composites makes its input its one source."""

    backup: Union[BackupInput, None]
    # Switched to when the input stalls, and back when it returns.
    params: Optional[Dict[str, Any]]
    # Per transport: `interface` for multicast, `latency` for SRT, `passphrase`. Passed to the host as given.
    program: Optional[int]
    # The MPEG-TS program of a feed that carries several. Left out: the first.
    uri: str
    # `udp://@239.1.1.1:5000`, `srt://...`, `rtmp://host/app/key`, `rtsp://...`, `https://.../x.m3u8`, `file:///clip.ts`, `rist://...`, or a channel's stream, `channel:<app>/<stream>`.

class InputStats(TypedDict, total=False):
    """What the input is doing, as the host last counted it."""

    audio_channels: int
    audio_codec: Optional[str]
    cc_errors: int
    fps: float
    height: int
    kbps: int
    keyframe_ms: Optional[int]
    # Between the last two keyframes.
    last_frame_ms: Optional[int]
    # Since the last frame arrived.
    packets_lost: int
    video_codec: Optional[str]
    width: int

class InstanceRecord(TypedDict, total=False):
    """One running instance and its cost."""

    buffers_dropped: int
    cpu_percent: Optional[float]
    instance: str
    media_latency_ms: Optional[int]
    pid: Optional[int]
    plugin: str
    # The plugin it belongs to. Carried on the instance as well as on the plugin, because `plugin.stats` is a flat list and a caller holding one row should not have to go back for the name.
    provide: str
    restarts: int
    rss_bytes: Optional[int]
    state: str

class ItemFilterRequest(TypedDict, total=False):
    """`scene.item.filter.set` and `remove`."""

    draft: Optional[str]
    enabled: Optional[bool]
    # Turn a filter off without taking it out.
    filter: str
    # The filter's name, or its position in the item's chain from 0.
    item: str
    params: Dict[str, Any]
    scene: str

class ItemProps(TypedDict, total=False):
    """An item's props, which is an `Item` with the children lifted out into their own records."""

    audio: Audio
    bind: Dict[str, Any]
    blend: Blend
    content: FlatContent
    crop: Crop
    filters: List[Filter]
    locked: bool
    name: Optional[str]
    opacity: float
    transform: Transform
    visible: bool

class ItemRequest(TypedDict, total=False):
    """Anything that names one item."""

    draft: Optional[str]
    item: str
    # The item's name or its id.
    scene: str

class ItemSchemaRequest(TypedDict, total=False):
    """`scene.item.schema`."""

    type: str
    # The item type: a graphic id like `ograf/lower-third`, or a plugin provide like `camera/source`.

class ItemsRequest(TypedDict, total=False):
    """`scene.item.align`, `distribute`, `fit_to_canvas`, `cover_canvas`, `arrange_grid`, `match_size`, `group`."""

    axis: Optional[str]
    # `distribute`: horizontal or vertical.
    cols: Optional[int]
    # `arrange_grid`: how many columns.
    draft: Optional[str]
    duration_ms: Optional[int]
    easing: Optional[str]
    edge: Optional[str]
    # `align`: left, right, top, bottom, center-x, center-y.
    items: List[str]
    # Item names or ids.
    name: Optional[str]
    # `group`: what to call the group.
    scene: str
    seq: Optional[int]
    # A client's own sequence number, echoed on the patch. See `SetItemRequest::seq`.
    to: Optional[str]
    # `match_size`: the item to match.

class KeyAdded(TypedDict, total=False):
    """What `channel.key.add` answers."""

    key: NewKey

class KeyRevealed(TypedDict, total=False):
    """What `channel.key.reveal` answers: the key itself, and nothing a list would carry."""

    secret: str

class LadderRef(TypedDict, total=False):
    """A custom ladder."""

    ladder: List[RenditionRequest]

class Layout(TypedDict, total=False):
    """A scene's geometry, for copying onto another one."""

    canvas: Canvas
    items: List[LayoutItem]
    scene: str
    # The scene it came from, for a message.

class LayoutClipboardRequest(TypedDict, total=False):
    """`scene.layout.copy` and `paste`."""

    layout: Any
    # What `scene.layout.copy` answered with.
    match: Optional[str]
    # `name` matches item names first and falls back to slot order; `order` uses slot order alone.
    scene: str

class LayoutInfo(TypedDict, total=False):
    description: str
    # What the layout calls its own scene, which is the nearest thing it has to a description.
    name: str
    params: Any
    # The whole JSON Schema, so a client renders an inspector from it.
    sources: List[str]
    # The parameters that take a source id, in the order sources are poured into them.

class LayoutItem(TypedDict, total=False):
    """One item's geometry: everything about where it sits and nothing about what it shows."""

    crop: Crop
    name: Optional[str]
    opacity: float
    transform: Transform
    visible: bool

class LayoutListing(TypedDict, total=False):
    layouts: List[LayoutInfo]

class Limits(TypedDict, total=False):
    """The ceilings a client should plan against rather than discover by being refused."""

    event_queue: int
    # How many events the bus holds before a slow client is told to resync.
    max_call_secs: int
    # Longest any method blocks before it answers. The tightest client default in the wild, so nothing here can time out a client that used its own.
    max_gain: float
    # Loudest a fader can be asked for. Out of range is clamped to this rather than refused.
    max_idempotency_key_bytes: int
    # Longest `idempotency_key` accepted, in bytes.
    max_upload_bytes: int
    # Largest upload the media endpoint accepts, in bytes.

class Listener(TypedDict, total=False):
    """One listener a channel needs, and whether it is open now."""

    because: List[str]
    # The channels it is open for. Empty when nothing needs it.
    last_port: Optional[int]
    # The last port of a range, for WebRTC media.
    loopback: bool
    # Bound to 127.0.0.1 only, so nothing off this machine reaches it.
    open: bool
    port: int
    problem: Optional[str]
    # Why it is not open although a channel wants it, and what to do.
    protocol: str
    # `rtmp`, `rtmps`, `srt`, `whip`, `webrtc` (the media ports WHIP sessions use) or `relay` (the RTMP port on the loopback alone, for the mixer's own sources, while no channel has RTMP on).
    transport: str
    # `tcp` or `udp`.

class LogGstRequest(TypedDict, total=False):
    """`log.gst`."""

    categories: str
    # `GST_DEBUG` spelling: `rtmp2src:6,rtpjitterbuffer:5`.
    duration_secs: int
    # How long before it goes back down. A minute by default, which is long enough to reproduce a fault and short enough that a forgotten firehose stops on its own.
    instance: Optional[str]

class LogGstResult(TypedDict, total=False):
    """What `log.gst` answers with."""

    categories: List[str]
    # The categories actually raised, which is what the caller asked for with anything GStreamer does not know dropped.
    duration_secs: int

class LogSetRequest(TypedDict, total=False):
    """`log.set`. Name an instance or a target, not both."""

    instance: Optional[str]
    # A plugin instance: a source or an output id. Its lines carry the id, so raising this one raises only that camera.
    level: str
    # `off`, `error`, `warn`, `info`, `debug`, `trace`, or `default` to stop overriding this one.
    target: Optional[str]
    # A module path prefix such as `godwinmix::mixer`. The longest match wins, so a more specific override still beats a broader one.

class MarkRequest(TypedDict, total=False):
    """`scene.history.mark`."""

    label: Optional[str]
    # What to call the group of changes that follows. Omit it to end the group, so the next change is its own undo step.

class MediaItem(TypedDict, total=False):
    audio_codec: Optional[str]
    conversion: Union[ConversionState, None]
    # Where a conversion of this file stands, None when none was asked for in this process's lifetime.
    converted_path: Optional[str]
    duration_ms: Optional[int]
    # None when the file could not be inspected; it is still listed, because an operator would rather see a clip they cannot read the length of than wonder why it is missing.
    faststart: Optional[bool]
    # The converted copy's absolute path when one exists on disk. This is what "add as source" should prefer over `path`. Whether the moov atom comes first (a player can start before the whole file arrives). None for a non-ISO container, never a reason to convert on its own. See `convert::moov_first`.
    has_audio: bool
    has_video: bool
    height: Optional[int]
    name: str
    # Name shown in the UI, relative to the library root.
    path: str
    # Absolute path, which is what gets handed back to the ad break API.
    reasons: List[str]
    # Why it is not, in words an operator can act on. Empty when it is.
    size_bytes: int
    video_codec: Optional[str]
    # Short codec name of the first video stream ("h264", "vp9"), None when there is no video or it could not be inspected.
    web_safe: bool
    # True when the file needs no conversion to play in a browser: H.264 plus AAC (or no audio) in an MP4. See `convert::web_safety`.
    width: Optional[int]

class MediaListing(TypedDict, total=False):
    created: bool
    # True when the folder was not there and this listing made it, so a client can say "made the media folder" once instead of nothing.
    dir: str
    error: Optional[str]
    # Set when the directory itself could not be read, so the UI can say why the list is empty instead of just showing nothing.
    items: List[MediaItem]

class Meters(TypedDict, total=False):
    """`event/meters`: the programme bus and every source, in one message at 10 per second, rather than one message per meter as the legacy stream sends."""

    program: List[float]
    # Peak dBFS per channel on the programme bus.
    sources: Dict[str, Any]
    # Peak dBFS per channel, per source id.

class MissingRequest(TypedDict, total=False):
    """`source.missing`: the ids a scene draws, or none for every source the mixer knows is not running."""

    ids: List[str]

class MissingSource(TypedDict, total=False):
    """One source that is not running, and what would bring it back."""

    action: Union[ErrorAction, None]
    # The button that fixes it, when the error carries one.
    error: Optional[str]
    # The error it failed with, which names the next step.
    id: str
    name: Optional[str]
    restore: bool
    # True when `source.restore` can ask for it again.
    type: Optional[str]
    # The kind, such as `camera/source`, when the mixer knows it.
    why: MissingWhy

class MixerStatus(TypedDict, total=False):
    ad: Union[AdStatus, None]
    # Present while an ad break is armed or running.
    backend: BackendInfo
    multiview: MultiviewStatus
    outputs: List[OutputStatus]
    program: Optional[str]
    # Source currently on program, or None while the slate is showing. A scene of one full canvas item reports that item's source here too, so anything written against this before scenes existed still reads.
    running_time_ms: int
    # Program pipeline running time. Cues are scheduled against this, not against wall clock, so a client can place a break on a known frame.
    scene: Optional[str]
    # The scene on air, when one was taken by name.
    sources: List[SourceStatus]
    uptime_secs: int

class MoveItemRequest(TypedDict, total=False):
    """`scene.item.move` and `scene.item.copy`."""

    item: str
    scene: str
    to_scene: str
    # The scene it is going to.

class MultiviewLayout(TypedDict, total=False):
    """`event/multiview.layout`: how to read the binary frames that follow."""

    cells: List[CellAssignment]
    height: int
    id: int
    # Stable for as long as the cells are unchanged, and carried in the header of every frame, so a client that falls behind can tell which layout a late frame belongs to.
    width: int

class MultiviewStatus(TypedDict, total=False):
    cells: List[CellAssignment]
    # Cell index to source id, in reading order. Cell 0 is the program return when it is enabled.
    cols: int
    enabled: bool
    fps: int
    # Frame rate of the mosaic, so the UI can size its own expectations.
    height: int
    rows: int
    width: int

class NameRequest(TypedDict, total=False):
    """`media.convert` and `media.remove` name a file rather than an id."""

    name: str
    # File name as it appears in the media listing. The REST layer puts it in the path, where the transform rule calls it `id`, so both spellings are read.

class NewKey(TypedDict, total=False):
    """A key as it is made, with its secret. Afterwards only an admin gets the secret again, one key at a time, from `channel.key.reveal`."""

    id: str
    label: str
    secret: str
    # The key. A list never carries it; `channel.key.reveal` reads it back.

class NodeInstance(TypedDict, total=False):
    detail: Optional[str]
    instance: str
    latency_ms: int
    state: str

class NodeListing(TypedDict, total=False):
    listening: bool
    # Whether this core is listening for nodes at all.
    nodes: List[NodeView]

class NodeName(TypedDict, total=False):
    id: str
    # The node's name, as it was enrolled.

class NodePlugin(TypedDict, total=False):
    name: str
    provides: List[str]
    version: str

class NodeView(TypedDict, total=False):
    """What `node.get` reports about one node, and what `node.list` reports about all of them."""

    address: Optional[str]
    clock_jitter_ms: float
    clock_offset_ms: float
    clock_synced: bool
    heartbeat_age_ms: int
    # Milliseconds since the last heartbeat. The same number `gmx_node_heartbeat_age_ms` carries.
    identity: Optional[str]
    instances: List[NodeInstance]
    # The instances it is hosting right now.
    name: str
    platform: Optional[str]
    plugins: List[NodePlugin]
    # The plugins this node has, name and version.
    provides: List[str]
    # The provide ids this node can run, `<plugin>/<provide>`.
    state: str
    # `online`, `offline`, or `expected` for a node listed in the config that has never dialled in.
    version: Optional[str]

class Ograf(TypedDict, total=False):
    """The OGraf manifest, in the subset this host reads. Everything else the file carries is kept in `rest` and passed on: OGraf is an EBU specification that will grow, and a key this build has not heard of is a key a newer client may want. Dropping it here would make the core the thing that has to be upgraded first."""

    description: Optional[str]
    id: str
    # The graphic's own id, as the OGraf file gives it.
    main: str
    # The module the web component is in, relative to the manifest.
    name: str
    schema: Any
    # The JSON Schema of the graphic's own data. What the inspector renders and what `scene.apply_graphic` fills by name.
    stepCount: int
    # How many steps `playAction` walks through. One means in and out.
    supportsNonRealTime: bool
    supportsRealTime: bool
    version: Optional[str]

class OutputStats(TypedDict, total=False):
    """What one output is doing."""

    cpu_millicores: int
    encoder: Optional[str]
    id: str
    kbps: int
    reconnects: int
    rendition_text: str
    # `copy`, or what the plan gave it, such as `h264 1280x720`.
    state: str
    # waiting, connecting, live, reconnecting, failed, or off.

class OutputStatus(TypedDict, total=False):
    has_key: bool
    # False while the address still carries a placeholder a preset wrote in for somebody to replace, such as `YOUR-STREAM-KEY`. The key itself never leaves the core, so this is how a client knows to put its own form up and say "needs a stream key" without ever seeing the key. True for an address with no key in it at all, an SRT one for instance, because there is nothing there for anybody to replace.
    id: str
    queue_secs: float
    # Seconds of encoded data waiting in the pre-muxer queue. A number that climbs and stays high means the destination cannot keep up.
    reconnects: int
    rendition: Union[RenditionChoice, None]
    # Per kind data from whatever plugin owns this output. Empty for the RTMP outputs the core builds itself. What this output asked to be made, when it asked: a rendition request or a preset. Absent means the programme encoder.
    shed: Optional[str]
    # Why the governor has this output's rendition stopped just now, while it has. The output stays connected and resumes by itself.
    state: OutputState
    uri_host: str

class Override(TypedDict, total=False):
    """A sparse change to one item of a referenced scene."""

    crop: Union[Crop, None]
    opacity: Optional[float]
    params: Any
    transform: Union[Transform, None]
    visible: Optional[bool]

class ParamsRequest(TypedDict, total=False):
    """`scene.params.set`."""

    scene: Optional[str]
    values: Dict[str, Any]

class Patch(TypedDict, total=False):
    """What changed in one transaction."""

    added: List[Record]
    client_seq: Optional[int]
    # The client's own sequence number, echoed back. A drag cannot wait for a round trip, so the client kit draws the move itself and reconciles when the echo arrives. Without this it cannot tell an echo of the move it has already drawn past from a correction, and the handle rubber bands backwards under the cursor. Every geometry command carries a `seq`; this is that number coming back.
    header: Union[HeaderChange, None]
    # The collection's own properties, when they changed. Absent for the ordinary case, which is every command that moves an item.
    label: Optional[str]
    # What the client called this change, for a label in an undo menu.
    removed: List[Id]
    scope: str
    # `document` today. `presence` (who is looking at what) is the other scope 11 section 4 names and is not implemented.
    seq: int
    # Monotonic, per core. A client that sees a gap asks for a fresh snapshot rather than guessing.
    source_client: Optional[str]
    # Whoever asked for the change, so a client can suppress the echo of its own edits and not fight its own optimistic drawing.
    updated: List[Update]

class PathCreateRequest(TypedDict, total=False):
    name: str
    # The new folder's name: one folder, no separators, not hidden.
    parent: str
    # The folder to make it in, as `path.list` names it.

class PathEntry(TypedDict, total=False):
    """One folder inside the one listed."""

    name: str
    path: str
    # Absolute, ready to pass back as `path`.
    writable: bool
    # Whether the mixer can write into it.

class PathListRequest(TypedDict, total=False):
    path: Optional[str]
    # The folder to list. Absent, empty or `~` is the mixer's home folder; a relative path is taken from there.

class PathListing(TypedDict, total=False):
    """What `path.list` and `path.create` answer with."""

    dirs: List[PathEntry]
    # The folders in it, sorted by name. Files are never listed.
    parent: Optional[str]
    # One level up, or null at the top of a root.
    path: str
    # The folder listed, absolute and with links resolved.
    roots: List[PathRoot]
    truncated: bool
    # True when there were more than 500 and the rest were left out.
    writable: bool
    # Whether the mixer can write into this folder.

class PathRoot(TypedDict, total=False):
    """A place the picker may start from."""

    label: str
    # `Home`, `Media folder` or `Config folder`.
    path: str

class PipelineDot(TypedDict, total=False):
    """What `pipeline.dot` answers with on `/rpc`. The REST route serves the same graph as `text/vnd.graphviz`, so `gmx dot | dot -Tsvg` needs no unwrapping."""

    dot: str
    # The graph itself, in the dot language.
    pipeline: str

class PipelineRequest(TypedDict, total=False):
    """Which pipeline to look at. A source id, an output id, `programme` or `multiview`. `pipeline.list` says what is running."""

    name: str

class PlanNode(TypedDict, total=False):
    """One node of a plan, as the page draws it."""

    cost: Cost
    encoder: Optional[str]
    # The catalogue id of the encoder, on an encode node.
    id: str
    # Stable across plans: `encode:programme:h264:1280x720p30:2800k:g2000`.
    kind: str
    # `source`, `copy`, `decode`, `scale`, `encode`, `audio-convert`, `audio-encode`, `mux`.
    reason: Union[PlanReason, None]
    serves: List[str]
    # The outputs it works for, each once (for a channel's plan, the destination ids), however many rungs of one ladder it serves.
    shed: Optional[str]
    # Set while the governor has this node stopped to keep what is on air.

class PlanReason(TypedDict, total=False):
    """Why the planner decided what it did."""

    code: str
    # `hardware`, `software-only`, `device-full`, `shape-unsupported`, `copied`, `transcoded`.
    text: str

class PlanRequest(TypedDict, total=False):
    """`rendition.plan`."""

    scope: Optional[str]
    # `programme` (the default) or `channel:<id>`.

class PlanTotals(TypedDict, total=False):
    cpu_millicores: int
    devices: Dict[str, Any]
    egress_kbps: int

class PlanView(TypedDict, total=False):
    """`rendition.plan`, and the `plan` of `event/rendition.plan`."""

    nodes: List[PlanNode]
    totals: PlanTotals

class Playback(TypedDict, total=False):
    """The links of an output served as HLS from the control port, each with the output's viewer key on it."""

    dash_url_path: str
    # The same segments as a DASH MPD.
    master_url_path: str
    # `/hls/viewers/master.m3u8?show=bbc-one&key=...`.
    viewers: int
    # Players that fetched something in the last two windows.

class PluginDescription(TypedDict, total=False):
    """The whole of one plugin, for an agent about to use it."""

    description: str
    enabled: bool
    hooks: List[str]
    instances: List[InstanceRecord]
    # Every running instance of it, with what it costs.
    manifest: Any
    # The manifest as JSON, every table of it.
    name: str
    problem: Optional[str]
    # Why it is not loaded, when it is not.
    provides: List[str]
    # The type ids it contributes: what goes in `type` on a source, an output or a filter.
    root: str
    # Where it is installed.
    schemas: Dict[str, Any]
    # Per provide id, its settings schema.
    skills: Dict[str, Any]
    # Per provide id, the description line from its SKILL.md.
    source: str
    # Where it was installed from, as it was typed.
    tools: List[str]
    # Its MCP tools, as `gmx_<plugin>_<tool>`. Reachable with `search_tools`; never in the hot list.
    trust: str
    # What was checked about where this came from: "signed", "signed, digest only", or "custom, unreviewed". 06 section 4: an operator can only judge a plugin if the catalogue says what was checked.
    trust_detail: str
    # The sentence behind the label.
    version: str

class PluginListing(TypedDict, total=False):
    plugins: List[PluginRecord]
    plugins_dir: str
    # Where plugins are read from on this machine.

class PluginName(TypedDict, total=False):
    """Anything that names one plugin. The field is `id` because that is what the REST layer fills in from `/api/v1/plugins/{id}`, and a plugin's id is its name: the namespace of every id it contributes. `name` is accepted as well, for a JSON-RPC caller who wrote the obvious thing."""

    id: str

class PluginRecord(TypedDict, total=False):
    """One plugin as the core reports it."""

    description: str
    enabled: bool
    hooks: List[str]
    instances: List[InstanceRecord]
    # Every running instance of it, with what it costs.
    name: str
    problem: Optional[str]
    # Why it is not loaded, when it is not.
    provides: List[str]
    # The type ids it contributes: what goes in `type` on a source, an output or a filter.
    root: str
    # Where it is installed.
    source: str
    # Where it was installed from, as it was typed.
    tools: List[str]
    # Its MCP tools, as `gmx_<plugin>_<tool>`. Reachable with `search_tools`; never in the hot list.
    trust: str
    # What was checked about where this came from: "signed", "signed, digest only", or "custom, unreviewed". 06 section 4: an operator can only judge a plugin if the catalogue says what was checked.
    trust_detail: str
    # The sentence behind the label.
    version: str

class PluginRemoved(TypedDict, total=False):
    provides: List[str]
    # What went with it, so a caller can see the blast radius.
    removed: str
    tools: List[str]

class PluginSettings(TypedDict, total=False):
    name: str
    schemas: Dict[str, Any]
    # The JSON Schema every surface renders, one per provide.
    settings: Dict[str, Any]

PluginUpdated = TypedDict("PluginUpdated", {
    "from": str,
    "handshake_ms": int,
    "plugin": PluginRecord,
    "to": str,
}, total=False)

class PresetRef(TypedDict, total=False):
    """A preset named by id."""

    preset: str

class PresetsResult(TypedDict, total=False):
    """`rendition.presets`."""

    presets: List[RenditionPreset]

class PreviewClosed(TypedDict, total=False):
    """What `preview.close` answers with."""

    closed: bool
    target: str

class PreviewFrameRequest(TypedDict, total=False):
    """`scene.preview.frame`."""

    width: Optional[int]

class PreviewOpenRequest(TypedDict, total=False):
    """`preview.open {target}`."""

    target: str
    # `program`, or a source id.

class PreviewRequest(TypedDict, total=False):
    """`scene.preview.set`."""

    draft: Optional[str]
    # A draft from `scene.edit.begin` for the preview to draw in place of the armed scene, which is how a designer sees what it is laying out. What is armed is left as it is, and `scene` is ignored. An empty string goes back to the armed scene; so does applying or discarding the draft.
    scene: Optional[str]
    # The scene to arm. Null or omitted disarms.

class PreviewSocket(TypedDict, total=False):
    """What `preview.open` answers with."""

    path: str
    # The Unix socket to connect to, absolute. Read it with `unixfdsrc` in GStreamer, or with the media contract's own reader.
    target: str
    transport: str
    # What is on the far end, so a client knows what to expect before it connects.

class ProgramState(TypedDict, total=False):
    """What `program.get` answers with, and what `program.take` returns so that no follow up read is needed."""

    ad: Union[AdStatus, None]
    # Present while an ad break is armed or on air.
    missing: List[str]
    # Sources the scene on air draws that this mixer does not have. The take went ahead without them and they draw nothing, so the slate or whatever sits under them shows through, until they are added back. Left out when every source is here.
    preview: Optional[str]
    # The scene armed for the next `program.take` with no argument.
    previous: Optional[str]
    # The previous source, which is what `program.revert` would take back to.
    program: Optional[str]
    # Source on air, or null for the slate. A scene of one full canvas item reports that item's source here too, so anything written against this before scenes existed still reads.
    running_time_ms: int
    # Programme pipeline running time, in milliseconds.
    scene: Optional[str]
    # The scene on air, when one was taken by name.

class PublishAddress(TypedDict, total=False):
    """Where an encoder is pointed for one protocol."""

    example: str
    # The whole address with `<key>` where the key goes.
    protocol: str
    # `rtmp`, `rtmps`, `srt` or `whip`.
    server: str
    # The address without the key: `srt://10.0.0.5:9000`.

class Record(TypedDict, total=False):
    """One scene or one item."""

    id: Id
    order: str
    # A fractional key. Siblings sort by it; see `order.rs`.
    parent: Union[Id, None]
    # The scene this item is in, or the group item it is a child of. Absent for a scene, which hangs off the document itself.

class Relink(TypedDict, total=False):
    """One asset an import could not put back."""

    asset: Id
    items: List[str]
    # The items that draw it, by scene and item name, so the person fixing it knows what will be blank until they do.
    path: str
    # The path the document asks for.
    reason: str
    # Why it could not be used: missing, or a hash that does not match.

class RemoveDestinationRequest(TypedDict, total=False):
    """`channel.destination.remove`."""

    destination: str
    # The destination's id within the channel.
    id: str
    # The channel.

class RenameSceneRequest(TypedDict, total=False):
    color: Optional[str]
    name: Optional[str]
    scene: str

class RenditionAdvice(TypedDict, total=False):
    """One thing a refused rendition could be instead, as a button."""

    request: RenditionRequest
    # Send this as the output's `rendition` to take the advice.
    text: str
    # "720p30 H.264 on h264-videotoolbox fits".

class RenditionPlanEvent(TypedDict, total=False):
    """`event/rendition.plan`."""

    plan: PlanView
    scope: str

class RenditionPreset(TypedDict, total=False):
    """One built in preset."""

    available: bool
    # Whether this machine can make it.
    cost: Union[Cost, None]
    # What the whole preset would cost here (every rung, the scaling and the sound), as the governor prices it on this machine.
    group: str
    # `platform`, `ladder`, `audio` or `copy`, for grouping in a menu.
    id: str
    # `youtube-1080p30`, `abr-ladder-4`.
    ladder: Optional[List[RenditionRequest]]
    # Every rung, top first, for a ladder preset.
    request: RenditionRequest
    # The one rendition, or the top rung of a ladder.
    title: str
    # What the page shows: "YouTube 1080p30".
    why: Optional[str]
    # Why not, when it cannot.

class RenditionRequest(TypedDict, total=False):
    """What one output wants. A field left out means "whatever the source has", so an empty request is a plain copy."""

    audio: Union[AudioWant, None]
    container: Container
    # How the bytes are wrapped on the way out. FLV when left out.
    id: str
    # Slug, unique within the show or channel that asks. Left out, the output's own id is used.
    no_audio: bool
    # Drop the audio altogether.
    no_video: bool
    # Drop the video altogether (an audio only stream).
    video: Union[VideoWant, None]

class ReorderRequest(TypedDict, total=False):
    """`scene.item.reorder`."""

    after: Optional[str]
    # Put it in front of this one. With neither, it goes to the front.
    before: Optional[str]
    # Put it behind this one.
    draft: Optional[str]
    item: str
    scene: str
    seq: Optional[int]
    # A client's own sequence number, echoed on the patch.

class Report(TypedDict, total=False):
    """What `project.import` answers with."""

    changes: List[Change]
    dry_run: bool
    failed: List[str]
    # What was tried and refused, each with the reason.
    name: str
    needs_restart: List[str]
    # Settings written to the file that take effect on the next start.
    page: Any
    # The page part of the file, for the page to put back.
    waiting: List[str]
    # What a person still has to do: a key to give again, a clip to copy.
    written_by: str

class Requirement(TypedDict, total=False):
    """One plugin the collection needs."""

    plugin: str
    # The plugin name, `ograf`.
    provides: List[str]
    # The provide ids used, `ograf/lower-third`, so a reader can see what the collection actually asks the plugin for.
    versions: str
    # A semver range, `^0.2.0`, or `*` when the exporter had no version to name because the plugin was not installed where the export ran.

class RestartAnswer(TypedDict, total=False):
    """`core.restart`: what happened."""

    how: RestartHow
    message: str
    # One sentence for a person: what happens now, or how to restart it.
    restarting: bool
    # True when the core is on its way out and will be started again. False when nothing would start it again, in which case it keeps running.

class RestartInfo(TypedDict, total=False):
    """`core.info.restart`: can this core be restarted from a client."""

    how: RestartHow
    possible: bool
    # True when `core.restart` will bring the core back by itself.

class Resync(TypedDict, total=False):
    """`event/resync`: the client fell behind and the stream has a hole in it."""

    dropped: int
    # How many events were dropped.
    from_seq: int
    # The last sequence number the client is known to have. Everything after it was dropped; re-subscribe for a fresh snapshot.

class RtmpInfo(TypedDict, total=False):
    """The RTMP port every channel shares."""

    listening: bool
    # Whether the listener is running. False until the ingest plugin is installed and enabled.
    port: int
    problem: Optional[str]
    # Why not, and what to do, when it is not.
    urls: List[str]
    # `rtmp://<address>:<port>` for each address this machine has.

class Rtmps(TypedDict, total=False):
    """RTMPS for one channel: off, or on at a port."""

    enabled: bool
    port: int
    # The port it listens on while enabled. 443 is offered first.

class SaveRequest(TypedDict, total=False):
    name: str
    # The new preset's name. A slug: lower case letters, digits and hyphens.
    out: Optional[str]
    # Where to write it. Defaults to `~/.godwinmix/presets/<name>`.

class SceneListing(TypedDict, total=False):
    """`scene.list`."""

    scenes: List[SceneSummary]

class SceneRemoved(TypedDict, total=False):
    removed: str

class SceneRequest(TypedDict, total=False):
    """Anything that names one scene."""

    scene: str
    # The scene's name or its id.

class SceneSummary(TypedDict, total=False):
    """What `scene.list` answers with per scene."""

    armed: bool
    # True for the armed scene, which is the preview.
    color: Optional[str]
    id: Id
    items: int
    # How many items, groups counted with their children.
    name: str
    sources: List[str]
    # Every source the scene draws, so a picker can grey out one whose sources are missing without reading the whole document.

class SceneView(TypedDict, total=False):
    """One scene as a command answers with it."""

    canvas: Canvas
    color: Optional[str]
    findings: List[Finding]
    # What `scene.validate` would say about it, so a client shows a warning without asking again.
    geometry: List[Geometry]
    # Where each item actually lands, after groups are flattened and references resolved. Bottom of the stack first, which is the order the compositor takes them in.
    id: Id
    name: str
    records: List[Record]
    # The scene's own record and one per item, parents before children.

class SearchRequest(TypedDict, total=False):
    """`plugin.search`."""

    term: str
    # A word to look for in a plugin's name, description or kind. Empty lists everything.

class SearchResult(TypedDict, total=False):
    """One plugin a marketplace lists."""

    description: str
    installed: bool
    # Whether it is already on this mixer.
    kinds: List[str]
    marketplace: str
    name: str
    source: str
    # What to pass to `plugin.add`.
    tier: str
    # custom, bronze, silver or gold. 06 section 4.
    version: str
    # The newest listed version this core's api range can run.

class SearchResults(TypedDict, total=False):
    """What `plugin.search` answers with."""

    marketplaces: List[str]
    # The marketplaces that were searched.
    results: List[SearchResult]

class SeekParams(TypedDict, total=False):
    """`source.seek`."""

    id: str
    # Source id.
    position_ms: float
    # Milliseconds from the start of the clip. Off either end is clamped rather than refused, so a scrubber flicked past the end lands there.

class SessionLogRequest(TypedDict, total=False):
    """`core.session_log`."""

    secs: int
    # How far back to read, in seconds. An hour by default, a day at most.

class SetDestinationRequest(TypedDict, total=False):
    """`channel.destination.set`. Change one destination, naming only what moves."""

    destination: str
    # The destination's id within the channel.
    enabled: Optional[bool]
    id: str
    # The channel.
    key: Optional[str]
    # A new stream key. Left out keeps the one it has; an empty string clears it, where the platform allows none.
    label: Optional[str]
    rendition: Union[RenditionChoice, None]
    # A new rendition. Left out keeps the one it has; `null` or `{"preset": "copy"}` goes back to sending the stream as it arrives.
    server: Optional[str]
    stream: Optional[str]

class SetFilterRequest(TypedDict, total=False):
    """`filter.set`."""

    id: str
    params: Dict[str, Any]
    # The settings to apply. Only the keys named are changed.

class SetItemRequest(TypedDict, total=False):
    """`scene.item.set`: a state assignment. Only the keys named move."""

    draft: Optional[str]
    duration_ms: Optional[int]
    # How long to take getting there, in milliseconds. 0 is a cut.
    easing: Optional[str]
    # `linear` or `ease`. Only meaningful with a duration.
    item: str
    props: Dict[str, Any]
    # Any of `name`, `transform`, `crop`, `opacity`, `blend`, `visible`, `locked`, `audio`, `content`. A key left out is left alone.
    scene: str
    seq: Optional[int]
    # A client's own sequence number, echoed on the patch so a drag can discard the echoes of moves it has already drawn past.

class SetOutputRequest(TypedDict, total=False):
    """`output.set`. Change one destination in place, naming only what moves. The id picks the output and is never changed by this; renaming one is a remove and an add, because the id is what alerts, hooks and the runtime store call it."""

    id: str
    # The destination to change.
    policy: Optional[str]
    # "own" or "cdn", as `output.add`.
    queue_secs: Optional[float]
    # Seconds of encoded data to hold before the muxer.
    rendition: Optional[Dict[str, Any]]
    # A new rendition, as `output.add` takes it. `null` puts the output back on the programme's own encode; left out keeps what it has.
    uri: Optional[str]
    # The whole new address, stream key and all. Write only: no method ever reads it back, so leaving it out keeps the address already in force and a client can offer "change the buffer" without holding the key.

class SetSettingsRequest(TypedDict, total=False):
    """`plugin.settings.set`."""

    id: str
    settings: Dict[str, Any]
    # Only the keys named are changed.

class SetSourceRequest(TypedDict, total=False):
    """`source.set`: a full state assignment for one source. Every field is optional and only what is named moves, which is how every other setter in this protocol works. The one that matters here is `place`: it moves a running source between the core, a sidecar and a node. Unknown fields are refused rather than dropped. Serde's default is to ignore what it does not recognise, and a setter that answers 200 to a field it threw away is indistinguishable from one that saved it: the first party drawer sent `uri` here for months and told the operator it was saved."""

    color: Optional[str]
    # The colour the UI and the tally show it in. On the scene document, like the name.
    id: str
    # Source id. `source` is accepted too, which is what the scene side of this method has always been called with.
    latency_ms: Optional[int]
    # The latency budget in milliseconds, answered on the LATENCY query.
    name: Optional[str]
    # What to call it in the UI. Kept on the scene document, so every client, the tally and an agent read the same name.
    params: Optional[Dict[str, Any]]
    # Params for the source's own kind. Merged over what it has.
    place: Union[Place, None]
    # Where it runs: `core`, `in-process`, `sidecar` or `node:<name>`.
    transport: Union[BridgeTransport, None]
    # How a remote source's media travels: `rtp`, `srt` or `whip`.

class ShedNote(TypedDict, total=False):
    """One thing the governor stopped or slowed, and why."""

    what: str
    # "the 360p30 H.264 rendition for hls-main".
    why: str
    # The alert text.

class Show(TypedDict, total=False):
    """One show, as `show.list` and `event/show.changed` carry it."""

    alarms: Union[AlarmSettings, None]
    # The alarms a person set for it, when they set any.
    compositing: bool
    # true: scenes, transitions and a programme encode, in a process of its own. false: one input straight to its outputs, in the shared direct host, with no compositor.
    cpu_millicores: int
    # Its process's CPU, in thousandths of one core, measured between two reads of `show.list`. Zero on the first read and while it is stopped.
    error: Optional[str]
    # Why it is not running, when it is not and a person did not ask.
    health: Health
    id: str
    # A slug: `main`, `second-room`.
    input: Union[InputSpec, None]
    # What it takes in. A show that composites makes it its one source.
    memory_mib: int
    # Its process's resident memory, in MiB.
    name: str
    on_air: Optional[str]
    # The scene, or the source, on its programme. None while it shows the slate or is not running.
    outputs: List[Destination]
    # The outputs of a show without compositing. A show that composites keeps its outputs inside it, under `output.*` with `?show=<id>`.
    programme_kbps: int
    # What its outputs are sending, summed, in kilobits a second.
    restarts: int
    # How many times the station has started it again after it died.
    state: ShowState

ShowAdd = TypedDict("ShowAdd", {
    "compositing": Optional[bool],
    "from": Union[ShowFrom, None],
    "input": Union[InputSpec, None],
    "name": str,
    "outputs": List[ShowOutputSpec],
}, total=False)

class ShowAddManyRequest(TypedDict, total=False):
    """`show.add_many`."""

    dry_run: Optional[bool]
    # Left out: true. Says what would be made and what it would cost, and makes nothing.
    shows: List[ShowAdd]

class ShowAddManyResult(TypedDict, total=False):
    """`show.add_many`'s answer."""

    added: List[str]
    # The ids made, or that would be made on a dry run.
    dry_run: bool
    plan: BulkPlan
    refused: List[ShowRefused]

ShowAddRequest = TypedDict("ShowAddRequest", {
    "compositing": Optional[bool],
    "from": Union[ShowFrom, None],
    "input": Union[InputSpec, None],
    "name": str,
    "outputs": List[ShowOutputSpec],
}, total=False)

class ShowChanged(TypedDict, total=False):
    """`event/show.changed`."""

    show: Show

class ShowHealthEvent(TypedDict, total=False):
    """`event/show.health`."""

    health: Health
    id: str

class ShowList(TypedDict, total=False):
    """`show.list`."""

    current: str
    # The show a client reaches when it names none: the first one, which is the one the station was started with.
    shows: List[Show]

class ShowOutputAddRequest(TypedDict, total=False):
    """`show.output.add`."""

    enabled: Optional[bool]
    id: str
    # The show. `show` is taken as another name for it.
    key: Optional[str]
    # Write only.
    label: Optional[str]
    output: Optional[str]
    # The new output's own id, a slug. Made from the label when left out.
    params: HlsOutputParams
    platform: Optional[str]
    rendition: Union[RenditionChoice, None]
    uri: Optional[str]

class ShowOutputRemoveRequest(TypedDict, total=False):
    """`show.output.remove`."""

    id: str
    # The show. `show` is taken as another name for it.
    output: str

class ShowOutputSetRequest(TypedDict, total=False):
    """`show.output.set`. Names only what moves."""

    enabled: Optional[bool]
    id: str
    # The show. `show` is taken as another name for it.
    key: Optional[str]
    # A new key. Left out keeps the one it has.
    label: Optional[str]
    output: str
    # The output's id.
    params: HlsOutputParams
    # Replaces them all.
    rendition: Union[RenditionChoice, None]
    # Left out keeps what it has; `null` or `{"preset": "copy"}` goes back to a copy.
    uri: Optional[str]

class ShowOutputSpec(TypedDict, total=False):
    """An output as it is given to a show without compositing: an address, or a platform and a key."""

    enabled: Optional[bool]
    # On by default.
    id: Optional[str]
    # A slug, unique within the show. Made from the label when left out.
    key: Optional[str]
    # A platform's stream key. Write only: no method reads it back.
    label: Optional[str]
    params: HlsOutputParams
    # For an `hls://` output only.
    platform: Optional[str]
    # youtube, facebook, twitch, custom or srt. Left out: custom, which takes a whole address in `uri`.
    rendition: Union[RenditionChoice, None]
    # Left out: a copy of the input's own bytes, repackaged. Otherwise a rendition request or `{"preset": "youtube-720p30"}`, planned and admitted by the governor.
    uri: Optional[str]
    # The whole address: `srt://10.0.0.9:9000`, `rtmp://host/app/key`, `udp://239.2.2.2:5000`, `hls://viewers`. For a platform, its ingest server when it is not the platform's own.

class ShowRefused(TypedDict, total=False):
    """A show of `show.add_many` that was not made, and why."""

    data: Any
    index: int
    # Its place in `shows`, from 0.
    name: str
    why: str

class ShowRemoveManyRequest(TypedDict, total=False):
    """`show.remove_many`."""

    ids: List[str]

class ShowRemoveManyResult(TypedDict, total=False):
    """`show.remove_many`'s answer."""

    refused: List[ShowRefused]
    # Ids that were not removed, each with why.
    removed: List[str]

class ShowRemoved(TypedDict, total=False):
    """`show.remove`."""

    removed: str

class ShowRemovedEvent(TypedDict, total=False):
    """`event/show.removed`."""

    id: str

class ShowRenameRequest(TypedDict, total=False):
    """`show.rename`."""

    id: str
    name: str

class ShowSetRequest(TypedDict, total=False):
    """`show.set`. Names only what moves."""

    alarms: Union[AlarmSettings, None]
    # Alarm settings; the fields named move, the rest stay.
    compositing: Optional[bool]
    # true starts a show process whose one source is the input and moves the outputs to it; false goes back to a show without compositing, when it has one source and no scenes in use.
    id: str
    input: Union[InputSpec, None]
    name: Optional[str]

class ShowSetResult(TypedDict, total=False):
    """`show.set`'s answer: the show, and what a switch of compositing did."""

    alarms: Union[AlarmSettings, None]
    # The alarms a person set for it, when they set any.
    compositing: bool
    # true: scenes, transitions and a programme encode, in a process of its own. false: one input straight to its outputs, in the shared direct host, with no compositor.
    cpu_millicores: int
    # Its process's CPU, in thousandths of one core, measured between two reads of `show.list`. Zero on the first read and while it is stopped.
    error: Optional[str]
    # Why it is not running, when it is not and a person did not ask.
    health: Health
    id: str
    # A slug: `main`, `second-room`.
    input: Union[InputSpec, None]
    # What it takes in. A show that composites makes it its one source.
    memory_mib: int
    # Its process's resident memory, in MiB.
    name: str
    on_air: Optional[str]
    # The scene, or the source, on its programme. None while it shows the slate or is not running.
    outputs: List[Destination]
    # The outputs of a show without compositing. A show that composites keeps its outputs inside it, under `output.*` with `?show=<id>`.
    programme_kbps: int
    # What its outputs are sending, summed, in kilobits a second.
    restarts: int
    # How many times the station has started it again after it died.
    state: ShowState
    switch: Union[SwitchReport, None]

class ShowStats(TypedDict, total=False):
    """One show's numbers, as `show.stats` answers them."""

    cpu_millicores: Optional[int]
    # What the show costs the machine, thousandths of a core: for a show that mixes, its process as last measured; for a direct show, its outputs' encodes summed. Left out while nothing has measured it.
    health: Health
    id: str
    input: Union[InputStats, None]
    # None for a show with no input, or before the host has counted any.
    outputs: List[OutputStats]
    work: ShowWork
    # `mix`, `transcode` or `copy`.

class ShowStatsList(TypedDict, total=False):
    """`show.stats`'s answer."""

    shows: List[ShowStats]

class ShowStatsRequest(TypedDict, total=False):
    """`show.stats`."""

    fields: Optional[List[str]]
    # Of `health`, `input` and `outputs`. Left out: all three.
    ids: Optional[List[str]]
    # Left out: every show.

class Snapshot(TypedDict, total=False):
    """`event/snapshot`: the full state, and where in the stream it sits."""

    seq: int
    state: MixerStatus

class SnapshotRequest(TypedDict, total=False):
    """`snapshot.get` on `/rpc` and through MCP. The REST route serves the same bytes raw, because an `<img>` tag cannot read base64 out of JSON."""

    allow_large: bool
    # Permit a width above the `[snapshot] max_width` ceiling.
    force: bool
    # Ignore the per client rate limit for this one request.
    id: str
    # "sheet", "program", or a source id. A `.jpg` on the end is accepted.
    width: Optional[int]
    # Scale down to this many pixels across, keeping the aspect. Never enlarges. Omit for the `[snapshot] default_width` of 320, which is enough to see who is in shot; `width: 0` for the cell's own size.

class SourceAudioState(TypedDict, total=False):
    """What a source's audio controls read back as, which is what the audio endpoint answers with. Wider than `SourceAudio` because the fader and the mute apply to every source, while the page and media balance belongs only to a superimposed one. Every number here is read off the elements after the request landed, so a request whose gain was clamped answers with the gain that took effect."""

    gain: float
    media: Optional[List[float]]
    muted: bool
    page: Optional[float]
    # Absent on anything but a superimposed source, which is the only kind with separate sounds to balance.

class SourceMeta(TypedDict, total=False):
    """A source's name, colour and tray folder, as this collection has them."""

    color: Optional[str]
    # Free text so a client can use whatever it draws with. Absent means the client picks one by kind.
    group: Optional[str]
    # A tray folder: a tag on the source, purely for finding things. Not a scene group, which is a thing on the canvas.
    name: Optional[str]

class SourceNotAdded(TypedDict, total=False):
    """A source the import found and did not add, and why."""

    id: str
    plugin: Optional[str]
    # The plugin that plays it, when that is what is missing, so a page can offer to install it.
    reason: str

class SourcePositionState(TypedDict, total=False):
    """Where a seekable source has got to, which is what the seek endpoint answers with. Both numbers are read back off the pipeline after the seek has landed, not taken from the request. A seek snaps to a key unit, so the frame an operator asked for and the frame they got are rarely the same millisecond, and a scrubber drawn from the request would sit a little away from the picture."""

    duration_ms: Optional[int]
    # Absent while the demuxer has not worked the duration out yet.
    position_ms: int

class SourceReport(TypedDict, total=False):
    """One line of the report: an OBS source and what happened to it."""

    obs_name: str
    obs_type: str
    # The OBS plugin type, for example `ffmpeg_source`.
    placements: int
    # How many items in the collection use it.

class SourceStatus(TypedDict, total=False):
    audio_idle_ms: Optional[int]
    # Same for audio. `None` here while `has_audio` is true means the source advertised an audio track that never produced a decoded sample.
    cell: Optional[int]
    # Index into the multiview grid, or None while the source has no cell.
    duration_ms: Optional[int]
    gain: float
    # The operator's fader for this source, 0.0 silent through 1.0 unity to a ceiling of 10.0. Read back off the volume element rather than remembered, so what the UI shows is what the pipeline is doing.
    has_audio: bool
    has_video: bool
    id: str
    muted: bool
    # Muted by the operator. Held apart from the fader so that unmuting returns the source to where it was rather than to unity.
    name: str
    position_ms: Optional[int]
    # Where this source has got to, and how long it runs, in milliseconds. `None` on anything not seekable, and on a seekable source whose duration the demuxer has not worked out yet.
    seekable: bool
    # True when this source can be scrubbed. A file can be. A camera, an RTMP feed or a page cannot, and asking one to is a mistake worth refusing rather than quietly doing nothing.
    state: SourceState
    uri: str
    video_idle_ms: Optional[int]
    # Milliseconds since the last video buffer, or None if none has arrived.

class StatsListing(TypedDict, total=False):
    instances: List[InstanceRecord]

class StreamAudio(TypedDict, total=False):
    channels: int
    codec: str
    kbps: int
    sample_rate: int

class StreamVideo(TypedDict, total=False):
    codec: str
    fps: float
    height: int
    kbps: int
    width: int

class SubscribeRequest(TypedDict, total=False):
    """`core.subscribe`: which events, and which expensive streams."""

    events: List[str]
    # Event name patterns, matched against the part after `event/`. `*` matches one or more characters: "program.*" matches `event/program.took`. An empty list subscribes to everything.
    ext: Ext
    # The expensive streams this client wants. Nothing here runs unless a client asks for it.
    show: Optional[str]
    # Which show this connection follows, on a station running several. The station opens the connection to that show from here on. Omitted means the show the URL named with `?show=`, or the first show.

class SubscribeResult(TypedDict, total=False):
    """What `core.subscribe` answers with, before the snapshot arrives."""

    events: List[str]
    # The event patterns now in force.
    ignored_ext: List[str]
    # `ext` keys this build ignored. Empty on a build that knows them all.
    seq: int
    # The sequence number the snapshot that follows is current as of.

class SwitchReport(TypedDict, total=False):
    """What a switch of compositing did, in `show.set`'s answer."""

    compositing: bool
    # What the show does now.
    gap_ms: Optional[int]
    # From the moment the outputs stopped where they were to the moment every one of them was live again where they went. None when they were not all live within the wait, or there were none.
    note: str
    # What a person should know: an output that was still connecting when the answer was sent, and so on.
    outputs: List[str]
    # The outputs that moved.

class TakeRecord(TypedDict, total=False):
    """One take, as `program.history` reports it."""

    at_running_time_ms: int
    # Programme running time the cut landed on.
    by: str
    # Token id that asked for it, or "core" when the mixer did it itself.
    seq: int
    # Event sequence number the take was published under.
    source: Optional[str]
    # What went on air. Null is the slate.

class TakeRequest(TypedDict, total=False):
    """`program.take`: put a source on programme."""

    at_running_time_ms: Optional[int]
    # Programme running time to land the cut on, in milliseconds. Omit for immediate. Read the current running time from `core.info` or a status snapshot first.
    scene: Optional[str]
    # The scene to take, by name or by id. `source` wins when both are given; with neither, the armed scene goes on air.
    source: Optional[str]
    # Id of the source to put on air.
    transition: Union[Transition, None]
    # "fade", or {type, duration_ms, params}. Absent is a cut.

class Tally(TypedDict, total=False):
    """`event/tally`."""

    sources: Dict[str, Any]
    # Source id to "program", "preview" or "off".

class TaskRequest(TypedDict, total=False):
    task_id: str
    # The id a long running method answered with. Spelled `id` on the REST route, where it is in the path, and `task_id` everywhere else, which is what 03 section 6 calls it.

class TaskView(TypedDict, total=False):
    """What `task.get` answers with."""

    age_secs: int
    # Seconds since the task was started.
    error: Optional[str]
    kind: str
    # The method that started it, so a client reading a list knows what it is looking at.
    poll_interval_ms: Optional[int]
    # How long to wait before asking again, while it is still running.
    progress: Optional[float]
    # 0 to 1 where the work can say, absent where it cannot.
    result: Any
    # The body the method would have answered with, once it is done.
    state: TaskState
    task_id: str

class TokenInfo(TypedDict, total=False):
    """What the calling token is allowed to do, echoed back so a surface can grey out what it cannot reach instead of discovering it at the first refusal."""

    confirm: str
    # "none" or "required": whether destructive calls need a confirm token.
    id: str
    profile: str
    # MCP tool profile this token is meant for: "standard" or "minimal".
    rehearsal: bool
    scopes: List[str]

class ToolCallRequest(TypedDict, total=False):
    """`tool.call`."""

    arguments: Any
    # The tool's own arguments, as its input schema describes them.
    name: str
    # `<plugin>/<tool>`, or the bare tool name when only one plugin has it.

class Transform(TypedDict, total=False):
    """Where an item sits and how it is sized."""

    align: Align
    anchor: Vec2
    # Normalised 0 to 1 within the item's own box: (0,0) top left, (0.5,0.5) centre, (1,1) bottom right.
    fit: Fit
    frame: Union[Frame, None]
    # The rectangle the content is fitted into, in canvas pixels. Absent means the content's own size, scaled.
    position: Vec2
    # Canvas pixels, of the item's anchor point.
    rotation: float
    # Degrees, clockwise, about the anchor.
    scale: Vec2

class Transition2(TypedDict, total=False):
    """A named transition between two scenes."""

    duration_ms: int
    id: Id
    name: str
    params: Any
    type: str

class TransitionRequest(TypedDict, total=False):
    """How a take gets there. See docs/reference/transitions.md."""

    duration_ms: Optional[int]
    # How long it takes. 0 is a cut.
    params: Dict[str, Any]
    # A stinger takes clip, cut_at_ms, luma.
    type: str
    # cut, fade, move, stinger, or a plugin name.

class UiDefaults(TypedDict, total=False):
    """What a surface starts with: the layout, the theme and the gallery mode. Chosen by a preset (`preset.apply`), carried in `core.info` and pushed as `event/ui.changed`. None of it changes what the core does. It exists so the first page a volunteer sees is the one their preset chose rather than the one the last person to use this browser chose. 05 section 3b is where the four gallery modes are defined."""

    gallery: Optional[str]
    # `live`, `snapshot`, `icon` or `label`. Absent means the surface asks the machine, which is what `gmx doctor` proposes.
    layout: Dict[str, Any]
    # Slot to panels, top to bottom. Empty means the surface's own default.
    preset: Optional[str]
    # The preset that set these, so a surface knows one has been applied.
    theme: Optional[str]
    # A theme id the surface resolves, for example `dark` or `calm`.

class Update(TypedDict, total=False):
    """One record as it was and as it is."""

    after: Record
    before: Record

class UpdatePluginRequest(TypedDict, total=False):
    """`plugin.update`."""

    id: str
    source: Optional[str]
    # Where the new build comes from. Defaults to wherever this plugin was installed from last time.

class ValidateRequest(TypedDict, total=False):
    scene: Optional[str]
    # Leave it out to check the whole collection.

class Validation(TypedDict, total=False):
    """`scene.validate`."""

    findings: List[Finding]
    ok: bool
    # True when there is nothing to fix.

class Vec2(TypedDict, total=False):
    """A point or a pair of factors."""

    x: float
    y: float

class VideoShape(TypedDict, total=False):
    """A picture as it is: codec, size, rate, bitrate."""

    bitrate_kbps: int
    # Measured or configured. 0 when unknown (a raw source).
    codec: VideoCodec
    fps: Fps
    height: int
    keyframe_ms: int
    # 0 when unknown.
    width: int

class VideoWant(TypedDict, total=False):
    """The video an output wants. Every field left out is taken from the source."""

    bitrate_kbps: Optional[int]
    # Target bitrate. A copy is kept when the source is within `bitrate_tolerance` of it.
    bitrate_tolerance: Optional[float]
    # Fraction either way a source's bitrate may differ and still be copied. 0.25 when left out.
    codec: Union[VideoCodec, None]
    fps: Union[Fps, None]
    height: Optional[int]
    keyframe_ms: Optional[int]
    # Keyframe interval. Renditions in one ladder share it.
    width: Optional[int]

class VitalsConfig(TypedDict, total=False):
    """`[vitals]`, and what `vitals.set` changes: the thresholds, and whether to keep a mosaic up for the picture alarms while nobody is looking."""

    alarms: bool
    black_luma: int
    # An 8 bit luma at or under which a pixel counts as black. 38 is ten percent of the way from video black (16) to white (235), the figure ffmpeg's blackdetect uses.
    black_ratio: float
    # The share of pixels that must be black for the picture to be.
    black_secs: float
    # Seconds a picture must stay black before `black` is raised. 0: off.
    cc_errors: int
    # Continuity errors within `window_secs` that raise `cc-errors`. 0: off.
    freeze_diff: float
    # The mean luma difference between two samples, 0 to 1, under which the picture counts as unchanged.
    freeze_secs: float
    # Seconds a picture must stay unchanged before `freeze` is raised. 0: off.
    loss: int
    # Packets lost within `window_secs` that raise `loss`. 0: off.
    silence_db: float
    # The peak level in dBFS under which the sound counts as quiet.
    silence_secs: float
    # Seconds the sound must stay quiet before `silence` is raised. 0: off.
    stall_secs: float
    # Seconds without a single packet of input before `stall` is raised.
    window_secs: float
    # The window the two counters are judged over.

class ProgramTookEvent(TypedDict, total=False):
    at_running_time_ms: int
    duration_ms: int
    scene: Optional[str]
    source: Optional[str]
    transition: str
    transition_id: int

class ScenePatchEvent(TypedDict, total=False):
    added: List[Dict[str, Any]]
    client_seq: Optional[int]
    # The client's own sequence number, from the `seq` on the command, so a drag discards echoes of moves it has already drawn past.
    label: Optional[str]
    removed: List[str]
    scope: Literal['document']
    seq: int
    source_client: Optional[str]
    # Who asked for the change, so a client suppresses the echo of its own edits.
    updated: List[Dict[str, Any]]

class PreviewChangedEvent(TypedDict, total=False):
    scene: Optional[str]

class SourceStateEvent(TypedDict, total=False):
    detail: Optional[str]
    source: str
    state: SourceState

class SourcePositionEvent(TypedDict, total=False):
    duration_ms: Optional[int]
    position_ms: int
    source: str

class OutputStateEvent(TypedDict, total=False):
    output: str
    reconnects: int
    state: OutputState

class AdbreakChangedEvent(TypedDict, total=False):
    ad: Union[AdStatus, None]

class UiChangedEvent(TypedDict, total=False):
    ui: UiDefaults

class HookBlockedEvent(TypedDict, total=False):
    hook: str
    # The hook name, for example take.before.
    plugin: str
    # The plugin that owns it, or the URL or command when it came from [[hooks]] in the config.
    reason: str
    # What went wrong and what to do about it.

class MediaChangedEvent(TypedDict, total=False):
    conversion: Any
    name: str

class ChannelChangedEvent(TypedDict, total=False):
    channel: Channel

class ChannelRemovedEvent(TypedDict, total=False):
    id: str

ChannelRefusedEvent = TypedDict("ChannelRefusedEvent", {
    "from": str,
    "id": str,
    "stream": str,
    "why": str,
}, total=False)

class AlertEvent(TypedDict, total=False):
    action: ErrorAction
    message: str
    severity: Severity2

class TelemetryEvent(TypedDict, total=False):
    black: float
    # fraction of the picture at or below black, 0 to 1
    freeze: bool
    lufs_i: Optional[float]
    lufs_s: Optional[float]
    # short term loudness over three seconds, approximated from the programme meter
    shot: float
    # how much the picture changed since the last frame, 0 to 1
    silence: bool
    sources: Dict[str, Any]
    # source id to 1 when it is live and 0 otherwise
    ts: int
    # milliseconds since the Unix epoch

# What pressing the button does.
ActionKind = Literal['set-config', 'install-plugin', 'enable-plugin', 'open', 'retry', 'restart']

# `ext.agent`. `true` takes the default thresholds; an object moves them.
AgentExt = Union[bool, Dict[str, Any]]

# What an alarm is about.
AlarmKind = Literal['no-input', 'stall', 'black', 'freeze', 'silence', 'cc-errors', 'loss', 'output-failed', 'governor-refused', 'shed']

# The nine alignment keywords, used to place content inside its frame.
Align = Literal['top-left', 'top-center', 'top-right', 'center-left', 'center', 'center-right', 'bottom-left', 'bottom-center', 'bottom-right']

# When a change to a key takes effect.
Applies = Literal['live', 'next_source', 'restart']

# Whether the item's source is heard. A source is audible when any live item of it says so, which is OBS's behaviour and changes no pad topology.
Audio = Literal['follow', 'always', 'never']

AudioCodec = Literal['aac', 'opus', 'mp3', 'ac3', 'pcm', 'other']

# OBS's blend enum, so an import carries across unchanged.
Blend = Literal['normal', 'add', 'screen', 'multiply', 'lighten', 'darken', 'subtract']

# How media crosses between a node and the core.
BridgeTransport = Literal['rtp', 'srt', 'whip']

# A way a publisher reaches a channel. RTMPS is `Rtmps`, set apart because it has a port of its own.
ChannelProtocol = Literal['rtmp', 'srt', 'whip']

# How the bytes leave. Decides which codecs are allowed: FLV carries H.264 (and HEVC and AV1 in enhanced RTMP), WebRTC wants VP8, VP9, H.264 or AV1.
Container = Literal['flv', 'mpeg-ts', 'mp4-fragmented', 'mkv', 'hls', 'll-hls', 'dash', 'rtp', 'webrtc']

ConversionPhase = Literal['running', 'done', 'failed']

# Copied as it arrives, or converted.
DestinationMode = Literal['copy', 'transcode']

# Where a destination has got to.
DestinationState = Literal['off', 'waiting', 'connecting', 'live', 'reconnecting', 'failed']

# How content fills its frame. SVG's vocabulary, which replaces OBS's seven bounds types and maps onto `sizing-policy` on a `glvideomixer` pad.
Fit = Literal['none', 'contain', 'cover', 'stretch', 'fit-width', 'fit-height', 'max']

# Content on the wire. The same four shapes as the tree, except that a group names no children: they are records whose parent is the group.
FlatContent = Dict[str, Any]

# The one word a monitoring wall colours a row by.
HealthState = Literal['ok', 'warning', 'alarm', 'off']

# A UUID in the hyphenated form. Minted ids are version 7 (time ordered); ids derived from a layout are version 8.
Id = str

# How a publisher gives its key.
KeyMode = Literal['query', 'stream']

# Why a source is not running.
MissingWhy = Literal['failed', 'not_started', 'removed', 'unknown']

Mode = Literal['replace', 'merge']

# `ext.multiview`. Accepts `false` to mean off, or an object.
MultiviewExt = Union[bool, Dict[str, Any]]

OutputState = Literal['connecting', 'live', 'reconnecting', 'failed']

# Where an instance runs: core, in-process, sidecar, or node:<name>.
Place = str

# `ext.preview`. Either `"full"`, `false`, or an object.
PreviewExt = Union[str, bool, Dict[str, Any]]

# What an output asks for: a whole request, or a preset by id. A request's `id` is replaced by the output's own id (a ladder's rungs get `<output>-<rung>`), so a client may send any slug there.
RenditionChoice = Union[PresetRef, LadderRef, RenditionRequest]

ResponseFormat = Literal['concise', 'detailed']

# How a core that exits gets started again.
RestartHow = Literal['supervised', 'none']

# How much the reader should care.
Severity = Literal['error', 'warning', 'info']

Severity2 = Union[Literal['info', 'warning', 'error'], Literal['critical']]

# What a new show starts from.
ShowFrom = Union[str, Dict[str, Any]]

# Where a show is in its life.
ShowState = Literal['starting', 'running', 'stopped', 'failed']

# What a show does to make its outputs, which is what its load pays for.
ShowWork = Literal['mix', 'transcode', 'copy']

SourceState = Literal['connecting', 'live', 'stalled', 'failed']

TaskState = Literal['running', 'completed', 'failed', 'cancelled']

# `ext.telemetry`. Accepts `false` to mean off, `true` for the default rate, or an object naming it.
TelemetryExt = Union[bool, Dict[str, Any]]

# A name, or an object.
Transition = Union[str, TransitionRequest]

VideoCodec = Union[Literal['h264', 'h265', 'av1', 'vp8', 'vp9', 'mpeg2', 'prores'], Literal['other']]

METHODS = (
    {"name": "adbreak.end", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/adbreak/end"), "summary": 'Cut a running ad short, or disarm one that is scheduled.'},
    {"name": "adbreak.start", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/adbreak/start"), "summary": 'Interrupt the programme with a clip, then rejoin live when it ends.'},
    {"name": "agent.state", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/agent/state"), "summary": "The compact document written for agents: the programme, each source's state and a motion score saying how much its picture is changing."},
    {"name": "channel.add", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/channels"), "summary": 'Make a channel and its first key, which is in this answer. channel.key.reveal reads it again later.'},
    {"name": "channel.certificate.generate", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/channels/certificate/generate"), "summary": "Make a self signed certificate for RTMPS, for this machine's addresses unless names are given. Encoders must be told to accept it; one from a certificate authority needs no such step."},
    {"name": "channel.certificate.set", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/channels/certificate/set"), "summary": 'Give RTMPS a certificate: the PEM of the certificate (and its chain) and of its private key, as a certificate authority issued them. Checked before it is kept; the key is sealed and never read back.'},
    {"name": "channel.destination.add", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/channels/{id}/destination/add"), "summary": "Send a channel's stream on to YouTube, Facebook, Twitch, an RTMP server or an SRT receiver as it arrives. Nothing is decoded or encoded. The key is write only."},
    {"name": "channel.destination.remove", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/channels/{id}/destination/remove"), "summary": "Stop sending a channel's stream to one destination and forget it. The publisher and the other destinations are not touched."},
    {"name": "channel.destination.set", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/channels/{id}/destination"), "summary": "Change one of a channel's destinations, naming only what moves: a new key, another server, which stream it sends, on or off. A key left out is kept."},
    {"name": "channel.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/channels/{id}"), "summary": 'One channel.'},
    {"name": "channel.key.add", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/channels/{id}/key/add"), "summary": 'Make another key for a channel, to give to one more person or encoder. The key is in this answer, and channel.key.reveal reads it again later.'},
    {"name": "channel.key.remove", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/channels/{id}/key/remove"), "summary": 'Take one key back. A publisher on air with it is cut off and the next one is turned away; the other keys are untouched.'},
    {"name": "channel.key.reveal", "scope": "admin", "mutating": False, "destructive": False, "rest": ("POST", "/api/v1/channels/{id}/key/reveal"), "summary": 'Read one key of a channel back, to give it to an encoder again. Admin only; a list shows only the last four characters. Each read is logged with who asked, never with the key.'},
    {"name": "channel.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/channels"), "summary": 'Every channel with its keys (as hints), the address to publish to over each protocol it has on, and what is live on it; and which ingest ports are open and for which channels.'},
    {"name": "channel.remove", "scope": "admin", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/channels/{id}"), "summary": 'Remove a channel and forget its keys. Sources it made that no scene holds go with it.'},
    {"name": "channel.set", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/channels/{id}/set"), "summary": 'Rename a channel, switch it on or off, or change its application name, whether its streams become sources, how its key is given, which protocols it takes (rtmp, srt, whip) or RTMPS and its port. A port opens when the first channel needs it and closes when the last one stops. Only what is named moves.'},
    {"name": "codec.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/codecs"), "summary": 'Every codec and element in the catalogue, which of them this machine actually has, and what it would pick.'},
    {"name": "config.get", "scope": "admin", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/config"), "summary": "The mixer's settings: each key's value in the config file, its default, when a change to it takes effect, and which keys are waiting for a restart. Secrets say only whether one is set."},
    {"name": "config.reset", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/config/reset"), "summary": 'Put settings back to their defaults by taking them out of the config file. Answers like config.set.'},
    {"name": "config.schema", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/config/schema"), "summary": 'Every setting config.set takes, as one JSON Schema: type, title, description, default, range or choices, and x-gmx-applies (live, next_source or restart).'},
    {"name": "config.set", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/config/set"), "summary": 'Change settings in the config file, keeping its comments. Every value is checked first and nothing is written unless all of them fit. Live keys take effect at once; the answer says which wait for the next source or a restart.'},
    {"name": "core.api", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/core/api"), "summary": 'Every method, event and type as JSON Schema. The same document as protocol.json and `godwinmix --api-info`.'},
    {"name": "core.doctor", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/core/doctor"), "summary": 'The environment checks: GStreamer, the elements, the config, the disk and the ports. The same list `gmx doctor` prints.'},
    {"name": "core.info", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/core/info"), "summary": 'What this core is, what it can do, and where its edges are.'},
    {"name": "core.restart", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/core/restart"), "summary": 'Stop the mixer and have it started again, when something will start it again. On a supervised core (core.info restart.possible) it answers restarting: true and exits; the programme is off air until it is back. On a core started by hand it answers restarting: false, says how to restart it, and keeps running.'},
    {"name": "core.session_log", "scope": "admin", "mutating": True, "destructive": False, "rest": ("GET", "/api/v1/core/session_log"), "summary": 'The append only record of everything that happened, back as far as you ask.'},
    {"name": "core.shutdown", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/core/shutdown"), "summary": 'Stop the mixer, and with it the programme. Nothing else takes the show off air, so this is deliberately its own call.'},
    {"name": "core.startup_report", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/core/startup_report"), "summary": 'How long each stage of the start took, and what was over the 250 ms mark.'},
    {"name": "core.status", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/core/status"), "summary": 'The full state: programme, every source, every output, the multiview grid, the encoder backend and any ad break.'},
    {"name": "core.subscribe", "scope": "read", "mutating": False, "destructive": False, "rest": None, "summary": 'Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush.'},
    {"name": "device.discover", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/device/discover"), "summary": "Ask every device plugin what it can see: cameras, NDI senders, publishers. Each candidate's params are ready for source.add."},
    {"name": "filter.add", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/filters"), "summary": 'Hang a filter on one source or on the programme, live.'},
    {"name": "filter.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/filters"), "summary": 'Every filter in place, with what it is and where it sits.'},
    {"name": "filter.remove", "scope": "operate", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/filters/{id}"), "summary": 'Take a filter out of the pipeline.'},
    {"name": "filter.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/filters/{id}/set"), "summary": "Change a filter's settings in place. A filter that cannot take the change while running says so rather than being restarted behind your back."},
    {"name": "governor.calibrate", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/governor/calibrate"), "summary": "Measure this machine's encoders again, in the background, a few seconds of every core. Refused while anything is on air unless `confirm` is true."},
    {"name": "governor.status", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/governor/status"), "summary": 'The resource governor: when this machine was measured, what is in use and free on the CPU and each GPU encoder, and what was shed to keep the programme whole.'},
    {"name": "log.gst", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/log/gst"), "summary": "Raise GStreamer's own debug categories for a while, then let them fall back on their own."},
    {"name": "log.levels", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/log/levels"), "summary": 'Every log level override in force, and the GStreamer categories still raised.'},
    {"name": "log.set", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/log/set"), "summary": "Change one instance's or one module's log level while the mixer runs."},
    {"name": "media.convert", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/media/{id}/convert"), "summary": 'Transcode a library file to a web safe copy, in the background.'},
    {"name": "media.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/media"), "summary": 'The clips in the library, with durations and whether each has audio.'},
    {"name": "media.remove", "scope": "operate", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/media/{id}"), "summary": 'Delete a library file and its converted copy. Refused while it is a live source.'},
    {"name": "media.upload", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/media/upload"), "summary": 'Stream a file into the library. HTTP only: the body is the file.'},
    {"name": "node.discover", "scope": "read", "mutating": False, "destructive": False, "rest": ("POST", "/api/v1/nodes/{id}/discover"), "summary": 'Look for nodes on the local network over mDNS. A network without multicast finds nothing and the [nodes] table in the config is the way there.'},
    {"name": "node.enrol", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/nodes/{id}/enrol"), "summary": 'Mint a one time enrolment token for a node. The answer carries the command to run on the other machine. The token is good for one enrolment and expires.'},
    {"name": "node.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/nodes/{id}"), "summary": 'One node: its clock offset, how long since its last heartbeat, the plugins it has, and the instances it is hosting.'},
    {"name": "node.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/nodes"), "summary": 'Every node this core knows about: the ones connected now, the ones that have gone quiet, and the ones the config expects that have never dialled in.'},
    {"name": "node.remove", "scope": "admin", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/nodes/{id}"), "summary": 'Forget a node. Its bridge is closed, every token minted for a plugin on it is revoked, and its certificate stops working. Sources placed on it go to the slate until they are moved or the node enrols again.'},
    {"name": "output.add", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/outputs"), "summary": 'Send the programme to another destination. The encoder is shared, so adding one costs nothing on air.'},
    {"name": "output.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/outputs/{id}"), "summary": 'One destination.'},
    {"name": "output.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/outputs"), "summary": 'Every destination, with its state, reconnect count and how much is buffered.'},
    {"name": "output.reconnect", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/outputs/{id}/reconnect"), "summary": "Drop and re-establish one destination's connection now, without waiting for its reconnect policy."},
    {"name": "output.remove", "scope": "operate", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/outputs/{id}"), "summary": 'Stop sending to a destination and forget it. Other outputs are unaffected.'},
    {"name": "output.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/outputs/{id}/set"), "summary": 'Change a destination in place: a new address with a new stream key, a new reconnect policy, a deeper outage buffer. The address is write only, so a client that only wants the buffer never has to hold the key.'},
    {"name": "path.create", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/path/create"), "summary": 'Make one new folder inside a folder path.list shows, and list it. A folder that is already there is listed rather than refused.'},
    {"name": "path.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/path/list"), "summary": "The folders in one folder on the mixer, and whether each is writable, for a folder picker. Only the home folder and the mixer's own folders are shown; files never are."},
    {"name": "pipeline.clock", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/pipeline/clock"), "summary": 'The clock every pipeline is running against, and how far each one has got.'},
    {"name": "pipeline.dot", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/pipeline/dot"), "summary": 'One pipeline as a graphviz graph: every element, every pad and the caps negotiated between them.'},
    {"name": "pipeline.latency", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/pipeline/latency"), "summary": 'How much delay one pipeline is carrying, and which stage put it there.'},
    {"name": "pipeline.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/pipeline/list"), "summary": 'Every pipeline running right now, by the name the other pipeline methods accept.'},
    {"name": "pipeline.queues", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/pipeline/queues"), "summary": 'Every queue in one pipeline with how full it is, fullest first. A queue that stays full is where the trouble is.'},
    {"name": "plugin.add", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/plugins"), "summary": 'Install a plugin, while live, from any source form: a GitHub release (owner/repo), a git URL, cargo:, npm:, pypi:, a local directory, or a bare name looked up in the marketplaces this mixer knows. The signature and the api level are checked before anything is copied.'},
    {"name": "plugin.describe", "scope": "read", "mutating": False, "destructive": False, "rest": ("POST", "/api/v1/plugins/{id}/describe"), "summary": 'One plugin in full: its manifest, the settings schema of every provide, and the description from each SKILL.md.'},
    {"name": "plugin.disable", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/plugins/{id}/disable"), "summary": 'Turn a plugin off without uninstalling it. It registers nothing and runs no process until it is enabled again.'},
    {"name": "plugin.enable", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/plugins/{id}/enable"), "summary": 'Turn a plugin back on. It registers what it declares and its instances start.'},
    {"name": "plugin.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/plugins"), "summary": 'Every plugin installed, with what it provides and what each running instance is costing in cpu, memory, latency, dropped buffers and restarts.'},
    {"name": "plugin.reload", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/plugins/{id}/reload"), "summary": "Read a plugin's directory again and swap its running instances one at a time, with the freeze frame covering each."},
    {"name": "plugin.remove", "scope": "admin", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/plugins/{id}"), "summary": 'Uninstall a plugin and unwind everything it registered: its provides, its tools, its panels, its hooks and its discovery matchers.'},
    {"name": "plugin.search", "scope": "read", "mutating": False, "destructive": False, "rest": ("POST", "/api/v1/plugins/{id}/search"), "summary": 'Search every marketplace this mixer knows for a plugin, by name, description or kind. Answers what `gmx plugin add <name>` would install.'},
    {"name": "plugin.settings.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/plugins/{id}/settings"), "summary": "A plugin's settings as they stand, with its schema beside them."},
    {"name": "plugin.settings.set", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/plugins/{id}/settings"), "summary": "Change a plugin's settings. A plugin that cannot take a change while running says so rather than being restarted behind your back."},
    {"name": "plugin.stats", "scope": "read", "mutating": False, "destructive": False, "rest": ("POST", "/api/v1/plugins/{id}/stats"), "summary": 'Per instance cpu, memory, media latency, dropped buffers and restarts, refreshed once a second.'},
    {"name": "plugin.update", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/plugins/{id}/update"), "summary": 'Fetch a newer build of a plugin, install it beside the one that is running, and prove it starts. A build that does not answer `initialize` within ten seconds is rolled back and the plugin that was working stays working.'},
    {"name": "preset.apply", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/preset/apply"), "summary": 'Put a preset on this core: its config, its scenes, its layout, its theme and its gallery mode. Pass dry_run to get the plan and write nothing.'},
    {"name": "preset.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/preset/list"), "summary": 'Every preset this core can apply: the six built in, plus anything installed beside the binary or under ~/.godwinmix/presets.'},
    {"name": "preset.save", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/preset/save"), "summary": "Turn this core's working setup into a preset directory somebody else can apply. Stream keys and the control token are replaced with placeholders."},
    {"name": "preview.close", "scope": "read", "mutating": False, "destructive": False, "rest": ("POST", "/api/v1/preview/close"), "summary": 'Give up a raw frame socket. The socket goes when the last holder closes it.'},
    {"name": "preview.open", "scope": "read", "mutating": False, "destructive": False, "rest": ("POST", "/api/v1/preview/open"), "summary": 'Open a raw frame socket on this machine for a source or the programme, and answer with its path. No encode anywhere: a client on the same host reads the frames the mixer already has. Close it with preview.close.'},
    {"name": "program.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/program"), "summary": 'What is on air, the programme running time, and what revert would go back to.'},
    {"name": "program.golive", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/program/golive"), "summary": 'One call to put a web page on air: add the page, add the destination, and take the page as soon as it renders.'},
    {"name": "program.history", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/program/history"), "summary": 'The last hundred takes, newest first, with the token that asked for each.'},
    {"name": "program.revert", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/program/revert"), "summary": 'Take back to the shot before this one.'},
    {"name": "program.take", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/program/take"), "summary": 'Put a scene or a source on programme. The cut is instant and the outgoing stream is not disturbed.'},
    {"name": "project.export", "scope": "admin", "mutating": False, "destructive": False, "rest": ("POST", "/api/v1/project/export"), "summary": "This mixer as one project file: settings, sources, outputs and renditions, channels, scenes, the page's layout, and its clips by name and size. Keys only with include_secrets."},
    {"name": "project.import", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/project/import"), "summary": "Open a project file: answers with what it would change (dry_run is true unless false is sent), then replaces this mixer's setup or merges beside it. Says which settings wait for a restart."},
    {"name": "rendition.plan", "scope": "read", "mutating": False, "destructive": False, "rest": ("POST", "/api/v1/rendition/plan"), "summary": 'What the planner built for every output that asked for a rendition: each node, what it serves, which encoder and why, and the totals.'},
    {"name": "rendition.presets", "scope": "read", "mutating": False, "destructive": False, "rest": ("POST", "/api/v1/rendition/presets"), "summary": 'Every rendition preset, priced on this machine by the governor. One this machine cannot make says so, with why.'},
    {"name": "scene.add", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes"), "summary": 'Make an empty scene, or one built from a set of sources.'},
    {"name": "scene.apply_graphic", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/apply_graphic"), "summary": 'Fill a graphic that is on a scene, by field name, and optionally play it on or take it off. Answers with the records and, if asked, a still.'},
    {"name": "scene.apply_layout", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/apply_layout"), "summary": 'Apply a layout, making a scene or reshaping one that exists. Applying onto an existing scene keeps the item ids, so the change is a ramp and not a cut.'},
    {"name": "scene.create_from", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/create_from"), "summary": 'A scene from a set of sources, laid out by the built in layout for that count (full, two-box, three-box, quad, then a grid) or by a named one.'},
    {"name": "scene.duplicate", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/{id}/duplicate"), "summary": 'A copy of a scene with new ids throughout, so editing the copy cannot touch the original.'},
    {"name": "scene.edit.apply", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/edit/apply"), "summary": 'Write a draft back into the live document.'},
    {"name": "scene.edit.begin", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/edit/begin"), "summary": 'Take a working copy of a scene. Editing is off air by default: the draft is written back on the next take of that scene, or when you apply it.'},
    {"name": "scene.edit.discard", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/edit/discard"), "summary": 'Throw a draft away. The live document is untouched.'},
    {"name": "scene.export", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/export"), "summary": 'The whole collection: as JSON, or as a zip bundle carrying its assets with a hash each, which is what you send somebody.'},
    {"name": "scene.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/{id}"), "summary": 'One scene: its records and where every item actually lands on the canvas.'},
    {"name": "scene.graphic.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/graphic/list"), "summary": 'Every graphic template this core can place, with what each one takes.'},
    {"name": "scene.history.mark", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/history/mark"), "summary": 'Group the changes that follow into one undo step, until the next mark. This is what makes a drag of forty moves one Ctrl+Z.'},
    {"name": "scene.import", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/import"), "summary": 'Read a collection bundle, a zip or the directory it unpacks to, and add its scenes to this one. Answers with a relink report for any asset that did not come across.'},
    {"name": "scene.import.obs", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/import/obs"), "summary": "Read an OBS Studio scene collection and add its scenes to this one. Send the file's text as `content` (what a page's file picker reads) or a `path` on the mixer's machine. With `add_sources: true` the sources the scenes draw are added through source.add, and the answer says which were added and why any were not."},
    {"name": "scene.item.add", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/add"), "summary": "Put something on a scene's canvas. With no transform it lands in the next free cell, so a drop never needs a dialog."},
    {"name": "scene.item.align", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/align"), "summary": 'Line items up on an edge: left, right, top, bottom, center-x or center-y.'},
    {"name": "scene.item.arrange_grid", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/arrange_grid"), "summary": 'Lay items out in a grid of `cols` columns.'},
    {"name": "scene.item.bind", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/bind"), "summary": "Bind a geometry property to an expression over the collection's parameters, so changing a number moves everything that follows it."},
    {"name": "scene.item.copy", "scope": "operate", "mutating": True, "destructive": False, "rest": ("GET", "/api/v1/scenes/item/copy"), "summary": 'Copy an item into another scene. The copy keeps the transform and the filters and gets a new id.'},
    {"name": "scene.item.cover_canvas", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/cover_canvas"), "summary": 'Put items over the whole canvas, filling it and letting the overflow go.'},
    {"name": "scene.item.distribute", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/distribute"), "summary": 'Space items evenly between the two on the ends, horizontally or vertically.'},
    {"name": "scene.item.filter.add", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/filter/add"), "summary": 'Hang a filter on one item, so a camera keyed in one scene is not keyed in all of them.'},
    {"name": "scene.item.filter.remove", "scope": "operate", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/scenes/item/filter/remove"), "summary": 'Take a filter off an item.'},
    {"name": "scene.item.filter.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/filter/set"), "summary": "Change one of an item's filters, or turn it off without taking it out."},
    {"name": "scene.item.fit_to_canvas", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/fit_to_canvas"), "summary": 'Put items over the whole canvas, keeping their aspect ratio inside it.'},
    {"name": "scene.item.group", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/group"), "summary": 'Put items into a group. The picture does not change.'},
    {"name": "scene.item.match_size", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/match_size"), "summary": 'Make items the same size as another one.'},
    {"name": "scene.item.move", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/move"), "summary": 'Move an item to another scene, keeping its transform and filters.'},
    {"name": "scene.item.remove", "scope": "operate", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/scenes/item/remove"), "summary": 'Take an item off a scene.'},
    {"name": "scene.item.reorder", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/reorder"), "summary": 'Move an item up or down the stack, between two named neighbours.'},
    {"name": "scene.item.schema", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/item/schema"), "summary": "What one item type takes: a graphic's OGraf schema, or a source or filter plugin's settings schema. The same JSON Schema every client renders an inspector from."},
    {"name": "scene.item.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/set"), "summary": "Assign an item's properties. Only the keys named move; the rest are left alone, so calling it twice with the same body changes nothing the second time."},
    {"name": "scene.item.ungroup", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/ungroup"), "summary": 'Take a group apart, leaving every child exactly where it looked.'},
    {"name": "scene.layout.copy", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/layout/copy"), "summary": "Read one scene's geometry, to paste onto another."},
    {"name": "scene.layout.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/layout/list"), "summary": 'The layouts that ship with the core, with the parameters each one takes.'},
    {"name": "scene.layout.paste", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/layout/paste"), "summary": "Put one scene's geometry onto another's items, matched by name first and slot order second. Items that match nothing are left alone."},
    {"name": "scene.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes"), "summary": 'Every scene in the collection, with how many items it has, the sources it draws and whether it is armed.'},
    {"name": "scene.params.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/params/get"), "summary": "The collection's typed parameters, readable without their values, so a client discovers what is fillable before filling it."},
    {"name": "scene.params.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/params/set"), "summary": "Set the collection's parameter values, declaring any that are new. A `{{name}}` in any string property of any item follows them, so one call changes every lower third that uses it."},
    {"name": "scene.preview.frame", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/preview/frame"), "summary": 'A still of the armed scene as base64 JPEG, the floor every client has.'},
    {"name": "scene.preview.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/preview/set"), "summary": 'Arm a scene. The armed scene is the preview, and program.take with no argument takes it.'},
    {"name": "scene.redo", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/redo"), "summary": 'Put back what undo took away.'},
    {"name": "scene.remove", "scope": "operate", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/scenes/{id}"), "summary": 'Delete a scene. What is on air is not touched.'},
    {"name": "scene.rename", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/{id}/rename"), "summary": "Change a scene's name, its colour, or both. Names and colours live on the document, so every client, the tally and an agent see the same ones."},
    {"name": "scene.transaction.abort", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/transaction/abort"), "summary": 'Throw the batch away. The document goes back to where it was when the batch opened.'},
    {"name": "scene.transaction.begin", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/transaction/begin"), "summary": 'Start a batch. Everything until the commit applies on one frame or not at all, and undoes in one step.'},
    {"name": "scene.transaction.commit", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/transaction/commit"), "summary": 'Apply the batch.'},
    {"name": "scene.undo", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/undo"), "summary": 'Undo the last change. A drag marked with scene.history.mark undoes as one step.'},
    {"name": "scene.validate", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/validate"), "summary": 'Overlaps, items off the canvas, safe area breaches and missing sources: what to fix before saying a scene is done.'},
    {"name": "show.add", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/shows"), "summary": 'Make another show and start it: empty, a copy of a show (without its outputs, so nothing goes out twice), or from a project file.'},
    {"name": "show.add_many", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/shows/add_many"), "summary": 'Make many shows in one call, such as every channel of a headend. The whole batch is checked first. With dry_run (the default) nothing is made: the answer says what would be, what its renditions would cost and whether the governor would admit them. Without it, every show that fits is made and the rest are refused with why; a show is made whole or not at all.'},
    {"name": "show.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/shows"), "summary": 'Every show on this machine: its name, whether it is running, what is on air, what its outputs send and what its process costs. `current` is the show a client reaches when it names none.'},
    {"name": "show.output.add", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/shows/{id}/output/add"), "summary": "Send a show without compositing to another place: an address (SRT, RTMP, UDP, RTP or RIST), a platform and its key, or hls://<name> to serve it as HLS from this port. Left without a rendition it copies the input's bytes; with one it is planned and admitted by the governor. The key is write only."},
    {"name": "show.output.remove", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/shows/{id}/output/remove"), "summary": 'Stop one output of a show without compositing and forget it, key and all.'},
    {"name": "show.output.set", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/shows/{id}/output"), "summary": 'Change one output of a show without compositing, naming only what moves: another address, a new key, on or off, copy or a rendition.'},
    {"name": "show.remove", "scope": "admin", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/shows/{id}"), "summary": 'Stop a show and remove it with its folder. Refused for the last show and for main, the show the station was started with.'},
    {"name": "show.remove_many", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/shows/remove_many"), "summary": 'Stop and remove many shows. Each id that cannot go (main, or one not there) is refused with why, and the rest go.'},
    {"name": "show.rename", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/shows/{id}/rename"), "summary": 'Give a show another name. Its id stays.'},
    {"name": "show.set", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/shows/{id}/set"), "summary": "Change a show's name, its input, or whether it composites. Turning compositing on starts a show process whose one source is the input and moves the outputs to it; turning it off hands them back to the direct host, when the show has one source and no scenes in use. A switch can take half a minute, so it answers at once with a task_id and the show as it is; task.get with that id carries this answer, with how long the outputs were off, once it is done."},
    {"name": "show.start", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/shows/{id}/start"), "summary": 'Start a stopped or failed show.'},
    {"name": "show.stats", "scope": "read", "mutating": False, "destructive": False, "rest": ("POST", "/api/v1/shows/stats"), "summary": "Health, input numbers and each output's numbers for many shows in one read, from what the station already holds, so it is cheap to call every second for two hundred shows. `fields` narrows it to health, input or outputs."},
    {"name": "show.stop", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/shows/{id}/stop"), "summary": 'Stop a show. It keeps its config, and stays stopped when the station starts again, until show.start.'},
    {"name": "snapshot.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/snapshot/{id}"), "summary": 'One JPEG: the whole contact sheet, the programme, or one source cut out of the mosaic.'},
    {"name": "source.add", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources"), "summary": 'Add a source while the mixer runs. Answers with the id it got and the whole source record.'},
    {"name": "source.audio.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources/{id}/audio"), "summary": "Move a source's audio: the fader, the mute, and for a superimposed page the balance between its own sound and the videos under it."},
    {"name": "source.duplicate", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources/{id}/duplicate"), "summary": 'Add another source like one the mixer has: the same address and settings under a new id. A client cannot do this with source.add, because the address it is shown has everything after the host cut off.'},
    {"name": "source.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/sources/{id}"), "summary": 'One source. Refused with the ids that exist when there is no such source.'},
    {"name": "source.group", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources/{id}/group"), "summary": 'Put sources in a tray folder. A tag for finding things, not a group on the canvas.'},
    {"name": "source.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/sources"), "summary": 'Every source, with its state, whether it has video and audio, and its fader.'},
    {"name": "source.missing", "scope": "read", "mutating": False, "destructive": False, "rest": ("POST", "/api/v1/sources/{id}/missing"), "summary": 'Sources that are not running, and why: failed, could not be started (with the error and the action that fixes it), removed, or unknown. Pass the ids a scene draws, or none for every one the mixer knows about.'},
    {"name": "source.remove", "scope": "operate", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/sources/{id}"), "summary": 'Remove a source. If it is on programme the mixer cuts to the slate first.'},
    {"name": "source.restart", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources/{id}/restart"), "summary": "Build a source's pipeline again now, rather than waiting for its next retry. For a source that could not be started or was removed, use source.restore."},
    {"name": "source.restore", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources/{id}/restore"), "summary": 'Put back a source that source.remove took away, as it was: same id, address, settings, fader and mute. The mixer remembers the last sixteen it removed, until it restarts.'},
    {"name": "source.seek", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources/{id}/seek"), "summary": 'Move a seekable source to a position. Answers with where it actually landed.'},
    {"name": "source.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources/{id}/set"), "summary": 'Change a running source: its name and colour, its params, or where it runs. The name and colour live on the scene document. Moving a source between the core, a sidecar and a node is `place`; the programme keeps its frame rate across the move and the compositor covers the swap.'},
    {"name": "task.cancel", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/tasks/{id}/cancel"), "summary": 'Ask a piece of long running work to stop. Cooperative: the answer says the request landed, not that the work has stopped yet.'},
    {"name": "task.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/tasks/{id}"), "summary": 'How a piece of long running work is getting on, and its answer once it has one.'},
    {"name": "task.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/tasks"), "summary": 'Every background job this core knows about, newest first.'},
    {"name": "tool.call", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/tool/call"), "summary": "Call one of a plugin's tools, in MCP's shape. The name is `<plugin>/<tool>`, or the bare tool name when only one plugin has it."},
    {"name": "vitals.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/vitals"), "summary": "This show's health (its state and alarms, null in the first second) and the thresholds they are judged by."},
    {"name": "vitals.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/vitals/set"), "summary": 'Change the alarm thresholds, or whether a mosaic is kept up for the black and freeze checks while nobody is looking. Fields left out keep their defaults; a duration of 0 switches that check off. Applies within a second.'},
)

EVENT_NAMES = (
    "snapshot",
    "program.took",
    "scene.patch",
    "preview.changed",
    "source.state",
    "source.position",
    "output.state",
    "adbreak.changed",
    "ui.changed",
    "hook.blocked",
    "media.changed",
    "channel.changed",
    "channel.removed",
    "channel.refused",
    "meters",
    "tally",
    "alert",
    "telemetry",
    "agent.state",
    "multiview.layout",
    "multiview.frame",
    "preview.frame",
    "resync",
    "flush",
    "rendition.plan",
    "governor.shed",
    "show.changed",
    "show.removed",
    "show.health",
    "health",
)

EXT_KEYS = {
    "multiview": {"value": '{fps: 1..30, width: 320..1920} or false', "implemented": True},
    "meters": {"value": 'true', "implemented": True},
    "tally": {"value": 'true', "implemented": True},
    "positions": {"value": 'true', "implemented": True},
    "thumb": {"value": '{fps}', "implemented": False},
    "preview": {"value": '{fps, width} or "full"', "implemented": True},
    "telemetry": {"value": '{hz: 1..10}', "implemented": False},
    "agent": {"value": 'true or thresholds', "implemented": False},
}

class GeneratedMethods:
    """One coroutine per protocol method, over whatever transport the subclass has.

    `Client` inherits this and provides `_call`. Nothing here knows how the
    call travels, so the same generated file serves the WebSocket client and
    anything else that can answer a JSON-RPC request.
    """

    async def _call(self, method: str, params: Dict[str, Any]) -> Any:
        raise NotImplementedError(
            "this object has no transport: build it with godwinmix.connect()"
        )

    async def adbreak_end(
        self,
    ) -> Dict[str, Any]:
        """Cut a running ad short, or disarm one that is scheduled."""
        params: Dict[str, Any] = {}
        return await self._call("adbreak.end", params)

    async def adbreak_start(
        self,
        uri: str,
        *,
        at_running_time_ms: Optional[int] = None,
        return_to: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Interrupt the programme with a clip, then rejoin live when it ends."""
        params: Dict[str, Any] = {}
        params["uri"] = uri
        if at_running_time_ms is not None:
            params["at_running_time_ms"] = at_running_time_ms
        if return_to is not None:
            params["return_to"] = return_to
        return await self._call("adbreak.start", params)

    async def agent_state(
        self,
        *,
        response_format: Optional[ResponseFormat] = None,
    ) -> Dict[str, Any]:
        """The compact document written for agents: the programme, each source's state and a motion score saying how much its picture is changing."""
        params: Dict[str, Any] = {}
        if response_format is not None:
            params["response_format"] = response_format
        return await self._call("agent.state", params)

    async def channel_add(
        self,
        name: str,
        *,
        app: Optional[str] = None,
        auto_source: Optional[bool] = None,
        key_mode: Optional[Union[KeyMode, None]] = None,
        protocols: Optional[List[ChannelProtocol]] = None,
    ) -> ChannelAdded:
        """Make a channel and its first key, which is in this answer. channel.key.reveal reads it again later."""
        params: Dict[str, Any] = {}
        params["name"] = name
        if app is not None:
            params["app"] = app
        if auto_source is not None:
            params["auto_source"] = auto_source
        if key_mode is not None:
            params["key_mode"] = key_mode
        if protocols is not None:
            params["protocols"] = protocols
        return await self._call("channel.add", params)

    async def channel_certificate_generate(
        self,
        *,
        names: Optional[List[str]] = None,
    ) -> CertificateInfo:
        """Make a self signed certificate for RTMPS, for this machine's addresses unless names are given. Encoders must be told to accept it; one from a certificate authority needs no such step."""
        params: Dict[str, Any] = {}
        if names is not None:
            params["names"] = names
        return await self._call("channel.certificate.generate", params)

    async def channel_certificate_set(
        self,
        cert: str,
        key: str,
    ) -> CertificateInfo:
        """Give RTMPS a certificate: the PEM of the certificate (and its chain) and of its private key, as a certificate authority issued them. Checked before it is kept; the key is sealed and never read back."""
        params: Dict[str, Any] = {}
        params["cert"] = cert
        params["key"] = key
        return await self._call("channel.certificate.set", params)

    async def channel_destination_add(
        self,
        id: str,
        platform: str,
        *,
        enabled: Optional[bool] = None,
        key: Optional[str] = None,
        label: Optional[str] = None,
        rendition: Optional[Union[RenditionChoice, None]] = None,
        server: Optional[str] = None,
        stream: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Send a channel's stream on to YouTube, Facebook, Twitch, an RTMP server or an SRT receiver as it arrives. Nothing is decoded or encoded. The key is write only."""
        params: Dict[str, Any] = {}
        params["id"] = id
        params["platform"] = platform
        if enabled is not None:
            params["enabled"] = enabled
        if key is not None:
            params["key"] = key
        if label is not None:
            params["label"] = label
        if rendition is not None:
            params["rendition"] = rendition
        if server is not None:
            params["server"] = server
        if stream is not None:
            params["stream"] = stream
        return await self._call("channel.destination.add", params)

    async def channel_destination_remove(
        self,
        destination: str,
        id: str,
    ) -> Dict[str, Any]:
        """Stop sending a channel's stream to one destination and forget it. The publisher and the other destinations are not touched."""
        params: Dict[str, Any] = {}
        params["destination"] = destination
        params["id"] = id
        return await self._call("channel.destination.remove", params)

    async def channel_destination_set(
        self,
        destination: str,
        id: str,
        *,
        enabled: Optional[bool] = None,
        key: Optional[str] = None,
        label: Optional[str] = None,
        rendition: Optional[Union[RenditionChoice, None]] = None,
        server: Optional[str] = None,
        stream: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Change one of a channel's destinations, naming only what moves: a new key, another server, which stream it sends, on or off. A key left out is kept."""
        params: Dict[str, Any] = {}
        params["destination"] = destination
        params["id"] = id
        if enabled is not None:
            params["enabled"] = enabled
        if key is not None:
            params["key"] = key
        if label is not None:
            params["label"] = label
        if rendition is not None:
            params["rendition"] = rendition
        if server is not None:
            params["server"] = server
        if stream is not None:
            params["stream"] = stream
        return await self._call("channel.destination.set", params)

    async def channel_get(
        self,
        id: str,
    ) -> Channel:
        """One channel."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("channel.get", params)

    async def channel_key_add(
        self,
        id: str,
        *,
        label: Optional[str] = None,
    ) -> KeyAdded:
        """Make another key for a channel, to give to one more person or encoder. The key is in this answer, and channel.key.reveal reads it again later."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if label is not None:
            params["label"] = label
        return await self._call("channel.key.add", params)

    async def channel_key_remove(
        self,
        id: str,
        key: str,
    ) -> Channel:
        """Take one key back. A publisher on air with it is cut off and the next one is turned away; the other keys are untouched."""
        params: Dict[str, Any] = {}
        params["id"] = id
        params["key"] = key
        return await self._call("channel.key.remove", params)

    async def channel_key_reveal(
        self,
        id: str,
        key: str,
    ) -> KeyRevealed:
        """Read one key of a channel back, to give it to an encoder again. Admin only; a list shows only the last four characters. Each read is logged with who asked, never with the key."""
        params: Dict[str, Any] = {}
        params["id"] = id
        params["key"] = key
        return await self._call("channel.key.reveal", params)

    async def channel_list(
        self,
    ) -> ChannelList:
        """Every channel with its keys (as hints), the address to publish to over each protocol it has on, and what is live on it; and which ingest ports are open and for which channels."""
        params: Dict[str, Any] = {}
        return await self._call("channel.list", params)

    async def channel_remove(
        self,
        id: str,
    ) -> ChannelRemoved:
        """Remove a channel and forget its keys. Sources it made that no scene holds go with it."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("channel.remove", params)

    async def channel_set(
        self,
        id: str,
        *,
        app: Optional[str] = None,
        auto_source: Optional[bool] = None,
        enabled: Optional[bool] = None,
        key_mode: Optional[Union[KeyMode, None]] = None,
        name: Optional[str] = None,
        protocols: Optional[List[ChannelProtocol]] = None,
        rtmps: Optional[Union[Rtmps, None]] = None,
    ) -> Channel:
        """Rename a channel, switch it on or off, or change its application name, whether its streams become sources, how its key is given, which protocols it takes (rtmp, srt, whip) or RTMPS and its port. A port opens when the first channel needs it and closes when the last one stops. Only what is named moves."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if app is not None:
            params["app"] = app
        if auto_source is not None:
            params["auto_source"] = auto_source
        if enabled is not None:
            params["enabled"] = enabled
        if key_mode is not None:
            params["key_mode"] = key_mode
        if name is not None:
            params["name"] = name
        if protocols is not None:
            params["protocols"] = protocols
        if rtmps is not None:
            params["rtmps"] = rtmps
        return await self._call("channel.set", params)

    async def codec_list(
        self,
    ) -> Dict[str, Any]:
        """Every codec and element in the catalogue, which of them this machine actually has, and what it would pick."""
        params: Dict[str, Any] = {}
        return await self._call("codec.list", params)

    async def config_get(
        self,
        *,
        keys: Optional[List[str]] = None,
    ) -> ConfigGetResult:
        """The mixer's settings: each key's value in the config file, its default, when a change to it takes effect, and which keys are waiting for a restart. Secrets say only whether one is set."""
        params: Dict[str, Any] = {}
        if keys is not None:
            params["keys"] = keys
        return await self._call("config.get", params)

    async def config_reset(
        self,
        keys: List[str],
        *,
        dry_run: Optional[bool] = None,
    ) -> ConfigSetResult:
        """Put settings back to their defaults by taking them out of the config file. Answers like config.set."""
        params: Dict[str, Any] = {}
        params["keys"] = keys
        if dry_run is not None:
            params["dry_run"] = dry_run
        return await self._call("config.reset", params)

    async def config_schema(
        self,
    ) -> Dict[str, Any]:
        """Every setting config.set takes, as one JSON Schema: type, title, description, default, range or choices, and x-gmx-applies (live, next_source or restart)."""
        params: Dict[str, Any] = {}
        return await self._call("config.schema", params)

    async def config_set(
        self,
        values: Dict[str, Any],
        *,
        dry_run: Optional[bool] = None,
    ) -> ConfigSetResult:
        """Change settings in the config file, keeping its comments. Every value is checked first and nothing is written unless all of them fit. Live keys take effect at once; the answer says which wait for the next source or a restart."""
        params: Dict[str, Any] = {}
        params["values"] = values
        if dry_run is not None:
            params["dry_run"] = dry_run
        return await self._call("config.set", params)

    async def core_api(
        self,
    ) -> Dict[str, Any]:
        """Every method, event and type as JSON Schema. The same document as protocol.json and `godwinmix --api-info`."""
        params: Dict[str, Any] = {}
        return await self._call("core.api", params)

    async def core_doctor(
        self,
    ) -> Dict[str, Any]:
        """The environment checks: GStreamer, the elements, the config, the disk and the ports. The same list `gmx doctor` prints."""
        params: Dict[str, Any] = {}
        return await self._call("core.doctor", params)

    async def core_info(
        self,
    ) -> CoreInfo:
        """What this core is, what it can do, and where its edges are."""
        params: Dict[str, Any] = {}
        return await self._call("core.info", params)

    async def core_restart(
        self,
    ) -> RestartAnswer:
        """Stop the mixer and have it started again, when something will start it again. On a supervised core (core.info restart.possible) it answers restarting: true and exits; the programme is off air until it is back. On a core started by hand it answers restarting: false, says how to restart it, and keeps running."""
        params: Dict[str, Any] = {}
        return await self._call("core.restart", params)

    async def core_session_log(
        self,
        *,
        secs: Optional[int] = None,
    ) -> Dict[str, Any]:
        """The append only record of everything that happened, back as far as you ask."""
        params: Dict[str, Any] = {}
        if secs is not None:
            params["secs"] = secs
        return await self._call("core.session_log", params)

    async def core_shutdown(
        self,
    ) -> Dict[str, Any]:
        """Stop the mixer, and with it the programme. Nothing else takes the show off air, so this is deliberately its own call."""
        params: Dict[str, Any] = {}
        return await self._call("core.shutdown", params)

    async def core_startup_report(
        self,
    ) -> Dict[str, Any]:
        """How long each stage of the start took, and what was over the 250 ms mark."""
        params: Dict[str, Any] = {}
        return await self._call("core.startup_report", params)

    async def core_status(
        self,
    ) -> MixerStatus:
        """The full state: programme, every source, every output, the multiview grid, the encoder backend and any ad break."""
        params: Dict[str, Any] = {}
        return await self._call("core.status", params)

    async def core_subscribe(
        self,
        *,
        events: Optional[List[str]] = None,
        ext: Optional[Ext] = None,
        show: Optional[str] = None,
    ) -> SubscribeResult:
        """Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush."""
        params: Dict[str, Any] = {}
        if events is not None:
            params["events"] = events
        if ext is not None:
            params["ext"] = ext
        if show is not None:
            params["show"] = show
        return await self._call("core.subscribe", params)

    async def device_discover(
        self,
        *,
        timeout_ms: Optional[int] = None,
    ) -> Dict[str, Any]:
        """Ask every device plugin what it can see: cameras, NDI senders, publishers. Each candidate's params are ready for source.add."""
        params: Dict[str, Any] = {}
        if timeout_ms is not None:
            params["timeout_ms"] = timeout_ms
        return await self._call("device.discover", params)

    async def filter_add(
        self,
        id: str,
        type: str,
        *,
        params: Optional[Dict[str, Any]] = None,
        programme: Optional[bool] = None,
        side: Optional[str] = None,
        source: Optional[str] = None,
    ) -> FilterRecord:
        """Hang a filter on one source or on the programme, live."""
        params: Dict[str, Any] = {}
        params["id"] = id
        params["type"] = type
        if params is not None:
            params["params"] = params
        if programme is not None:
            params["programme"] = programme
        if side is not None:
            params["side"] = side
        if source is not None:
            params["source"] = source
        return await self._call("filter.add", params)

    async def filter_list(
        self,
    ) -> FilterListing:
        """Every filter in place, with what it is and where it sits."""
        params: Dict[str, Any] = {}
        return await self._call("filter.list", params)

    async def filter_remove(
        self,
        id: str,
    ) -> FilterRemoved:
        """Take a filter out of the pipeline."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("filter.remove", params)

    async def filter_set(
        self,
        id: str,
        *,
        params: Optional[Dict[str, Any]] = None,
    ) -> FilterRecord:
        """Change a filter's settings in place. A filter that cannot take the change while running says so rather than being restarted behind your back."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if params is not None:
            params["params"] = params
        return await self._call("filter.set", params)

    async def governor_calibrate(
        self,
        *,
        confirm: Optional[bool] = None,
    ) -> CalibrateResult:
        """Measure this machine's encoders again, in the background, a few seconds of every core. Refused while anything is on air unless `confirm` is true."""
        params: Dict[str, Any] = {}
        if confirm is not None:
            params["confirm"] = confirm
        return await self._call("governor.calibrate", params)

    async def governor_status(
        self,
    ) -> GovernorStatus:
        """The resource governor: when this machine was measured, what is in use and free on the CPU and each GPU encoder, and what was shed to keep the programme whole."""
        params: Dict[str, Any] = {}
        return await self._call("governor.status", params)

    async def log_gst(
        self,
        categories: str,
        *,
        duration_secs: Optional[int] = None,
        instance: Optional[str] = None,
    ) -> LogGstResult:
        """Raise GStreamer's own debug categories for a while, then let them fall back on their own."""
        params: Dict[str, Any] = {}
        params["categories"] = categories
        if duration_secs is not None:
            params["duration_secs"] = duration_secs
        if instance is not None:
            params["instance"] = instance
        return await self._call("log.gst", params)

    async def log_levels(
        self,
    ) -> Dict[str, Any]:
        """Every log level override in force, and the GStreamer categories still raised."""
        params: Dict[str, Any] = {}
        return await self._call("log.levels", params)

    async def log_set(
        self,
        level: str,
        *,
        instance: Optional[str] = None,
        target: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Change one instance's or one module's log level while the mixer runs."""
        params: Dict[str, Any] = {}
        params["level"] = level
        if instance is not None:
            params["instance"] = instance
        if target is not None:
            params["target"] = target
        return await self._call("log.set", params)

    async def media_convert(
        self,
        name: str,
    ) -> ConversionState:
        """Transcode a library file to a web safe copy, in the background."""
        params: Dict[str, Any] = {}
        params["name"] = name
        return await self._call("media.convert", params)

    async def media_list(
        self,
    ) -> MediaListing:
        """The clips in the library, with durations and whether each has audio."""
        params: Dict[str, Any] = {}
        return await self._call("media.list", params)

    async def media_remove(
        self,
        name: str,
    ) -> Dict[str, Any]:
        """Delete a library file and its converted copy. Refused while it is a live source."""
        params: Dict[str, Any] = {}
        params["name"] = name
        return await self._call("media.remove", params)

    async def media_upload(
        self,
    ) -> Dict[str, Any]:
        """Stream a file into the library. HTTP only: the body is the file."""
        params: Dict[str, Any] = {}
        return await self._call("media.upload", params)

    async def node_discover(
        self,
        *,
        timeout_ms: Optional[int] = None,
    ) -> DiscoverAnswer:
        """Look for nodes on the local network over mDNS. A network without multicast finds nothing and the [nodes] table in the config is the way there."""
        params: Dict[str, Any] = {}
        if timeout_ms is not None:
            params["timeout_ms"] = timeout_ms
        return await self._call("node.discover", params)

    async def node_enrol(
        self,
        name: str,
        *,
        address: Optional[str] = None,
        ttl_secs: Optional[int] = None,
    ) -> Dict[str, Any]:
        """Mint a one time enrolment token for a node. The answer carries the command to run on the other machine. The token is good for one enrolment and expires."""
        params: Dict[str, Any] = {}
        params["name"] = name
        if address is not None:
            params["address"] = address
        if ttl_secs is not None:
            params["ttl_secs"] = ttl_secs
        return await self._call("node.enrol", params)

    async def node_get(
        self,
        id: str,
    ) -> NodeView:
        """One node: its clock offset, how long since its last heartbeat, the plugins it has, and the instances it is hosting."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("node.get", params)

    async def node_list(
        self,
    ) -> NodeListing:
        """Every node this core knows about: the ones connected now, the ones that have gone quiet, and the ones the config expects that have never dialled in."""
        params: Dict[str, Any] = {}
        return await self._call("node.list", params)

    async def node_remove(
        self,
        id: str,
    ) -> Dict[str, Any]:
        """Forget a node. Its bridge is closed, every token minted for a plugin on it is revoked, and its certificate stops working. Sources placed on it go to the slate until they are moved or the node enrols again."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("node.remove", params)

    async def output_add(
        self,
        id: str,
        *,
        policy: Optional[str] = None,
        rendition: Optional[Dict[str, Any]] = None,
        uri: Optional[str] = None,
        **extra: Any,
    ) -> OutputStatus:
        """Send the programme to another destination. The encoder is shared, so adding one costs nothing on air."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if policy is not None:
            params["policy"] = policy
        if rendition is not None:
            params["rendition"] = rendition
        if uri is not None:
            params["uri"] = uri
        params.update(extra)
        return await self._call("output.add", params)

    async def output_get(
        self,
        id: str,
    ) -> OutputStatus:
        """One destination."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("output.get", params)

    async def output_list(
        self,
    ) -> List[OutputStatus]:
        """Every destination, with its state, reconnect count and how much is buffered."""
        params: Dict[str, Any] = {}
        return await self._call("output.list", params)

    async def output_reconnect(
        self,
        id: str,
    ) -> OutputStatus:
        """Drop and re-establish one destination's connection now, without waiting for its reconnect policy."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("output.reconnect", params)

    async def output_remove(
        self,
        id: str,
    ) -> Dict[str, Any]:
        """Stop sending to a destination and forget it. Other outputs are unaffected."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("output.remove", params)

    async def output_set(
        self,
        id: str,
        *,
        policy: Optional[str] = None,
        queue_secs: Optional[float] = None,
        rendition: Optional[Dict[str, Any]] = None,
        uri: Optional[str] = None,
        **extra: Any,
    ) -> OutputStatus:
        """Change a destination in place: a new address with a new stream key, a new reconnect policy, a deeper outage buffer. The address is write only, so a client that only wants the buffer never has to hold the key."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if policy is not None:
            params["policy"] = policy
        if queue_secs is not None:
            params["queue_secs"] = queue_secs
        if rendition is not None:
            params["rendition"] = rendition
        if uri is not None:
            params["uri"] = uri
        params.update(extra)
        return await self._call("output.set", params)

    async def path_create(
        self,
        name: str,
        parent: str,
    ) -> PathListing:
        """Make one new folder inside a folder path.list shows, and list it. A folder that is already there is listed rather than refused."""
        params: Dict[str, Any] = {}
        params["name"] = name
        params["parent"] = parent
        return await self._call("path.create", params)

    async def path_list(
        self,
        *,
        path: Optional[str] = None,
    ) -> PathListing:
        """The folders in one folder on the mixer, and whether each is writable, for a folder picker. Only the home folder and the mixer's own folders are shown; files never are."""
        params: Dict[str, Any] = {}
        if path is not None:
            params["path"] = path
        return await self._call("path.list", params)

    async def pipeline_clock(
        self,
    ) -> Dict[str, Any]:
        """The clock every pipeline is running against, and how far each one has got."""
        params: Dict[str, Any] = {}
        return await self._call("pipeline.clock", params)

    async def pipeline_dot(
        self,
        *,
        name: Optional[str] = None,
    ) -> PipelineDot:
        """One pipeline as a graphviz graph: every element, every pad and the caps negotiated between them."""
        params: Dict[str, Any] = {}
        if name is not None:
            params["name"] = name
        return await self._call("pipeline.dot", params)

    async def pipeline_latency(
        self,
        *,
        name: Optional[str] = None,
    ) -> Dict[str, Any]:
        """How much delay one pipeline is carrying, and which stage put it there."""
        params: Dict[str, Any] = {}
        if name is not None:
            params["name"] = name
        return await self._call("pipeline.latency", params)

    async def pipeline_list(
        self,
    ) -> Dict[str, Any]:
        """Every pipeline running right now, by the name the other pipeline methods accept."""
        params: Dict[str, Any] = {}
        return await self._call("pipeline.list", params)

    async def pipeline_queues(
        self,
        *,
        name: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Every queue in one pipeline with how full it is, fullest first. A queue that stays full is where the trouble is."""
        params: Dict[str, Any] = {}
        if name is not None:
            params["name"] = name
        return await self._call("pipeline.queues", params)

    async def plugin_add(
        self,
        source: str,
    ) -> PluginRecord:
        """Install a plugin, while live, from any source form: a GitHub release (owner/repo), a git URL, cargo:, npm:, pypi:, a local directory, or a bare name looked up in the marketplaces this mixer knows. The signature and the api level are checked before anything is copied."""
        params: Dict[str, Any] = {}
        params["source"] = source
        return await self._call("plugin.add", params)

    async def plugin_describe(
        self,
        id: str,
    ) -> PluginDescription:
        """One plugin in full: its manifest, the settings schema of every provide, and the description from each SKILL.md."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("plugin.describe", params)

    async def plugin_disable(
        self,
        id: str,
    ) -> PluginRecord:
        """Turn a plugin off without uninstalling it. It registers nothing and runs no process until it is enabled again."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("plugin.disable", params)

    async def plugin_enable(
        self,
        id: str,
    ) -> PluginRecord:
        """Turn a plugin back on. It registers what it declares and its instances start."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("plugin.enable", params)

    async def plugin_list(
        self,
    ) -> PluginListing:
        """Every plugin installed, with what it provides and what each running instance is costing in cpu, memory, latency, dropped buffers and restarts."""
        params: Dict[str, Any] = {}
        return await self._call("plugin.list", params)

    async def plugin_reload(
        self,
        id: str,
    ) -> PluginRecord:
        """Read a plugin's directory again and swap its running instances one at a time, with the freeze frame covering each."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("plugin.reload", params)

    async def plugin_remove(
        self,
        id: str,
    ) -> PluginRemoved:
        """Uninstall a plugin and unwind everything it registered: its provides, its tools, its panels, its hooks and its discovery matchers."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("plugin.remove", params)

    async def plugin_search(
        self,
        *,
        term: Optional[str] = None,
    ) -> SearchResults:
        """Search every marketplace this mixer knows for a plugin, by name, description or kind. Answers what `gmx plugin add <name>` would install."""
        params: Dict[str, Any] = {}
        if term is not None:
            params["term"] = term
        return await self._call("plugin.search", params)

    async def plugin_settings_get(
        self,
        id: str,
    ) -> PluginSettings:
        """A plugin's settings as they stand, with its schema beside them."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("plugin.settings.get", params)

    async def plugin_settings_set(
        self,
        id: str,
        *,
        settings: Optional[Dict[str, Any]] = None,
    ) -> PluginSettings:
        """Change a plugin's settings. A plugin that cannot take a change while running says so rather than being restarted behind your back."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if settings is not None:
            params["settings"] = settings
        return await self._call("plugin.settings.set", params)

    async def plugin_stats(
        self,
    ) -> StatsListing:
        """Per instance cpu, memory, media latency, dropped buffers and restarts, refreshed once a second."""
        params: Dict[str, Any] = {}
        return await self._call("plugin.stats", params)

    async def plugin_update(
        self,
        id: str,
        *,
        source: Optional[str] = None,
    ) -> PluginUpdated:
        """Fetch a newer build of a plugin, install it beside the one that is running, and prove it starts. A build that does not answer `initialize` within ten seconds is rolled back and the plugin that was working stays working."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if source is not None:
            params["source"] = source
        return await self._call("plugin.update", params)

    async def preset_apply(
        self,
        name: str,
        *,
        dry_run: Optional[bool] = None,
        force: Optional[bool] = None,
        keep_sources: Optional[bool] = None,
    ) -> ApplyResult:
        """Put a preset on this core: its config, its scenes, its layout, its theme and its gallery mode. Pass dry_run to get the plan and write nothing."""
        params: Dict[str, Any] = {}
        params["name"] = name
        if dry_run is not None:
            params["dry_run"] = dry_run
        if force is not None:
            params["force"] = force
        if keep_sources is not None:
            params["keep_sources"] = keep_sources
        return await self._call("preset.apply", params)

    async def preset_list(
        self,
    ) -> Dict[str, Any]:
        """Every preset this core can apply: the six built in, plus anything installed beside the binary or under ~/.godwinmix/presets."""
        params: Dict[str, Any] = {}
        return await self._call("preset.list", params)

    async def preset_save(
        self,
        name: str,
        *,
        out: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Turn this core's working setup into a preset directory somebody else can apply. Stream keys and the control token are replaced with placeholders."""
        params: Dict[str, Any] = {}
        params["name"] = name
        if out is not None:
            params["out"] = out
        return await self._call("preset.save", params)

    async def preview_close(
        self,
        target: str,
    ) -> PreviewClosed:
        """Give up a raw frame socket. The socket goes when the last holder closes it."""
        params: Dict[str, Any] = {}
        params["target"] = target
        return await self._call("preview.close", params)

    async def preview_open(
        self,
        target: str,
    ) -> PreviewSocket:
        """Open a raw frame socket on this machine for a source or the programme, and answer with its path. No encode anywhere: a client on the same host reads the frames the mixer already has. Close it with preview.close."""
        params: Dict[str, Any] = {}
        params["target"] = target
        return await self._call("preview.open", params)

    async def program_get(
        self,
    ) -> ProgramState:
        """What is on air, the programme running time, and what revert would go back to."""
        params: Dict[str, Any] = {}
        return await self._call("program.get", params)

    async def program_golive(
        self,
        url: str,
        *,
        id: Optional[str] = None,
        rtmp: Optional[str] = None,
        superimpose: Optional[str] = None,
    ) -> GoLiveResult:
        """One call to put a web page on air: add the page, add the destination, and take the page as soon as it renders."""
        params: Dict[str, Any] = {}
        params["url"] = url
        if id is not None:
            params["id"] = id
        if rtmp is not None:
            params["rtmp"] = rtmp
        if superimpose is not None:
            params["superimpose"] = superimpose
        return await self._call("program.golive", params)

    async def program_history(
        self,
        *,
        limit: Optional[int] = None,
    ) -> List[TakeRecord]:
        """The last hundred takes, newest first, with the token that asked for each."""
        params: Dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        return await self._call("program.history", params)

    async def program_revert(
        self,
    ) -> ProgramState:
        """Take back to the shot before this one."""
        params: Dict[str, Any] = {}
        return await self._call("program.revert", params)

    async def program_take(
        self,
        *,
        at_running_time_ms: Optional[int] = None,
        scene: Optional[str] = None,
        source: Optional[str] = None,
        transition: Optional[Union[Transition, None]] = None,
    ) -> ProgramState:
        """Put a scene or a source on programme. The cut is instant and the outgoing stream is not disturbed."""
        params: Dict[str, Any] = {}
        if at_running_time_ms is not None:
            params["at_running_time_ms"] = at_running_time_ms
        if scene is not None:
            params["scene"] = scene
        if source is not None:
            params["source"] = source
        if transition is not None:
            params["transition"] = transition
        return await self._call("program.take", params)

    async def project_export(
        self,
        *,
        include_media: Optional[bool] = None,
        include_secrets: Optional[bool] = None,
        name: Optional[str] = None,
        page: Any = None,
    ) -> Dict[str, Any]:
        """This mixer as one project file: settings, sources, outputs and renditions, channels, scenes, the page's layout, and its clips by name and size. Keys only with include_secrets."""
        params: Dict[str, Any] = {}
        if include_media is not None:
            params["include_media"] = include_media
        if include_secrets is not None:
            params["include_secrets"] = include_secrets
        if name is not None:
            params["name"] = name
        if page is not None:
            params["page"] = page
        return await self._call("project.export", params)

    async def project_import(
        self,
        file: Any,
        *,
        dry_run: Optional[bool] = None,
        machine: Optional[bool] = None,
        mode: Optional[Mode] = None,
    ) -> Report:
        """Open a project file: answers with what it would change (dry_run is true unless false is sent), then replaces this mixer's setup or merges beside it. Says which settings wait for a restart."""
        params: Dict[str, Any] = {}
        params["file"] = file
        if dry_run is not None:
            params["dry_run"] = dry_run
        if machine is not None:
            params["machine"] = machine
        if mode is not None:
            params["mode"] = mode
        return await self._call("project.import", params)

    async def rendition_plan(
        self,
        *,
        scope: Optional[str] = None,
    ) -> PlanView:
        """What the planner built for every output that asked for a rendition: each node, what it serves, which encoder and why, and the totals."""
        params: Dict[str, Any] = {}
        if scope is not None:
            params["scope"] = scope
        return await self._call("rendition.plan", params)

    async def rendition_presets(
        self,
    ) -> PresetsResult:
        """Every rendition preset, priced on this machine by the governor. One this machine cannot make says so, with why."""
        params: Dict[str, Any] = {}
        return await self._call("rendition.presets", params)

    async def scene_add(
        self,
        name: str,
        *,
        color: Optional[str] = None,
    ) -> SceneView:
        """Make an empty scene, or one built from a set of sources."""
        params: Dict[str, Any] = {}
        params["name"] = name
        if color is not None:
            params["color"] = color
        return await self._call("scene.add", params)

    async def scene_apply_graphic(
        self,
        graphic: str,
        *,
        frame: Optional[bool] = None,
        item: Optional[str] = None,
        play: Optional[bool] = None,
        stop: Optional[bool] = None,
        values: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Fill a graphic that is on a scene, by field name, and optionally play it on or take it off. Answers with the records and, if asked, a still."""
        params: Dict[str, Any] = {}
        params["graphic"] = graphic
        if frame is not None:
            params["frame"] = frame
        if item is not None:
            params["item"] = item
        if play is not None:
            params["play"] = play
        if stop is not None:
            params["stop"] = stop
        if values is not None:
            params["values"] = values
        return await self._call("scene.apply_graphic", params)

    async def scene_apply_layout(
        self,
        layout: str,
        *,
        duration_ms: Optional[int] = None,
        easing: Optional[str] = None,
        name: Optional[str] = None,
        scene: Optional[str] = None,
        values: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Apply a layout, making a scene or reshaping one that exists. Applying onto an existing scene keeps the item ids, so the change is a ramp and not a cut."""
        params: Dict[str, Any] = {}
        params["layout"] = layout
        if duration_ms is not None:
            params["duration_ms"] = duration_ms
        if easing is not None:
            params["easing"] = easing
        if name is not None:
            params["name"] = name
        if scene is not None:
            params["scene"] = scene
        if values is not None:
            params["values"] = values
        return await self._call("scene.apply_layout", params)

    async def scene_create_from(
        self,
        sources: List[str],
        *,
        layout: Optional[str] = None,
        name: Optional[str] = None,
    ) -> SceneView:
        """A scene from a set of sources, laid out by the built in layout for that count (full, two-box, three-box, quad, then a grid) or by a named one."""
        params: Dict[str, Any] = {}
        params["sources"] = sources
        if layout is not None:
            params["layout"] = layout
        if name is not None:
            params["name"] = name
        return await self._call("scene.create_from", params)

    async def scene_duplicate(
        self,
        scene: str,
        *,
        name: Optional[str] = None,
    ) -> SceneView:
        """A copy of a scene with new ids throughout, so editing the copy cannot touch the original."""
        params: Dict[str, Any] = {}
        params["scene"] = scene
        if name is not None:
            params["name"] = name
        return await self._call("scene.duplicate", params)

    async def scene_edit_apply(
        self,
        draft: str,
    ) -> Dict[str, Any]:
        """Write a draft back into the live document."""
        params: Dict[str, Any] = {}
        params["draft"] = draft
        return await self._call("scene.edit.apply", params)

    async def scene_edit_begin(
        self,
        scene: str,
        *,
        live: Optional[bool] = None,
    ) -> DraftRecord:
        """Take a working copy of a scene. Editing is off air by default: the draft is written back on the next take of that scene, or when you apply it."""
        params: Dict[str, Any] = {}
        params["scene"] = scene
        if live is not None:
            params["live"] = live
        return await self._call("scene.edit.begin", params)

    async def scene_edit_discard(
        self,
        draft: str,
    ) -> Dict[str, Any]:
        """Throw a draft away. The live document is untouched."""
        params: Dict[str, Any] = {}
        params["draft"] = draft
        return await self._call("scene.edit.discard", params)

    async def scene_export(
        self,
        *,
        collection: Optional[str] = None,
        format: Optional[str] = None,
        path: Optional[str] = None,
    ) -> Dict[str, Any]:
        """The whole collection: as JSON, or as a zip bundle carrying its assets with a hash each, which is what you send somebody."""
        params: Dict[str, Any] = {}
        if collection is not None:
            params["collection"] = collection
        if format is not None:
            params["format"] = format
        if path is not None:
            params["path"] = path
        return await self._call("scene.export", params)

    async def scene_get(
        self,
        scene: str,
    ) -> SceneView:
        """One scene: its records and where every item actually lands on the canvas."""
        params: Dict[str, Any] = {}
        params["scene"] = scene
        return await self._call("scene.get", params)

    async def scene_graphic_list(
        self,
    ) -> GraphicListing:
        """Every graphic template this core can place, with what each one takes."""
        params: Dict[str, Any] = {}
        return await self._call("scene.graphic.list", params)

    async def scene_history_mark(
        self,
        *,
        label: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Group the changes that follow into one undo step, until the next mark. This is what makes a drag of forty moves one Ctrl+Z."""
        params: Dict[str, Any] = {}
        if label is not None:
            params["label"] = label
        return await self._call("scene.history.mark", params)

    async def scene_import(
        self,
        path: str,
    ) -> ImportedReport:
        """Read a collection bundle, a zip or the directory it unpacks to, and add its scenes to this one. Answers with a relink report for any asset that did not come across."""
        params: Dict[str, Any] = {}
        params["path"] = path
        return await self._call("scene.import", params)

    async def scene_import_obs(
        self,
        *,
        add_sources: Optional[bool] = None,
        content: Optional[str] = None,
        path: Optional[str] = None,
    ) -> ImportReport:
        """Read an OBS Studio scene collection and add its scenes to this one. Send the file's text as `content` (what a page's file picker reads) or a `path` on the mixer's machine. With `add_sources: true` the sources the scenes draw are added through source.add, and the answer says which were added and why any were not."""
        params: Dict[str, Any] = {}
        if add_sources is not None:
            params["add_sources"] = add_sources
        if content is not None:
            params["content"] = content
        if path is not None:
            params["path"] = path
        return await self._call("scene.import.obs", params)

    async def scene_item_add(
        self,
        content: Any,
        scene: str,
        *,
        draft: Optional[str] = None,
        name: Optional[str] = None,
        transform: Any = None,
    ) -> Dict[str, Any]:
        """Put something on a scene's canvas. With no transform it lands in the next free cell, so a drop never needs a dialog."""
        params: Dict[str, Any] = {}
        params["content"] = content
        params["scene"] = scene
        if draft is not None:
            params["draft"] = draft
        if name is not None:
            params["name"] = name
        if transform is not None:
            params["transform"] = transform
        return await self._call("scene.item.add", params)

    async def scene_item_align(
        self,
        items: List[str],
        scene: str,
        *,
        axis: Optional[str] = None,
        cols: Optional[int] = None,
        draft: Optional[str] = None,
        duration_ms: Optional[int] = None,
        easing: Optional[str] = None,
        edge: Optional[str] = None,
        name: Optional[str] = None,
        seq: Optional[int] = None,
        to: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Line items up on an edge: left, right, top, bottom, center-x or center-y."""
        params: Dict[str, Any] = {}
        params["items"] = items
        params["scene"] = scene
        if axis is not None:
            params["axis"] = axis
        if cols is not None:
            params["cols"] = cols
        if draft is not None:
            params["draft"] = draft
        if duration_ms is not None:
            params["duration_ms"] = duration_ms
        if easing is not None:
            params["easing"] = easing
        if edge is not None:
            params["edge"] = edge
        if name is not None:
            params["name"] = name
        if seq is not None:
            params["seq"] = seq
        if to is not None:
            params["to"] = to
        return await self._call("scene.item.align", params)

    async def scene_item_arrange_grid(
        self,
        items: List[str],
        scene: str,
        *,
        axis: Optional[str] = None,
        cols: Optional[int] = None,
        draft: Optional[str] = None,
        duration_ms: Optional[int] = None,
        easing: Optional[str] = None,
        edge: Optional[str] = None,
        name: Optional[str] = None,
        seq: Optional[int] = None,
        to: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Lay items out in a grid of `cols` columns."""
        params: Dict[str, Any] = {}
        params["items"] = items
        params["scene"] = scene
        if axis is not None:
            params["axis"] = axis
        if cols is not None:
            params["cols"] = cols
        if draft is not None:
            params["draft"] = draft
        if duration_ms is not None:
            params["duration_ms"] = duration_ms
        if easing is not None:
            params["easing"] = easing
        if edge is not None:
            params["edge"] = edge
        if name is not None:
            params["name"] = name
        if seq is not None:
            params["seq"] = seq
        if to is not None:
            params["to"] = to
        return await self._call("scene.item.arrange_grid", params)

    async def scene_item_bind(
        self,
        item: str,
        param: str,
        prop: str,
        scene: str,
        *,
        draft: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Bind a geometry property to an expression over the collection's parameters, so changing a number moves everything that follows it."""
        params: Dict[str, Any] = {}
        params["item"] = item
        params["param"] = param
        params["prop"] = prop
        params["scene"] = scene
        if draft is not None:
            params["draft"] = draft
        return await self._call("scene.item.bind", params)

    async def scene_item_copy(
        self,
        item: str,
        scene: str,
        to_scene: str,
    ) -> Dict[str, Any]:
        """Copy an item into another scene. The copy keeps the transform and the filters and gets a new id."""
        params: Dict[str, Any] = {}
        params["item"] = item
        params["scene"] = scene
        params["to_scene"] = to_scene
        return await self._call("scene.item.copy", params)

    async def scene_item_cover_canvas(
        self,
        items: List[str],
        scene: str,
        *,
        axis: Optional[str] = None,
        cols: Optional[int] = None,
        draft: Optional[str] = None,
        duration_ms: Optional[int] = None,
        easing: Optional[str] = None,
        edge: Optional[str] = None,
        name: Optional[str] = None,
        seq: Optional[int] = None,
        to: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Put items over the whole canvas, filling it and letting the overflow go."""
        params: Dict[str, Any] = {}
        params["items"] = items
        params["scene"] = scene
        if axis is not None:
            params["axis"] = axis
        if cols is not None:
            params["cols"] = cols
        if draft is not None:
            params["draft"] = draft
        if duration_ms is not None:
            params["duration_ms"] = duration_ms
        if easing is not None:
            params["easing"] = easing
        if edge is not None:
            params["edge"] = edge
        if name is not None:
            params["name"] = name
        if seq is not None:
            params["seq"] = seq
        if to is not None:
            params["to"] = to
        return await self._call("scene.item.cover_canvas", params)

    async def scene_item_distribute(
        self,
        items: List[str],
        scene: str,
        *,
        axis: Optional[str] = None,
        cols: Optional[int] = None,
        draft: Optional[str] = None,
        duration_ms: Optional[int] = None,
        easing: Optional[str] = None,
        edge: Optional[str] = None,
        name: Optional[str] = None,
        seq: Optional[int] = None,
        to: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Space items evenly between the two on the ends, horizontally or vertically."""
        params: Dict[str, Any] = {}
        params["items"] = items
        params["scene"] = scene
        if axis is not None:
            params["axis"] = axis
        if cols is not None:
            params["cols"] = cols
        if draft is not None:
            params["draft"] = draft
        if duration_ms is not None:
            params["duration_ms"] = duration_ms
        if easing is not None:
            params["easing"] = easing
        if edge is not None:
            params["edge"] = edge
        if name is not None:
            params["name"] = name
        if seq is not None:
            params["seq"] = seq
        if to is not None:
            params["to"] = to
        return await self._call("scene.item.distribute", params)

    async def scene_item_filter_add(
        self,
        item: str,
        scene: str,
        type: str,
        *,
        draft: Optional[str] = None,
        name: Optional[str] = None,
        params: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Hang a filter on one item, so a camera keyed in one scene is not keyed in all of them."""
        params: Dict[str, Any] = {}
        params["item"] = item
        params["scene"] = scene
        params["type"] = type
        if draft is not None:
            params["draft"] = draft
        if name is not None:
            params["name"] = name
        if params is not None:
            params["params"] = params
        return await self._call("scene.item.filter.add", params)

    async def scene_item_filter_remove(
        self,
        filter: str,
        item: str,
        scene: str,
        *,
        draft: Optional[str] = None,
        enabled: Optional[bool] = None,
        params: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Take a filter off an item."""
        params: Dict[str, Any] = {}
        params["filter"] = filter
        params["item"] = item
        params["scene"] = scene
        if draft is not None:
            params["draft"] = draft
        if enabled is not None:
            params["enabled"] = enabled
        if params is not None:
            params["params"] = params
        return await self._call("scene.item.filter.remove", params)

    async def scene_item_filter_set(
        self,
        filter: str,
        item: str,
        scene: str,
        *,
        draft: Optional[str] = None,
        enabled: Optional[bool] = None,
        params: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Change one of an item's filters, or turn it off without taking it out."""
        params: Dict[str, Any] = {}
        params["filter"] = filter
        params["item"] = item
        params["scene"] = scene
        if draft is not None:
            params["draft"] = draft
        if enabled is not None:
            params["enabled"] = enabled
        if params is not None:
            params["params"] = params
        return await self._call("scene.item.filter.set", params)

    async def scene_item_fit_to_canvas(
        self,
        items: List[str],
        scene: str,
        *,
        axis: Optional[str] = None,
        cols: Optional[int] = None,
        draft: Optional[str] = None,
        duration_ms: Optional[int] = None,
        easing: Optional[str] = None,
        edge: Optional[str] = None,
        name: Optional[str] = None,
        seq: Optional[int] = None,
        to: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Put items over the whole canvas, keeping their aspect ratio inside it."""
        params: Dict[str, Any] = {}
        params["items"] = items
        params["scene"] = scene
        if axis is not None:
            params["axis"] = axis
        if cols is not None:
            params["cols"] = cols
        if draft is not None:
            params["draft"] = draft
        if duration_ms is not None:
            params["duration_ms"] = duration_ms
        if easing is not None:
            params["easing"] = easing
        if edge is not None:
            params["edge"] = edge
        if name is not None:
            params["name"] = name
        if seq is not None:
            params["seq"] = seq
        if to is not None:
            params["to"] = to
        return await self._call("scene.item.fit_to_canvas", params)

    async def scene_item_group(
        self,
        items: List[str],
        scene: str,
        *,
        axis: Optional[str] = None,
        cols: Optional[int] = None,
        draft: Optional[str] = None,
        duration_ms: Optional[int] = None,
        easing: Optional[str] = None,
        edge: Optional[str] = None,
        name: Optional[str] = None,
        seq: Optional[int] = None,
        to: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Put items into a group. The picture does not change."""
        params: Dict[str, Any] = {}
        params["items"] = items
        params["scene"] = scene
        if axis is not None:
            params["axis"] = axis
        if cols is not None:
            params["cols"] = cols
        if draft is not None:
            params["draft"] = draft
        if duration_ms is not None:
            params["duration_ms"] = duration_ms
        if easing is not None:
            params["easing"] = easing
        if edge is not None:
            params["edge"] = edge
        if name is not None:
            params["name"] = name
        if seq is not None:
            params["seq"] = seq
        if to is not None:
            params["to"] = to
        return await self._call("scene.item.group", params)

    async def scene_item_match_size(
        self,
        items: List[str],
        scene: str,
        *,
        axis: Optional[str] = None,
        cols: Optional[int] = None,
        draft: Optional[str] = None,
        duration_ms: Optional[int] = None,
        easing: Optional[str] = None,
        edge: Optional[str] = None,
        name: Optional[str] = None,
        seq: Optional[int] = None,
        to: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Make items the same size as another one."""
        params: Dict[str, Any] = {}
        params["items"] = items
        params["scene"] = scene
        if axis is not None:
            params["axis"] = axis
        if cols is not None:
            params["cols"] = cols
        if draft is not None:
            params["draft"] = draft
        if duration_ms is not None:
            params["duration_ms"] = duration_ms
        if easing is not None:
            params["easing"] = easing
        if edge is not None:
            params["edge"] = edge
        if name is not None:
            params["name"] = name
        if seq is not None:
            params["seq"] = seq
        if to is not None:
            params["to"] = to
        return await self._call("scene.item.match_size", params)

    async def scene_item_move(
        self,
        item: str,
        scene: str,
        to_scene: str,
    ) -> Dict[str, Any]:
        """Move an item to another scene, keeping its transform and filters."""
        params: Dict[str, Any] = {}
        params["item"] = item
        params["scene"] = scene
        params["to_scene"] = to_scene
        return await self._call("scene.item.move", params)

    async def scene_item_remove(
        self,
        item: str,
        scene: str,
        *,
        draft: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Take an item off a scene."""
        params: Dict[str, Any] = {}
        params["item"] = item
        params["scene"] = scene
        if draft is not None:
            params["draft"] = draft
        return await self._call("scene.item.remove", params)

    async def scene_item_reorder(
        self,
        item: str,
        scene: str,
        *,
        after: Optional[str] = None,
        before: Optional[str] = None,
        draft: Optional[str] = None,
        seq: Optional[int] = None,
    ) -> Dict[str, Any]:
        """Move an item up or down the stack, between two named neighbours."""
        params: Dict[str, Any] = {}
        params["item"] = item
        params["scene"] = scene
        if after is not None:
            params["after"] = after
        if before is not None:
            params["before"] = before
        if draft is not None:
            params["draft"] = draft
        if seq is not None:
            params["seq"] = seq
        return await self._call("scene.item.reorder", params)

    async def scene_item_schema(
        self,
        type: str,
    ) -> Dict[str, Any]:
        """What one item type takes: a graphic's OGraf schema, or a source or filter plugin's settings schema. The same JSON Schema every client renders an inspector from."""
        params: Dict[str, Any] = {}
        params["type"] = type
        return await self._call("scene.item.schema", params)

    async def scene_item_set(
        self,
        item: str,
        props: Dict[str, Any],
        scene: str,
        *,
        draft: Optional[str] = None,
        duration_ms: Optional[int] = None,
        easing: Optional[str] = None,
        seq: Optional[int] = None,
    ) -> Dict[str, Any]:
        """Assign an item's properties. Only the keys named move; the rest are left alone, so calling it twice with the same body changes nothing the second time."""
        params: Dict[str, Any] = {}
        params["item"] = item
        params["props"] = props
        params["scene"] = scene
        if draft is not None:
            params["draft"] = draft
        if duration_ms is not None:
            params["duration_ms"] = duration_ms
        if easing is not None:
            params["easing"] = easing
        if seq is not None:
            params["seq"] = seq
        return await self._call("scene.item.set", params)

    async def scene_item_ungroup(
        self,
        item: str,
        scene: str,
        *,
        draft: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Take a group apart, leaving every child exactly where it looked."""
        params: Dict[str, Any] = {}
        params["item"] = item
        params["scene"] = scene
        if draft is not None:
            params["draft"] = draft
        return await self._call("scene.item.ungroup", params)

    async def scene_layout_copy(
        self,
        scene: str,
    ) -> Layout:
        """Read one scene's geometry, to paste onto another."""
        params: Dict[str, Any] = {}
        params["scene"] = scene
        return await self._call("scene.layout.copy", params)

    async def scene_layout_list(
        self,
    ) -> LayoutListing:
        """The layouts that ship with the core, with the parameters each one takes."""
        params: Dict[str, Any] = {}
        return await self._call("scene.layout.list", params)

    async def scene_layout_paste(
        self,
        scene: str,
        *,
        layout: Any = None,
        match: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Put one scene's geometry onto another's items, matched by name first and slot order second. Items that match nothing are left alone."""
        params: Dict[str, Any] = {}
        params["scene"] = scene
        if layout is not None:
            params["layout"] = layout
        if match is not None:
            params["match"] = match
        return await self._call("scene.layout.paste", params)

    async def scene_list(
        self,
    ) -> SceneListing:
        """Every scene in the collection, with how many items it has, the sources it draws and whether it is armed."""
        params: Dict[str, Any] = {}
        return await self._call("scene.list", params)

    async def scene_params_get(
        self,
    ) -> Dict[str, Any]:
        """The collection's typed parameters, readable without their values, so a client discovers what is fillable before filling it."""
        params: Dict[str, Any] = {}
        return await self._call("scene.params.get", params)

    async def scene_params_set(
        self,
        *,
        scene: Optional[str] = None,
        values: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Set the collection's parameter values, declaring any that are new. A `{{name}}` in any string property of any item follows them, so one call changes every lower third that uses it."""
        params: Dict[str, Any] = {}
        if scene is not None:
            params["scene"] = scene
        if values is not None:
            params["values"] = values
        return await self._call("scene.params.set", params)

    async def scene_preview_frame(
        self,
        *,
        width: Optional[int] = None,
    ) -> Dict[str, Any]:
        """A still of the armed scene as base64 JPEG, the floor every client has."""
        params: Dict[str, Any] = {}
        if width is not None:
            params["width"] = width
        return await self._call("scene.preview.frame", params)

    async def scene_preview_set(
        self,
        *,
        draft: Optional[str] = None,
        scene: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Arm a scene. The armed scene is the preview, and program.take with no argument takes it."""
        params: Dict[str, Any] = {}
        if draft is not None:
            params["draft"] = draft
        if scene is not None:
            params["scene"] = scene
        return await self._call("scene.preview.set", params)

    async def scene_redo(
        self,
    ) -> HistoryStep:
        """Put back what undo took away."""
        params: Dict[str, Any] = {}
        return await self._call("scene.redo", params)

    async def scene_remove(
        self,
        scene: str,
    ) -> SceneRemoved:
        """Delete a scene. What is on air is not touched."""
        params: Dict[str, Any] = {}
        params["scene"] = scene
        return await self._call("scene.remove", params)

    async def scene_rename(
        self,
        scene: str,
        *,
        color: Optional[str] = None,
        name: Optional[str] = None,
    ) -> SceneView:
        """Change a scene's name, its colour, or both. Names and colours live on the document, so every client, the tally and an agent see the same ones."""
        params: Dict[str, Any] = {}
        params["scene"] = scene
        if color is not None:
            params["color"] = color
        if name is not None:
            params["name"] = name
        return await self._call("scene.rename", params)

    async def scene_transaction_abort(
        self,
    ) -> Dict[str, Any]:
        """Throw the batch away. The document goes back to where it was when the batch opened."""
        params: Dict[str, Any] = {}
        return await self._call("scene.transaction.abort", params)

    async def scene_transaction_begin(
        self,
    ) -> Dict[str, Any]:
        """Start a batch. Everything until the commit applies on one frame or not at all, and undoes in one step."""
        params: Dict[str, Any] = {}
        return await self._call("scene.transaction.begin", params)

    async def scene_transaction_commit(
        self,
    ) -> Dict[str, Any]:
        """Apply the batch."""
        params: Dict[str, Any] = {}
        return await self._call("scene.transaction.commit", params)

    async def scene_undo(
        self,
    ) -> HistoryStep:
        """Undo the last change. A drag marked with scene.history.mark undoes as one step."""
        params: Dict[str, Any] = {}
        return await self._call("scene.undo", params)

    async def scene_validate(
        self,
        *,
        scene: Optional[str] = None,
    ) -> Validation:
        """Overlaps, items off the canvas, safe area breaches and missing sources: what to fix before saying a scene is done."""
        params: Dict[str, Any] = {}
        if scene is not None:
            params["scene"] = scene
        return await self._call("scene.validate", params)

    async def show_add(
        self,
        name: str,
        *,
        compositing: Optional[bool] = None,
        from_: Optional[Union[ShowFrom, None]] = None,
        input: Optional[Union[InputSpec, None]] = None,
        outputs: Optional[List[ShowOutputSpec]] = None,
    ) -> Show:
        """Make another show and start it: empty, a copy of a show (without its outputs, so nothing goes out twice), or from a project file."""
        params: Dict[str, Any] = {}
        params["name"] = name
        if compositing is not None:
            params["compositing"] = compositing
        if from_ is not None:
            params["from"] = from_
        if input is not None:
            params["input"] = input
        if outputs is not None:
            params["outputs"] = outputs
        return await self._call("show.add", params)

    async def show_add_many(
        self,
        shows: List[ShowAdd],
        *,
        dry_run: Optional[bool] = None,
    ) -> ShowAddManyResult:
        """Make many shows in one call, such as every channel of a headend. The whole batch is checked first. With dry_run (the default) nothing is made: the answer says what would be, what its renditions would cost and whether the governor would admit them. Without it, every show that fits is made and the rest are refused with why; a show is made whole or not at all."""
        params: Dict[str, Any] = {}
        params["shows"] = shows
        if dry_run is not None:
            params["dry_run"] = dry_run
        return await self._call("show.add_many", params)

    async def show_list(
        self,
    ) -> ShowList:
        """Every show on this machine: its name, whether it is running, what is on air, what its outputs send and what its process costs. `current` is the show a client reaches when it names none."""
        params: Dict[str, Any] = {}
        return await self._call("show.list", params)

    async def show_output_add(
        self,
        id: str,
        *,
        enabled: Optional[bool] = None,
        key: Optional[str] = None,
        label: Optional[str] = None,
        output: Optional[str] = None,
        params: Optional[HlsOutputParams] = None,
        platform: Optional[str] = None,
        rendition: Optional[Union[RenditionChoice, None]] = None,
        uri: Optional[str] = None,
    ) -> Show:
        """Send a show without compositing to another place: an address (SRT, RTMP, UDP, RTP or RIST), a platform and its key, or hls://<name> to serve it as HLS from this port. Left without a rendition it copies the input's bytes; with one it is planned and admitted by the governor. The key is write only."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if enabled is not None:
            params["enabled"] = enabled
        if key is not None:
            params["key"] = key
        if label is not None:
            params["label"] = label
        if output is not None:
            params["output"] = output
        if params is not None:
            params["params"] = params
        if platform is not None:
            params["platform"] = platform
        if rendition is not None:
            params["rendition"] = rendition
        if uri is not None:
            params["uri"] = uri
        return await self._call("show.output.add", params)

    async def show_output_remove(
        self,
        id: str,
        output: str,
    ) -> Show:
        """Stop one output of a show without compositing and forget it, key and all."""
        params: Dict[str, Any] = {}
        params["id"] = id
        params["output"] = output
        return await self._call("show.output.remove", params)

    async def show_output_set(
        self,
        id: str,
        output: str,
        *,
        enabled: Optional[bool] = None,
        key: Optional[str] = None,
        label: Optional[str] = None,
        params: Optional[HlsOutputParams] = None,
        rendition: Optional[Union[RenditionChoice, None]] = None,
        uri: Optional[str] = None,
    ) -> Show:
        """Change one output of a show without compositing, naming only what moves: another address, a new key, on or off, copy or a rendition."""
        params: Dict[str, Any] = {}
        params["id"] = id
        params["output"] = output
        if enabled is not None:
            params["enabled"] = enabled
        if key is not None:
            params["key"] = key
        if label is not None:
            params["label"] = label
        if params is not None:
            params["params"] = params
        if rendition is not None:
            params["rendition"] = rendition
        if uri is not None:
            params["uri"] = uri
        return await self._call("show.output.set", params)

    async def show_remove(
        self,
        id: str,
    ) -> ShowRemoved:
        """Stop a show and remove it with its folder. Refused for the last show and for main, the show the station was started with."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("show.remove", params)

    async def show_remove_many(
        self,
        ids: List[str],
    ) -> ShowRemoveManyResult:
        """Stop and remove many shows. Each id that cannot go (main, or one not there) is refused with why, and the rest go."""
        params: Dict[str, Any] = {}
        params["ids"] = ids
        return await self._call("show.remove_many", params)

    async def show_rename(
        self,
        id: str,
        name: str,
    ) -> Show:
        """Give a show another name. Its id stays."""
        params: Dict[str, Any] = {}
        params["id"] = id
        params["name"] = name
        return await self._call("show.rename", params)

    async def show_set(
        self,
        id: str,
        *,
        alarms: Optional[Union[AlarmSettings, None]] = None,
        compositing: Optional[bool] = None,
        input: Optional[Union[InputSpec, None]] = None,
        name: Optional[str] = None,
    ) -> ShowSetResult:
        """Change a show's name, its input, or whether it composites. Turning compositing on starts a show process whose one source is the input and moves the outputs to it; turning it off hands them back to the direct host, when the show has one source and no scenes in use. A switch can take half a minute, so it answers at once with a task_id and the show as it is; task.get with that id carries this answer, with how long the outputs were off, once it is done."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if alarms is not None:
            params["alarms"] = alarms
        if compositing is not None:
            params["compositing"] = compositing
        if input is not None:
            params["input"] = input
        if name is not None:
            params["name"] = name
        return await self._call("show.set", params)

    async def show_start(
        self,
        id: str,
    ) -> Show:
        """Start a stopped or failed show."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("show.start", params)

    async def show_stats(
        self,
        *,
        fields: Optional[List[str]] = None,
        ids: Optional[List[str]] = None,
    ) -> ShowStatsList:
        """Health, input numbers and each output's numbers for many shows in one read, from what the station already holds, so it is cheap to call every second for two hundred shows. `fields` narrows it to health, input or outputs."""
        params: Dict[str, Any] = {}
        if fields is not None:
            params["fields"] = fields
        if ids is not None:
            params["ids"] = ids
        return await self._call("show.stats", params)

    async def show_stop(
        self,
        id: str,
    ) -> Show:
        """Stop a show. It keeps its config, and stays stopped when the station starts again, until show.start."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("show.stop", params)

    async def snapshot_get(
        self,
        id: str,
        *,
        allow_large: Optional[bool] = None,
        force: Optional[bool] = None,
        width: Optional[int] = None,
    ) -> Dict[str, Any]:
        """One JPEG: the whole contact sheet, the programme, or one source cut out of the mosaic."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if allow_large is not None:
            params["allow_large"] = allow_large
        if force is not None:
            params["force"] = force
        if width is not None:
            params["width"] = width
        return await self._call("snapshot.get", params)

    async def source_add(
        self,
        uri: str,
        *,
        id: Optional[str] = None,
        kind: Optional[str] = None,
        name: Optional[str] = None,
        superimpose: Optional[str] = None,
        **extra: Any,
    ) -> SourceStatus:
        """Add a source while the mixer runs. Answers with the id it got and the whole source record."""
        params: Dict[str, Any] = {}
        params["uri"] = uri
        if id is not None:
            params["id"] = id
        if kind is not None:
            params["kind"] = kind
        if name is not None:
            params["name"] = name
        if superimpose is not None:
            params["superimpose"] = superimpose
        params.update(extra)
        return await self._call("source.add", params)

    async def source_audio_set(
        self,
        id: str,
        *,
        gain: Optional[float] = None,
        media: Optional[List[Optional[float]]] = None,
        muted: Optional[bool] = None,
        page: Optional[float] = None,
    ) -> SourceAudioState:
        """Move a source's audio: the fader, the mute, and for a superimposed page the balance between its own sound and the videos under it."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if gain is not None:
            params["gain"] = gain
        if media is not None:
            params["media"] = media
        if muted is not None:
            params["muted"] = muted
        if page is not None:
            params["page"] = page
        return await self._call("source.audio.set", params)

    async def source_duplicate(
        self,
        id: str,
        *,
        name: Optional[str] = None,
        new_id: Optional[str] = None,
    ) -> SourceStatus:
        """Add another source like one the mixer has: the same address and settings under a new id. A client cannot do this with source.add, because the address it is shown has everything after the host cut off."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if name is not None:
            params["name"] = name
        if new_id is not None:
            params["new_id"] = new_id
        return await self._call("source.duplicate", params)

    async def source_get(
        self,
        id: str,
    ) -> SourceStatus:
        """One source. Refused with the ids that exist when there is no such source."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("source.get", params)

    async def source_group(
        self,
        sources: List[str],
        *,
        name: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Put sources in a tray folder. A tag for finding things, not a group on the canvas."""
        params: Dict[str, Any] = {}
        params["sources"] = sources
        if name is not None:
            params["name"] = name
        return await self._call("source.group", params)

    async def source_list(
        self,
    ) -> List[SourceStatus]:
        """Every source, with its state, whether it has video and audio, and its fader."""
        params: Dict[str, Any] = {}
        return await self._call("source.list", params)

    async def source_missing(
        self,
        *,
        ids: Optional[List[str]] = None,
    ) -> List[MissingSource]:
        """Sources that are not running, and why: failed, could not be started (with the error and the action that fixes it), removed, or unknown. Pass the ids a scene draws, or none for every one the mixer knows about."""
        params: Dict[str, Any] = {}
        if ids is not None:
            params["ids"] = ids
        return await self._call("source.missing", params)

    async def source_remove(
        self,
        id: str,
    ) -> Dict[str, Any]:
        """Remove a source. If it is on programme the mixer cuts to the slate first."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("source.remove", params)

    async def source_restart(
        self,
        id: str,
    ) -> SourceStatus:
        """Build a source's pipeline again now, rather than waiting for its next retry. For a source that could not be started or was removed, use source.restore."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("source.restart", params)

    async def source_restore(
        self,
        id: str,
    ) -> SourceStatus:
        """Put back a source that source.remove took away, as it was: same id, address, settings, fader and mute. The mixer remembers the last sixteen it removed, until it restarts."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("source.restore", params)

    async def source_seek(
        self,
        id: str,
        position_ms: float,
    ) -> SourcePositionState:
        """Move a seekable source to a position. Answers with where it actually landed."""
        params: Dict[str, Any] = {}
        params["id"] = id
        params["position_ms"] = position_ms
        return await self._call("source.seek", params)

    async def source_set(
        self,
        id: str,
        *,
        color: Optional[str] = None,
        latency_ms: Optional[int] = None,
        name: Optional[str] = None,
        params: Optional[Dict[str, Any]] = None,
        place: Optional[Union[Place, None]] = None,
        transport: Optional[Union[BridgeTransport, None]] = None,
    ) -> SourceStatus:
        """Change a running source: its name and colour, its params, or where it runs. The name and colour live on the scene document. Moving a source between the core, a sidecar and a node is `place`; the programme keeps its frame rate across the move and the compositor covers the swap."""
        params: Dict[str, Any] = {}
        params["id"] = id
        if color is not None:
            params["color"] = color
        if latency_ms is not None:
            params["latency_ms"] = latency_ms
        if name is not None:
            params["name"] = name
        if params is not None:
            params["params"] = params
        if place is not None:
            params["place"] = place
        if transport is not None:
            params["transport"] = transport
        return await self._call("source.set", params)

    async def task_cancel(
        self,
        task_id: str,
    ) -> Dict[str, Any]:
        """Ask a piece of long running work to stop. Cooperative: the answer says the request landed, not that the work has stopped yet."""
        params: Dict[str, Any] = {}
        params["task_id"] = task_id
        return await self._call("task.cancel", params)

    async def task_get(
        self,
        task_id: str,
    ) -> TaskView:
        """How a piece of long running work is getting on, and its answer once it has one."""
        params: Dict[str, Any] = {}
        params["task_id"] = task_id
        return await self._call("task.get", params)

    async def task_list(
        self,
    ) -> List[TaskView]:
        """Every background job this core knows about, newest first."""
        params: Dict[str, Any] = {}
        return await self._call("task.list", params)

    async def tool_call(
        self,
        name: str,
        *,
        arguments: Any = None,
    ) -> Dict[str, Any]:
        """Call one of a plugin's tools, in MCP's shape. The name is `<plugin>/<tool>`, or the bare tool name when only one plugin has it."""
        params: Dict[str, Any] = {}
        params["name"] = name
        if arguments is not None:
            params["arguments"] = arguments
        return await self._call("tool.call", params)

    async def vitals_get(
        self,
    ) -> Dict[str, Any]:
        """This show's health (its state and alarms, null in the first second) and the thresholds they are judged by."""
        params: Dict[str, Any] = {}
        return await self._call("vitals.get", params)

    async def vitals_set(
        self,
        *,
        alarms: Optional[bool] = None,
        black_luma: Optional[int] = None,
        black_ratio: Optional[float] = None,
        black_secs: Optional[float] = None,
        cc_errors: Optional[int] = None,
        freeze_diff: Optional[float] = None,
        freeze_secs: Optional[float] = None,
        loss: Optional[int] = None,
        silence_db: Optional[float] = None,
        silence_secs: Optional[float] = None,
        stall_secs: Optional[float] = None,
        window_secs: Optional[float] = None,
    ) -> Dict[str, Any]:
        """Change the alarm thresholds, or whether a mosaic is kept up for the black and freeze checks while nobody is looking. Fields left out keep their defaults; a duration of 0 switches that check off. Applies within a second."""
        params: Dict[str, Any] = {}
        if alarms is not None:
            params["alarms"] = alarms
        if black_luma is not None:
            params["black_luma"] = black_luma
        if black_ratio is not None:
            params["black_ratio"] = black_ratio
        if black_secs is not None:
            params["black_secs"] = black_secs
        if cc_errors is not None:
            params["cc_errors"] = cc_errors
        if freeze_diff is not None:
            params["freeze_diff"] = freeze_diff
        if freeze_secs is not None:
            params["freeze_secs"] = freeze_secs
        if loss is not None:
            params["loss"] = loss
        if silence_db is not None:
            params["silence_db"] = silence_db
        if silence_secs is not None:
            params["silence_secs"] = silence_secs
        if stall_secs is not None:
            params["stall_secs"] = stall_secs
        if window_secs is not None:
            params["window_secs"] = window_secs
        return await self._call("vitals.set", params)
