// Generated from protocol.json by clients/gen/generate.py. Do not edit.
//
// Every type, method and event the core describes in `core.api`, as TypeScript.
// A method added to the core reaches this file by running:
//
//     python3 clients/gen/generate.py
//
// The drift test in test/generated.test.ts fails if this file and protocol.json
// have parted company.
/* eslint-disable */

export const API_LEVEL = 1;
export const API_COMPATIBLE = 1;

/** `adbreak.start`. */
export interface AdBreakRequest {
  at_running_time_ms?: number | null;
  return_to?: string | null;
  uri: string;
}

/** An ad break, either armed for a future cue or currently on air. */
export interface AdStatus {
  on_air: boolean;
  return_to?: string | null;
  uri: string;
}

/** `filter.add`. */
export interface AddFilterRequest {
  id: string;
  params?: Record<string, unknown>;
  programme?: boolean;
  side?: string;
  source?: string | null;
  type: string;
}

/**
 * `output.add`. The id and the URL are the whole of it for an RTMP
 * destination; anything else a kind understands rides in `params`.
 */
export interface AddOutputRequest {
  id: string;
  policy?: string | null;
  uri: string;
  [key: string]: unknown;
}

/** `source.add`. */
export interface AddSourceRequest {
  id?: string | null;
  kind?: string | null;
  name?: string | null;
  superimpose?: string | null;
  uri: string;
  [key: string]: unknown;
}

/** `ext.agent`. `true` takes the default thresholds; an object moves them. */
export type AgentExt = boolean | {
  black?: number | null;
  freeze_ms?: number | null;
  shot?: number | null;
  silence_ms?: number | null;
};

export interface AgentStateRequest {
  response_format?: ResponseFormat;
}

export interface ApplyRequest {
  dry_run?: boolean;
  force?: boolean;
  keep_sources?: boolean;
  name: string;
}

/** What `preset.apply` answers with. */
export interface ApplyResult {
  applied?: unknown;
  dry_run: boolean;
  live: string[];
  needs_restart: string[];
  plan: unknown;
}

/**
 * `source.audio.set` takes an id as well as the levels: the id comes off the
 * path on REST and out of the params on `/rpc`, and both land in one object.
 */
export interface AudioSetParams {
  gain?: number | null;
  id: string;
  media?: Array<number | null>;
  muted?: boolean | null;
  page?: number | null;
}

export interface BackendInfo {
  audio_decoder: string;
  audio_encoder: string;
  hardware_accelerated: boolean;
  video_decoder: string;
  video_encoder: string;
}

/** The canvas every source is scaled onto and every output leaves by. */
export interface CanvasInfo {
  fps: number;
  height: number;
  width: number;
}

export interface CellAssignment {
  h: number;
  index: number;
  source?: string | null;
  w: number;
  x: number;
  y: number;
}

export type ConversionPhase = "running" | "done" | "failed";

/** One conversion, in flight or remembered after it finished. */
export interface ConversionState {
  error?: string | null;
  output?: string | null;
  progress: number;
  state: ConversionPhase;
}

/** `core.info`: what this core is, what it can do, and where its edges are. */
export interface CoreInfo {
  api_compatible: number;
  api_level: number;
  canvas: CanvasInfo;
  core: string;
  features: string[];
  limits: Limits;
  rehearsal: boolean;
  token?: TokenInfo | null;
  ui?: UiDefaults | null;
  version: string;
}

/**
 * The `ext` table from 03 section 6.
 *
 * Every key is off by default. A terminal UI takes meters and tally and
 * declines multiview; a Stream Deck takes tally only; an agent takes nothing.
 */
export interface Ext {
  agent?: AgentExt | null;
  meters?: boolean;
  multiview?: MultiviewExt | null;
  positions?: boolean;
  tally?: boolean;
  telemetry?: TelemetryExt | null;
  [key: string]: unknown;
}

/** `filter.remove`, and anything else that names one filter. */
export interface FilterIdRequest {
  id: string;
}

export interface FilterListing {
  filters: FilterRecord[];
}

/** One filter as the core reports it. */
export interface FilterRecord {
  id: string;
  side: string;
  source?: string | null;
  type: string;
}

export interface FilterRemoved {
  removed: string;
}

/** `event/flush`: the end of a batch. A client renders here and not before. */
export interface Flush {
  seq: number;
}

/** `program.golive`: add the page, add the destination, take the page. */
export interface GoLiveRequest {
  id?: string | null;
  rtmp?: string | null;
  superimpose?: string | null;
  url: string;
}

/** What `program.golive` answers with. */
export interface GoLiveResult {
  output?: string | null;
  source: string;
  state: SourceState;
}

/** `program.history`. */
export interface HistoryRequest {
  limit?: number | null;
}

/**
 * An id on its own: `source.get`, `source.remove`, `output.remove`,
 * `output.reconnect`, `media.remove`.
 */
export interface IdRequest {
  id: string;
}

/**
 * The ceilings a client should plan against rather than discover by being
 * refused.
 */
export interface Limits {
  event_queue: number;
  max_call_secs: number;
  max_gain: number;
  max_idempotency_key_bytes: number;
  max_upload_bytes: number;
}

/** `log.gst`. */
export interface LogGstRequest {
  categories: string;
  duration_secs?: number;
  instance?: string | null;
}

/** What `log.gst` answers with. */
export interface LogGstResult {
  categories: string[];
  duration_secs: number;
}

/** `log.set`. Name an instance or a target, not both. */
export interface LogSetRequest {
  instance?: string | null;
  level: string;
  target?: string | null;
}

export interface MediaItem {
  audio_codec?: string | null;
  conversion?: ConversionState | null;
  converted_path?: string | null;
  duration_ms?: number | null;
  faststart?: boolean | null;
  has_audio: boolean;
  has_video: boolean;
  height?: number | null;
  name: string;
  path: string;
  reasons?: string[];
  size_bytes: number;
  video_codec?: string | null;
  web_safe?: boolean;
  width?: number | null;
}

export interface MediaListing {
  dir: string;
  error?: string | null;
  items: MediaItem[];
}

/**
 * `event/meters`: the programme bus and every source, in one message at 10
 * per second, rather than one message per meter as the legacy stream sends.
 */
export interface Meters {
  program: number[];
  sources: Record<string, unknown>;
}

export interface MixerStatus {
  ad?: AdStatus | null;
  backend: BackendInfo;
  multiview: MultiviewStatus;
  outputs: OutputStatus[];
  program?: string | null;
  running_time_ms: number;
  sources: SourceStatus[];
  uptime_secs: number;
}

/** `ext.multiview`. Accepts `false` to mean off, or an object. */
export type MultiviewExt = boolean | {
  fps?: number | null;
  width?: number | null;
};

/** `event/multiview.layout`: how to read the binary frames that follow. */
export interface MultiviewLayout {
  cells: CellAssignment[];
  height: number;
  id: number;
  width: number;
}

export interface MultiviewStatus {
  cells: CellAssignment[];
  cols: number;
  enabled: boolean;
  fps: number;
  height: number;
  rows: number;
  width: number;
}

/** `media.convert` and `media.remove` name a file rather than an id. */
export interface NameRequest {
  name: string;
}

export type OutputState = "connecting" | "live" | "reconnecting" | "failed";

export interface OutputStatus {
  id: string;
  queue_secs: number;
  reconnects: number;
  state: OutputState;
  uri_host: string;
  [key: string]: unknown;
}

/**
 * What `pipeline.dot` answers with on `/rpc`. The REST route serves the same
 * graph as `text/vnd.graphviz`, so `gmx dot | dot -Tsvg` needs no unwrapping.
 */
export interface PipelineDot {
  dot: string;
  pipeline: string;
}

/**
 * Which pipeline to look at. A source id, an output id, `programme` or
 * `multiview`. `pipeline.list` says what is running.
 */
export interface PipelineRequest {
  name?: string;
}

/**
 * What `program.get` answers with, and what `program.take` returns so that no
 * follow up read is needed.
 */
export interface ProgramState {
  ad?: AdStatus | null;
  previous?: string | null;
  program?: string | null;
  running_time_ms: number;
}

export type ResponseFormat = "concise" | "detailed";

/** `event/resync`: the client fell behind and the stream has a hole in it. */
export interface Resync {
  dropped: number;
  from_seq: number;
}

export interface SaveRequest {
  name: string;
  out?: string | null;
}

/** `source.seek`. */
export interface SeekParams {
  id: string;
  position_ms: number;
}

/** `core.session_log`. */
export interface SessionLogRequest {
  secs?: number;
}

/** `filter.set`. */
export interface SetFilterRequest {
  id: string;
  params?: Record<string, unknown>;
}

export type Severity = "info" | "warning" | "error" | "critical";

/** `event/snapshot`: the full state, and where in the stream it sits. */
export interface Snapshot {
  seq: number;
  state: MixerStatus;
}

/**
 * `snapshot.get` on `/rpc` and through MCP. The REST route serves the same
 * bytes raw, because an `<img>` tag cannot read base64 out of JSON.
 */
export interface SnapshotRequest {
  allow_large?: boolean;
  force?: boolean;
  id: string;
  width?: number | null;
}

/**
 * What a source's audio controls read back as, which is what the audio
 * endpoint answers with.
 *
 * Wider than `SourceAudio` because the fader and the mute apply to every
 * source, while the page and media balance belongs only to a superimposed one.
 * Every number here is read off the elements after the request landed, so a
 * request whose gain was clamped answers with the gain that took effect.
 */
export interface SourceAudioState {
  gain: number;
  media?: number[] | null;
  muted: boolean;
  page?: number | null;
}

/**
 * Where a seekable source has got to, which is what the seek endpoint answers
 * with.
 *
 * Both numbers are read back off the pipeline after the seek has landed, not
 * taken from the request. A seek snaps to a key unit, so the frame an operator
 * asked for and the frame they got are rarely the same millisecond, and a
 * scrubber drawn from the request would sit a little away from the picture.
 */
export interface SourcePositionState {
  duration_ms?: number | null;
  position_ms: number;
}

export type SourceState = "connecting" | "live" | "stalled" | "failed";

export interface SourceStatus {
  audio_idle_ms?: number | null;
  cell?: number | null;
  duration_ms?: number | null;
  gain?: number;
  has_audio: boolean;
  has_video: boolean;
  id: string;
  muted?: boolean;
  name: string;
  position_ms?: number | null;
  seekable?: boolean;
  state: SourceState;
  uri: string;
  video_idle_ms?: number | null;
  [key: string]: unknown;
}

/** `core.subscribe`: which events, and which expensive streams. */
export interface SubscribeRequest {
  events?: string[];
  ext?: Ext;
}

/** What `core.subscribe` answers with, before the snapshot arrives. */
export interface SubscribeResult {
  events: string[];
  ignored_ext: string[];
  seq: number;
}

/** One take, as `program.history` reports it. */
export interface TakeRecord {
  at_running_time_ms: number;
  by: string;
  seq: number;
  source?: string | null;
}

/** `program.take`: put a source on programme. */
export interface TakeRequest {
  at_running_time_ms?: number | null;
  scene?: string | null;
  source?: string | null;
}

/** `event/tally`. */
export interface Tally {
  sources: Record<string, unknown>;
}

export interface TaskRequest {
  task_id: string;
}

export type TaskState = "running" | "completed" | "failed" | "cancelled";

/** What `task.get` answers with. */
export interface TaskView {
  age_secs: number;
  error?: string | null;
  kind: string;
  poll_interval_ms?: number | null;
  progress?: number | null;
  result?: unknown;
  state: TaskState;
  task_id: string;
}

/**
 * `ext.telemetry`. Accepts `false` to mean off, `true` for the default rate,
 * or an object naming it.
 */
export type TelemetryExt = boolean | {
  hz?: number | null;
};

/**
 * What the calling token is allowed to do, echoed back so a surface can grey
 * out what it cannot reach instead of discovering it at the first refusal.
 */
export interface TokenInfo {
  confirm: string;
  id: string;
  profile: string;
  rehearsal: boolean;
  scopes: string[];
}

/**
 * What a surface starts with: the layout, the theme and the gallery mode.
 *
 * Chosen by a preset (`preset.apply`), carried in `core.info` and pushed as
 * `event/ui.changed`. None of it changes what the core does. It exists so the
 * first page a volunteer sees is the one their preset chose rather than the
 * one the last person to use this browser chose. 05 section 3b is where the
 * four gallery modes are defined.
 */
export interface UiDefaults {
  gallery?: string | null;
  layout?: Record<string, unknown>;
  preset?: string | null;
  theme?: string | null;
}

export interface ProgramTookEvent {
  at_running_time_ms?: number;
  duration_ms?: number;
  scene?: string | null;
  source?: string | null;
  transition?: string;
}

export interface SourceStateEvent {
  detail?: string | null;
  source?: string;
  state?: SourceState;
}

export interface SourcePositionEvent {
  duration_ms?: number | null;
  position_ms?: number;
  source?: string;
}

export interface OutputStateEvent {
  output?: string;
  reconnects?: number;
  state?: OutputState;
}

export interface AdbreakChangedEvent {
  ad?: AdStatus | null;
}

export interface UiChangedEvent {
  ui?: UiDefaults;
}

export interface MediaChangedEvent {
  conversion?: unknown;
  name?: string;
}

export interface AlertEvent {
  message?: string;
  severity?: Severity;
}

export interface TelemetryEvent {
  black: number;
  freeze: boolean;
  lufs_i?: number | null;
  lufs_s?: number | null;
  shot: number;
  silence: boolean;
  sources: Record<string, unknown>;
  ts: number;
}

/** The params each method takes, by method name. */
export interface MethodParams {
  "adbreak.end": Record<string, never>;
  "adbreak.start": AdBreakRequest;
  "agent.state": AgentStateRequest;
  "codec.list": Record<string, never>;
  "core.api": Record<string, never>;
  "core.doctor": Record<string, never>;
  "core.info": Record<string, never>;
  "core.session_log": SessionLogRequest;
  "core.shutdown": Record<string, never>;
  "core.startup_report": Record<string, never>;
  "core.status": Record<string, never>;
  "core.subscribe": SubscribeRequest;
  "filter.add": AddFilterRequest;
  "filter.list": Record<string, never>;
  "filter.remove": FilterIdRequest;
  "filter.set": SetFilterRequest;
  "log.gst": LogGstRequest;
  "log.levels": Record<string, never>;
  "log.set": LogSetRequest;
  "media.convert": NameRequest;
  "media.list": Record<string, never>;
  "media.remove": NameRequest;
  "media.upload": Record<string, never>;
  "output.add": AddOutputRequest;
  "output.get": IdRequest;
  "output.list": Record<string, never>;
  "output.reconnect": IdRequest;
  "output.remove": IdRequest;
  "pipeline.clock": Record<string, never>;
  "pipeline.dot": PipelineRequest;
  "pipeline.latency": PipelineRequest;
  "pipeline.list": Record<string, never>;
  "pipeline.queues": PipelineRequest;
  "preset.apply": ApplyRequest;
  "preset.list": Record<string, never>;
  "preset.save": SaveRequest;
  "program.get": Record<string, never>;
  "program.golive": GoLiveRequest;
  "program.history": HistoryRequest;
  "program.revert": Record<string, never>;
  "program.take": TakeRequest;
  "snapshot.get": SnapshotRequest;
  "source.add": AddSourceRequest;
  "source.audio.set": AudioSetParams;
  "source.get": IdRequest;
  "source.list": Record<string, never>;
  "source.remove": IdRequest;
  "source.seek": SeekParams;
  "task.cancel": TaskRequest;
  "task.get": TaskRequest;
  "task.list": Record<string, never>;
}

/** What each method answers with, by method name. */
export interface MethodResults {
  "adbreak.end": Record<string, unknown>;
  "adbreak.start": Record<string, unknown>;
  "agent.state": Record<string, unknown>;
  "codec.list": Record<string, unknown>;
  "core.api": Record<string, unknown>;
  "core.doctor": Record<string, unknown>;
  "core.info": CoreInfo;
  "core.session_log": Record<string, unknown>;
  "core.shutdown": Record<string, unknown>;
  "core.startup_report": Record<string, unknown>;
  "core.status": MixerStatus;
  "core.subscribe": SubscribeResult;
  "filter.add": FilterRecord;
  "filter.list": FilterListing;
  "filter.remove": FilterRemoved;
  "filter.set": FilterRecord;
  "log.gst": LogGstResult;
  "log.levels": Record<string, unknown>;
  "log.set": Record<string, unknown>;
  "media.convert": ConversionState;
  "media.list": MediaListing;
  "media.remove": Record<string, unknown>;
  "media.upload": Record<string, unknown>;
  "output.add": OutputStatus;
  "output.get": OutputStatus;
  "output.list": OutputStatus[];
  "output.reconnect": OutputStatus;
  "output.remove": Record<string, unknown>;
  "pipeline.clock": Record<string, unknown>;
  "pipeline.dot": PipelineDot;
  "pipeline.latency": Record<string, unknown>;
  "pipeline.list": Record<string, unknown>;
  "pipeline.queues": Record<string, unknown>;
  "preset.apply": ApplyResult;
  "preset.list": Record<string, unknown>;
  "preset.save": Record<string, unknown>;
  "program.get": ProgramState;
  "program.golive": GoLiveResult;
  "program.history": TakeRecord[];
  "program.revert": ProgramState;
  "program.take": ProgramState;
  "snapshot.get": Record<string, unknown>;
  "source.add": SourceStatus;
  "source.audio.set": SourceAudioState;
  "source.get": SourceStatus;
  "source.list": SourceStatus[];
  "source.remove": Record<string, unknown>;
  "source.seek": SourcePositionState;
  "task.cancel": Record<string, unknown>;
  "task.get": TaskView;
  "task.list": TaskView[];
}

export type MethodName = keyof MethodParams;

/** The payload of each event, by the name after `event/`. */
export interface EventPayloads {
  "snapshot": Snapshot;
  "program.took": ProgramTookEvent;
  "source.state": SourceStateEvent;
  "source.position": SourcePositionEvent;
  "output.state": OutputStateEvent;
  "adbreak.changed": AdbreakChangedEvent;
  "ui.changed": UiChangedEvent;
  "media.changed": MediaChangedEvent;
  "meters": Meters;
  "tally": Tally;
  "alert": AlertEvent;
  "telemetry": TelemetryEvent;
  "agent.state": Record<string, unknown>;
  "multiview.layout": MultiviewLayout;
  "multiview.frame": Uint8Array;
  "resync": Resync;
  "flush": Flush;
}

export type EventName = keyof EventPayloads;

/** What a method is, for a UI that builds its own buttons or its own REST calls. */
export interface MethodInfo {
  name: MethodName;
  summary: string;
  scope: string;
  mutating: boolean;
  destructive: boolean;
  rest?: { method: string; path: string };
}

export const METHODS: readonly MethodInfo[] = [
  { name: "adbreak.end", summary: "Cut a running ad short, or disarm one that is scheduled.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/adbreak/end" } },
  { name: "adbreak.start", summary: "Interrupt the programme with a clip, then rejoin live when it ends.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/adbreak/start" } },
  { name: "agent.state", summary: "The compact document written for agents: the programme, each source's state and a motion score saying how much its picture is changing.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/agent/state" } },
  { name: "codec.list", summary: "Every codec and element in the catalogue, which of them this machine actually has, and what it would pick.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/codecs" } },
  { name: "core.api", summary: "Every method, event and type as JSON Schema. The same document as protocol.json and `godwinmix --api-info`.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/core/api" } },
  { name: "core.doctor", summary: "The environment checks: GStreamer, the elements, the config, the disk and the ports. The same list `gmx doctor` prints.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/core/doctor" } },
  { name: "core.info", summary: "What this core is, what it can do, and where its edges are.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/core/info" } },
  { name: "core.session_log", summary: "The append only record of everything that happened, back as far as you ask.", scope: "admin", mutating: true, destructive: false, rest: { method: "GET", path: "/api/v1/core/session_log" } },
  { name: "core.shutdown", summary: "Stop the mixer, and with it the programme. Nothing else takes the show off air, so this is deliberately its own call.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/core/shutdown" } },
  { name: "core.startup_report", summary: "How long each stage of the start took, and what was over the 250 ms mark.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/core/startup_report" } },
  { name: "core.status", summary: "The full state: programme, every source, every output, the multiview grid, the encoder backend and any ad break.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/core/status" } },
  { name: "core.subscribe", summary: "Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush.", scope: "read", mutating: false, destructive: false },
  { name: "filter.add", summary: "Hang a filter on one source or on the programme, live.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/filters" } },
  { name: "filter.list", summary: "Every filter in place, with what it is and where it sits.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/filters" } },
  { name: "filter.remove", summary: "Take a filter out of the pipeline.", scope: "operate", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/filters/{id}" } },
  { name: "filter.set", summary: "Change a filter's settings in place. A filter that cannot take the change while running says so rather than being restarted behind your back.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/filters/{id}/set" } },
  { name: "log.gst", summary: "Raise GStreamer's own debug categories for a while, then let them fall back on their own.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/log/gst" } },
  { name: "log.levels", summary: "Every log level override in force, and the GStreamer categories still raised.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/log/levels" } },
  { name: "log.set", summary: "Change one instance's or one module's log level while the mixer runs.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/log/set" } },
  { name: "media.convert", summary: "Transcode a library file to a web safe copy, in the background.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/media/{id}/convert" } },
  { name: "media.list", summary: "The clips in the library, with durations and whether each has audio.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/media" } },
  { name: "media.remove", summary: "Delete a library file and its converted copy. Refused while it is a live source.", scope: "operate", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/media/{id}" } },
  { name: "media.upload", summary: "Stream a file into the library. HTTP only: the body is the file.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/media/upload" } },
  { name: "output.add", summary: "Send the programme to another destination. The encoder is shared, so adding one costs nothing on air.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/outputs" } },
  { name: "output.get", summary: "One destination.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/outputs/{id}" } },
  { name: "output.list", summary: "Every destination, with its state, reconnect count and how much is buffered.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/outputs" } },
  { name: "output.reconnect", summary: "Drop and re-establish one destination's connection now, without waiting for its reconnect policy.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/outputs/{id}/reconnect" } },
  { name: "output.remove", summary: "Stop sending to a destination and forget it. Other outputs are unaffected.", scope: "operate", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/outputs/{id}" } },
  { name: "pipeline.clock", summary: "The clock every pipeline is running against, and how far each one has got.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/pipeline/clock" } },
  { name: "pipeline.dot", summary: "One pipeline as a graphviz graph: every element, every pad and the caps negotiated between them.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/pipeline/dot" } },
  { name: "pipeline.latency", summary: "How much delay one pipeline is carrying, and which stage put it there.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/pipeline/latency" } },
  { name: "pipeline.list", summary: "Every pipeline running right now, by the name the other pipeline methods accept.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/pipeline/list" } },
  { name: "pipeline.queues", summary: "Every queue in one pipeline with how full it is, fullest first. A queue that stays full is where the trouble is.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/pipeline/queues" } },
  { name: "preset.apply", summary: "Put a preset on this core: its config, its scenes, its layout, its theme and its gallery mode. Pass dry_run to get the plan and write nothing.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/preset/apply" } },
  { name: "preset.list", summary: "Every preset this core can apply: the six built in, plus anything installed beside the binary or under ~/.godwinmix/presets.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/preset/list" } },
  { name: "preset.save", summary: "Turn this core's working setup into a preset directory somebody else can apply. Stream keys and the control token are replaced with placeholders.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/preset/save" } },
  { name: "program.get", summary: "What is on air, the programme running time, and what revert would go back to.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/program" } },
  { name: "program.golive", summary: "One call to put a web page on air: add the page, add the destination, and take the page as soon as it renders.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/program/golive" } },
  { name: "program.history", summary: "The last hundred takes, newest first, with the token that asked for each.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/program/history" } },
  { name: "program.revert", summary: "Take back to the shot before this one.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/program/revert" } },
  { name: "program.take", summary: "Put a source on programme. The cut is instant and the outgoing stream is not disturbed.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/program/take" } },
  { name: "snapshot.get", summary: "One JPEG: the whole contact sheet, the programme, or one source cut out of the mosaic.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/snapshot/{id}" } },
  { name: "source.add", summary: "Add a source while the mixer runs. Answers with the id it got and the whole source record.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/sources" } },
  { name: "source.audio.set", summary: "Move a source's audio: the fader, the mute, and for a superimposed page the balance between its own sound and the videos under it.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/sources/{id}/audio" } },
  { name: "source.get", summary: "One source. Refused with the ids that exist when there is no such source.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/sources/{id}" } },
  { name: "source.list", summary: "Every source, with its state, whether it has video and audio, and its fader.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/sources" } },
  { name: "source.remove", summary: "Remove a source. If it is on programme the mixer cuts to the slate first.", scope: "operate", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/sources/{id}" } },
  { name: "source.seek", summary: "Move a seekable source to a position. Answers with where it actually landed.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/sources/{id}/seek" } },
  { name: "task.cancel", summary: "Ask a piece of long running work to stop. Cooperative: the answer says the request landed, not that the work has stopped yet.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/task/cancel" } },
  { name: "task.get", summary: "How a piece of long running work is getting on, and its answer once it has one.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/task" } },
  { name: "task.list", summary: "Every background job this core knows about, newest first.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/task/list" } },
] as const;

/** The `ext` keys this api_level knows, and whether the core implements them yet. */
export const EXT_KEYS: Readonly<Record<string, { value: string; implemented: boolean }>> = {
  "multiview": { value: "{fps: 1..30, width: 320..1920} or false", implemented: true },
  "meters": { value: "true", implemented: true },
  "tally": { value: "true", implemented: true },
  "positions": { value: "true", implemented: true },
  "thumb": { value: "{fps}", implemented: false },
  "preview": { value: "{fps, width} or \"full\"", implemented: false },
  "telemetry": { value: "{hz: 1..10}", implemented: false },
  "agent": { value: "true or thresholds", implemented: false },
};

export const EVENT_NAMES: readonly EventName[] = [
  "snapshot",
  "program.took",
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
];

/**
 * One method per protocol method, over whatever transport the subclass has.
 *
 * `Client` extends this and replaces `_call`. Nothing else here knows how
 * the call travels, which is why the same generated file serves the
 * WebSocket client and anything else that can answer a JSON-RPC request.
 */
export class GeneratedMethods {
  /** Replaced by Client. Sends one call and answers its result. */
  _call(method: string, params: Record<string, unknown>): Promise<unknown> {
    return Promise.reject(new Error(`no transport for ${method}: build this object with connect()`));
  }

  /** Cut a running ad short, or disarm one that is scheduled. */
  adbreakEnd(): Promise<Record<string, unknown>> {
    return this._call("adbreak.end", {}) as Promise<Record<string, unknown>>;
  }

  /** Interrupt the programme with a clip, then rejoin live when it ends. */
  adbreakStart(params: AdBreakRequest): Promise<Record<string, unknown>> {
    return this._call("adbreak.start", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** The compact document written for agents: the programme, each source's state and a motion score saying how much its picture is changing. */
  agentState(params: AgentStateRequest = {}): Promise<Record<string, unknown>> {
    return this._call("agent.state", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Every codec and element in the catalogue, which of them this machine actually has, and what it would pick. */
  codecList(): Promise<Record<string, unknown>> {
    return this._call("codec.list", {}) as Promise<Record<string, unknown>>;
  }

  /** Every method, event and type as JSON Schema. The same document as protocol.json and `godwinmix --api-info`. */
  coreApi(): Promise<Record<string, unknown>> {
    return this._call("core.api", {}) as Promise<Record<string, unknown>>;
  }

  /** The environment checks: GStreamer, the elements, the config, the disk and the ports. The same list `gmx doctor` prints. */
  coreDoctor(): Promise<Record<string, unknown>> {
    return this._call("core.doctor", {}) as Promise<Record<string, unknown>>;
  }

  /** What this core is, what it can do, and where its edges are. */
  coreInfo(): Promise<CoreInfo> {
    return this._call("core.info", {}) as Promise<CoreInfo>;
  }

  /** The append only record of everything that happened, back as far as you ask. */
  coreSessionLog(params: SessionLogRequest = {}): Promise<Record<string, unknown>> {
    return this._call("core.session_log", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Stop the mixer, and with it the programme. Nothing else takes the show off air, so this is deliberately its own call. */
  coreShutdown(): Promise<Record<string, unknown>> {
    return this._call("core.shutdown", {}) as Promise<Record<string, unknown>>;
  }

  /** How long each stage of the start took, and what was over the 250 ms mark. */
  coreStartupReport(): Promise<Record<string, unknown>> {
    return this._call("core.startup_report", {}) as Promise<Record<string, unknown>>;
  }

  /** The full state: programme, every source, every output, the multiview grid, the encoder backend and any ad break. */
  coreStatus(): Promise<MixerStatus> {
    return this._call("core.status", {}) as Promise<MixerStatus>;
  }

  /** Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush. */
  coreSubscribe(params: SubscribeRequest = {}): Promise<SubscribeResult> {
    return this._call("core.subscribe", params as unknown as Record<string, unknown>) as Promise<SubscribeResult>;
  }

  /** Hang a filter on one source or on the programme, live. */
  filterAdd(params: AddFilterRequest): Promise<FilterRecord> {
    return this._call("filter.add", params as unknown as Record<string, unknown>) as Promise<FilterRecord>;
  }

  /** Every filter in place, with what it is and where it sits. */
  filterList(): Promise<FilterListing> {
    return this._call("filter.list", {}) as Promise<FilterListing>;
  }

  /** Take a filter out of the pipeline. */
  filterRemove(params: FilterIdRequest): Promise<FilterRemoved> {
    return this._call("filter.remove", params as unknown as Record<string, unknown>) as Promise<FilterRemoved>;
  }

  /** Change a filter's settings in place. A filter that cannot take the change while running says so rather than being restarted behind your back. */
  filterSet(params: SetFilterRequest): Promise<FilterRecord> {
    return this._call("filter.set", params as unknown as Record<string, unknown>) as Promise<FilterRecord>;
  }

  /** Raise GStreamer's own debug categories for a while, then let them fall back on their own. */
  logGst(params: LogGstRequest): Promise<LogGstResult> {
    return this._call("log.gst", params as unknown as Record<string, unknown>) as Promise<LogGstResult>;
  }

  /** Every log level override in force, and the GStreamer categories still raised. */
  logLevels(): Promise<Record<string, unknown>> {
    return this._call("log.levels", {}) as Promise<Record<string, unknown>>;
  }

  /** Change one instance's or one module's log level while the mixer runs. */
  logSet(params: LogSetRequest): Promise<Record<string, unknown>> {
    return this._call("log.set", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Transcode a library file to a web safe copy, in the background. */
  mediaConvert(params: NameRequest): Promise<ConversionState> {
    return this._call("media.convert", params as unknown as Record<string, unknown>) as Promise<ConversionState>;
  }

  /** The clips in the library, with durations and whether each has audio. */
  mediaList(): Promise<MediaListing> {
    return this._call("media.list", {}) as Promise<MediaListing>;
  }

  /** Delete a library file and its converted copy. Refused while it is a live source. */
  mediaRemove(params: NameRequest): Promise<Record<string, unknown>> {
    return this._call("media.remove", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Stream a file into the library. HTTP only: the body is the file. */
  mediaUpload(): Promise<Record<string, unknown>> {
    return this._call("media.upload", {}) as Promise<Record<string, unknown>>;
  }

  /** Send the programme to another destination. The encoder is shared, so adding one costs nothing on air. */
  outputAdd(params: AddOutputRequest): Promise<OutputStatus> {
    return this._call("output.add", params as unknown as Record<string, unknown>) as Promise<OutputStatus>;
  }

  /** One destination. */
  outputGet(params: IdRequest): Promise<OutputStatus> {
    return this._call("output.get", params as unknown as Record<string, unknown>) as Promise<OutputStatus>;
  }

  /** Every destination, with its state, reconnect count and how much is buffered. */
  outputList(): Promise<OutputStatus[]> {
    return this._call("output.list", {}) as Promise<OutputStatus[]>;
  }

  /** Drop and re-establish one destination's connection now, without waiting for its reconnect policy. */
  outputReconnect(params: IdRequest): Promise<OutputStatus> {
    return this._call("output.reconnect", params as unknown as Record<string, unknown>) as Promise<OutputStatus>;
  }

  /** Stop sending to a destination and forget it. Other outputs are unaffected. */
  outputRemove(params: IdRequest): Promise<Record<string, unknown>> {
    return this._call("output.remove", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** The clock every pipeline is running against, and how far each one has got. */
  pipelineClock(): Promise<Record<string, unknown>> {
    return this._call("pipeline.clock", {}) as Promise<Record<string, unknown>>;
  }

  /** One pipeline as a graphviz graph: every element, every pad and the caps negotiated between them. */
  pipelineDot(params: PipelineRequest = {}): Promise<PipelineDot> {
    return this._call("pipeline.dot", params as unknown as Record<string, unknown>) as Promise<PipelineDot>;
  }

  /** How much delay one pipeline is carrying, and which stage put it there. */
  pipelineLatency(params: PipelineRequest = {}): Promise<Record<string, unknown>> {
    return this._call("pipeline.latency", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Every pipeline running right now, by the name the other pipeline methods accept. */
  pipelineList(): Promise<Record<string, unknown>> {
    return this._call("pipeline.list", {}) as Promise<Record<string, unknown>>;
  }

  /** Every queue in one pipeline with how full it is, fullest first. A queue that stays full is where the trouble is. */
  pipelineQueues(params: PipelineRequest = {}): Promise<Record<string, unknown>> {
    return this._call("pipeline.queues", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Put a preset on this core: its config, its scenes, its layout, its theme and its gallery mode. Pass dry_run to get the plan and write nothing. */
  presetApply(params: ApplyRequest): Promise<ApplyResult> {
    return this._call("preset.apply", params as unknown as Record<string, unknown>) as Promise<ApplyResult>;
  }

  /** Every preset this core can apply: the six built in, plus anything installed beside the binary or under ~/.godwinmix/presets. */
  presetList(): Promise<Record<string, unknown>> {
    return this._call("preset.list", {}) as Promise<Record<string, unknown>>;
  }

  /** Turn this core's working setup into a preset directory somebody else can apply. Stream keys and the control token are replaced with placeholders. */
  presetSave(params: SaveRequest): Promise<Record<string, unknown>> {
    return this._call("preset.save", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** What is on air, the programme running time, and what revert would go back to. */
  programGet(): Promise<ProgramState> {
    return this._call("program.get", {}) as Promise<ProgramState>;
  }

  /** One call to put a web page on air: add the page, add the destination, and take the page as soon as it renders. */
  programGolive(params: GoLiveRequest): Promise<GoLiveResult> {
    return this._call("program.golive", params as unknown as Record<string, unknown>) as Promise<GoLiveResult>;
  }

  /** The last hundred takes, newest first, with the token that asked for each. */
  programHistory(params: HistoryRequest = {}): Promise<TakeRecord[]> {
    return this._call("program.history", params as unknown as Record<string, unknown>) as Promise<TakeRecord[]>;
  }

  /** Take back to the shot before this one. */
  programRevert(): Promise<ProgramState> {
    return this._call("program.revert", {}) as Promise<ProgramState>;
  }

  /** Put a source on programme. The cut is instant and the outgoing stream is not disturbed. */
  programTake(params: TakeRequest = {}): Promise<ProgramState> {
    return this._call("program.take", params as unknown as Record<string, unknown>) as Promise<ProgramState>;
  }

  /** One JPEG: the whole contact sheet, the programme, or one source cut out of the mosaic. */
  snapshotGet(params: SnapshotRequest): Promise<Record<string, unknown>> {
    return this._call("snapshot.get", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Add a source while the mixer runs. Answers with the id it got and the whole source record. */
  sourceAdd(params: AddSourceRequest): Promise<SourceStatus> {
    return this._call("source.add", params as unknown as Record<string, unknown>) as Promise<SourceStatus>;
  }

  /** Move a source's audio: the fader, the mute, and for a superimposed page the balance between its own sound and the videos under it. */
  sourceAudioSet(params: AudioSetParams): Promise<SourceAudioState> {
    return this._call("source.audio.set", params as unknown as Record<string, unknown>) as Promise<SourceAudioState>;
  }

  /** One source. Refused with the ids that exist when there is no such source. */
  sourceGet(params: IdRequest): Promise<SourceStatus> {
    return this._call("source.get", params as unknown as Record<string, unknown>) as Promise<SourceStatus>;
  }

  /** Every source, with its state, whether it has video and audio, and its fader. */
  sourceList(): Promise<SourceStatus[]> {
    return this._call("source.list", {}) as Promise<SourceStatus[]>;
  }

  /** Remove a source. If it is on programme the mixer cuts to the slate first. */
  sourceRemove(params: IdRequest): Promise<Record<string, unknown>> {
    return this._call("source.remove", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Move a seekable source to a position. Answers with where it actually landed. */
  sourceSeek(params: SeekParams): Promise<SourcePositionState> {
    return this._call("source.seek", params as unknown as Record<string, unknown>) as Promise<SourcePositionState>;
  }

  /** Ask a piece of long running work to stop. Cooperative: the answer says the request landed, not that the work has stopped yet. */
  taskCancel(params: TaskRequest): Promise<Record<string, unknown>> {
    return this._call("task.cancel", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** How a piece of long running work is getting on, and its answer once it has one. */
  taskGet(params: TaskRequest): Promise<TaskView> {
    return this._call("task.get", params as unknown as Record<string, unknown>) as Promise<TaskView>;
  }

  /** Every background job this core knows about, newest first. */
  taskList(): Promise<TaskView[]> {
    return this._call("task.list", {}) as Promise<TaskView[]>;
  }

}
