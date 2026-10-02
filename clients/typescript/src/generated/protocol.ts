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

/** What pressing the button does. */
export type ActionKind = "set-config" | "install-plugin" | "enable-plugin" | "open" | "retry" | "restart";

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

/** `channel.destination.add`. Send a channel's stream on to a platform. */
export interface AddDestinationRequest {
  enabled?: boolean | null;
  id: string;
  key?: string | null;
  label?: string | null;
  platform: string;
  rendition?: RenditionChoice | null;
  server?: string | null;
  stream?: string | null;
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

/** `scene.item.filter.add`. */
export interface AddItemFilterRequest {
  draft?: string | null;
  item: string;
  name?: string | null;
  params?: Record<string, unknown>;
  scene: string;
  type: string;
}

/** `scene.item.add`. */
export interface AddItemRequest {
  content: unknown;
  draft?: string | null;
  enter?: unknown;
  exit?: unknown;
  name?: string | null;
  scene: string;
  transform?: unknown;
  visible?: boolean | null;
}

/**
 * `output.add`. The id and the URL are the whole of it for an RTMP
 * destination; anything else a kind understands rides in `params`.
 */
export interface AddOutputRequest {
  id: string;
  policy?: string | null;
  rendition?: Record<string, unknown> | null;
  uri?: string;
  [key: string]: unknown;
}

/** `plugin.add`. */
export interface AddPluginRequest {
  source: string;
}

export interface AddSceneRequest {
  color?: string | null;
  name: string;
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

/** One condition that holds now. */
export interface Alarm {
  detail: string;
  kind: AlarmKind;
  since_ms: number;
}

/** What an alarm is about. */
export type AlarmKind = "no-input" | "stall" | "black" | "freeze" | "silence" | "cc-errors" | "loss" | "output-failed" | "governor-refused" | "shed";

/**
 * A show's alarms, as a person sets them from the page. Left out fields
 * keep the measuring side's defaults; a duration of 0 switches that check
 * off.
 */
export interface AlarmSettings {
  black_ms?: number | null;
  enabled?: boolean | null;
  freeze_ms?: number | null;
  silence_dbfs?: number | null;
  silence_ms?: number | null;
}

/** The nine alignment keywords, used to place content inside its frame. */
export type Align = "top-left" | "top-center" | "top-right" | "center-left" | "center" | "center-right" | "bottom-left" | "bottom-center" | "bottom-right";

/** When a change to a key takes effect. */
export type Applies = "live" | "next_source" | "restart";

/** `scene.apply_graphic`. */
export interface ApplyGraphicRequest {
  frame?: boolean;
  graphic: string;
  item?: string | null;
  play?: boolean;
  stop?: boolean;
  values?: Record<string, unknown>;
}

/** `scene.apply_layout`. */
export interface ApplyLayoutRequest {
  duration_ms?: number | null;
  easing?: string | null;
  layout: string;
  name?: string | null;
  scene?: string | null;
  values?: Record<string, unknown>;
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

/** A file the collection carries with it. */
export interface Asset {
  path: string;
  sha256?: string | null;
  size?: number | null;
}

/**
 * Whether the item's source is heard. A source is audible when any live item
 * of it says so, which is OBS's behaviour and changes no pad topology.
 */
export type Audio = "follow" | "always" | "never";

export type AudioCodec = "aac" | "opus" | "mp3" | "ac3" | "pcm" | "other";

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

/** A sound as it is. */
export interface AudioShape {
  bitrate_kbps: number;
  channels: number;
  codec: AudioCodec;
  sample_rate: number;
}

/** The audio an output wants. Every field left out is taken from the source. */
export interface AudioWant {
  bitrate_kbps?: number | null;
  channels?: number | null;
  codec?: AudioCodec | null;
  sample_rate?: number | null;
}

export interface BackendInfo {
  audio_decoder: string;
  audio_encoder: string;
  hardware_accelerated: boolean;
  video_decoder: string;
  video_encoder: string;
}

/** An input's backup: an input with no backup of its own. */
export interface BackupInput {
  params?: Record<string, unknown> | null;
  program?: number | null;
  uri: string;
}

/** `scene.item.bind`. */
export interface BindRequest {
  draft?: string | null;
  item: string;
  param: string;
  prop: string;
  scene: string;
}

/** OBS's blend enum, so an import carries across unchanged. */
export type Blend = "normal" | "add" | "screen" | "multiply" | "lighten" | "darken" | "subtract";

/** How media crosses between a node and the core. */
export type BridgeTransport = "rtp" | "srt" | "whip";

/** What a batch costs, priced by the governor without taking anything. */
export interface BulkPlan {
  assumed_input: string;
  cost: Cost;
  fits: boolean;
  have: Cost;
}

/** What an importer is told before it reads the document. */
export interface Bundle {
  assets: BundleAsset[];
  bundle_version: number;
  canvas: Canvas;
  id: Id;
  name: string;
  requires: Requirement[];
  skipped?: string[];
  written_by: string;
}

/** One file carried in the bundle. */
export interface BundleAsset {
  id: Id;
  path: string;
  sha256: string;
  size: number;
}

/** `governor.calibrate`. */
export interface CalibrateRequest {
  confirm?: boolean;
}

export interface CalibrateResult {
  started: boolean;
}

/**
 * The output raster. One per collection in this release; 11 section 1 leaves
 * room for several.
 */
export interface Canvas {
  fps: number;
  height: number;
  width: number;
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

/** `channel.certificate.generate`: a self signed certificate. */
export interface CertificateGenerateRequest {
  names?: string[];
}

/** The certificate RTMPS answers with. The private key never leaves the core. */
export interface CertificateInfo {
  created: string;
  fingerprint: string;
  names?: string[];
  source: string;
}

/** `channel.certificate.set`: a certificate and its private key, as PEM. */
export interface CertificateSetRequest {
  cert: string;
  key: string;
}

/** One thing an import does or would do. */
export interface Change {
  action: string;
  id: string;
  note?: string | null;
  part: string;
  to?: string | null;
}

/** A named place encoders publish to, over every protocol it has switched on. */
export interface Channel {
  app: string;
  auto_source: boolean;
  destinations?: Destination[];
  enabled: boolean;
  id: string;
  key_mode: KeyMode;
  keys: ChannelKey[];
  name: string;
  protocols?: ChannelProtocol[];
  publish: ChannelPublish;
  rtmps?: Rtmps;
  streams: ChannelStream[];
}

/** `channel.add`. */
export interface ChannelAddRequest {
  app?: string | null;
  auto_source?: boolean | null;
  key_mode?: KeyMode | null;
  name: string;
  protocols?: ChannelProtocol[] | null;
}

/** What `channel.add` answers: the channel and its first key. */
export interface ChannelAdded {
  channel: Channel;
  key: NewKey;
}

/** One key, as a list shows it. */
export interface ChannelKey {
  created: string;
  hint: string;
  id: string;
  label: string;
}

/** `channel.key.add`. */
export interface ChannelKeyAddRequest {
  id: string;
  label?: string | null;
}

/** `channel.key.remove`. */
export interface ChannelKeyRemoveRequest {
  id: string;
  key: string;
}

/** `channel.key.reveal`: one key of one channel. */
export interface ChannelKeyRevealRequest {
  id: string;
  key: string;
}

/** `channel.list`. */
export interface ChannelList {
  certificate?: CertificateInfo | null;
  channels: Channel[];
  hosts?: string[];
  listeners?: Listener[];
  rtmp: RtmpInfo;
}

/**
 * A way a publisher reaches a channel. RTMPS is `Rtmps`, set apart
 * because it has a port of its own.
 */
export type ChannelProtocol = "rtmp" | "srt" | "whip";

/** Where an encoder is pointed. */
export interface ChannelPublish {
  addresses?: PublishAddress[];
  example: string;
  server: string;
}

/** What `channel.remove` answers. */
export interface ChannelRemoved {
  removed: string;
}

/** `channel.set`: only what is named moves. */
export interface ChannelSetRequest {
  app?: string | null;
  auto_source?: boolean | null;
  enabled?: boolean | null;
  id: string;
  key_mode?: KeyMode | null;
  name?: string | null;
  protocols?: ChannelProtocol[] | null;
  rtmps?: Rtmps | null;
}

/** One stream on a channel. */
export interface ChannelStream {
  audio?: StreamAudio | null;
  dropped_gops?: number;
  from: string;
  key?: string | null;
  name: string;
  protocol?: string | null;
  relay?: string | null;
  since_ms: number;
  source?: string | null;
  state: string;
  video?: StreamVideo | null;
}

/** One key this call changed, and when the change takes effect. */
export interface ConfigChanged {
  applies: Applies;
  key: string;
  note?: string | null;
}

export interface ConfigGetRequest {
  keys?: string[];
}

export interface ConfigGetResult {
  keys: ConfigKey[];
  needs_restart: string[];
  path: string;
}

/** One setting as `config.get` reports it. */
export interface ConfigKey {
  applies: Applies;
  default: unknown;
  key: string;
  overridden_by?: string | null;
  pending: boolean;
  secret: boolean;
  set?: boolean | null;
  source: string;
  value: unknown;
}

export interface ConfigResetRequest {
  dry_run?: boolean;
  keys: string[];
}

export interface ConfigSetRequest {
  dry_run?: boolean;
  values: Record<string, unknown>;
}

/** What `config.set` and `config.reset` answer with. */
export interface ConfigSetResult {
  applied: string[];
  changed: ConfigChanged[];
  dry_run: boolean;
  needs_restart: string[];
  next_source: string[];
  path: string;
  unchanged: string[];
}

/**
 * How the bytes leave. Decides which codecs are allowed: FLV carries H.264
 * (and HEVC and AV1 in enhanced RTMP), WebRTC wants VP8, VP9, H.264 or AV1.
 */
export type Container = "flv" | "mpeg-ts" | "mp4-fragmented" | "mkv" | "hls" | "ll-hls" | "dash" | "rtp" | "webrtc";

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
  restart?: RestartInfo;
  supervised?: boolean;
  tls?: TlsInfo | null;
  token?: TokenInfo | null;
  ui?: UiDefaults | null;
  version: string;
}

/** What running one piece of work costs, in units the governor adds up. */
export interface Cost {
  cpu_millicores: number;
  device_millis: number;
  device_sessions: number;
  egress_kbps: number;
  memory_mib: number;
}

export interface CpuUse {
  cores: number;
  measured_millicores?: number | null;
  room_millicores: number;
  used_millicores: number;
}

export interface CreateFromRequest {
  layout?: string | null;
  name?: string | null;
  sources: string[];
}

/**
 * How much of the content's own pixels to trim, normalised 0 to 1 so it
 * survives a canvas change. vMix and CasparCG do this; OBS crops in pixels,
 * which is why an OBS collection moved from 1080p to 720p loses its crops.
 */
export interface Crop {
  bottom: number;
  left: number;
  right: number;
  top: number;
}

/**
 * One destination as a client sees it. The key never appears: `has_key`
 * says whether there is one.
 */
export interface Destination {
  enabled: boolean;
  error?: string | null;
  has_key: boolean;
  id: string;
  kbps: number;
  label: string;
  plan?: DestinationPlan | null;
  platform: string;
  playback?: Playback | null;
  reconnects: number;
  refused?: DestinationRefusal | null;
  rendition?: RenditionChoice | null;
  since_ms: number;
  state: DestinationState;
  stream: string;
  uri_host: string;
}

/** Copied as it arrives, or converted. */
export type DestinationMode = "copy" | "transcode";

/** The plan's answer for one destination. */
export interface DestinationPlan {
  audio?: AudioShape | null;
  encoder?: string | null;
  encoder_reason?: string | null;
  mode: DestinationMode;
  nodes: string[];
  reason: string;
  stream: string;
  video?: VideoShape | null;
}

/**
 * Why a destination that asked for a rendition is not sending, and what
 * would. `error` on the destination carries the same sentence.
 */
export interface DestinationRefusal {
  advice?: RenditionAdvice[];
  code: string;
  have?: Cost | null;
  message: string;
  need?: Cost | null;
}

/** Where a destination has got to. */
export type DestinationState = "off" | "waiting" | "connecting" | "live" | "reconnecting" | "failed";

/** Use of one hardware device by a plan. */
export interface DeviceTotal {
  millis: number;
  sessions: number;
}

export interface DeviceUse {
  id: string;
  kind: string;
  room_millis: number;
  sessions_max?: number | null;
  sessions_used: number;
  used_millis: number;
}

export interface DiscoverAnswer {
  found: Found[];
}

/** `device.discover`. */
export interface DiscoverRequest {
  timeout_ms?: number | null;
}

export interface DiscoverRequest2 {
  timeout_ms?: number | null;
}

/** `scene.edit.begin`. */
export interface DraftRecord {
  draft: string;
  live: boolean;
  scene: string;
  view?: SceneView | null;
}

/** `scene.edit.apply` and `discard`. */
export interface DraftRequest {
  draft: string;
}

export interface DuplicateSceneRequest {
  name?: string | null;
  scene: string;
}

/** `source.duplicate`. */
export interface DuplicateSourceRequest {
  id: string;
  name?: string | null;
  new_id?: string | null;
}

/** `scene.edit.begin`. */
export interface EditBeginRequest {
  live?: boolean;
  scene: string;
}

export interface EnrolRequest {
  address?: string | null;
  name: string;
  ttl_secs?: number | null;
}

/**
 * One thing a client can offer as a button. `label` is the button's text,
 * `kind` says what pressing it does, and the other fields are the ones that
 * kind uses. Flat rather than an enum with data, so every generated client
 * reads every field.
 */
export interface ErrorAction {
  after_ms?: number | null;
  applies?: string | null;
  dialog?: string | null;
  key?: string | null;
  kind: ActionKind;
  label: string;
  name?: string | null;
  panel?: string | null;
  value?: unknown;
}

export interface ExportRequest {
  include_media?: boolean;
  include_secrets?: boolean;
  name?: string | null;
  page?: unknown;
}

/** `scene.export`. */
export interface ExportRequest2 {
  collection?: string | null;
  format?: string | null;
  path?: string | null;
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
  preview?: PreviewExt | null;
  tally?: boolean;
  telemetry?: TelemetryExt | null;
  [key: string]: unknown;
}

/** What a field holds. */
export type FieldType = "text" | "color";

/** One field and what it shows now. */
export interface FieldValue {
  default: string;
  fit?: number | null;
  label: string;
  name: string;
  set: boolean;
  type: FieldType;
  value: string;
}

/** One filter in an item's chain. */
export interface Filter {
  enabled?: boolean;
  name?: string | null;
  params?: unknown;
  type: string;
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

/** A source filter that had to be copied onto each placement. */
export interface FilterReport {
  filter: string;
  obs_type: string;
  placements: string[];
  source: string;
}

/** One thing the validator found. */
export interface Finding {
  code: string;
  detail?: unknown;
  items?: Id[];
  message: string;
  scene?: Id | null;
  severity: Severity;
}

/**
 * How content fills its frame. SVG's vocabulary, which replaces OBS's seven
 * bounds types and maps onto `sizing-policy` on a `glvideomixer` pad.
 */
export type Fit = "none" | "contain" | "cover" | "stretch" | "fit-width" | "fit-height" | "max";

/**
 * Content on the wire. The same four shapes as the tree, except that a group
 * names no children: they are records whose parent is the group.
 */
export type FlatContent = {
  source: string;
  type: "source";
} | {
  overrides?: Record<string, unknown>;
  ref: Id;
  type: "ref";
} | {
  graphic: string;
  params?: unknown;
  type: "graphic";
} | {
  type: "group";
};

/** `event/flush`: the end of a batch. A client renders here and not before. */
export interface Flush {
  seq: number;
}

/** One thing found on the network. */
export interface Found {
  address: string;
  api: number;
  name: string;
  role: string;
}

/** A frame rate as a fraction, so 29.97 is exact. */
export interface Fps {
  den: number;
  num: number;
}

/** The rectangle an item is fitted into. */
export interface Frame {
  h: number;
  w: number;
}

/** One item's derived box. */
export interface Geometry {
  height: number;
  item: Id;
  opacity: number;
  path: string;
  source?: string | null;
  source_height: number;
  source_width: number;
  width: number;
  x: number;
  y: number;
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

/** `governor.status`. */
export interface GovernorStatus {
  calibrated_at?: number | null;
  calibrating?: boolean;
  cpu: CpuUse;
  devices: DeviceUse[];
  egress_kbps: number;
  fingerprint?: string | null;
  ingress_kbps?: number;
  shed: ShedNote[];
}

/** `scene.graphic.list`. */
export interface GraphicListing {
  graphics: GraphicType[];
}

/** One graphic this core can place, as the catalogue has it. */
export interface GraphicType {
  designer?: unknown;
  manifest: string;
  ograf: Ograf;
  plugin: string;
  provide: string;
  type_id: string;
}

/** `source.group`. */
export interface GroupSourcesRequest {
  name?: string | null;
  sources: string[];
}

/**
 * Everything in the document that is not a scene or an item: the name, the
 * canvas, the collection's parameters, the transitions it carries, the assets
 * and the source labels.
 *
 * It is not a record and it has no id, so it cannot be diffed the way the
 * tree is. It is carried whole, because it is small and because the
 * alternative is that a command touching only the header produces an empty
 * patch and is thrown away by `edit`, which is exactly what used to happen to
 * `scene.params.set`, `source.set` and `source.group`: all three answered with
 * the change and none of them kept it.
 */
export interface Header {
  assets?: Record<string, unknown>;
  canvas: Canvas;
  name: string;
  params: unknown;
  sources?: Record<string, unknown>;
  transitions?: Transition2[];
}

/** The header as it was and as it is. */
export interface HeaderChange {
  after: Header;
  before: Header;
}

/** A show's health. */
export interface Health {
  alarms: Alarm[];
  state: HealthState;
}

/**
 * `event/health`, from a show that composites, about itself. The station
 * sends it on to clients as `event/show.health` with the show's id.
 */
export interface HealthEvent {
  health: Health;
}

/** The one word a monitoring wall colours a row by. */
export type HealthState = "ok" | "warning" | "alarm" | "off";

/** `program.history`. */
export interface HistoryRequest {
  limit?: number | null;
}

/** `scene.undo` and `scene.redo`. */
export interface HistoryStep {
  patch: Patch;
  redo: number;
  undo: number;
}

/** An `hls://` output's params, as an `hls/output` takes them. */
export interface HlsOutputParams {
  low_latency?: boolean | null;
  part_ms?: number | null;
  segment_ms?: number | null;
  viewer_key?: string | null;
  window?: number | null;
}

/** A UUID in the hyphenated form. Minted ids are version 7 (time ordered); ids derived from a layout are version 8. */
export type Id = string;

/**
 * An id on its own: `source.get`, `source.remove`, `output.remove`,
 * `output.reconnect`, `media.remove`.
 */
export interface IdRequest {
  id: string;
}

export interface ImportObsRequest {
  add_sources?: boolean;
  content?: string | null;
  path?: string | null;
}

export interface ImportReport {
  config_toml?: string | null;
  filters_duplicated?: FilterReport[];
  items: number;
  scenes: string[];
  skipped: string[];
  source_report?: SourceReport[];
  sources: string[];
  sources_added?: string[] | null;
  sources_not_added?: SourceNotAdded[] | null;
}

export interface ImportRequest {
  dry_run?: boolean | null;
  file: unknown;
  machine?: boolean;
  mode?: Mode;
}

/** `scene.import`. */
export interface ImportRequest2 {
  path: string;
}

/** What `scene.import` answers with. */
export interface ImportedReport {
  assets_at?: string | null;
  bundle: Bundle;
  items: number;
  missing_plugins?: string[];
  relink?: Relink[];
  scenes: string[];
}

/**
 * What a show without compositing takes in. A show that composites makes
 * its input its one source.
 */
export interface InputSpec {
  backup?: BackupInput | null;
  params?: Record<string, unknown> | null;
  program?: number | null;
  uri: string;
}

/** What the input is doing, as the host last counted it. */
export interface InputStats {
  audio_channels?: number;
  audio_codec?: string | null;
  cc_errors?: number;
  fps?: number;
  height?: number;
  kbps?: number;
  keyframe_ms?: number | null;
  last_frame_ms?: number | null;
  packets_lost?: number;
  video_codec?: string | null;
  width?: number;
}

/** One running instance and its cost. */
export interface InstanceRecord {
  buffers_dropped: number;
  cpu_percent?: number | null;
  instance: string;
  media_latency_ms?: number | null;
  pid?: number | null;
  plugin: string;
  provide: string;
  restarts: number;
  rss_bytes?: number | null;
  state: string;
}

export type ItemEdge = "left" | "right" | "top" | "bottom";

/** `scene.item.filter.set` and `remove`. */
export interface ItemFilterRequest {
  draft?: string | null;
  enabled?: boolean | null;
  filter: string;
  item: string;
  params?: Record<string, unknown>;
  scene: string;
}

/**
 * An item's props, which is an `Item` with the children lifted out into their
 * own records.
 */
export interface ItemProps {
  audio: Audio;
  bind?: Record<string, unknown>;
  blend: Blend;
  content: FlatContent;
  crop: Crop;
  enter?: ItemTransition | null;
  exit?: ItemTransition | null;
  filters?: Filter[];
  locked: boolean;
  name?: string | null;
  opacity: number;
  transform: Transform;
  visible: boolean;
}

/** Anything that names one item. */
export interface ItemRequest {
  draft?: string | null;
  item: string;
  scene: string;
}

/** `scene.item.schema`. */
export interface ItemSchemaRequest {
  type: string;
}

/** One way on or off the canvas for one item. */
export interface ItemTransition {
  duration_ms?: number;
  easing?: string | null;
  edge?: ItemEdge | null;
  on_take?: boolean;
  type: ItemTransitionKind;
}

export type ItemTransitionKind = "cut" | "fade" | "slide" | "zoom" | "wipe";

/**
 * `scene.item.align`, `distribute`, `fit_to_canvas`, `cover_canvas`,
 * `arrange_grid`, `match_size`, `group`.
 */
export interface ItemsRequest {
  axis?: string | null;
  cols?: number | null;
  draft?: string | null;
  duration_ms?: number | null;
  easing?: string | null;
  edge?: string | null;
  items: string[];
  name?: string | null;
  scene: string;
  seq?: number | null;
  to?: string | null;
}

/** What `channel.key.add` answers. */
export interface KeyAdded {
  key: NewKey;
}

/** How a publisher gives its key. */
export type KeyMode = "query" | "stream";

/**
 * What `channel.key.reveal` answers: the key itself, and nothing a list
 * would carry.
 */
export interface KeyRevealed {
  secret: string;
}

/** A custom ladder. */
export interface LadderRef {
  ladder: RenditionRequest[];
}

/** A scene's geometry, for copying onto another one. */
export interface Layout {
  canvas: Canvas;
  items: LayoutItem[];
  scene: string;
}

/** `scene.layout.copy` and `paste`. */
export interface LayoutClipboardRequest {
  layout?: unknown;
  match?: string | null;
  scene: string;
}

export interface LayoutInfo {
  description: string;
  name: string;
  params: unknown;
  sources: string[];
}

/**
 * One item's geometry: everything about where it sits and nothing about what
 * it shows.
 */
export interface LayoutItem {
  crop: Crop;
  name?: string | null;
  opacity: number;
  transform: Transform;
  visible: boolean;
}

export interface LayoutListing {
  layouts: LayoutInfo[];
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

/** One listener a channel needs, and whether it is open now. */
export interface Listener {
  because: string[];
  last_port?: number | null;
  loopback?: boolean;
  open: boolean;
  port: number;
  problem?: string | null;
  protocol: string;
  transport: string;
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

/** `scene.history.mark`. */
export interface MarkRequest {
  label?: string | null;
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
  created?: boolean;
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

/**
 * `source.missing`: the ids a scene draws, or none for every source the
 * mixer knows is not running.
 */
export interface MissingRequest {
  ids?: string[];
}

/** One source that is not running, and what would bring it back. */
export interface MissingSource {
  action?: ErrorAction | null;
  error?: string | null;
  id: string;
  name?: string | null;
  restore: boolean;
  type?: string | null;
  why: MissingWhy;
}

/** Why a source is not running. */
export type MissingWhy = "failed" | "not_started" | "removed" | "unknown";

export interface MixerStatus {
  ad?: AdStatus | null;
  backend: BackendInfo;
  multiview: MultiviewStatus;
  outputs: OutputStatus[];
  program?: string | null;
  running_time_ms: number;
  scene?: string | null;
  sources: SourceStatus[];
  uptime_secs: number;
}

export type Mode = "replace" | "merge";

/** `scene.item.move` and `scene.item.copy`. */
export interface MoveItemRequest {
  item: string;
  scene: string;
  to_scene: string;
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

/**
 * A key as it is made, with its secret. Afterwards only an admin gets the
 * secret again, one key at a time, from `channel.key.reveal`.
 */
export interface NewKey {
  id: string;
  label: string;
  secret: string;
}

export interface NodeInstance {
  detail?: string | null;
  instance: string;
  latency_ms: number;
  state: string;
}

export interface NodeListing {
  listening: boolean;
  nodes: NodeView[];
}

export interface NodeName {
  id: string;
}

export interface NodePlugin {
  name: string;
  provides?: string[];
  version: string;
}

/**
 * What `node.get` reports about one node, and what `node.list` reports about
 * all of them.
 */
export interface NodeView {
  address?: string | null;
  clock_jitter_ms: number;
  clock_offset_ms: number;
  clock_synced: boolean;
  heartbeat_age_ms: number;
  identity?: string | null;
  instances?: NodeInstance[];
  name: string;
  platform?: string | null;
  plugins?: NodePlugin[];
  provides?: string[];
  state: string;
  version?: string | null;
}

/**
 * The OGraf manifest, in the subset this host reads.
 *
 * Everything else the file carries is kept in `rest` and passed on: OGraf is
 * an EBU specification that will grow, and a key this build has not heard of
 * is a key a newer client may want. Dropping it here would make the core the
 * thing that has to be upgraded first.
 */
export interface Ograf {
  description?: string | null;
  id?: string;
  main?: string;
  name?: string;
  schema?: unknown;
  stepCount?: number;
  supportsNonRealTime?: boolean;
  supportsRealTime?: boolean;
  version?: string | null;
}

export type OutputState = "connecting" | "live" | "reconnecting" | "failed";

/** What one output is doing. */
export interface OutputStats {
  cpu_millicores?: number;
  encoder?: string | null;
  id: string;
  kbps?: number;
  reconnects?: number;
  rendition_text?: string;
  state?: string;
}

export interface OutputStatus {
  has_key: boolean;
  id: string;
  queue_secs: number;
  reconnects: number;
  rendition?: RenditionChoice | null;
  shed?: string | null;
  state: OutputState;
  uri_host: string;
  [key: string]: unknown;
}

/** A sparse change to one item of a referenced scene. */
export interface Override {
  crop?: Crop | null;
  opacity?: number | null;
  params?: unknown;
  transform?: Transform | null;
  visible?: boolean | null;
}

/** `scene.params.set`. */
export interface ParamsRequest {
  scene?: string | null;
  values?: Record<string, unknown>;
}

/** What changed in one transaction. */
export interface Patch {
  added?: ProtocolRecord[];
  client_seq?: number | null;
  header?: HeaderChange | null;
  label?: string | null;
  removed?: Id[];
  scope: string;
  seq: number;
  source_client?: string | null;
  updated?: Update[];
}

export interface PathCreateRequest {
  name: string;
  parent: string;
}

/** One folder inside the one listed. */
export interface PathEntry {
  name: string;
  path: string;
  writable: boolean;
}

export interface PathListRequest {
  path?: string | null;
}

/** What `path.list` and `path.create` answer with. */
export interface PathListing {
  dirs: PathEntry[];
  parent?: string | null;
  path: string;
  roots: PathRoot[];
  truncated: boolean;
  writable: boolean;
}

/** A place the picker may start from. */
export interface PathRoot {
  label: string;
  path: string;
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

/** Where an instance runs: core, in-process, sidecar, or node:<name>. */
export type Place = string;

/** One node of a plan, as the page draws it. */
export interface PlanNode {
  cost: Cost;
  encoder?: string | null;
  id: string;
  kind: string;
  reason?: PlanReason | null;
  serves: string[];
  shed?: string | null;
}

/** Why the planner decided what it did. */
export interface PlanReason {
  code: string;
  text: string;
}

/** `rendition.plan`. */
export interface PlanRequest {
  scope?: string | null;
}

export interface PlanTotals {
  cpu_millicores: number;
  devices: Record<string, unknown>;
  egress_kbps: number;
}

/** `rendition.plan`, and the `plan` of `event/rendition.plan`. */
export interface PlanView {
  nodes: PlanNode[];
  totals: PlanTotals;
}

/**
 * The links of an output served as HLS from the control port, each with
 * the output's viewer key on it.
 */
export interface Playback {
  dash_url_path: string;
  master_url_path: string;
  viewers: number;
}

/** The whole of one plugin, for an agent about to use it. */
export interface PluginDescription {
  description: string;
  enabled: boolean;
  hooks: string[];
  instances: InstanceRecord[];
  manifest: unknown;
  name: string;
  problem?: string | null;
  provides: string[];
  root: string;
  schemas: Record<string, unknown>;
  skills: Record<string, unknown>;
  source?: string;
  tools: string[];
  trust: string;
  trust_detail: string;
  version: string;
}

export interface PluginListing {
  plugins: PluginRecord[];
  plugins_dir: string;
}

/**
 * Anything that names one plugin.
 *
 * The field is `id` because that is what the REST layer fills in from
 * `/api/v1/plugins/{id}`, and a plugin's id is its name: the namespace of
 * every id it contributes. `name` is accepted as well, for a JSON-RPC caller
 * who wrote the obvious thing.
 */
export interface PluginName {
  id: string;
}

/** One plugin as the core reports it. */
export interface PluginRecord {
  description: string;
  enabled: boolean;
  hooks: string[];
  instances: InstanceRecord[];
  name: string;
  problem?: string | null;
  provides: string[];
  root: string;
  source?: string;
  tools: string[];
  trust: string;
  trust_detail: string;
  version: string;
}

export interface PluginRemoved {
  provides: string[];
  removed: string;
  tools: string[];
}

export interface PluginSettings {
  name: string;
  schemas: Record<string, unknown>;
  settings: Record<string, unknown>;
}

/** What `plugin.update` answers with. */
export interface PluginUpdated {
  from: string;
  handshake_ms: number;
  plugin: PluginRecord;
  to: string;
}

/** A preset named by id. */
export interface PresetRef {
  preset: string;
}

/** `rendition.presets`. */
export interface PresetsResult {
  presets: RenditionPreset[];
}

/** What `preview.close` answers with. */
export interface PreviewClosed {
  closed: boolean;
  target: string;
}

/** `ext.preview`. Either `"full"`, `false`, or an object. */
export type PreviewExt = string | boolean | {
  fps?: number | null;
  width?: number | null;
};

/** `scene.preview.frame`. */
export interface PreviewFrameRequest {
  width?: number | null;
}

/** `preview.open {target}`. */
export interface PreviewOpenRequest {
  target: string;
}

/** `scene.preview.set`. */
export interface PreviewRequest {
  draft?: string | null;
  scene?: string | null;
}

/** What `preview.open` answers with. */
export interface PreviewSocket {
  path: string;
  target: string;
  transport: string;
}

/**
 * What `program.get` answers with, and what `program.take` returns so that no
 * follow up read is needed.
 */
export interface ProgramState {
  ad?: AdStatus | null;
  missing?: string[];
  preview?: string | null;
  previous?: string | null;
  program?: string | null;
  running_time_ms: number;
  scene?: string | null;
}

/** Where an encoder is pointed for one protocol. */
export interface PublishAddress {
  example: string;
  protocol: string;
  server: string;
}

/** One scene or one item. */
export interface ProtocolRecord {
  id: Id;
  order: string;
  parent?: Id | null;
}
export type { ProtocolRecord as Record };

/** One asset an import could not put back. */
export interface Relink {
  asset: Id;
  items: string[];
  path: string;
  reason: string;
}

/** `channel.destination.remove`. */
export interface RemoveDestinationRequest {
  destination: string;
  id: string;
}

export interface RenameSceneRequest {
  color?: string | null;
  name?: string | null;
  scene: string;
}

/** One thing a refused rendition could be instead, as a button. */
export interface RenditionAdvice {
  request: RenditionRequest;
  text: string;
}

/**
 * What an output asks for: a whole request, or a preset by id.
 *
 * A request's `id` is replaced by the output's own id (a ladder's rungs get
 * `<output>-<rung>`), so a client may send any slug there.
 */
export type RenditionChoice = PresetRef | LadderRef | RenditionRequest;

/** `event/rendition.plan`. */
export interface RenditionPlanEvent {
  plan: PlanView;
  scope: string;
}

/** One built in preset. */
export interface RenditionPreset {
  available?: boolean;
  cost?: Cost | null;
  group: string;
  id: string;
  ladder?: RenditionRequest[] | null;
  request: RenditionRequest;
  title: string;
  why?: string | null;
}

/**
 * What one output wants. A field left out means "whatever the source has",
 * so an empty request is a plain copy.
 */
export interface RenditionRequest {
  audio?: AudioWant | null;
  container?: Container;
  id?: string;
  no_audio?: boolean;
  no_video?: boolean;
  video?: VideoWant | null;
}

/** `scene.item.reorder`. */
export interface ReorderRequest {
  after?: string | null;
  before?: string | null;
  draft?: string | null;
  item: string;
  scene: string;
  seq?: number | null;
}

/** What `project.import` answers with. */
export interface Report {
  changes: Change[];
  dry_run: boolean;
  failed: string[];
  name: string;
  needs_restart: string[];
  page: unknown;
  waiting: string[];
  written_by: string;
}

/** One plugin the collection needs. */
export interface Requirement {
  plugin: string;
  provides: string[];
  versions: string;
}

export type ResponseFormat = "concise" | "detailed";

/** `core.restart`: what happened. */
export interface RestartAnswer {
  how: RestartHow;
  message: string;
  restarting: boolean;
}

/** How a core that exits gets started again. */
export type RestartHow = "supervised" | "none";

/** `core.info.restart`: can this core be restarted from a client. */
export interface RestartInfo {
  how: RestartHow;
  possible: boolean;
}

/** `event/resync`: the client fell behind and the stream has a hole in it. */
export interface Resync {
  dropped: number;
  from_seq: number;
}

/** The RTMP port every channel shares. */
export interface RtmpInfo {
  listening: boolean;
  port: number;
  problem?: string | null;
  urls: string[];
}

/** RTMPS for one channel: off, or on at a port. */
export interface Rtmps {
  enabled: boolean;
  port: number;
}

export interface SaveRequest {
  name: string;
  out?: string | null;
}

/** `scene.list`. */
export interface SceneListing {
  scenes: SceneSummary[];
}

export interface SceneRemoved {
  removed: string;
}

/** Anything that names one scene. */
export interface SceneRequest {
  scene: string;
}

/** What `scene.list` answers with per scene. */
export interface SceneSummary {
  armed: boolean;
  color?: string | null;
  id: Id;
  items: number;
  name: string;
  sources: string[];
}

/** One scene as a command answers with it. */
export interface SceneView {
  canvas: Canvas;
  color?: string | null;
  findings?: Finding[];
  geometry: Geometry[];
  id: Id;
  name: string;
  records: ProtocolRecord[];
}

/** `plugin.search`. */
export interface SearchRequest {
  term?: string;
}

/** One plugin a marketplace lists. */
export interface SearchResult {
  description: string;
  installed: boolean;
  kinds: string[];
  marketplace: string;
  name: string;
  source: string;
  tier: string;
  version: string;
}

/** What `plugin.search` answers with. */
export interface SearchResults {
  marketplaces: string[];
  results: SearchResult[];
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

/** `channel.destination.set`. Change one destination, naming only what moves. */
export interface SetDestinationRequest {
  destination: string;
  enabled?: boolean | null;
  id: string;
  key?: string | null;
  label?: string | null;
  rendition?: RenditionChoice | null;
  server?: string | null;
  stream?: string | null;
}

/** `filter.set`. */
export interface SetFilterRequest {
  id: string;
  params?: Record<string, unknown>;
}

/** `scene.item.set`: a state assignment. Only the keys named move. */
export interface SetItemRequest {
  draft?: string | null;
  duration_ms?: number | null;
  easing?: string | null;
  item: string;
  props: Record<string, unknown>;
  scene: string;
  seq?: number | null;
}

/**
 * `output.set`. Change one destination in place, naming only what moves.
 *
 * The id picks the output and is never changed by this; renaming one is a
 * remove and an add, because the id is what alerts, hooks and the runtime
 * store call it.
 */
export interface SetOutputRequest {
  id: string;
  policy?: string | null;
  queue_secs?: number | null;
  rendition?: Record<string, unknown> | null;
  uri?: string | null;
  [key: string]: unknown;
}

/** `plugin.settings.set`. */
export interface SetSettingsRequest {
  id: string;
  settings?: Record<string, unknown>;
}

/**
 * `source.set`: a full state assignment for one source.
 *
 * Every field is optional and only what is named moves, which is how every
 * other setter in this protocol works. The one that matters here is `place`:
 * it moves a running source between the core, a sidecar and a node.
 *
 * Unknown fields are refused rather than dropped. Serde's default is to
 * ignore what it does not recognise, and a setter that answers 200 to a field
 * it threw away is indistinguishable from one that saved it: the first party
 * drawer sent `uri` here for months and told the operator it was saved.
 */
export interface SetSourceRequest {
  color?: string | null;
  id: string;
  latency_ms?: number | null;
  name?: string | null;
  params?: Record<string, unknown> | null;
  place?: Place | null;
  transport?: BridgeTransport | null;
}

/** How much the reader should care. */
export type Severity = "error" | "warning" | "info";

export type Severity2 = "info" | "warning" | "error" | "critical";

/** One thing the governor stopped or slowed, and why. */
export interface ShedNote {
  what: string;
  why: string;
}

/** One show, as `show.list` and `event/show.changed` carry it. */
export interface Show {
  alarms?: AlarmSettings | null;
  compositing?: boolean;
  cpu_millicores: number;
  error?: string | null;
  health?: Health;
  id: string;
  input?: InputSpec | null;
  memory_mib?: number;
  name: string;
  on_air?: string | null;
  outputs?: Destination[];
  programme_kbps: number;
  restarts?: number;
  state: ShowState;
}

/** One show of `show.add_many`: what `show.add` takes. */
export interface ShowAdd {
  compositing?: boolean | null;
  from?: ShowFrom | null;
  input?: InputSpec | null;
  name: string;
  outputs?: ShowOutputSpec[];
}

/** `show.add_many`. */
export interface ShowAddManyRequest {
  dry_run?: boolean | null;
  shows: ShowAdd[];
}

/** `show.add_many`'s answer. */
export interface ShowAddManyResult {
  added: string[];
  dry_run: boolean;
  plan: BulkPlan;
  refused: ShowRefused[];
}

/** `show.add`. */
export interface ShowAddRequest {
  compositing?: boolean | null;
  from?: ShowFrom | null;
  input?: InputSpec | null;
  name: string;
  outputs?: ShowOutputSpec[];
}

/** `event/show.changed`. */
export interface ShowChanged {
  show: Show;
}

/** What a new show starts from. */
export type ShowFrom = string | {
  project: unknown;
};

/** `event/show.health`. */
export interface ShowHealthEvent {
  health: Health;
  id: string;
}

/** `show.list`. */
export interface ShowList {
  current: string;
  shows: Show[];
}

/** `show.output.add`. */
export interface ShowOutputAddRequest {
  enabled?: boolean | null;
  id: string;
  key?: string | null;
  label?: string | null;
  output?: string | null;
  params?: HlsOutputParams;
  platform?: string | null;
  rendition?: RenditionChoice | null;
  uri?: string | null;
}

/** `show.output.remove`. */
export interface ShowOutputRemoveRequest {
  id: string;
  output: string;
}

/** `show.output.set`. Names only what moves. */
export interface ShowOutputSetRequest {
  enabled?: boolean | null;
  id: string;
  key?: string | null;
  label?: string | null;
  output: string;
  params?: HlsOutputParams;
  rendition?: RenditionChoice | null;
  uri?: string | null;
}

/**
 * An output as it is given to a show without compositing: an address, or
 * a platform and a key.
 */
export interface ShowOutputSpec {
  enabled?: boolean | null;
  id?: string | null;
  key?: string | null;
  label?: string | null;
  params?: HlsOutputParams;
  platform?: string | null;
  rendition?: RenditionChoice | null;
  uri?: string | null;
}

/** A show of `show.add_many` that was not made, and why. */
export interface ShowRefused {
  data?: unknown;
  index: number;
  name: string;
  why: string;
}

/** `show.remove_many`. */
export interface ShowRemoveManyRequest {
  ids: string[];
}

/** `show.remove_many`'s answer. */
export interface ShowRemoveManyResult {
  refused: ShowRefused[];
  removed: string[];
}

/** `show.remove`. */
export interface ShowRemoved {
  removed: string;
}

/** `event/show.removed`. */
export interface ShowRemovedEvent {
  id: string;
}

/** `show.rename`. */
export interface ShowRenameRequest {
  id: string;
  name: string;
}

/** `show.set`. Names only what moves. */
export interface ShowSetRequest {
  alarms?: AlarmSettings | null;
  compositing?: boolean | null;
  id: string;
  input?: InputSpec | null;
  name?: string | null;
}

/** `show.set`'s answer: the show, and what a switch of compositing did. */
export interface ShowSetResult {
  alarms?: AlarmSettings | null;
  compositing?: boolean;
  cpu_millicores: number;
  error?: string | null;
  health?: Health;
  id: string;
  input?: InputSpec | null;
  memory_mib?: number;
  name: string;
  on_air?: string | null;
  outputs?: Destination[];
  programme_kbps: number;
  restarts?: number;
  state: ShowState;
  switch?: SwitchReport | null;
}

/** Where a show is in its life. */
export type ShowState = "starting" | "running" | "stopped" | "failed";

/** One show's numbers, as `show.stats` answers them. */
export interface ShowStats {
  cpu_millicores?: number | null;
  health: Health;
  id: string;
  input?: InputStats | null;
  outputs?: OutputStats[];
  work?: ShowWork;
}

/** `show.stats`'s answer. */
export interface ShowStatsList {
  shows: ShowStats[];
}

/** `show.stats`. */
export interface ShowStatsRequest {
  fields?: string[] | null;
  ids?: string[] | null;
}

/** What a show does to make its outputs, which is what its load pays for. */
export type ShowWork = "mix" | "transcode" | "copy";

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

/** A source's name, colour and tray folder, as this collection has them. */
export interface SourceMeta {
  color?: string | null;
  group?: string | null;
  name?: string | null;
}

/** A source the import found and did not add, and why. */
export interface SourceNotAdded {
  id: string;
  plugin?: string | null;
  reason: string;
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

/** One line of the report: an OBS source and what happened to it. */
export interface SourceReport {
  obs_name: string;
  obs_type: string;
  placements: number;
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

export interface StatsListing {
  instances: InstanceRecord[];
}

export interface StreamAudio {
  channels: number;
  codec: string;
  kbps: number;
  sample_rate: number;
}

export interface StreamVideo {
  codec: string;
  fps: number;
  height: number;
  kbps: number;
  width: number;
}

/** `core.subscribe`: which events, and which expensive streams. */
export interface SubscribeRequest {
  events?: string[];
  ext?: Ext;
  show?: string | null;
}

/** What `core.subscribe` answers with, before the snapshot arrives. */
export interface SubscribeResult {
  events: string[];
  ignored_ext: string[];
  seq: number;
}

/** What a switch of compositing did, in `show.set`'s answer. */
export interface SwitchReport {
  compositing: boolean;
  gap_ms?: number | null;
  note?: string;
  outputs: string[];
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
  transition?: Transition | null;
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

/** A template and its SVG. */
export interface TemplateDoc {
  description: string;
  fields: TemplateField[];
  height: number;
  name: string;
  origin: TemplateOrigin;
  svg: string;
  title: string;
  uri: string;
  width: number;
}

/** One named field of a template. */
export interface TemplateField {
  default: string;
  fit?: number | null;
  label: string;
  name: string;
  type: FieldType;
}

/** The answer to `template.fields`. */
export interface TemplateFields {
  fields: FieldValue[];
  id: string;
  path: string;
  template: string;
}

/** `template.fields`: the fields of a running graphic. */
export interface TemplateFieldsRequest {
  id: string;
}

/** `template.get`. */
export interface TemplateGetRequest {
  name: string;
}

/** One template, as `template.list` and `template.get` describe it. */
export interface TemplateInfo {
  description: string;
  fields: TemplateField[];
  height: number;
  name: string;
  origin: TemplateOrigin;
  title: string;
  uri: string;
  width: number;
}

/** The answer to `template.list`. */
export interface TemplateList {
  errors?: string[];
  templates: TemplateInfo[];
}

/** Where a template comes from. */
export type TemplateOrigin = "pack" | "library";

/**
 * `template.save`: check an SVG template and write it into the media
 * library.
 */
export interface TemplateSaveRequest {
  name: string;
  replace?: boolean;
  svg: string;
}

/** The answer to `template.save`. */
export interface TemplateSaved {
  path: string;
  redrawn: string[];
  template: TemplateInfo;
}

/** `program.thumbnail`. */
export interface ThumbnailRequest {
  width?: number | null;
}

/**
 * `core.info.tls`: what the control port answers HTTPS with.
 *
 * Enough for a page to say "open this address, accept the certificate
 * warning once, and check the fingerprint is this one".
 */
export interface TlsInfo {
  fingerprint: string;
  names?: string[];
  source: string;
  urls: string[];
}

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

/** `tool.call`. */
export interface ToolCallRequest {
  arguments?: unknown;
  name: string;
}

/** Where an item sits and how it is sized. */
export interface Transform {
  align?: Align;
  anchor?: Vec2;
  fit?: Fit;
  frame?: Frame | null;
  position?: Vec2;
  rotation?: number;
  scale?: Vec2;
}

/** A name, or an object. */
export type Transition = string | TransitionRequest;

/** A named transition between two scenes. */
export interface Transition2 {
  duration_ms: number;
  id: Id;
  name: string;
  params?: unknown;
  type: string;
}

/** `program.transitions`: every transition a take may name on this core. */
export interface TransitionCatalogue {
  default_duration_ms: number;
  directions: string[];
  easings: string[];
  edges: string[];
  item_transitions: string[];
  max_duration_ms: number;
  transitions: TransitionEntry[];
}

/** One name a take accepts. */
export interface TransitionEntry {
  duration_ms?: number | null;
  name: string;
  origin: string;
  params?: string[];
  type: string;
}

/** How a take gets there. See docs/reference/transitions.md. */
export interface TransitionRequest {
  duration_ms?: number | null;
  params?: Record<string, unknown>;
  type: string;
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

/** One record as it was and as it is. */
export interface Update {
  after: ProtocolRecord;
  before: ProtocolRecord;
}

/** `plugin.update`. */
export interface UpdatePluginRequest {
  id: string;
  source?: string | null;
}

export interface ValidateRequest {
  scene?: string | null;
}

/** `scene.validate`. */
export interface Validation {
  findings: Finding[];
  ok: boolean;
}

/** A point or a pair of factors. */
export interface Vec2 {
  x?: number;
  y?: number;
}

export type VideoCodec = "h264" | "h265" | "av1" | "vp8" | "vp9" | "mpeg2" | "prores" | "other";

/** A picture as it is: codec, size, rate, bitrate. */
export interface VideoShape {
  bitrate_kbps: number;
  codec: VideoCodec;
  fps: Fps;
  height: number;
  keyframe_ms: number;
  width: number;
}

/** The video an output wants. Every field left out is taken from the source. */
export interface VideoWant {
  bitrate_kbps?: number | null;
  bitrate_tolerance?: number | null;
  codec?: VideoCodec | null;
  fps?: Fps | null;
  height?: number | null;
  keyframe_ms?: number | null;
  width?: number | null;
}

/**
 * `[vitals]`, and what `vitals.set` changes: the thresholds, and whether
 * to keep a mosaic up for the picture alarms while nobody is looking.
 */
export interface VitalsConfig {
  alarms?: boolean;
  black_luma?: number;
  black_ratio?: number;
  black_secs?: number;
  cc_errors?: number;
  freeze_diff?: number;
  freeze_secs?: number;
  loss?: number;
  silence_db?: number;
  silence_secs?: number;
  stall_secs?: number;
  window_secs?: number;
}

export interface ProgramTookEvent {
  at_running_time_ms?: number;
  duration_ms?: number;
  scene?: string | null;
  source?: string | null;
  transition?: string;
  transition_id?: number;
}

export interface ScenePatchEvent {
  added?: Array<Record<string, unknown>>;
  client_seq?: number | null;
  label?: string | null;
  removed?: string[];
  scope?: "document";
  seq?: number;
  source_client?: string | null;
  updated?: Array<{
    after?: Record<string, unknown>;
    before?: Record<string, unknown>;
  }>;
}

export interface PreviewChangedEvent {
  scene?: string | null;
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

export interface HookBlockedEvent {
  hook: string;
  plugin: string;
  reason: string;
}

export interface MediaChangedEvent {
  conversion?: unknown;
  name?: string;
}

export interface ChannelChangedEvent {
  channel?: Channel;
}

export interface ChannelRemovedEvent {
  id?: string;
}

export interface ChannelRefusedEvent {
  from: string;
  id: string;
  stream: string;
  why: string;
}

export interface AlertEvent {
  action?: ErrorAction;
  message?: string;
  severity?: Severity2;
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
  "channel.add": ChannelAddRequest;
  "channel.certificate.generate": CertificateGenerateRequest;
  "channel.certificate.set": CertificateSetRequest;
  "channel.destination.add": AddDestinationRequest;
  "channel.destination.remove": RemoveDestinationRequest;
  "channel.destination.set": SetDestinationRequest;
  "channel.get": IdRequest;
  "channel.key.add": ChannelKeyAddRequest;
  "channel.key.remove": ChannelKeyRemoveRequest;
  "channel.key.reveal": ChannelKeyRevealRequest;
  "channel.list": Record<string, never>;
  "channel.remove": IdRequest;
  "channel.set": ChannelSetRequest;
  "codec.list": Record<string, never>;
  "config.get": ConfigGetRequest;
  "config.reset": ConfigResetRequest;
  "config.schema": Record<string, never>;
  "config.set": ConfigSetRequest;
  "core.api": Record<string, never>;
  "core.doctor": Record<string, never>;
  "core.info": Record<string, never>;
  "core.restart": Record<string, never>;
  "core.session_log": SessionLogRequest;
  "core.shutdown": Record<string, never>;
  "core.startup_report": Record<string, never>;
  "core.status": Record<string, never>;
  "core.subscribe": SubscribeRequest;
  "device.discover": DiscoverRequest;
  "filter.add": AddFilterRequest;
  "filter.list": Record<string, never>;
  "filter.remove": FilterIdRequest;
  "filter.set": SetFilterRequest;
  "governor.calibrate": CalibrateRequest;
  "governor.status": Record<string, never>;
  "log.gst": LogGstRequest;
  "log.levels": Record<string, never>;
  "log.set": LogSetRequest;
  "media.convert": NameRequest;
  "media.list": Record<string, never>;
  "media.remove": NameRequest;
  "media.upload": Record<string, never>;
  "node.discover": DiscoverRequest2;
  "node.enrol": EnrolRequest;
  "node.get": NodeName;
  "node.list": Record<string, never>;
  "node.remove": NodeName;
  "output.add": AddOutputRequest;
  "output.get": IdRequest;
  "output.list": Record<string, never>;
  "output.reconnect": IdRequest;
  "output.remove": IdRequest;
  "output.set": SetOutputRequest;
  "path.create": PathCreateRequest;
  "path.list": PathListRequest;
  "pipeline.clock": Record<string, never>;
  "pipeline.dot": PipelineRequest;
  "pipeline.latency": PipelineRequest;
  "pipeline.list": Record<string, never>;
  "pipeline.queues": PipelineRequest;
  "plugin.add": AddPluginRequest;
  "plugin.describe": PluginName;
  "plugin.disable": PluginName;
  "plugin.enable": PluginName;
  "plugin.list": Record<string, never>;
  "plugin.reload": PluginName;
  "plugin.remove": PluginName;
  "plugin.search": SearchRequest;
  "plugin.settings.get": PluginName;
  "plugin.settings.set": SetSettingsRequest;
  "plugin.stats": Record<string, never>;
  "plugin.update": UpdatePluginRequest;
  "preset.apply": ApplyRequest;
  "preset.list": Record<string, never>;
  "preset.save": SaveRequest;
  "preview.close": PreviewOpenRequest;
  "preview.open": PreviewOpenRequest;
  "program.get": Record<string, never>;
  "program.golive": GoLiveRequest;
  "program.history": HistoryRequest;
  "program.revert": Record<string, never>;
  "program.take": TakeRequest;
  "program.thumbnail": ThumbnailRequest;
  "program.transitions": Record<string, never>;
  "project.export": ExportRequest;
  "project.import": ImportRequest;
  "rendition.plan": PlanRequest;
  "rendition.presets": Record<string, never>;
  "scene.add": AddSceneRequest;
  "scene.apply_graphic": ApplyGraphicRequest;
  "scene.apply_layout": ApplyLayoutRequest;
  "scene.create_from": CreateFromRequest;
  "scene.duplicate": DuplicateSceneRequest;
  "scene.edit.apply": DraftRequest;
  "scene.edit.begin": EditBeginRequest;
  "scene.edit.discard": DraftRequest;
  "scene.export": ExportRequest2;
  "scene.get": SceneRequest;
  "scene.graphic.list": Record<string, never>;
  "scene.history.mark": MarkRequest;
  "scene.import": ImportRequest2;
  "scene.import.obs": ImportObsRequest;
  "scene.item.add": AddItemRequest;
  "scene.item.align": ItemsRequest;
  "scene.item.arrange_grid": ItemsRequest;
  "scene.item.bind": BindRequest;
  "scene.item.copy": MoveItemRequest;
  "scene.item.cover_canvas": ItemsRequest;
  "scene.item.distribute": ItemsRequest;
  "scene.item.filter.add": AddItemFilterRequest;
  "scene.item.filter.remove": ItemFilterRequest;
  "scene.item.filter.set": ItemFilterRequest;
  "scene.item.fit_to_canvas": ItemsRequest;
  "scene.item.group": ItemsRequest;
  "scene.item.match_size": ItemsRequest;
  "scene.item.move": MoveItemRequest;
  "scene.item.remove": ItemRequest;
  "scene.item.reorder": ReorderRequest;
  "scene.item.schema": ItemSchemaRequest;
  "scene.item.set": SetItemRequest;
  "scene.item.ungroup": ItemRequest;
  "scene.layout.copy": SceneRequest;
  "scene.layout.list": Record<string, never>;
  "scene.layout.paste": LayoutClipboardRequest;
  "scene.list": Record<string, never>;
  "scene.params.get": Record<string, never>;
  "scene.params.set": ParamsRequest;
  "scene.preview.frame": PreviewFrameRequest;
  "scene.preview.set": PreviewRequest;
  "scene.redo": Record<string, never>;
  "scene.remove": SceneRequest;
  "scene.rename": RenameSceneRequest;
  "scene.transaction.abort": Record<string, never>;
  "scene.transaction.begin": Record<string, never>;
  "scene.transaction.commit": Record<string, never>;
  "scene.undo": Record<string, never>;
  "scene.validate": ValidateRequest;
  "show.add": ShowAddRequest;
  "show.add_many": ShowAddManyRequest;
  "show.list": Record<string, never>;
  "show.output.add": ShowOutputAddRequest;
  "show.output.remove": ShowOutputRemoveRequest;
  "show.output.set": ShowOutputSetRequest;
  "show.remove": IdRequest;
  "show.remove_many": ShowRemoveManyRequest;
  "show.rename": ShowRenameRequest;
  "show.set": ShowSetRequest;
  "show.start": IdRequest;
  "show.stats": ShowStatsRequest;
  "show.stop": IdRequest;
  "snapshot.get": SnapshotRequest;
  "source.add": AddSourceRequest;
  "source.audio.set": AudioSetParams;
  "source.duplicate": DuplicateSourceRequest;
  "source.get": IdRequest;
  "source.group": GroupSourcesRequest;
  "source.list": Record<string, never>;
  "source.missing": MissingRequest;
  "source.remove": IdRequest;
  "source.restart": IdRequest;
  "source.restore": IdRequest;
  "source.seek": SeekParams;
  "source.set": SetSourceRequest;
  "task.cancel": TaskRequest;
  "task.get": TaskRequest;
  "task.list": Record<string, never>;
  "template.fields": TemplateFieldsRequest;
  "template.get": TemplateGetRequest;
  "template.list": Record<string, never>;
  "template.save": TemplateSaveRequest;
  "tool.call": ToolCallRequest;
  "vitals.get": Record<string, never>;
  "vitals.set": VitalsConfig;
}

/** What each method answers with, by method name. */
export interface MethodResults {
  "adbreak.end": Record<string, unknown>;
  "adbreak.start": Record<string, unknown>;
  "agent.state": Record<string, unknown>;
  "channel.add": ChannelAdded;
  "channel.certificate.generate": CertificateInfo;
  "channel.certificate.set": CertificateInfo;
  "channel.destination.add": Record<string, unknown>;
  "channel.destination.remove": Record<string, unknown>;
  "channel.destination.set": Record<string, unknown>;
  "channel.get": Channel;
  "channel.key.add": KeyAdded;
  "channel.key.remove": Channel;
  "channel.key.reveal": KeyRevealed;
  "channel.list": ChannelList;
  "channel.remove": ChannelRemoved;
  "channel.set": Channel;
  "codec.list": Record<string, unknown>;
  "config.get": ConfigGetResult;
  "config.reset": ConfigSetResult;
  "config.schema": Record<string, unknown>;
  "config.set": ConfigSetResult;
  "core.api": Record<string, unknown>;
  "core.doctor": Record<string, unknown>;
  "core.info": CoreInfo;
  "core.restart": RestartAnswer;
  "core.session_log": Record<string, unknown>;
  "core.shutdown": Record<string, unknown>;
  "core.startup_report": Record<string, unknown>;
  "core.status": MixerStatus;
  "core.subscribe": SubscribeResult;
  "device.discover": Record<string, unknown>;
  "filter.add": FilterRecord;
  "filter.list": FilterListing;
  "filter.remove": FilterRemoved;
  "filter.set": FilterRecord;
  "governor.calibrate": CalibrateResult;
  "governor.status": GovernorStatus;
  "log.gst": LogGstResult;
  "log.levels": Record<string, unknown>;
  "log.set": Record<string, unknown>;
  "media.convert": ConversionState;
  "media.list": MediaListing;
  "media.remove": Record<string, unknown>;
  "media.upload": Record<string, unknown>;
  "node.discover": DiscoverAnswer;
  "node.enrol": Record<string, unknown>;
  "node.get": NodeView;
  "node.list": NodeListing;
  "node.remove": Record<string, unknown>;
  "output.add": OutputStatus;
  "output.get": OutputStatus;
  "output.list": OutputStatus[];
  "output.reconnect": OutputStatus;
  "output.remove": Record<string, unknown>;
  "output.set": OutputStatus;
  "path.create": PathListing;
  "path.list": PathListing;
  "pipeline.clock": Record<string, unknown>;
  "pipeline.dot": PipelineDot;
  "pipeline.latency": Record<string, unknown>;
  "pipeline.list": Record<string, unknown>;
  "pipeline.queues": Record<string, unknown>;
  "plugin.add": PluginRecord;
  "plugin.describe": PluginDescription;
  "plugin.disable": PluginRecord;
  "plugin.enable": PluginRecord;
  "plugin.list": PluginListing;
  "plugin.reload": PluginRecord;
  "plugin.remove": PluginRemoved;
  "plugin.search": SearchResults;
  "plugin.settings.get": PluginSettings;
  "plugin.settings.set": PluginSettings;
  "plugin.stats": StatsListing;
  "plugin.update": PluginUpdated;
  "preset.apply": ApplyResult;
  "preset.list": Record<string, unknown>;
  "preset.save": Record<string, unknown>;
  "preview.close": PreviewClosed;
  "preview.open": PreviewSocket;
  "program.get": ProgramState;
  "program.golive": GoLiveResult;
  "program.history": TakeRecord[];
  "program.revert": ProgramState;
  "program.take": ProgramState;
  "program.thumbnail": Record<string, unknown>;
  "program.transitions": TransitionCatalogue;
  "project.export": Record<string, unknown>;
  "project.import": Report;
  "rendition.plan": PlanView;
  "rendition.presets": PresetsResult;
  "scene.add": SceneView;
  "scene.apply_graphic": Record<string, unknown>;
  "scene.apply_layout": Record<string, unknown>;
  "scene.create_from": SceneView;
  "scene.duplicate": SceneView;
  "scene.edit.apply": Record<string, unknown>;
  "scene.edit.begin": DraftRecord;
  "scene.edit.discard": Record<string, unknown>;
  "scene.export": Record<string, unknown>;
  "scene.get": SceneView;
  "scene.graphic.list": GraphicListing;
  "scene.history.mark": Record<string, unknown>;
  "scene.import": ImportedReport;
  "scene.import.obs": ImportReport;
  "scene.item.add": Record<string, unknown>;
  "scene.item.align": Record<string, unknown>;
  "scene.item.arrange_grid": Record<string, unknown>;
  "scene.item.bind": Record<string, unknown>;
  "scene.item.copy": Record<string, unknown>;
  "scene.item.cover_canvas": Record<string, unknown>;
  "scene.item.distribute": Record<string, unknown>;
  "scene.item.filter.add": Record<string, unknown>;
  "scene.item.filter.remove": Record<string, unknown>;
  "scene.item.filter.set": Record<string, unknown>;
  "scene.item.fit_to_canvas": Record<string, unknown>;
  "scene.item.group": Record<string, unknown>;
  "scene.item.match_size": Record<string, unknown>;
  "scene.item.move": Record<string, unknown>;
  "scene.item.remove": Record<string, unknown>;
  "scene.item.reorder": Record<string, unknown>;
  "scene.item.schema": Record<string, unknown>;
  "scene.item.set": Record<string, unknown>;
  "scene.item.ungroup": Record<string, unknown>;
  "scene.layout.copy": Layout;
  "scene.layout.list": LayoutListing;
  "scene.layout.paste": Record<string, unknown>;
  "scene.list": SceneListing;
  "scene.params.get": Record<string, unknown>;
  "scene.params.set": Record<string, unknown>;
  "scene.preview.frame": Record<string, unknown>;
  "scene.preview.set": Record<string, unknown>;
  "scene.redo": HistoryStep;
  "scene.remove": SceneRemoved;
  "scene.rename": SceneView;
  "scene.transaction.abort": Record<string, unknown>;
  "scene.transaction.begin": Record<string, unknown>;
  "scene.transaction.commit": Record<string, unknown>;
  "scene.undo": HistoryStep;
  "scene.validate": Validation;
  "show.add": Show;
  "show.add_many": ShowAddManyResult;
  "show.list": ShowList;
  "show.output.add": Show;
  "show.output.remove": Show;
  "show.output.set": Show;
  "show.remove": ShowRemoved;
  "show.remove_many": ShowRemoveManyResult;
  "show.rename": Show;
  "show.set": ShowSetResult;
  "show.start": Show;
  "show.stats": ShowStatsList;
  "show.stop": Show;
  "snapshot.get": Record<string, unknown>;
  "source.add": SourceStatus;
  "source.audio.set": SourceAudioState;
  "source.duplicate": SourceStatus;
  "source.get": SourceStatus;
  "source.group": Record<string, unknown>;
  "source.list": SourceStatus[];
  "source.missing": MissingSource[];
  "source.remove": Record<string, unknown>;
  "source.restart": SourceStatus;
  "source.restore": SourceStatus;
  "source.seek": SourcePositionState;
  "source.set": SourceStatus;
  "task.cancel": Record<string, unknown>;
  "task.get": TaskView;
  "task.list": TaskView[];
  "template.fields": TemplateFields;
  "template.get": TemplateDoc;
  "template.list": TemplateList;
  "template.save": TemplateSaved;
  "tool.call": Record<string, unknown>;
  "vitals.get": Record<string, unknown>;
  "vitals.set": Record<string, unknown>;
}

export type MethodName = keyof MethodParams;

/** The payload of each event, by the name after `event/`. */
export interface EventPayloads {
  "snapshot": Snapshot;
  "program.took": ProgramTookEvent;
  "scene.patch": ScenePatchEvent;
  "preview.changed": PreviewChangedEvent;
  "source.state": SourceStateEvent;
  "source.position": SourcePositionEvent;
  "output.state": OutputStateEvent;
  "adbreak.changed": AdbreakChangedEvent;
  "ui.changed": UiChangedEvent;
  "hook.blocked": HookBlockedEvent;
  "media.changed": MediaChangedEvent;
  "channel.changed": ChannelChangedEvent;
  "channel.removed": ChannelRemovedEvent;
  "channel.refused": ChannelRefusedEvent;
  "meters": Meters;
  "tally": Tally;
  "alert": AlertEvent;
  "telemetry": TelemetryEvent;
  "agent.state": Record<string, unknown>;
  "multiview.layout": MultiviewLayout;
  "multiview.frame": Uint8Array;
  "preview.frame": Uint8Array;
  "resync": Resync;
  "flush": Flush;
  "rendition.plan": RenditionPlanEvent;
  "governor.shed": ShedNote;
  "show.changed": ShowChanged;
  "show.removed": ShowRemovedEvent;
  "show.health": ShowHealthEvent;
  "health": HealthEvent;
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
  { name: "channel.add", summary: "Make a channel and its first key, which is in this answer. channel.key.reveal reads it again later.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/channels" } },
  { name: "channel.certificate.generate", summary: "Make a self signed certificate for RTMPS, for this machine's addresses unless names are given. Encoders must be told to accept it; one from a certificate authority needs no such step.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/channels/certificate/generate" } },
  { name: "channel.certificate.set", summary: "Give RTMPS a certificate: the PEM of the certificate (and its chain) and of its private key, as a certificate authority issued them. Checked before it is kept; the key is sealed and never read back.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/channels/certificate/set" } },
  { name: "channel.destination.add", summary: "Send a channel's stream on to YouTube, Facebook, Twitch, an RTMP server or an SRT receiver as it arrives. Nothing is decoded or encoded. The key is write only.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/channels/{id}/destination/add" } },
  { name: "channel.destination.remove", summary: "Stop sending a channel's stream to one destination and forget it. The publisher and the other destinations are not touched.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/channels/{id}/destination/remove" } },
  { name: "channel.destination.set", summary: "Change one of a channel's destinations, naming only what moves: a new key, another server, which stream it sends, on or off. A key left out is kept.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/channels/{id}/destination" } },
  { name: "channel.get", summary: "One channel.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/channels/{id}" } },
  { name: "channel.key.add", summary: "Make another key for a channel, to give to one more person or encoder. The key is in this answer, and channel.key.reveal reads it again later.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/channels/{id}/key/add" } },
  { name: "channel.key.remove", summary: "Take one key back. A publisher on air with it is cut off and the next one is turned away; the other keys are untouched.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/channels/{id}/key/remove" } },
  { name: "channel.key.reveal", summary: "Read one key of a channel back, to give it to an encoder again. Admin only; a list shows only the last four characters. Each read is logged with who asked, never with the key.", scope: "admin", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/channels/{id}/key/reveal" } },
  { name: "channel.list", summary: "Every channel with its keys (as hints), the address to publish to over each protocol it has on, and what is live on it; and which ingest ports are open and for which channels.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/channels" } },
  { name: "channel.remove", summary: "Remove a channel and forget its keys. Sources it made that no scene holds go with it.", scope: "admin", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/channels/{id}" } },
  { name: "channel.set", summary: "Rename a channel, switch it on or off, or change its application name, whether its streams become sources, how its key is given, which protocols it takes (rtmp, srt, whip) or RTMPS and its port. A port opens when the first channel needs it and closes when the last one stops. Only what is named moves.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/channels/{id}/set" } },
  { name: "codec.list", summary: "Every codec and element in the catalogue, which of them this machine actually has, and what it would pick.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/codecs" } },
  { name: "config.get", summary: "The mixer's settings: each key's value in the config file, its default, when a change to it takes effect, and which keys are waiting for a restart. Secrets say only whether one is set.", scope: "admin", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/config" } },
  { name: "config.reset", summary: "Put settings back to their defaults by taking them out of the config file. Answers like config.set.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/config/reset" } },
  { name: "config.schema", summary: "Every setting config.set takes, as one JSON Schema: type, title, description, default, range or choices, and x-gmx-applies (live, next_source or restart).", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/config/schema" } },
  { name: "config.set", summary: "Change settings in the config file, keeping its comments. Every value is checked first and nothing is written unless all of them fit. Live keys take effect at once; the answer says which wait for the next source or a restart.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/config/set" } },
  { name: "core.api", summary: "Every method, event and type as JSON Schema. The same document as protocol.json and `godwinmix --api-info`.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/core/api" } },
  { name: "core.doctor", summary: "The environment checks: GStreamer, the elements, the config, the disk and the ports. The same list `gmx doctor` prints.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/core/doctor" } },
  { name: "core.info", summary: "What this core is, what it can do, and where its edges are.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/core/info" } },
  { name: "core.restart", summary: "Stop the mixer and have it started again, when something will start it again. On a supervised core (core.info restart.possible) it answers restarting: true and exits; the programme is off air until it is back. On a core started by hand it answers restarting: false, says how to restart it, and keeps running.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/core/restart" } },
  { name: "core.session_log", summary: "The append only record of everything that happened, back as far as you ask.", scope: "admin", mutating: true, destructive: false, rest: { method: "GET", path: "/api/v1/core/session_log" } },
  { name: "core.shutdown", summary: "Stop the mixer, and with it the programme. Nothing else takes the show off air, so this is deliberately its own call.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/core/shutdown" } },
  { name: "core.startup_report", summary: "How long each stage of the start took, and what was over the 250 ms mark.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/core/startup_report" } },
  { name: "core.status", summary: "The full state: programme, every source, every output, the multiview grid, the encoder backend and any ad break.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/core/status" } },
  { name: "core.subscribe", summary: "Subscribe to the event stream. WebSocket only: the core answers event/snapshot then deltas, ending every batch with event/flush.", scope: "read", mutating: false, destructive: false },
  { name: "device.discover", summary: "Ask every device plugin what it can see: cameras, NDI senders, publishers. Each candidate's params are ready for source.add.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/device/discover" } },
  { name: "filter.add", summary: "Hang a filter on one source or on the programme, live.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/filters" } },
  { name: "filter.list", summary: "Every filter in place, with what it is and where it sits.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/filters" } },
  { name: "filter.remove", summary: "Take a filter out of the pipeline.", scope: "operate", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/filters/{id}" } },
  { name: "filter.set", summary: "Change a filter's settings in place. A filter that cannot take the change while running says so rather than being restarted behind your back.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/filters/{id}/set" } },
  { name: "governor.calibrate", summary: "Measure this machine's encoders again, in the background, a few seconds of every core. Refused while anything is on air unless `confirm` is true.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/governor/calibrate" } },
  { name: "governor.status", summary: "The resource governor: when this machine was measured, what is in use and free on the CPU and each GPU encoder, and what was shed to keep the programme whole.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/governor/status" } },
  { name: "log.gst", summary: "Raise GStreamer's own debug categories for a while, then let them fall back on their own.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/log/gst" } },
  { name: "log.levels", summary: "Every log level override in force, and the GStreamer categories still raised.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/log/levels" } },
  { name: "log.set", summary: "Change one instance's or one module's log level while the mixer runs.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/log/set" } },
  { name: "media.convert", summary: "Transcode a library file to a web safe copy, in the background.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/media/{id}/convert" } },
  { name: "media.list", summary: "The clips in the library, with durations and whether each has audio.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/media" } },
  { name: "media.remove", summary: "Delete a library file and its converted copy. Refused while it is a live source.", scope: "operate", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/media/{id}" } },
  { name: "media.upload", summary: "Stream a file into the library. HTTP only: the body is the file.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/media/upload" } },
  { name: "node.discover", summary: "Look for nodes on the local network over mDNS. A network without multicast finds nothing and the [nodes] table in the config is the way there.", scope: "read", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/nodes/{id}/discover" } },
  { name: "node.enrol", summary: "Mint a one time enrolment token for a node. The answer carries the command to run on the other machine. The token is good for one enrolment and expires.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/nodes/{id}/enrol" } },
  { name: "node.get", summary: "One node: its clock offset, how long since its last heartbeat, the plugins it has, and the instances it is hosting.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/nodes/{id}" } },
  { name: "node.list", summary: "Every node this core knows about: the ones connected now, the ones that have gone quiet, and the ones the config expects that have never dialled in.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/nodes" } },
  { name: "node.remove", summary: "Forget a node. Its bridge is closed, every token minted for a plugin on it is revoked, and its certificate stops working. Sources placed on it go to the slate until they are moved or the node enrols again.", scope: "admin", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/nodes/{id}" } },
  { name: "output.add", summary: "Send the programme to another destination. The encoder is shared, so adding one costs nothing on air.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/outputs" } },
  { name: "output.get", summary: "One destination.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/outputs/{id}" } },
  { name: "output.list", summary: "Every destination, with its state, reconnect count and how much is buffered.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/outputs" } },
  { name: "output.reconnect", summary: "Drop and re-establish one destination's connection now, without waiting for its reconnect policy.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/outputs/{id}/reconnect" } },
  { name: "output.remove", summary: "Stop sending to a destination and forget it. Other outputs are unaffected.", scope: "operate", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/outputs/{id}" } },
  { name: "output.set", summary: "Change a destination in place: a new address with a new stream key, a new reconnect policy, a deeper outage buffer. The address is write only, so a client that only wants the buffer never has to hold the key.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/outputs/{id}/set" } },
  { name: "path.create", summary: "Make one new folder inside a folder path.list shows, and list it. A folder that is already there is listed rather than refused.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/path/create" } },
  { name: "path.list", summary: "The folders in one folder on the mixer, and whether each is writable, for a folder picker. Only the home folder and the mixer's own folders are shown; files never are.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/path/list" } },
  { name: "pipeline.clock", summary: "The clock every pipeline is running against, and how far each one has got.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/pipeline/clock" } },
  { name: "pipeline.dot", summary: "One pipeline as a graphviz graph: every element, every pad and the caps negotiated between them.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/pipeline/dot" } },
  { name: "pipeline.latency", summary: "How much delay one pipeline is carrying, and which stage put it there.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/pipeline/latency" } },
  { name: "pipeline.list", summary: "Every pipeline running right now, by the name the other pipeline methods accept.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/pipeline/list" } },
  { name: "pipeline.queues", summary: "Every queue in one pipeline with how full it is, fullest first. A queue that stays full is where the trouble is.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/pipeline/queues" } },
  { name: "plugin.add", summary: "Install a plugin, while live, from any source form: a GitHub release (owner/repo), a git URL, cargo:, npm:, pypi:, a local directory, or a bare name looked up in the marketplaces this mixer knows. The signature and the api level are checked before anything is copied.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/plugins" } },
  { name: "plugin.describe", summary: "One plugin in full: its manifest, the settings schema of every provide, and the description from each SKILL.md.", scope: "read", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/plugins/{id}/describe" } },
  { name: "plugin.disable", summary: "Turn a plugin off without uninstalling it. It registers nothing and runs no process until it is enabled again.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/plugins/{id}/disable" } },
  { name: "plugin.enable", summary: "Turn a plugin back on. It registers what it declares and its instances start.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/plugins/{id}/enable" } },
  { name: "plugin.list", summary: "Every plugin installed, with what it provides and what each running instance is costing in cpu, memory, latency, dropped buffers and restarts.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/plugins" } },
  { name: "plugin.reload", summary: "Read a plugin's directory again and swap its running instances one at a time, with the freeze frame covering each.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/plugins/{id}/reload" } },
  { name: "plugin.remove", summary: "Uninstall a plugin and unwind everything it registered: its provides, its tools, its panels, its hooks and its discovery matchers.", scope: "admin", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/plugins/{id}" } },
  { name: "plugin.search", summary: "Search every marketplace this mixer knows for a plugin, by name, description or kind. Answers what `gmx plugin add <name>` would install.", scope: "read", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/plugins/{id}/search" } },
  { name: "plugin.settings.get", summary: "A plugin's settings as they stand, with its schema beside them.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/plugins/{id}/settings" } },
  { name: "plugin.settings.set", summary: "Change a plugin's settings. A plugin that cannot take a change while running says so rather than being restarted behind your back.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/plugins/{id}/settings" } },
  { name: "plugin.stats", summary: "Per instance cpu, memory, media latency, dropped buffers and restarts, refreshed once a second.", scope: "read", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/plugins/{id}/stats" } },
  { name: "plugin.update", summary: "Fetch a newer build of a plugin, install it beside the one that is running, and prove it starts. A build that does not answer `initialize` within ten seconds is rolled back and the plugin that was working stays working.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/plugins/{id}/update" } },
  { name: "preset.apply", summary: "Put a preset on this core: its config, its scenes, its layout, its theme and its gallery mode. Pass dry_run to get the plan and write nothing.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/preset/apply" } },
  { name: "preset.list", summary: "Every preset this core can apply: the six built in, plus anything installed beside the binary or under ~/.godwinmix/presets.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/preset/list" } },
  { name: "preset.save", summary: "Turn this core's working setup into a preset directory somebody else can apply. Stream keys and the control token are replaced with placeholders.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/preset/save" } },
  { name: "preview.close", summary: "Give up a raw frame socket. The socket goes when the last holder closes it.", scope: "read", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/preview/close" } },
  { name: "preview.open", summary: "Open a raw frame socket on this machine for a source or the programme, and answer with its path. No encode anywhere: a client on the same host reads the frames the mixer already has. Close it with preview.close.", scope: "read", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/preview/open" } },
  { name: "program.get", summary: "What is on air, the programme running time, and what revert would go back to.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/program" } },
  { name: "program.golive", summary: "One call to put a web page on air: add the page, add the destination, and take the page as soon as it renders.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/program/golive" } },
  { name: "program.history", summary: "The last hundred takes, newest first, with the token that asked for each.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/program/history" } },
  { name: "program.revert", summary: "Take back to the shot before this one.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/program/revert" } },
  { name: "program.take", summary: "Put a scene or a source on programme. The cut is instant and the outgoing stream is not disturbed.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/program/take" } },
  { name: "program.thumbnail", summary: "What is on air as a small JPEG in base64, {jpeg, width, height, at_ms}, or {pending: true} while the first picture is on its way. An ask keeps one picture a second coming for ten seconds; nothing runs between asks.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/program/thumbnail" } },
  { name: "program.transitions", summary: "Every transition a take may name on this core, with the params each reads, and what an item's enter and exit may be.", scope: "read", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/program/transitions" } },
  { name: "project.export", summary: "This mixer as one project file: settings, sources, outputs and renditions, channels, scenes, the page's layout, and its clips by name and size. Keys only with include_secrets.", scope: "admin", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/project/export" } },
  { name: "project.import", summary: "Open a project file: answers with what it would change (dry_run is true unless false is sent), then replaces this mixer's setup or merges beside it. Says which settings wait for a restart.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/project/import" } },
  { name: "rendition.plan", summary: "What the planner built for every output that asked for a rendition: each node, what it serves, which encoder and why, and the totals.", scope: "read", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/rendition/plan" } },
  { name: "rendition.presets", summary: "Every rendition preset, priced on this machine by the governor. One this machine cannot make says so, with why.", scope: "read", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/rendition/presets" } },
  { name: "scene.add", summary: "Make an empty scene, or one built from a set of sources.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes" } },
  { name: "scene.apply_graphic", summary: "Fill a graphic that is on a scene, by field name, and optionally play it on or take it off. Answers with the records and, if asked, a still.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/apply_graphic" } },
  { name: "scene.apply_layout", summary: "Apply a layout, making a scene or reshaping one that exists. Applying onto an existing scene keeps the item ids, so the change is a ramp and not a cut.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/apply_layout" } },
  { name: "scene.create_from", summary: "A scene from a set of sources, laid out by the built in layout for that count (full, two-box, three-box, quad, then a grid) or by a named one.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/create_from" } },
  { name: "scene.duplicate", summary: "A copy of a scene with new ids throughout, so editing the copy cannot touch the original.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/{id}/duplicate" } },
  { name: "scene.edit.apply", summary: "Write a draft back into the live document.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/edit/apply" } },
  { name: "scene.edit.begin", summary: "Take a working copy of a scene. Editing is off air by default: the draft is written back on the next take of that scene, or when you apply it.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/edit/begin" } },
  { name: "scene.edit.discard", summary: "Throw a draft away. The live document is untouched.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/edit/discard" } },
  { name: "scene.export", summary: "The whole collection: as JSON, or as a zip bundle carrying its assets with a hash each, which is what you send somebody.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/scenes/export" } },
  { name: "scene.get", summary: "One scene: its records and where every item actually lands on the canvas.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/scenes/{id}" } },
  { name: "scene.graphic.list", summary: "Every graphic template this core can place, with what each one takes.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/scenes/graphic/list" } },
  { name: "scene.history.mark", summary: "Group the changes that follow into one undo step, until the next mark. This is what makes a drag of forty moves one Ctrl+Z.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/history/mark" } },
  { name: "scene.import", summary: "Read a collection bundle, a zip or the directory it unpacks to, and add its scenes to this one. Answers with a relink report for any asset that did not come across.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/import" } },
  { name: "scene.import.obs", summary: "Read an OBS Studio scene collection and add its scenes to this one. Send the file's text as `content` (what a page's file picker reads) or a `path` on the mixer's machine. With `add_sources: true` the sources the scenes draw are added through source.add, and the answer says which were added and why any were not.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/import/obs" } },
  { name: "scene.item.add", summary: "Put something on a scene's canvas. With no transform it lands in the next free cell, so a drop never needs a dialog.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/add" } },
  { name: "scene.item.align", summary: "Line items up on an edge: left, right, top, bottom, center-x or center-y.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/align" } },
  { name: "scene.item.arrange_grid", summary: "Lay items out in a grid of `cols` columns.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/arrange_grid" } },
  { name: "scene.item.bind", summary: "Bind a geometry property to an expression over the collection's parameters, so changing a number moves everything that follows it.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/bind" } },
  { name: "scene.item.copy", summary: "Copy an item into another scene. The copy keeps the transform and the filters and gets a new id.", scope: "operate", mutating: true, destructive: false, rest: { method: "GET", path: "/api/v1/scenes/item/copy" } },
  { name: "scene.item.cover_canvas", summary: "Put items over the whole canvas, filling it and letting the overflow go.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/cover_canvas" } },
  { name: "scene.item.distribute", summary: "Space items evenly between the two on the ends, horizontally or vertically.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/distribute" } },
  { name: "scene.item.filter.add", summary: "Hang a filter on one item, so a camera keyed in one scene is not keyed in all of them.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/filter/add" } },
  { name: "scene.item.filter.remove", summary: "Take a filter off an item.", scope: "operate", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/scenes/item/filter/remove" } },
  { name: "scene.item.filter.set", summary: "Change one of an item's filters, or turn it off without taking it out.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/filter/set" } },
  { name: "scene.item.fit_to_canvas", summary: "Put items over the whole canvas, keeping their aspect ratio inside it.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/fit_to_canvas" } },
  { name: "scene.item.group", summary: "Put items into a group. The picture does not change.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/group" } },
  { name: "scene.item.match_size", summary: "Make items the same size as another one.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/match_size" } },
  { name: "scene.item.move", summary: "Move an item to another scene, keeping its transform and filters.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/move" } },
  { name: "scene.item.remove", summary: "Take an item off a scene.", scope: "operate", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/scenes/item/remove" } },
  { name: "scene.item.reorder", summary: "Move an item up or down the stack, between two named neighbours.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/reorder" } },
  { name: "scene.item.schema", summary: "What one item type takes: a graphic's OGraf schema, or a source or filter plugin's settings schema. The same JSON Schema every client renders an inspector from.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/scenes/item/schema" } },
  { name: "scene.item.set", summary: "Assign an item's properties. Only the keys named move; the rest are left alone, so calling it twice with the same body changes nothing the second time.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/set" } },
  { name: "scene.item.ungroup", summary: "Take a group apart, leaving every child exactly where it looked.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/item/ungroup" } },
  { name: "scene.layout.copy", summary: "Read one scene's geometry, to paste onto another.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/scenes/layout/copy" } },
  { name: "scene.layout.list", summary: "The layouts that ship with the core, with the parameters each one takes.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/scenes/layout/list" } },
  { name: "scene.layout.paste", summary: "Put one scene's geometry onto another's items, matched by name first and slot order second. Items that match nothing are left alone.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/layout/paste" } },
  { name: "scene.list", summary: "Every scene in the collection, with how many items it has, the sources it draws and whether it is armed.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/scenes" } },
  { name: "scene.params.get", summary: "The collection's typed parameters, readable without their values, so a client discovers what is fillable before filling it.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/scenes/params/get" } },
  { name: "scene.params.set", summary: "Set the collection's parameter values, declaring any that are new. A `{{name}}` in any string property of any item follows them, so one call changes every lower third that uses it.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/params/set" } },
  { name: "scene.preview.frame", summary: "A still of the armed scene as base64 JPEG, the floor every client has.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/scenes/preview/frame" } },
  { name: "scene.preview.set", summary: "Arm a scene. The armed scene is the preview, and program.take with no argument takes it.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/preview/set" } },
  { name: "scene.redo", summary: "Put back what undo took away.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/redo" } },
  { name: "scene.remove", summary: "Delete a scene. What is on air is not touched.", scope: "operate", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/scenes/{id}" } },
  { name: "scene.rename", summary: "Change a scene's name, its colour, or both. Names and colours live on the document, so every client, the tally and an agent see the same ones.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/{id}/rename" } },
  { name: "scene.transaction.abort", summary: "Throw the batch away. The document goes back to where it was when the batch opened.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/transaction/abort" } },
  { name: "scene.transaction.begin", summary: "Start a batch. Everything until the commit applies on one frame or not at all, and undoes in one step.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/transaction/begin" } },
  { name: "scene.transaction.commit", summary: "Apply the batch.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/transaction/commit" } },
  { name: "scene.undo", summary: "Undo the last change. A drag marked with scene.history.mark undoes as one step.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/scenes/undo" } },
  { name: "scene.validate", summary: "Overlaps, items off the canvas, safe area breaches and missing sources: what to fix before saying a scene is done.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/scenes/validate" } },
  { name: "show.add", summary: "Make another show and start it: empty, a copy of a show (without its outputs, so nothing goes out twice), or from a project file.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/shows" } },
  { name: "show.add_many", summary: "Make many shows in one call, such as every channel of a headend. The whole batch is checked first. With dry_run (the default) nothing is made: the answer says what would be, what its renditions would cost and whether the governor would admit them. Without it, every show that fits is made and the rest are refused with why; a show is made whole or not at all.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/shows/add_many" } },
  { name: "show.list", summary: "Every show on this machine: its name, whether it is running, what is on air, what its outputs send and what its process costs. `current` is the show a client reaches when it names none.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/shows" } },
  { name: "show.output.add", summary: "Send a show without compositing to another place: an address (SRT, RTMP, UDP, RTP or RIST), a platform and its key, or hls://<name> to serve it as HLS from this port. Left without a rendition it copies the input's bytes; with one it is planned and admitted by the governor. The key is write only.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/shows/{id}/output/add" } },
  { name: "show.output.remove", summary: "Stop one output of a show without compositing and forget it, key and all.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/shows/{id}/output/remove" } },
  { name: "show.output.set", summary: "Change one output of a show without compositing, naming only what moves: another address, a new key, on or off, copy or a rendition.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/shows/{id}/output" } },
  { name: "show.remove", summary: "Stop a show and remove it with its folder. Refused for the last show and for main, the show the station was started with.", scope: "admin", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/shows/{id}" } },
  { name: "show.remove_many", summary: "Stop and remove many shows. Each id that cannot go (main, or one not there) is refused with why, and the rest go.", scope: "admin", mutating: true, destructive: true, rest: { method: "POST", path: "/api/v1/shows/remove_many" } },
  { name: "show.rename", summary: "Give a show another name. Its id stays.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/shows/{id}/rename" } },
  { name: "show.set", summary: "Change a show's name, its input, or whether it composites. Turning compositing on starts a show process whose one source is the input and moves the outputs to it; turning it off hands them back to the direct host, when the show has one source and no scenes in use. A switch can take half a minute, so it answers at once with a task_id and the show as it is; task.get with that id carries this answer, with how long the outputs were off, once it is done.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/shows/{id}/set" } },
  { name: "show.start", summary: "Start a stopped or failed show.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/shows/{id}/start" } },
  { name: "show.stats", summary: "Health, input numbers and each output's numbers for many shows in one read, from what the station already holds, so it is cheap to call every second for two hundred shows. `fields` narrows it to health, input or outputs.", scope: "read", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/shows/stats" } },
  { name: "show.stop", summary: "Stop a show. It keeps its config, and stays stopped when the station starts again, until show.start.", scope: "admin", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/shows/{id}/stop" } },
  { name: "snapshot.get", summary: "One JPEG: the whole contact sheet, the programme, or one source cut out of the mosaic.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/snapshot/{id}" } },
  { name: "source.add", summary: "Add a source while the mixer runs. Answers with the id it got and the whole source record.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/sources" } },
  { name: "source.audio.set", summary: "Move a source's audio: the fader, the mute, and for a superimposed page the balance between its own sound and the videos under it.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/sources/{id}/audio" } },
  { name: "source.duplicate", summary: "Add another source like one the mixer has: the same address and settings under a new id. A client cannot do this with source.add, because the address it is shown has everything after the host cut off.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/sources/{id}/duplicate" } },
  { name: "source.get", summary: "One source. Refused with the ids that exist when there is no such source.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/sources/{id}" } },
  { name: "source.group", summary: "Put sources in a tray folder. A tag for finding things, not a group on the canvas.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/sources/{id}/group" } },
  { name: "source.list", summary: "Every source, with its state, whether it has video and audio, and its fader.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/sources" } },
  { name: "source.missing", summary: "Sources that are not running, and why: failed, could not be started (with the error and the action that fixes it), removed, or unknown. Pass the ids a scene draws, or none for every one the mixer knows about.", scope: "read", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/sources/{id}/missing" } },
  { name: "source.remove", summary: "Remove a source. If it is on programme the mixer cuts to the slate first.", scope: "operate", mutating: true, destructive: true, rest: { method: "DELETE", path: "/api/v1/sources/{id}" } },
  { name: "source.restart", summary: "Build a source's pipeline again now, rather than waiting for its next retry. For a source that could not be started or was removed, use source.restore.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/sources/{id}/restart" } },
  { name: "source.restore", summary: "Put back a source that source.remove took away, as it was: same id, address, settings, fader and mute. The mixer remembers the last sixteen it removed, until it restarts.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/sources/{id}/restore" } },
  { name: "source.seek", summary: "Move a seekable source to a position. Answers with where it actually landed.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/sources/{id}/seek" } },
  { name: "source.set", summary: "Change a running source: its name and colour, its params, or where it runs. The name and colour live on the scene document. Moving a source between the core, a sidecar and a node is `place`; the programme keeps its frame rate across the move and the compositor covers the swap.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/sources/{id}/set" } },
  { name: "task.cancel", summary: "Ask a piece of long running work to stop. Cooperative: the answer says the request landed, not that the work has stopped yet.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/tasks/{id}/cancel" } },
  { name: "task.get", summary: "How a piece of long running work is getting on, and its answer once it has one.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/tasks/{id}" } },
  { name: "task.list", summary: "Every background job this core knows about, newest first.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/tasks" } },
  { name: "template.fields", summary: "A running graphic's fields: each one's label, type, default and what it shows now.", scope: "read", mutating: false, destructive: false, rest: { method: "POST", path: "/api/v1/template/fields" } },
  { name: "template.get", summary: "One template, with its SVG as written.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/template" } },
  { name: "template.list", summary: "The graphic templates: the built in pack and the SVG templates in the media library, each with its fields.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/template/list" } },
  { name: "template.save", summary: "Check an SVG template and write it into the media library.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/template/save" } },
  { name: "tool.call", summary: "Call one of a plugin's tools, in MCP's shape. The name is `<plugin>/<tool>`, or the bare tool name when only one plugin has it.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/tool/call" } },
  { name: "vitals.get", summary: "This show's health (its state and alarms, null in the first second) and the thresholds they are judged by.", scope: "read", mutating: false, destructive: false, rest: { method: "GET", path: "/api/v1/vitals" } },
  { name: "vitals.set", summary: "Change the alarm thresholds, or whether a mosaic is kept up for the black and freeze checks while nobody is looking. Fields left out keep their defaults; a duration of 0 switches that check off. Applies within a second.", scope: "operate", mutating: true, destructive: false, rest: { method: "POST", path: "/api/v1/vitals/set" } },
] as const;

/** The `ext` keys this api_level knows, and whether the core implements them yet. */
export const EXT_KEYS: Readonly<Record<string, { value: string; implemented: boolean }>> = {
  "multiview": { value: "{fps: 1..30, width: 320..1920} or false", implemented: true },
  "meters": { value: "true", implemented: true },
  "tally": { value: "true", implemented: true },
  "positions": { value: "true", implemented: true },
  "thumb": { value: "{fps}", implemented: false },
  "preview": { value: "{fps, width} or \"full\"", implemented: true },
  "telemetry": { value: "{hz: 1..10}", implemented: false },
  "agent": { value: "true or thresholds", implemented: false },
};

export const EVENT_NAMES: readonly EventName[] = [
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

  /** Make a channel and its first key, which is in this answer. channel.key.reveal reads it again later. */
  channelAdd(params: ChannelAddRequest): Promise<ChannelAdded> {
    return this._call("channel.add", params as unknown as Record<string, unknown>) as Promise<ChannelAdded>;
  }

  /** Make a self signed certificate for RTMPS, for this machine's addresses unless names are given. Encoders must be told to accept it; one from a certificate authority needs no such step. */
  channelCertificateGenerate(params: CertificateGenerateRequest = {}): Promise<CertificateInfo> {
    return this._call("channel.certificate.generate", params as unknown as Record<string, unknown>) as Promise<CertificateInfo>;
  }

  /** Give RTMPS a certificate: the PEM of the certificate (and its chain) and of its private key, as a certificate authority issued them. Checked before it is kept; the key is sealed and never read back. */
  channelCertificateSet(params: CertificateSetRequest): Promise<CertificateInfo> {
    return this._call("channel.certificate.set", params as unknown as Record<string, unknown>) as Promise<CertificateInfo>;
  }

  /** Send a channel's stream on to YouTube, Facebook, Twitch, an RTMP server or an SRT receiver as it arrives. Nothing is decoded or encoded. The key is write only. */
  channelDestinationAdd(params: AddDestinationRequest): Promise<Record<string, unknown>> {
    return this._call("channel.destination.add", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Stop sending a channel's stream to one destination and forget it. The publisher and the other destinations are not touched. */
  channelDestinationRemove(params: RemoveDestinationRequest): Promise<Record<string, unknown>> {
    return this._call("channel.destination.remove", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Change one of a channel's destinations, naming only what moves: a new key, another server, which stream it sends, on or off. A key left out is kept. */
  channelDestinationSet(params: SetDestinationRequest): Promise<Record<string, unknown>> {
    return this._call("channel.destination.set", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** One channel. */
  channelGet(params: IdRequest): Promise<Channel> {
    return this._call("channel.get", params as unknown as Record<string, unknown>) as Promise<Channel>;
  }

  /** Make another key for a channel, to give to one more person or encoder. The key is in this answer, and channel.key.reveal reads it again later. */
  channelKeyAdd(params: ChannelKeyAddRequest): Promise<KeyAdded> {
    return this._call("channel.key.add", params as unknown as Record<string, unknown>) as Promise<KeyAdded>;
  }

  /** Take one key back. A publisher on air with it is cut off and the next one is turned away; the other keys are untouched. */
  channelKeyRemove(params: ChannelKeyRemoveRequest): Promise<Channel> {
    return this._call("channel.key.remove", params as unknown as Record<string, unknown>) as Promise<Channel>;
  }

  /** Read one key of a channel back, to give it to an encoder again. Admin only; a list shows only the last four characters. Each read is logged with who asked, never with the key. */
  channelKeyReveal(params: ChannelKeyRevealRequest): Promise<KeyRevealed> {
    return this._call("channel.key.reveal", params as unknown as Record<string, unknown>) as Promise<KeyRevealed>;
  }

  /** Every channel with its keys (as hints), the address to publish to over each protocol it has on, and what is live on it; and which ingest ports are open and for which channels. */
  channelList(): Promise<ChannelList> {
    return this._call("channel.list", {}) as Promise<ChannelList>;
  }

  /** Remove a channel and forget its keys. Sources it made that no scene holds go with it. */
  channelRemove(params: IdRequest): Promise<ChannelRemoved> {
    return this._call("channel.remove", params as unknown as Record<string, unknown>) as Promise<ChannelRemoved>;
  }

  /** Rename a channel, switch it on or off, or change its application name, whether its streams become sources, how its key is given, which protocols it takes (rtmp, srt, whip) or RTMPS and its port. A port opens when the first channel needs it and closes when the last one stops. Only what is named moves. */
  channelSet(params: ChannelSetRequest): Promise<Channel> {
    return this._call("channel.set", params as unknown as Record<string, unknown>) as Promise<Channel>;
  }

  /** Every codec and element in the catalogue, which of them this machine actually has, and what it would pick. */
  codecList(): Promise<Record<string, unknown>> {
    return this._call("codec.list", {}) as Promise<Record<string, unknown>>;
  }

  /** The mixer's settings: each key's value in the config file, its default, when a change to it takes effect, and which keys are waiting for a restart. Secrets say only whether one is set. */
  configGet(params: ConfigGetRequest = {}): Promise<ConfigGetResult> {
    return this._call("config.get", params as unknown as Record<string, unknown>) as Promise<ConfigGetResult>;
  }

  /** Put settings back to their defaults by taking them out of the config file. Answers like config.set. */
  configReset(params: ConfigResetRequest): Promise<ConfigSetResult> {
    return this._call("config.reset", params as unknown as Record<string, unknown>) as Promise<ConfigSetResult>;
  }

  /** Every setting config.set takes, as one JSON Schema: type, title, description, default, range or choices, and x-gmx-applies (live, next_source or restart). */
  configSchema(): Promise<Record<string, unknown>> {
    return this._call("config.schema", {}) as Promise<Record<string, unknown>>;
  }

  /** Change settings in the config file, keeping its comments. Every value is checked first and nothing is written unless all of them fit. Live keys take effect at once; the answer says which wait for the next source or a restart. */
  configSet(params: ConfigSetRequest): Promise<ConfigSetResult> {
    return this._call("config.set", params as unknown as Record<string, unknown>) as Promise<ConfigSetResult>;
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

  /** Stop the mixer and have it started again, when something will start it again. On a supervised core (core.info restart.possible) it answers restarting: true and exits; the programme is off air until it is back. On a core started by hand it answers restarting: false, says how to restart it, and keeps running. */
  coreRestart(): Promise<RestartAnswer> {
    return this._call("core.restart", {}) as Promise<RestartAnswer>;
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

  /** Ask every device plugin what it can see: cameras, NDI senders, publishers. Each candidate's params are ready for source.add. */
  deviceDiscover(params: DiscoverRequest = {}): Promise<Record<string, unknown>> {
    return this._call("device.discover", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
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

  /** Measure this machine's encoders again, in the background, a few seconds of every core. Refused while anything is on air unless `confirm` is true. */
  governorCalibrate(params: CalibrateRequest = {}): Promise<CalibrateResult> {
    return this._call("governor.calibrate", params as unknown as Record<string, unknown>) as Promise<CalibrateResult>;
  }

  /** The resource governor: when this machine was measured, what is in use and free on the CPU and each GPU encoder, and what was shed to keep the programme whole. */
  governorStatus(): Promise<GovernorStatus> {
    return this._call("governor.status", {}) as Promise<GovernorStatus>;
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

  /** Look for nodes on the local network over mDNS. A network without multicast finds nothing and the [nodes] table in the config is the way there. */
  nodeDiscover(params: DiscoverRequest2 = {}): Promise<DiscoverAnswer> {
    return this._call("node.discover", params as unknown as Record<string, unknown>) as Promise<DiscoverAnswer>;
  }

  /** Mint a one time enrolment token for a node. The answer carries the command to run on the other machine. The token is good for one enrolment and expires. */
  nodeEnrol(params: EnrolRequest): Promise<Record<string, unknown>> {
    return this._call("node.enrol", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** One node: its clock offset, how long since its last heartbeat, the plugins it has, and the instances it is hosting. */
  nodeGet(params: NodeName): Promise<NodeView> {
    return this._call("node.get", params as unknown as Record<string, unknown>) as Promise<NodeView>;
  }

  /** Every node this core knows about: the ones connected now, the ones that have gone quiet, and the ones the config expects that have never dialled in. */
  nodeList(): Promise<NodeListing> {
    return this._call("node.list", {}) as Promise<NodeListing>;
  }

  /** Forget a node. Its bridge is closed, every token minted for a plugin on it is revoked, and its certificate stops working. Sources placed on it go to the slate until they are moved or the node enrols again. */
  nodeRemove(params: NodeName): Promise<Record<string, unknown>> {
    return this._call("node.remove", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
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

  /** Change a destination in place: a new address with a new stream key, a new reconnect policy, a deeper outage buffer. The address is write only, so a client that only wants the buffer never has to hold the key. */
  outputSet(params: SetOutputRequest): Promise<OutputStatus> {
    return this._call("output.set", params as unknown as Record<string, unknown>) as Promise<OutputStatus>;
  }

  /** Make one new folder inside a folder path.list shows, and list it. A folder that is already there is listed rather than refused. */
  pathCreate(params: PathCreateRequest): Promise<PathListing> {
    return this._call("path.create", params as unknown as Record<string, unknown>) as Promise<PathListing>;
  }

  /** The folders in one folder on the mixer, and whether each is writable, for a folder picker. Only the home folder and the mixer's own folders are shown; files never are. */
  pathList(params: PathListRequest = {}): Promise<PathListing> {
    return this._call("path.list", params as unknown as Record<string, unknown>) as Promise<PathListing>;
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

  /** Install a plugin, while live, from any source form: a GitHub release (owner/repo), a git URL, cargo:, npm:, pypi:, a local directory, or a bare name looked up in the marketplaces this mixer knows. The signature and the api level are checked before anything is copied. */
  pluginAdd(params: AddPluginRequest): Promise<PluginRecord> {
    return this._call("plugin.add", params as unknown as Record<string, unknown>) as Promise<PluginRecord>;
  }

  /** One plugin in full: its manifest, the settings schema of every provide, and the description from each SKILL.md. */
  pluginDescribe(params: PluginName): Promise<PluginDescription> {
    return this._call("plugin.describe", params as unknown as Record<string, unknown>) as Promise<PluginDescription>;
  }

  /** Turn a plugin off without uninstalling it. It registers nothing and runs no process until it is enabled again. */
  pluginDisable(params: PluginName): Promise<PluginRecord> {
    return this._call("plugin.disable", params as unknown as Record<string, unknown>) as Promise<PluginRecord>;
  }

  /** Turn a plugin back on. It registers what it declares and its instances start. */
  pluginEnable(params: PluginName): Promise<PluginRecord> {
    return this._call("plugin.enable", params as unknown as Record<string, unknown>) as Promise<PluginRecord>;
  }

  /** Every plugin installed, with what it provides and what each running instance is costing in cpu, memory, latency, dropped buffers and restarts. */
  pluginList(): Promise<PluginListing> {
    return this._call("plugin.list", {}) as Promise<PluginListing>;
  }

  /** Read a plugin's directory again and swap its running instances one at a time, with the freeze frame covering each. */
  pluginReload(params: PluginName): Promise<PluginRecord> {
    return this._call("plugin.reload", params as unknown as Record<string, unknown>) as Promise<PluginRecord>;
  }

  /** Uninstall a plugin and unwind everything it registered: its provides, its tools, its panels, its hooks and its discovery matchers. */
  pluginRemove(params: PluginName): Promise<PluginRemoved> {
    return this._call("plugin.remove", params as unknown as Record<string, unknown>) as Promise<PluginRemoved>;
  }

  /** Search every marketplace this mixer knows for a plugin, by name, description or kind. Answers what `gmx plugin add <name>` would install. */
  pluginSearch(params: SearchRequest = {}): Promise<SearchResults> {
    return this._call("plugin.search", params as unknown as Record<string, unknown>) as Promise<SearchResults>;
  }

  /** A plugin's settings as they stand, with its schema beside them. */
  pluginSettingsGet(params: PluginName): Promise<PluginSettings> {
    return this._call("plugin.settings.get", params as unknown as Record<string, unknown>) as Promise<PluginSettings>;
  }

  /** Change a plugin's settings. A plugin that cannot take a change while running says so rather than being restarted behind your back. */
  pluginSettingsSet(params: SetSettingsRequest): Promise<PluginSettings> {
    return this._call("plugin.settings.set", params as unknown as Record<string, unknown>) as Promise<PluginSettings>;
  }

  /** Per instance cpu, memory, media latency, dropped buffers and restarts, refreshed once a second. */
  pluginStats(): Promise<StatsListing> {
    return this._call("plugin.stats", {}) as Promise<StatsListing>;
  }

  /** Fetch a newer build of a plugin, install it beside the one that is running, and prove it starts. A build that does not answer `initialize` within ten seconds is rolled back and the plugin that was working stays working. */
  pluginUpdate(params: UpdatePluginRequest): Promise<PluginUpdated> {
    return this._call("plugin.update", params as unknown as Record<string, unknown>) as Promise<PluginUpdated>;
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

  /** Give up a raw frame socket. The socket goes when the last holder closes it. */
  previewClose(params: PreviewOpenRequest): Promise<PreviewClosed> {
    return this._call("preview.close", params as unknown as Record<string, unknown>) as Promise<PreviewClosed>;
  }

  /** Open a raw frame socket on this machine for a source or the programme, and answer with its path. No encode anywhere: a client on the same host reads the frames the mixer already has. Close it with preview.close. */
  previewOpen(params: PreviewOpenRequest): Promise<PreviewSocket> {
    return this._call("preview.open", params as unknown as Record<string, unknown>) as Promise<PreviewSocket>;
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

  /** Put a scene or a source on programme. The cut is instant and the outgoing stream is not disturbed. */
  programTake(params: TakeRequest = {}): Promise<ProgramState> {
    return this._call("program.take", params as unknown as Record<string, unknown>) as Promise<ProgramState>;
  }

  /** What is on air as a small JPEG in base64, {jpeg, width, height, at_ms}, or {pending: true} while the first picture is on its way. An ask keeps one picture a second coming for ten seconds; nothing runs between asks. */
  programThumbnail(params: ThumbnailRequest = {}): Promise<Record<string, unknown>> {
    return this._call("program.thumbnail", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Every transition a take may name on this core, with the params each reads, and what an item's enter and exit may be. */
  programTransitions(): Promise<TransitionCatalogue> {
    return this._call("program.transitions", {}) as Promise<TransitionCatalogue>;
  }

  /** This mixer as one project file: settings, sources, outputs and renditions, channels, scenes, the page's layout, and its clips by name and size. Keys only with include_secrets. */
  projectExport(params: ExportRequest = {}): Promise<Record<string, unknown>> {
    return this._call("project.export", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Open a project file: answers with what it would change (dry_run is true unless false is sent), then replaces this mixer's setup or merges beside it. Says which settings wait for a restart. */
  projectImport(params: ImportRequest): Promise<Report> {
    return this._call("project.import", params as unknown as Record<string, unknown>) as Promise<Report>;
  }

  /** What the planner built for every output that asked for a rendition: each node, what it serves, which encoder and why, and the totals. */
  renditionPlan(params: PlanRequest = {}): Promise<PlanView> {
    return this._call("rendition.plan", params as unknown as Record<string, unknown>) as Promise<PlanView>;
  }

  /** Every rendition preset, priced on this machine by the governor. One this machine cannot make says so, with why. */
  renditionPresets(): Promise<PresetsResult> {
    return this._call("rendition.presets", {}) as Promise<PresetsResult>;
  }

  /** Make an empty scene, or one built from a set of sources. */
  sceneAdd(params: AddSceneRequest): Promise<SceneView> {
    return this._call("scene.add", params as unknown as Record<string, unknown>) as Promise<SceneView>;
  }

  /** Fill a graphic that is on a scene, by field name, and optionally play it on or take it off. Answers with the records and, if asked, a still. */
  sceneApplyGraphic(params: ApplyGraphicRequest): Promise<Record<string, unknown>> {
    return this._call("scene.apply_graphic", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Apply a layout, making a scene or reshaping one that exists. Applying onto an existing scene keeps the item ids, so the change is a ramp and not a cut. */
  sceneApplyLayout(params: ApplyLayoutRequest): Promise<Record<string, unknown>> {
    return this._call("scene.apply_layout", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** A scene from a set of sources, laid out by the built in layout for that count (full, two-box, three-box, quad, then a grid) or by a named one. */
  sceneCreateFrom(params: CreateFromRequest): Promise<SceneView> {
    return this._call("scene.create_from", params as unknown as Record<string, unknown>) as Promise<SceneView>;
  }

  /** A copy of a scene with new ids throughout, so editing the copy cannot touch the original. */
  sceneDuplicate(params: DuplicateSceneRequest): Promise<SceneView> {
    return this._call("scene.duplicate", params as unknown as Record<string, unknown>) as Promise<SceneView>;
  }

  /** Write a draft back into the live document. */
  sceneEditApply(params: DraftRequest): Promise<Record<string, unknown>> {
    return this._call("scene.edit.apply", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Take a working copy of a scene. Editing is off air by default: the draft is written back on the next take of that scene, or when you apply it. */
  sceneEditBegin(params: EditBeginRequest): Promise<DraftRecord> {
    return this._call("scene.edit.begin", params as unknown as Record<string, unknown>) as Promise<DraftRecord>;
  }

  /** Throw a draft away. The live document is untouched. */
  sceneEditDiscard(params: DraftRequest): Promise<Record<string, unknown>> {
    return this._call("scene.edit.discard", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** The whole collection: as JSON, or as a zip bundle carrying its assets with a hash each, which is what you send somebody. */
  sceneExport(params: ExportRequest2 = {}): Promise<Record<string, unknown>> {
    return this._call("scene.export", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** One scene: its records and where every item actually lands on the canvas. */
  sceneGet(params: SceneRequest): Promise<SceneView> {
    return this._call("scene.get", params as unknown as Record<string, unknown>) as Promise<SceneView>;
  }

  /** Every graphic template this core can place, with what each one takes. */
  sceneGraphicList(): Promise<GraphicListing> {
    return this._call("scene.graphic.list", {}) as Promise<GraphicListing>;
  }

  /** Group the changes that follow into one undo step, until the next mark. This is what makes a drag of forty moves one Ctrl+Z. */
  sceneHistoryMark(params: MarkRequest = {}): Promise<Record<string, unknown>> {
    return this._call("scene.history.mark", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Read a collection bundle, a zip or the directory it unpacks to, and add its scenes to this one. Answers with a relink report for any asset that did not come across. */
  sceneImport(params: ImportRequest2): Promise<ImportedReport> {
    return this._call("scene.import", params as unknown as Record<string, unknown>) as Promise<ImportedReport>;
  }

  /** Read an OBS Studio scene collection and add its scenes to this one. Send the file's text as `content` (what a page's file picker reads) or a `path` on the mixer's machine. With `add_sources: true` the sources the scenes draw are added through source.add, and the answer says which were added and why any were not. */
  sceneImportObs(params: ImportObsRequest = {}): Promise<ImportReport> {
    return this._call("scene.import.obs", params as unknown as Record<string, unknown>) as Promise<ImportReport>;
  }

  /** Put something on a scene's canvas. With no transform it lands in the next free cell, so a drop never needs a dialog. */
  sceneItemAdd(params: AddItemRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.add", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Line items up on an edge: left, right, top, bottom, center-x or center-y. */
  sceneItemAlign(params: ItemsRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.align", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Lay items out in a grid of `cols` columns. */
  sceneItemArrangeGrid(params: ItemsRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.arrange_grid", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Bind a geometry property to an expression over the collection's parameters, so changing a number moves everything that follows it. */
  sceneItemBind(params: BindRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.bind", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Copy an item into another scene. The copy keeps the transform and the filters and gets a new id. */
  sceneItemCopy(params: MoveItemRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.copy", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Put items over the whole canvas, filling it and letting the overflow go. */
  sceneItemCoverCanvas(params: ItemsRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.cover_canvas", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Space items evenly between the two on the ends, horizontally or vertically. */
  sceneItemDistribute(params: ItemsRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.distribute", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Hang a filter on one item, so a camera keyed in one scene is not keyed in all of them. */
  sceneItemFilterAdd(params: AddItemFilterRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.filter.add", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Take a filter off an item. */
  sceneItemFilterRemove(params: ItemFilterRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.filter.remove", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Change one of an item's filters, or turn it off without taking it out. */
  sceneItemFilterSet(params: ItemFilterRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.filter.set", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Put items over the whole canvas, keeping their aspect ratio inside it. */
  sceneItemFitToCanvas(params: ItemsRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.fit_to_canvas", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Put items into a group. The picture does not change. */
  sceneItemGroup(params: ItemsRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.group", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Make items the same size as another one. */
  sceneItemMatchSize(params: ItemsRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.match_size", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Move an item to another scene, keeping its transform and filters. */
  sceneItemMove(params: MoveItemRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.move", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Take an item off a scene. */
  sceneItemRemove(params: ItemRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.remove", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Move an item up or down the stack, between two named neighbours. */
  sceneItemReorder(params: ReorderRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.reorder", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** What one item type takes: a graphic's OGraf schema, or a source or filter plugin's settings schema. The same JSON Schema every client renders an inspector from. */
  sceneItemSchema(params: ItemSchemaRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.schema", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Assign an item's properties. Only the keys named move; the rest are left alone, so calling it twice with the same body changes nothing the second time. */
  sceneItemSet(params: SetItemRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.set", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Take a group apart, leaving every child exactly where it looked. */
  sceneItemUngroup(params: ItemRequest): Promise<Record<string, unknown>> {
    return this._call("scene.item.ungroup", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Read one scene's geometry, to paste onto another. */
  sceneLayoutCopy(params: SceneRequest): Promise<Layout> {
    return this._call("scene.layout.copy", params as unknown as Record<string, unknown>) as Promise<Layout>;
  }

  /** The layouts that ship with the core, with the parameters each one takes. */
  sceneLayoutList(): Promise<LayoutListing> {
    return this._call("scene.layout.list", {}) as Promise<LayoutListing>;
  }

  /** Put one scene's geometry onto another's items, matched by name first and slot order second. Items that match nothing are left alone. */
  sceneLayoutPaste(params: LayoutClipboardRequest): Promise<Record<string, unknown>> {
    return this._call("scene.layout.paste", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Every scene in the collection, with how many items it has, the sources it draws and whether it is armed. */
  sceneList(): Promise<SceneListing> {
    return this._call("scene.list", {}) as Promise<SceneListing>;
  }

  /** The collection's typed parameters, readable without their values, so a client discovers what is fillable before filling it. */
  sceneParamsGet(): Promise<Record<string, unknown>> {
    return this._call("scene.params.get", {}) as Promise<Record<string, unknown>>;
  }

  /** Set the collection's parameter values, declaring any that are new. A `{{name}}` in any string property of any item follows them, so one call changes every lower third that uses it. */
  sceneParamsSet(params: ParamsRequest = {}): Promise<Record<string, unknown>> {
    return this._call("scene.params.set", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** A still of the armed scene as base64 JPEG, the floor every client has. */
  scenePreviewFrame(params: PreviewFrameRequest = {}): Promise<Record<string, unknown>> {
    return this._call("scene.preview.frame", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Arm a scene. The armed scene is the preview, and program.take with no argument takes it. */
  scenePreviewSet(params: PreviewRequest = {}): Promise<Record<string, unknown>> {
    return this._call("scene.preview.set", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Put back what undo took away. */
  sceneRedo(): Promise<HistoryStep> {
    return this._call("scene.redo", {}) as Promise<HistoryStep>;
  }

  /** Delete a scene. What is on air is not touched. */
  sceneRemove(params: SceneRequest): Promise<SceneRemoved> {
    return this._call("scene.remove", params as unknown as Record<string, unknown>) as Promise<SceneRemoved>;
  }

  /** Change a scene's name, its colour, or both. Names and colours live on the document, so every client, the tally and an agent see the same ones. */
  sceneRename(params: RenameSceneRequest): Promise<SceneView> {
    return this._call("scene.rename", params as unknown as Record<string, unknown>) as Promise<SceneView>;
  }

  /** Throw the batch away. The document goes back to where it was when the batch opened. */
  sceneTransactionAbort(): Promise<Record<string, unknown>> {
    return this._call("scene.transaction.abort", {}) as Promise<Record<string, unknown>>;
  }

  /** Start a batch. Everything until the commit applies on one frame or not at all, and undoes in one step. */
  sceneTransactionBegin(): Promise<Record<string, unknown>> {
    return this._call("scene.transaction.begin", {}) as Promise<Record<string, unknown>>;
  }

  /** Apply the batch. */
  sceneTransactionCommit(): Promise<Record<string, unknown>> {
    return this._call("scene.transaction.commit", {}) as Promise<Record<string, unknown>>;
  }

  /** Undo the last change. A drag marked with scene.history.mark undoes as one step. */
  sceneUndo(): Promise<HistoryStep> {
    return this._call("scene.undo", {}) as Promise<HistoryStep>;
  }

  /** Overlaps, items off the canvas, safe area breaches and missing sources: what to fix before saying a scene is done. */
  sceneValidate(params: ValidateRequest = {}): Promise<Validation> {
    return this._call("scene.validate", params as unknown as Record<string, unknown>) as Promise<Validation>;
  }

  /** Make another show and start it: empty, a copy of a show (without its outputs, so nothing goes out twice), or from a project file. */
  showAdd(params: ShowAddRequest): Promise<Show> {
    return this._call("show.add", params as unknown as Record<string, unknown>) as Promise<Show>;
  }

  /** Make many shows in one call, such as every channel of a headend. The whole batch is checked first. With dry_run (the default) nothing is made: the answer says what would be, what its renditions would cost and whether the governor would admit them. Without it, every show that fits is made and the rest are refused with why; a show is made whole or not at all. */
  showAddMany(params: ShowAddManyRequest): Promise<ShowAddManyResult> {
    return this._call("show.add_many", params as unknown as Record<string, unknown>) as Promise<ShowAddManyResult>;
  }

  /** Every show on this machine: its name, whether it is running, what is on air, what its outputs send and what its process costs. `current` is the show a client reaches when it names none. */
  showList(): Promise<ShowList> {
    return this._call("show.list", {}) as Promise<ShowList>;
  }

  /** Send a show without compositing to another place: an address (SRT, RTMP, UDP, RTP or RIST), a platform and its key, or hls://<name> to serve it as HLS from this port. Left without a rendition it copies the input's bytes; with one it is planned and admitted by the governor. The key is write only. */
  showOutputAdd(params: ShowOutputAddRequest): Promise<Show> {
    return this._call("show.output.add", params as unknown as Record<string, unknown>) as Promise<Show>;
  }

  /** Stop one output of a show without compositing and forget it, key and all. */
  showOutputRemove(params: ShowOutputRemoveRequest): Promise<Show> {
    return this._call("show.output.remove", params as unknown as Record<string, unknown>) as Promise<Show>;
  }

  /** Change one output of a show without compositing, naming only what moves: another address, a new key, on or off, copy or a rendition. */
  showOutputSet(params: ShowOutputSetRequest): Promise<Show> {
    return this._call("show.output.set", params as unknown as Record<string, unknown>) as Promise<Show>;
  }

  /** Stop a show and remove it with its folder. Refused for the last show and for main, the show the station was started with. */
  showRemove(params: IdRequest): Promise<ShowRemoved> {
    return this._call("show.remove", params as unknown as Record<string, unknown>) as Promise<ShowRemoved>;
  }

  /** Stop and remove many shows. Each id that cannot go (main, or one not there) is refused with why, and the rest go. */
  showRemoveMany(params: ShowRemoveManyRequest): Promise<ShowRemoveManyResult> {
    return this._call("show.remove_many", params as unknown as Record<string, unknown>) as Promise<ShowRemoveManyResult>;
  }

  /** Give a show another name. Its id stays. */
  showRename(params: ShowRenameRequest): Promise<Show> {
    return this._call("show.rename", params as unknown as Record<string, unknown>) as Promise<Show>;
  }

  /** Change a show's name, its input, or whether it composites. Turning compositing on starts a show process whose one source is the input and moves the outputs to it; turning it off hands them back to the direct host, when the show has one source and no scenes in use. A switch can take half a minute, so it answers at once with a task_id and the show as it is; task.get with that id carries this answer, with how long the outputs were off, once it is done. */
  showSet(params: ShowSetRequest): Promise<ShowSetResult> {
    return this._call("show.set", params as unknown as Record<string, unknown>) as Promise<ShowSetResult>;
  }

  /** Start a stopped or failed show. */
  showStart(params: IdRequest): Promise<Show> {
    return this._call("show.start", params as unknown as Record<string, unknown>) as Promise<Show>;
  }

  /** Health, input numbers and each output's numbers for many shows in one read, from what the station already holds, so it is cheap to call every second for two hundred shows. `fields` narrows it to health, input or outputs. */
  showStats(params: ShowStatsRequest = {}): Promise<ShowStatsList> {
    return this._call("show.stats", params as unknown as Record<string, unknown>) as Promise<ShowStatsList>;
  }

  /** Stop a show. It keeps its config, and stays stopped when the station starts again, until show.start. */
  showStop(params: IdRequest): Promise<Show> {
    return this._call("show.stop", params as unknown as Record<string, unknown>) as Promise<Show>;
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

  /** Add another source like one the mixer has: the same address and settings under a new id. A client cannot do this with source.add, because the address it is shown has everything after the host cut off. */
  sourceDuplicate(params: DuplicateSourceRequest): Promise<SourceStatus> {
    return this._call("source.duplicate", params as unknown as Record<string, unknown>) as Promise<SourceStatus>;
  }

  /** One source. Refused with the ids that exist when there is no such source. */
  sourceGet(params: IdRequest): Promise<SourceStatus> {
    return this._call("source.get", params as unknown as Record<string, unknown>) as Promise<SourceStatus>;
  }

  /** Put sources in a tray folder. A tag for finding things, not a group on the canvas. */
  sourceGroup(params: GroupSourcesRequest): Promise<Record<string, unknown>> {
    return this._call("source.group", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Every source, with its state, whether it has video and audio, and its fader. */
  sourceList(): Promise<SourceStatus[]> {
    return this._call("source.list", {}) as Promise<SourceStatus[]>;
  }

  /** Sources that are not running, and why: failed, could not be started (with the error and the action that fixes it), removed, or unknown. Pass the ids a scene draws, or none for every one the mixer knows about. */
  sourceMissing(params: MissingRequest = {}): Promise<MissingSource[]> {
    return this._call("source.missing", params as unknown as Record<string, unknown>) as Promise<MissingSource[]>;
  }

  /** Remove a source. If it is on programme the mixer cuts to the slate first. */
  sourceRemove(params: IdRequest): Promise<Record<string, unknown>> {
    return this._call("source.remove", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** Build a source's pipeline again now, rather than waiting for its next retry. For a source that could not be started or was removed, use source.restore. */
  sourceRestart(params: IdRequest): Promise<SourceStatus> {
    return this._call("source.restart", params as unknown as Record<string, unknown>) as Promise<SourceStatus>;
  }

  /** Put back a source that source.remove took away, as it was: same id, address, settings, fader and mute. The mixer remembers the last sixteen it removed, until it restarts. */
  sourceRestore(params: IdRequest): Promise<SourceStatus> {
    return this._call("source.restore", params as unknown as Record<string, unknown>) as Promise<SourceStatus>;
  }

  /** Move a seekable source to a position. Answers with where it actually landed. */
  sourceSeek(params: SeekParams): Promise<SourcePositionState> {
    return this._call("source.seek", params as unknown as Record<string, unknown>) as Promise<SourcePositionState>;
  }

  /** Change a running source: its name and colour, its params, or where it runs. The name and colour live on the scene document. Moving a source between the core, a sidecar and a node is `place`; the programme keeps its frame rate across the move and the compositor covers the swap. */
  sourceSet(params: SetSourceRequest): Promise<SourceStatus> {
    return this._call("source.set", params as unknown as Record<string, unknown>) as Promise<SourceStatus>;
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

  /** A running graphic's fields: each one's label, type, default and what it shows now. */
  templateFields(params: TemplateFieldsRequest): Promise<TemplateFields> {
    return this._call("template.fields", params as unknown as Record<string, unknown>) as Promise<TemplateFields>;
  }

  /** One template, with its SVG as written. */
  templateGet(params: TemplateGetRequest): Promise<TemplateDoc> {
    return this._call("template.get", params as unknown as Record<string, unknown>) as Promise<TemplateDoc>;
  }

  /** The graphic templates: the built in pack and the SVG templates in the media library, each with its fields. */
  templateList(): Promise<TemplateList> {
    return this._call("template.list", {}) as Promise<TemplateList>;
  }

  /** Check an SVG template and write it into the media library. */
  templateSave(params: TemplateSaveRequest): Promise<TemplateSaved> {
    return this._call("template.save", params as unknown as Record<string, unknown>) as Promise<TemplateSaved>;
  }

  /** Call one of a plugin's tools, in MCP's shape. The name is `<plugin>/<tool>`, or the bare tool name when only one plugin has it. */
  toolCall(params: ToolCallRequest): Promise<Record<string, unknown>> {
    return this._call("tool.call", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

  /** This show's health (its state and alarms, null in the first second) and the thresholds they are judged by. */
  vitalsGet(): Promise<Record<string, unknown>> {
    return this._call("vitals.get", {}) as Promise<Record<string, unknown>>;
  }

  /** Change the alarm thresholds, or whether a mosaic is kept up for the black and freeze checks while nobody is looking. Fields left out keep their defaults; a duration of 0 switches that check off. Applies within a second. */
  vitalsSet(params: VitalsConfig = {}): Promise<Record<string, unknown>> {
    return this._call("vitals.set", params as unknown as Record<string, unknown>) as Promise<Record<string, unknown>>;
  }

}
