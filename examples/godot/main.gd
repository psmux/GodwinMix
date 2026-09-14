# A GodwinMix control panel in Godot 4.
#
# Connect, subscribe, list the sources, take one on click, and show the
# programme. The whole client is `WebSocketPeer` plus `JSON`, which are both in
# the engine: there is no add-on to install and nothing to compile.
#
# The picture comes from `GET /api/v1/snapshot/program` on a timer, through
# `HTTPRequest`, because a JPEG into an `ImageTexture` is four lines. A core
# that serves `/mjpeg/program` can be read the same way with `HTTPClient` and a
# scan for the JPEG markers; WHEP wants a WebRTC extension and is the right
# answer when audio and latency matter.
#
# This file has not been run: no Godot is installed on the machine it was
# written on. It is small and reads correctly against the Godot 4.2 API, and
# the protocol side of it is the same sequence the tested clients use.

extends Control

const SNAPSHOT_EVERY := 2.0

# Only what this panel draws. The core does no work for a stream nobody asked
# for, so leaving `multiview` out of `ext` means no mosaic is ever built.
const WANTED_EVENTS := ["snapshot", "program.*", "source.*", "tally", "alert", "flush"]

var socket := WebSocketPeer.new()
var connected := false
var next_id := 1
var state := {"program": null, "sources": [], "tally": {}, "seq": 0}
var base_url := ""
var token := ""
var snapshot_timer := 0.0

@onready var url_field: LineEdit = $Layout/Connect/Url
@onready var token_field: LineEdit = $Layout/Connect/Token
@onready var status: Label = $Layout/Status
@onready var sources_box: VBoxContainer = $Layout/Body/Sources
@onready var preview: TextureRect = $Layout/Body/Preview

var http := HTTPRequest.new()


func _ready() -> void:
	add_child(http)
	http.request_completed.connect(_on_snapshot)
	$Layout/Connect/Connect.pressed.connect(_on_connect_pressed)


func _on_connect_pressed() -> void:
	base_url = url_field.text.strip_edges().rstrip("/")
	token = token_field.text.strip_edges()
	var ws_url := base_url.replace("https://", "wss://").replace("http://", "ws://") + "/rpc"
	if token != "":
		# A WebSocket cannot set a header, so the token rides in the query.
		ws_url += "?token=" + token.uri_encode()
	status.text = "connecting to %s" % ws_url
	var err := socket.connect_to_url(ws_url)
	if err != OK:
		status.text = "cannot open %s (error %d)" % [ws_url, err]


func _process(delta: float) -> void:
	socket.poll()
	var ready := socket.get_ready_state()
	if ready == WebSocketPeer.STATE_OPEN:
		if not connected:
			connected = true
			_subscribe()
		while socket.get_available_packet_count() > 0:
			_read_packet()
	elif ready == WebSocketPeer.STATE_CLOSED and connected:
		connected = false
		status.text = "the mixer closed the connection (%d)" % socket.get_close_code()

	if connected:
		snapshot_timer -= delta
		if snapshot_timer <= 0.0:
			snapshot_timer = SNAPSHOT_EVERY
			_ask_for_a_picture()


func _read_packet() -> void:
	var packet := socket.get_packet()
	if socket.was_string_packet():
		var message = JSON.parse_string(packet.get_string_from_utf8())
		if typeof(message) == TYPE_DICTIONARY:
			_handle(message)
		return
	# A binary packet is a mosaic frame: 16 byte header then JPEG. Only a panel
	# that asked for ext.multiview ever sees one, and this one does not.
	if packet.size() > 16:
		var header := packet.slice(0, 16)
		var seq := header.decode_u32(0)
		var layout := header.decode_u32(4)
		var running_ms := header.decode_u64(8)
		print("mosaic frame %d for layout %d at %d ms" % [seq, layout, running_ms])


func _handle(message: Dictionary) -> void:
	var method: String = message.get("method", "")
	if not method.begins_with("event/"):
		return
	var name := method.substr(6)
	var params: Dictionary = message.get("params", {})
	match name:
		"snapshot":
			var snapshot: Dictionary = params.get("state", {})
			state["program"] = snapshot.get("program")
			state["sources"] = snapshot.get("sources", [])
			state["seq"] = params.get("seq", 0)
		"program.took":
			state["program"] = params.get("source")
		"source.state":
			for source in state["sources"]:
				if source.get("id") == params.get("source"):
					source["state"] = params.get("state")
		"tally":
			state["tally"] = params.get("sources", {})
		"alert":
			status.text = "%s: %s" % [params.get("severity", "info"), params.get("message", "")]
		"flush":
			# Redraw here and never per event: one repaint for the whole batch.
			state["seq"] = params.get("seq", state["seq"])
			_redraw()


func _call(method: String, params: Dictionary) -> void:
	var body := {"jsonrpc": "2.0", "id": next_id, "method": method, "params": params}
	next_id += 1
	socket.send_text(JSON.stringify(body))


func _subscribe() -> void:
	# `ext` is empty: this panel wants events and pictures it fetches itself,
	# and asking for nothing is what keeps a core on a Pi quiet.
	_call("core.subscribe", {"events": WANTED_EVENTS, "ext": {}})
	status.text = "subscribed"


func _take(source_id: String) -> void:
	_call("program.take", {"source": source_id})


func _redraw() -> void:
	for child in sources_box.get_children():
		child.queue_free()
	for source in state["sources"]:
		var button := Button.new()
		var id: String = source.get("id", "")
		button.text = "%s  (%s)" % [source.get("name", id), source.get("state", "?")]
		button.modulate = _tally_colour(id)
		button.pressed.connect(_take.bind(id))
		sources_box.add_child(button)
	var on_air = state["program"] if state["program"] != null else "the slate"
	status.text = "programme: %s   seq %d" % [on_air, state["seq"]]


func _tally_colour(id: String) -> Color:
	var lamp: String = state["tally"].get(id, "off")
	if lamp == "off" and state["program"] == id:
		lamp = "program"
	match lamp:
		"program":
			return Color(1.0, 0.45, 0.45)
		"preview":
			return Color(0.5, 1.0, 0.5)
		_:
			return Color(1, 1, 1)


func _ask_for_a_picture() -> void:
	if http.get_http_client_status() != HTTPClient.STATUS_DISCONNECTED:
		return  # the last one is still in flight; skip this tick rather than queue
	var url := "%s/api/v1/snapshot/program?width=640" % base_url
	if token != "":
		url += "&token=" + token.uri_encode()
	http.request(url)


func _on_snapshot(_result: int, code: int, _headers: PackedStringArray, body: PackedByteArray) -> void:
	if code != 200 or body.is_empty():
		return
	var image := Image.new()
	if image.load_jpg_from_buffer(body) != OK:
		return
	preview.texture = ImageTexture.create_from_image(image)
