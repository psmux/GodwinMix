// {{description}}
//
// A GodwinMix source plugin. It draws colour bars at the canvas caps and writes
// them to stdout as raw I420 frames in a streamable Matroska stream. Control is
// JSON-RPC 2.0, one object per line, on stdin and stderr.
//
// Change draw() and schemas/source.json. Nothing else here needs touching.
package main

import (
	"bufio"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"os"
	"sync"
	"time"
)

const pluginName = "{{name}}"

const pluginVersion = "0.1.0"

const (
	apiLevel  = 1
	scaleNS   = 1000000
	clusterMS = 2000
)

const transportError = "this plugin only speaks the container transport. Declare transports = [\"container\"] in gmx-plugin.toml, which is the default."

const unknownMethod = "this plugin has no method '%s'. It implements start, stop, configure, health and shutdown."

// unknownSize is an EBML size that never ends. The Segment and every Cluster
// carry it, so the stream can go straight down a pipe with nothing to seek back
// and patch.
var unknownSize = []byte{0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff}

// bars holds Y, U and V for the eight standard colour bars, white through to
// black.
var bars = [8][3]byte{
	{235, 128, 128}, {210, 16, 146}, {170, 166, 16}, {145, 54, 34},
	{106, 202, 222}, {81, 90, 240}, {41, 240, 110}, {16, 128, 128},
}

// The EBML element ids, written the way the Matroska spec prints them.
var idEBML = []byte{0x1a, 0x45, 0xdf, 0xa3}
var idDocType = []byte{0x42, 0x82}
var idDocTypeVersion = []byte{0x42, 0x87}
var idDocTypeReadVersion = []byte{0x42, 0x85}
var idSegment = []byte{0x18, 0x53, 0x80, 0x67}
var idInfo = []byte{0x15, 0x49, 0xa9, 0x66}
var idTimecodeScale = []byte{0x2a, 0xd7, 0xb1}
var idMuxingApp = []byte{0x4d, 0x80}
var idWritingApp = []byte{0x57, 0x41}
var idTracks = []byte{0x16, 0x54, 0xae, 0x6b}
var idTrackEntry = []byte{0xae}
var idTrackNumber = []byte{0xd7}
var idTrackUID = []byte{0x73, 0xc5}
var idTrackType = []byte{0x83}
var idCodecID = []byte{0x86}
var idDefaultDuration = []byte{0x23, 0xe3, 0x83}
var idVideo = []byte{0xe0}
var idPixelWidth = []byte{0xb0}
var idPixelHeight = []byte{0xba}
var idFlagInterlaced = []byte{0x9a}
var idColourSpace = []byte{0x2e, 0xb5, 0x24}
var idCluster = []byte{0x1f, 0x43, 0xb6, 0x75}
var idTimecode = []byte{0xe7}
var idSimpleBlock = []byte{0xa3}

// media is stdout and carries nothing but the Matroska stream. ctrl is stderr
// and carries nothing but JSON-RPC, one object per line.
var media = bufio.NewWriterSize(os.Stdout, 65536)

var ctrl = bufio.NewWriter(os.Stderr)

var ctrlMu sync.Mutex

// mu guards the three things the media goroutine and the reader goroutine both
// touch. Everything below it belongs to the reader alone.
var mu sync.Mutex

var settings = map[string]any{}

var frames int64

var failed bool

var current canvas

var header bool

var clusterBase int64

var haveCluster bool

var stopCh chan struct{}

var mediaLoop sync.WaitGroup

// canvas is the size and rate the core told us to run at.
type canvas struct {
	width  int
	height int
	fps    int
}

// --- what you change --------------------------------------------------------

// draw returns one I420 frame of exactly the right size for the canvas.
//
// An I420 frame is a Y plane of width*height bytes, then a U plane and a V
// plane of ceil(width/2)*ceil(height/2) each. Y is brightness, 16 is black and
// 235 is white; U and V are colour, 128 each is grey.
//
// This draws eight colour bars and ignores the time. Your plugin will not.
func draw(c canvas, params map[string]any, ptsNS int64) []byte {
	count := 8
	if asked, ok := params["bars"].(float64); ok {
		count = int(asked)
	}
	if count < 1 {
		count = 1
	}
	if count > 8 {
		count = 8
	}
	cw := (c.width + 1) / 2
	ch := (c.height + 1) / 2
	luma := make([]byte, c.width)
	for x := 0; x < c.width; x++ {
		luma[x] = barAt(x, c.width, count)[0]
	}
	blue := make([]byte, cw)
	red := make([]byte, cw)
	for x := 0; x < cw; x++ {
		bar := barAt(x*2, c.width, count)
		blue[x] = bar[1]
		red[x] = bar[2]
	}
	size := c.width*c.height + 2*cw*ch
	frame := make([]byte, 0, size)
	for y := 0; y < c.height; y++ {
		frame = append(frame, luma...)
	}
	for y := 0; y < ch; y++ {
		frame = append(frame, blue...)
	}
	for y := 0; y < ch; y++ {
		frame = append(frame, red...)
	}
	return frame
}

// barAt picks the bar a column falls in.
func barAt(x, width, count int) [3]byte {
	index := x * count / width
	if index > count-1 {
		index = count - 1
	}
	return bars[index]
}

// --- the control channel ----------------------------------------------------

// send puts one JSON object on one line of stderr. stdout is media and only
// media.
func send(msg map[string]any) {
	data, err := json.Marshal(msg)
	if err != nil {
		return
	}
	ctrlMu.Lock()
	defer ctrlMu.Unlock()
	ctrl.Write(data)
	ctrl.WriteByte('\n')
	ctrl.Flush()
}

// logLine sends one log notification. level is debug, info, warn or error.
func logLine(level string, message string) {
	body := map[string]any{"level": level, "message": message}
	send(map[string]any{"jsonrpc": "2.0", "method": "log", "params": body})
}

// reply answers one call. A notification has no id and gets no answer.
func reply(rid any, result map[string]any) {
	if rid == nil {
		return
	}
	send(map[string]any{"jsonrpc": "2.0", "id": rid, "result": result})
}

// failCall answers one call with a JSON-RPC error.
func failCall(rid any, code int, message string, data map[string]any) {
	body := map[string]any{"code": code, "message": message, "data": data}
	send(map[string]any{"jsonrpc": "2.0", "id": rid, "error": body})
}

// --- the container transport: streamable Matroska on stdout -----------------

// vint returns a data size as an EBML variable length integer, in the shortest
// form that holds it.
func vint(value int64) []byte {
	for length := 1; length <= 8; length++ {
		shift := uint(7 * length)
		limit := int64(1) << shift
		if value < limit-1 {
			marked := value | limit
			out := make([]byte, 8)
			binary.BigEndian.PutUint64(out, uint64(marked))
			return out[8-length:]
		}
	}
	panic("a size no Matroska stream ever has")
}

// ebmlUint returns an unsigned integer with its leading zero bytes removed.
func ebmlUint(value int64) []byte {
	out := make([]byte, 8)
	binary.BigEndian.PutUint64(out, uint64(value))
	start := 0
	for start < 7 && out[start] == 0 {
		start++
	}
	return out[start:]
}

// zstr is a Matroska string: the bytes, then the zero byte that ends them.
func zstr(text string) []byte {
	return append([]byte(text), 0)
}

// join sticks byte slices together, which is how every element is built.
func join(parts ...[]byte) []byte {
	out := []byte{}
	for _, part := range parts {
		out = append(out, part...)
	}
	return out
}

// elem returns one EBML element: its id, then its size, then its payload.
func elem(id []byte, payload []byte) []byte {
	return join(id, vint(int64(len(payload))), payload)
}

// writeAll puts the parts on stdout in order, without copying them into one
// buffer first. A 1080p frame is 3 MB and does not need copying twice.
func writeAll(parts ...[]byte) error {
	for _, part := range parts {
		if _, err := media.Write(part); err != nil {
			return err
		}
	}
	return nil
}

// writeHeader puts an EBML header, an open ended Segment, Info and one raw
// video track on stdout.
func writeHeader(c canvas) error {
	step := int64(1000000000) / int64(c.fps)
	ebml := join(
		elem(idDocType, zstr("matroska")),
		elem(idDocTypeVersion, ebmlUint(4)),
		elem(idDocTypeReadVersion, ebmlUint(2)))
	info := join(
		elem(idTimecodeScale, ebmlUint(scaleNS)),
		elem(idMuxingApp, zstr(pluginName)),
		elem(idWritingApp, zstr(pluginName)))
	video := join(
		elem(idPixelWidth, ebmlUint(int64(c.width))),
		elem(idPixelHeight, ebmlUint(int64(c.height))),
		elem(idFlagInterlaced, ebmlUint(2)),
		elem(idColourSpace, []byte("I420")))
	track := join(
		elem(idTrackNumber, ebmlUint(1)),
		elem(idTrackUID, ebmlUint(1)),
		elem(idTrackType, ebmlUint(1)),
		elem(idCodecID, zstr("V_UNCOMPRESSED")),
		elem(idDefaultDuration, ebmlUint(step)),
		elem(idVideo, video))
	err := writeAll(
		elem(idEBML, ebml),
		idSegment,
		unknownSize,
		elem(idInfo, info),
		elem(idTracks, elem(idTrackEntry, track)))
	if err != nil {
		return err
	}
	return media.Flush()
}

// writeFrame writes one SimpleBlock, opening a Cluster when there is none or
// when the last one is full.
func writeFrame(ptsNS int64, data []byte) error {
	ms := ptsNS / scaleNS
	span := ms - clusterBase
	if !haveCluster || span >= clusterMS {
		err := writeAll(idCluster, unknownSize, elem(idTimecode, ebmlUint(ms)))
		if err != nil {
			return err
		}
		clusterBase = ms
		haveCluster = true
	}
	rel := ms - clusterBase
	block := make([]byte, 4)
	block[0] = 0x81
	binary.BigEndian.PutUint16(block[1:], uint16(int16(rel)))
	block[3] = 0x80
	size := len(block) + len(data)
	if err := writeAll(idSimpleBlock, vint(int64(size)), block, data); err != nil {
		return err
	}
	return media.Flush()
}

// produce writes frames at the canvas fps, with PTS on the plugin's own clock
// starting at zero.
//
// The deadline comes from the frame index, not from adding a sleep each time,
// so one slow draw does not push every later frame back.
func produce(c canvas, stop chan struct{}) {
	defer mediaLoop.Done()
	step := int64(1000000000) / int64(c.fps)
	cw := (c.width + 1) / 2
	ch := (c.height + 1) / 2
	expected := c.width*c.height + 2*cw*ch
	started := time.Now()
	index := int64(0)
	// A restart puts PTS back to zero, so the next frame opens a new Cluster
	// rather than landing before the base of the old one.
	haveCluster = false
	for {
		ptsNS := index * step
		due := time.Duration(ptsNS)
		wait := due - time.Since(started)
		if wait > 0 {
			timer := time.NewTimer(wait)
			select {
			case <-stop:
				timer.Stop()
				return
			case <-timer.C:
			}
		} else {
			select {
			case <-stop:
				return
			default:
			}
		}
		frame := draw(c, currentSettings(), ptsNS)
		if len(frame) != expected {
			markFailed()
			logLine("error", fmt.Sprintf("draw() returned %d bytes, the canvas needs %d", len(frame), expected))
			return
		}
		if err := writeFrame(ptsNS, frame); err != nil {
			markFailed()
			logLine("error", fmt.Sprintf("the media pipe closed: %v", err))
			return
		}
		countFrame()
		index++
	}
}

// --- the three things both goroutines touch ---------------------------------

// currentSettings hands the media goroutine the object configure last stored.
func currentSettings() map[string]any {
	mu.Lock()
	defer mu.Unlock()
	return settings
}

func markFailed() {
	mu.Lock()
	defer mu.Unlock()
	failed = true
}

func countFrame() {
	mu.Lock()
	defer mu.Unlock()
	frames++
}

// --- the methods the core calls ---------------------------------------------

// canvasOf reads {"width":..,"height":..,"fps":..} out of what the core sent.
func canvasOf(value any) (canvas, bool) {
	obj, ok := value.(map[string]any)
	if !ok {
		return canvas{}, false
	}
	w, wok := obj["width"].(float64)
	h, hok := obj["height"].(float64)
	f, fok := obj["fps"].(float64)
	if !wok || !hok || !fok || w < 1 || h < 1 || f < 1 {
		return canvas{}, false
	}
	return canvas{width: int(w), height: int(h), fps: int(f)}, true
}

func onStart(rid any, args map[string]any) {
	if t, ok := args["transport"].(string); ok && t != "" && t != "container" {
		data := map[string]any{"transport": t, "retryable": false}
		failCall(rid, -32602, transportError, data)
		return
	}
	if c, ok := canvasOf(args["canvas"]); ok {
		current = c
	}
	if current.fps < 1 {
		data := map[string]any{"retryable": false}
		failCall(rid, -32602, "start arrived with no canvas, and the answer to initialize carried none either.", data)
		return
	}
	stopMedia()
	if !header {
		if err := writeHeader(current); err != nil {
			markFailed()
			data := map[string]any{"retryable": false}
			failCall(rid, -32603, fmt.Sprintf("the media pipe closed before the header went out: %v", err), data)
			return
		}
		header = true
	}
	stopCh = make(chan struct{})
	mediaLoop.Add(1)
	go produce(current, stopCh)
	reply(rid, map[string]any{"latency_ms": 0})
}

// stopMedia ends the media goroutine and waits for it to let go of stdout.
func stopMedia() {
	if stopCh == nil {
		return
	}
	close(stopCh)
	mediaLoop.Wait()
	stopCh = nil
}

// dispatch answers one call. It returns false when the process should exit.
func dispatch(msg map[string]any) bool {
	rid := msg["id"]
	method, _ := msg["method"].(string)
	args, _ := msg["params"].(map[string]any)
	if args == nil {
		args = map[string]any{}
	}
	switch method {
	case "start":
		onStart(rid, args)
	case "stop":
		stopMedia()
		reply(rid, map[string]any{})
	case "configure":
		// The full validated object, not a diff. draw() reads it next frame.
		next := args
		if inner, ok := args["params"].(map[string]any); ok {
			next = inner
		}
		mu.Lock()
		settings = next
		mu.Unlock()
		reply(rid, map[string]any{"applied": true})
	case "health":
		// Answered here, on the reader goroutine. It never waits on the media
		// goroutine for anything but a mutex held for nanoseconds.
		mu.Lock()
		state := "ok"
		if failed {
			state = "failing"
		}
		detail := fmt.Sprintf("%d frames sent", frames)
		mu.Unlock()
		reply(rid, map[string]any{"state": state, "detail": detail, "latency_ms": 0})
	case "shutdown":
		stopMedia()
		reply(rid, map[string]any{})
		return false
	case "keyframe", "initialized":
		reply(rid, map[string]any{})
	default:
		data := map[string]any{"method": method, "retryable": false}
		failCall(rid, -32601, fmt.Sprintf(unknownMethod, method), data)
	}
	return true
}

// isReady tells the core's answer to our initialize from every other line.
func isReady(msg map[string]any) bool {
	id, ok := msg["id"].(float64)
	if !ok || id != 0 {
		return false
	}
	_, ok = msg["result"].(map[string]any)
	return ok
}

// onReady stores the canvas the core chose, then says the plugin is ready.
func onReady(msg map[string]any) {
	result, _ := msg["result"].(map[string]any)
	if c, ok := canvasOf(result["canvas"]); ok {
		current = c
	}
	if p, ok := result["params"].(map[string]any); ok {
		mu.Lock()
		settings = p
		mu.Unlock()
	}
	instance, _ := result["instance"].(string)
	if instance == "" {
		instance = "?"
	}
	logLine("info", fmt.Sprintf("%s at %dx%d@%d as '%s'", pluginName, current.width, current.height, current.fps, instance))
	send(map[string]any{"jsonrpc": "2.0", "method": "initialized", "params": map[string]any{}})
}

// shorten keeps a log line short enough to read.
func shorten(text string) string {
	if len(text) > 200 {
		return text[:200]
	}
	return text
}

// --- speak first, then answer until stdin ends ------------------------------

func main() {
	kinds := map[string]any{"video": "raw", "audio": "none", "alpha": false, "thumb": true}
	provide := map[string]any{"kind": "source", "id": "source"}
	provide["transports"] = []string{"container"}
	provide["media"] = kinds
	provide["capabilities"] = []string{"restart-in-place", "health"}
	provide["latency_ms"] = 0
	provide["settings"] = "schemas/source.json"
	provide["skill"] = "skills/source/SKILL.md"
	hello := map[string]any{"plugin": pluginName, "version": pluginVersion}
	hello["api"] = apiLevel
	hello["transports"] = []string{"container"}
	hello["provides"] = []any{provide}
	send(map[string]any{"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": hello})

	lines := bufio.NewScanner(os.Stdin)
	lines.Buffer(make([]byte, 0, 65536), 8388608)
	for lines.Scan() {
		line := lines.Bytes()
		if len(line) == 0 {
			continue
		}
		var msg map[string]any
		if err := json.Unmarshal(line, &msg); err != nil {
			logLine("info", fmt.Sprintf("ignored a line that was not JSON: %s", shorten(string(line))))
			continue
		}
		if isReady(msg) {
			onReady(msg)
			continue
		}
		if _, ok := msg["method"]; !ok {
			continue
		}
		if !dispatch(msg) {
			break
		}
	}
	stopMedia()
	media.Flush()
	ctrl.Flush()
}
