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
    uri: str
    # rtmp:// or rtmps:// URL including the stream key.

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

# `ext.agent`. `true` takes the default thresholds; an object moves them.
AgentExt = Union[bool, Dict[str, Any]]

class AgentStateRequest(TypedDict, total=False):
    response_format: ResponseFormat

# The nine alignment keywords, used to place content inside its frame.
Align = Literal['top-left', 'top-center', 'top-right', 'center-left', 'center', 'center-right', 'bottom-left', 'bottom-center', 'bottom-right']

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

# Whether the item's source is heard. A source is audible when any live item of it says so, which is OBS's behaviour and changes no pad topology.
Audio = Literal['follow', 'always', 'never']

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

class BackendInfo(TypedDict, total=False):
    audio_decoder: str
    audio_encoder: str
    hardware_accelerated: bool
    video_decoder: str
    video_encoder: str

class BindRequest(TypedDict, total=False):
    """`scene.item.bind`."""

    draft: Optional[str]
    item: str
    param: str
    # An expression over the collection's params and `W`, `H`. An empty string takes the binding off.
    prop: str
    # A geometry path such as `frame.w` or `position.x`.
    scene: str

# OBS's blend enum, so an import carries across unchanged.
Blend = Literal['normal', 'add', 'screen', 'multiply', 'lighten', 'darken', 'subtract']

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

ConversionPhase = Literal['running', 'done', 'failed']

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
    token: Union[TokenInfo, None]
    # Present when the request carried a token the core recognises.
    ui: Union[UiDefaults, None]
    # What a surface should start with, when a preset chose it. Absent on a core no preset has been applied to, which is what puts the welcome panel up in the reference UI.
    version: str
    # The build's own version, as in Cargo.toml.

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

class EditBeginRequest(TypedDict, total=False):
    """`scene.edit.begin`."""

    live: bool
    # True to edit the scene that is on air as you go. The default is off air: the draft is applied on the next take or on an explicit apply.
    scene: str

class ExportRequest(TypedDict, total=False):
    collection: Optional[str]
    format: Optional[str]
    # `json` in this build. `zip`, with the assets, is Phase 5.

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

# How content fills its frame. SVG's vocabulary, which replaces OBS's seven bounds types and maps onto `sizing-policy` on a `glvideomixer` pad.
Fit = Literal['none', 'contain', 'cover', 'stretch', 'fit-width', 'fit-height', 'max']

# Content on the wire. The same four shapes as the tree, except that a group names no children: they are records whose parent is the group.
FlatContent = Dict[str, Any]

class Flush(TypedDict, total=False):
    """`event/flush`: the end of a batch. A client renders here and not before."""

    seq: int
    # The sequence number of the last event in the batch.

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

class GroupSourcesRequest(TypedDict, total=False):
    """`source.group`."""

    name: Optional[str]
    # The tray folder to put them in. Null takes them out of the one they are in.
    sources: List[str]

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

# A UUID in the hyphenated form. Minted ids are version 7 (time ordered); ids derived from a layout are version 8.
Id = str

class IdRequest(TypedDict, total=False):
    """An id on its own: `source.get`, `source.remove`, `output.remove`, `output.reconnect`, `media.remove`."""

    id: str

class ImportObsRequest(TypedDict, total=False):
    path: str
    # The collection JSON exported from OBS (Scene Collection, Export), as a path on the machine the core is running on.

class ImportReport(TypedDict, total=False):
    items: int
    scenes: List[str]
    # The scenes that were added, by the names they ended up with.
    skipped: List[str]
    # What could not be brought across, and why, one line each.
    sources: List[str]
    # The sources the collection needs, which have to be added separately.

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
    to: Optional[str]
    # `match_size`: the item to match.

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

# `ext.multiview`. Accepts `false` to mean off, or an object.
MultiviewExt = Union[bool, Dict[str, Any]]

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

OutputState = Literal['connecting', 'live', 'reconnecting', 'failed']

class OutputStatus(TypedDict, total=False):
    id: str
    queue_secs: float
    # Seconds of encoded data waiting in the pre-muxer queue. A number that climbs and stays high means the destination cannot keep up.
    reconnects: int
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

class PipelineDot(TypedDict, total=False):
    """What `pipeline.dot` answers with on `/rpc`. The REST route serves the same graph as `text/vnd.graphviz`, so `gmx dot | dot -Tsvg` needs no unwrapping."""

    dot: str
    # The graph itself, in the dot language.
    pipeline: str

class PipelineRequest(TypedDict, total=False):
    """Which pipeline to look at. A source id, an output id, `programme` or `multiview`. `pipeline.list` says what is running."""

    name: str

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

class PluginUpdated(TypedDict, total=False):
    """What `plugin.update` answers with."""

    from: str
    handshake_ms: int
    # How long the new build took to answer `initialize`.
    plugin: PluginRecord
    to: str

class PreviewClosed(TypedDict, total=False):
    """What `preview.close` answers with."""

    closed: bool
    target: str

# `ext.preview`. Either `"full"`, `false`, or an object.
PreviewExt = Union[str, bool, Dict[str, Any]]

class PreviewFrameRequest(TypedDict, total=False):
    """`scene.preview.frame`."""

    width: Optional[int]

class PreviewOpenRequest(TypedDict, total=False):
    """`preview.open {target}`."""

    target: str
    # `program`, or a source id.

class PreviewRequest(TypedDict, total=False):
    """`scene.preview.set`."""

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

class Record(TypedDict, total=False):
    """One scene or one item."""

    id: Id
    order: str
    # A fractional key. Siblings sort by it; see `order.rs`.
    parent: Union[Id, None]
    # The scene this item is in, or the group item it is a child of. Absent for a scene, which hangs off the document itself.

class RenameSceneRequest(TypedDict, total=False):
    color: Optional[str]
    name: Optional[str]
    scene: str

class ReorderRequest(TypedDict, total=False):
    """`scene.item.reorder`."""

    after: Optional[str]
    # Put it in front of this one. With neither, it goes to the front.
    before: Optional[str]
    # Put it behind this one.
    draft: Optional[str]
    item: str
    scene: str

ResponseFormat = Literal['concise', 'detailed']

class Resync(TypedDict, total=False):
    """`event/resync`: the client fell behind and the stream has a hole in it."""

    dropped: int
    # How many events were dropped.
    from_seq: int
    # The last sequence number the client is known to have. Everything after it was dropped; re-subscribe for a fresh snapshot.

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

class SetSettingsRequest(TypedDict, total=False):
    """`plugin.settings.set`."""

    id: str
    settings: Dict[str, Any]
    # Only the keys named are changed.

class SetSourceMetaRequest(TypedDict, total=False):
    """`source.set`."""

    color: Optional[str]
    name: Optional[str]
    source: str

# How much the reader should care.
Severity = Literal['error', 'warning', 'info']

Severity2 = Union[Literal['info', 'warning', 'error'], Literal['critical']]

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

class SourcePositionState(TypedDict, total=False):
    """Where a seekable source has got to, which is what the seek endpoint answers with. Both numbers are read back off the pipeline after the seek has landed, not taken from the request. A seek snaps to a key unit, so the frame an operator asked for and the frame they got are rarely the same millisecond, and a scrubber drawn from the request would sit a little away from the picture."""

    duration_ms: Optional[int]
    # Absent while the demuxer has not worked the duration out yet.
    position_ms: int

SourceState = Literal['connecting', 'live', 'stalled', 'failed']

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

class SubscribeRequest(TypedDict, total=False):
    """`core.subscribe`: which events, and which expensive streams."""

    events: List[str]
    # Event name patterns, matched against the part after `event/`. `*` matches one or more characters: "program.*" matches `event/program.took`. An empty list subscribes to everything.
    ext: Ext
    # The expensive streams this client wants. Nothing here runs unless a client asks for it.

class SubscribeResult(TypedDict, total=False):
    """What `core.subscribe` answers with, before the snapshot arrives."""

    events: List[str]
    # The event patterns now in force.
    ignored_ext: List[str]
    # `ext` keys this build ignored. Empty on a build that knows them all.
    seq: int
    # The sequence number the snapshot that follows is current as of.

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
    transition: Optional[str]
    # `cut` in this build.

class Tally(TypedDict, total=False):
    """`event/tally`."""

    sources: Dict[str, Any]
    # Source id to "program", "preview" or "off".

class TaskRequest(TypedDict, total=False):
    task_id: str
    # The id a long running method answered with. Spelled `id` on the REST route, where it is in the path, and `task_id` everywhere else, which is what 03 section 6 calls it.

TaskState = Literal['running', 'completed', 'failed', 'cancelled']

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

# `ext.telemetry`. Accepts `false` to mean off, `true` for the default rate, or an object naming it.
TelemetryExt = Union[bool, Dict[str, Any]]

class TokenInfo(TypedDict, total=False):
    """What the calling token is allowed to do, echoed back so a surface can grey out what it cannot reach instead of discovering it at the first refusal."""

    confirm: str
    # "none" or "required": whether destructive calls need a confirm token.
    id: str
    profile: str
    # MCP tool profile this token is meant for: "standard" or "minimal".
    rehearsal: bool
    scopes: List[str]

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

class ProgramTookEvent(TypedDict, total=False):
    at_running_time_ms: int
    duration_ms: int
    scene: Optional[str]
    source: Optional[str]
    transition: str

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

class MediaChangedEvent(TypedDict, total=False):
    conversion: Any
    name: str

class AlertEvent(TypedDict, total=False):
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

METHODS = (
    {"name": "adbreak.end", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/adbreak/end"), "summary": 'Cut a running ad short, or disarm one that is scheduled.'},
    {"name": "adbreak.start", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/adbreak/start"), "summary": 'Interrupt the programme with a clip, then rejoin live when it ends.'},
    {"name": "agent.state", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/agent/state"), "summary": "The compact document written for agents: the programme, each source's state and a motion score saying how much its picture is changing."},
    {"name": "codec.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/codecs"), "summary": 'Every codec and element in the catalogue, which of them this machine actually has, and what it would pick.'},
    {"name": "core.api", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/core/api"), "summary": 'Every method, event and type as JSON Schema. The same document as protocol.json and `godwinmix --api-info`.'},
    {"name": "core.doctor", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/core/doctor"), "summary": 'The environment checks: GStreamer, the elements, the config, the disk and the ports. The same list `gmx doctor` prints.'},
    {"name": "core.info", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/core/info"), "summary": 'What this core is, what it can do, and where its edges are.'},
    {"name": "core.session_log", "scope": "admin", "mutating": True, "destructive": False, "rest": ("GET", "/api/v1/core/session_log"), "summary": 'The append only record of everything that happened, back as far as you ask.'},
    {"name": "core.shutdown", "scope": "admin", "mutating": True, "destructive": True, "rest": ("POST", "/api/v1/core/shutdown"), "summary": 'Stop the mixer, and with it the programme. Nothing else takes the show off air, so this is deliberately its own call.'},
    {"name": "core.startup_report", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/core/startup_report"), "summary": 'How long each stage of the start took, and what was over the 250 ms mark.'},
    {"name": "core.status", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/core/status"), "summary": 'The full state: programme, every source, every output, the multiview grid, the encoder backend and any ad break.'},
    {"name": "core.subscribe", "scope": "read", "mutating": False, "destructive": False, "rest": None, "summary": 'Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush.'},
    {"name": "filter.add", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/filters"), "summary": 'Hang a filter on one source or on the programme, live.'},
    {"name": "filter.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/filters"), "summary": 'Every filter in place, with what it is and where it sits.'},
    {"name": "filter.remove", "scope": "operate", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/filters/{id}"), "summary": 'Take a filter out of the pipeline.'},
    {"name": "filter.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/filters/{id}/set"), "summary": "Change a filter's settings in place. A filter that cannot take the change while running says so rather than being restarted behind your back."},
    {"name": "log.gst", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/log/gst"), "summary": "Raise GStreamer's own debug categories for a while, then let them fall back on their own."},
    {"name": "log.levels", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/log/levels"), "summary": 'Every log level override in force, and the GStreamer categories still raised.'},
    {"name": "log.set", "scope": "admin", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/log/set"), "summary": "Change one instance's or one module's log level while the mixer runs."},
    {"name": "media.convert", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/media/{id}/convert"), "summary": 'Transcode a library file to a web safe copy, in the background.'},
    {"name": "media.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/media"), "summary": 'The clips in the library, with durations and whether each has audio.'},
    {"name": "media.remove", "scope": "operate", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/media/{id}"), "summary": 'Delete a library file and its converted copy. Refused while it is a live source.'},
    {"name": "media.upload", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/media/upload"), "summary": 'Stream a file into the library. HTTP only: the body is the file.'},
    {"name": "output.add", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/outputs"), "summary": 'Send the programme to another destination. The encoder is shared, so adding one costs nothing on air.'},
    {"name": "output.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/outputs/{id}"), "summary": 'One destination.'},
    {"name": "output.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/outputs"), "summary": 'Every destination, with its state, reconnect count and how much is buffered.'},
    {"name": "output.reconnect", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/outputs/{id}/reconnect"), "summary": "Drop and re-establish one destination's connection now, without waiting for its reconnect policy."},
    {"name": "output.remove", "scope": "operate", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/outputs/{id}"), "summary": 'Stop sending to a destination and forget it. Other outputs are unaffected.'},
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
    {"name": "scene.add", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes"), "summary": 'Make an empty scene, or one built from a set of sources.'},
    {"name": "scene.apply_layout", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/apply_layout"), "summary": 'Apply a layout, making a scene or reshaping one that exists. Applying onto an existing scene keeps the item ids, so the change is a ramp and not a cut.'},
    {"name": "scene.create_from", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/create_from"), "summary": 'A scene from a set of sources, laid out by the built in layout for that count (full, two-box, three-box, quad, then a grid) or by a named one.'},
    {"name": "scene.duplicate", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/{id}/duplicate"), "summary": 'A copy of a scene with new ids throughout, so editing the copy cannot touch the original.'},
    {"name": "scene.edit.apply", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/edit/apply"), "summary": 'Write a draft back into the live document.'},
    {"name": "scene.edit.begin", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/edit/begin"), "summary": 'Take a working copy of a scene. Editing is off air by default: the draft is written back on the next take of that scene, or when you apply it.'},
    {"name": "scene.edit.discard", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/edit/discard"), "summary": 'Throw a draft away. The live document is untouched.'},
    {"name": "scene.export", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/export"), "summary": 'The whole collection as JSON. The zip bundle with assets is Phase 5.'},
    {"name": "scene.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/{id}"), "summary": 'One scene: its records and where every item actually lands on the canvas.'},
    {"name": "scene.history.mark", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/history/mark"), "summary": 'Group the changes that follow into one undo step, until the next mark. This is what makes a drag of forty moves one Ctrl+Z.'},
    {"name": "scene.import.obs", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/import/obs"), "summary": 'Read an OBS Studio scene collection and add its scenes to this one.'},
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
    {"name": "scene.item.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/set"), "summary": "Assign an item's properties. Only the keys named move; the rest are left alone, so calling it twice with the same body changes nothing the second time."},
    {"name": "scene.item.ungroup", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/item/ungroup"), "summary": 'Take a group apart, leaving every child exactly where it looked.'},
    {"name": "scene.layout.copy", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/layout/copy"), "summary": "Read one scene's geometry, to paste onto another."},
    {"name": "scene.layout.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/layout/list"), "summary": 'The layouts that ship with the core, with the parameters each one takes.'},
    {"name": "scene.layout.paste", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/layout/paste"), "summary": "Put one scene's geometry onto another's items, matched by name first and slot order second. Items that match nothing are left alone."},
    {"name": "scene.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes"), "summary": 'Every scene in the collection, with how many items it has, the sources it draws and whether it is armed.'},
    {"name": "scene.params.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/scenes/params/get"), "summary": "The collection's typed parameters, readable without their values, so a client discovers what is fillable before filling it."},
    {"name": "scene.params.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/scenes/params/set"), "summary": "Set the collection's parameter values. A `{{name}}` in a string property follows them."},
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
    {"name": "snapshot.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/snapshot/{id}"), "summary": 'One JPEG: the whole contact sheet, the programme, or one source cut out of the mosaic.'},
    {"name": "source.add", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources"), "summary": 'Add a source while the mixer runs. Answers with the id it got and the whole source record.'},
    {"name": "source.audio.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources/{id}/audio"), "summary": "Move a source's audio: the fader, the mute, and for a superimposed page the balance between its own sound and the videos under it."},
    {"name": "source.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/sources/{id}"), "summary": 'One source. Refused with the ids that exist when there is no such source.'},
    {"name": "source.group", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources/{id}/group"), "summary": 'Put sources in a tray folder. A tag for finding things, not a group on the canvas.'},
    {"name": "source.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/sources"), "summary": 'Every source, with its state, whether it has video and audio, and its fader.'},
    {"name": "source.remove", "scope": "operate", "mutating": True, "destructive": True, "rest": ("DELETE", "/api/v1/sources/{id}"), "summary": 'Remove a source. If it is on programme the mixer cuts to the slate first.'},
    {"name": "source.seek", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources/{id}/seek"), "summary": 'Move a seekable source to a position. Answers with where it actually landed.'},
    {"name": "source.set", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/sources/{id}/set"), "summary": 'Name and colour a source. Both live on the scene document, so every client, the tally and an agent see the same ones.'},
    {"name": "task.cancel", "scope": "operate", "mutating": True, "destructive": False, "rest": ("POST", "/api/v1/task/cancel"), "summary": 'Ask a piece of long running work to stop. Cooperative: the answer says the request landed, not that the work has stopped yet.'},
    {"name": "task.get", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/task"), "summary": 'How a piece of long running work is getting on, and its answer once it has one.'},
    {"name": "task.list", "scope": "read", "mutating": False, "destructive": False, "rest": ("GET", "/api/v1/task/list"), "summary": 'Every background job this core knows about, newest first.'},
)

EVENT_NAMES = (
    "snapshot",
    "program.took",
    "preview.changed",
    "source.state",
    "source.position",
    "output.state",
    "adbreak.changed",
    "ui.changed",
    "media.changed",
    "meters",
    "tally",
    "alert",
    "telemetry",
    "agent.state",
    "multiview.layout",
    "multiview.frame",
    "resync",
    "flush",
)

EXT_KEYS = {
    "multiview": {"value": '{fps: 1..30, width: 320..1920} or false', "implemented": True},
    "meters": {"value": 'true', "implemented": True},
    "tally": {"value": 'true', "implemented": True},
    "positions": {"value": 'true', "implemented": True},
    "thumb": {"value": '{fps}', "implemented": False},
    "preview": {"value": '{fps, width} or "full"', "implemented": False},
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

    async def codec_list(
        self,
    ) -> Dict[str, Any]:
        """Every codec and element in the catalogue, which of them this machine actually has, and what it would pick."""
        params: Dict[str, Any] = {}
        return await self._call("codec.list", params)

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
    ) -> SubscribeResult:
        """Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush."""
        params: Dict[str, Any] = {}
        if events is not None:
            params["events"] = events
        if ext is not None:
            params["ext"] = ext
        return await self._call("core.subscribe", params)

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

    async def output_add(
        self,
        id: str,
        uri: str,
        *,
        policy: Optional[str] = None,
        **extra: Any,
    ) -> OutputStatus:
        """Send the programme to another destination. The encoder is shared, so adding one costs nothing on air."""
        params: Dict[str, Any] = {}
        params["id"] = id
        params["uri"] = uri
        if policy is not None:
            params["policy"] = policy
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
        transition: Optional[str] = None,
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
    ) -> Dict[str, Any]:
        """The whole collection as JSON. The zip bundle with assets is Phase 5."""
        params: Dict[str, Any] = {}
        if collection is not None:
            params["collection"] = collection
        if format is not None:
            params["format"] = format
        return await self._call("scene.export", params)

    async def scene_get(
        self,
        scene: str,
    ) -> SceneView:
        """One scene: its records and where every item actually lands on the canvas."""
        params: Dict[str, Any] = {}
        params["scene"] = scene
        return await self._call("scene.get", params)

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

    async def scene_import_obs(
        self,
        path: str,
    ) -> ImportReport:
        """Read an OBS Studio scene collection and add its scenes to this one."""
        params: Dict[str, Any] = {}
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
        return await self._call("scene.item.reorder", params)

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
        """Set the collection's parameter values. A `{{name}}` in a string property follows them."""
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
        scene: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Arm a scene. The armed scene is the preview, and program.take with no argument takes it."""
        params: Dict[str, Any] = {}
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

    async def source_remove(
        self,
        id: str,
    ) -> Dict[str, Any]:
        """Remove a source. If it is on programme the mixer cuts to the slate first."""
        params: Dict[str, Any] = {}
        params["id"] = id
        return await self._call("source.remove", params)

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
        source: str,
        *,
        color: Optional[str] = None,
        name: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Name and colour a source. Both live on the scene document, so every client, the tally and an agent see the same ones."""
        params: Dict[str, Any] = {}
        params["source"] = source
        if color is not None:
            params["color"] = color
        if name is not None:
            params["name"] = name
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
