#!/bin/sh
# {{description}}
#
# A GodwinMix source plugin in POSIX sh. It answers the core on stderr, one
# JSON object per line, and it gets the picture from gst-launch-1.0, which
# writes a streamable Matroska stream of raw I420 frames straight to stdout.
#
# stdout is media and only media. An `echo` without `>&2` anywhere in this file
# corrupts the video stream.
#
# The pipeline in start_media() is the one thing you change.

set -u

NAME="{{name}}"
VERSION="0.1.0"

# The canvas, until the core tells us what it really is.
W=1280
H=720
FPS=30

GST_PID=""
PARAMS='{}'

# --- the control channel ----------------------------------------------------

# One JSON object on one line of stderr.
send() {
    printf '%s\n' "$1" >&2
}

# level is debug, info, warn or error. The message must already be safe to put
# between two quotes: callers run it through clean() first.
log() {
    send "{\"jsonrpc\":\"2.0\",\"method\":\"log\",\"params\":{\"level\":\"$1\",\"message\":\"$2\"}}"
}

# Everything that would need escaping, removed. A shell plugin does not build
# JSON strings out of what someone else sent it.
clean() {
    printf '%s' "$1" | tr -d '\\"' | tr -c 'A-Za-z0-9 ._:/@=,+-' ' ' | cut -c1-200
}

reply() {
    if [ -n "$1" ]; then
        send "{\"jsonrpc\":\"2.0\",\"id\":$1,\"result\":$2}"
    fi
}

# --- reading the core's lines without a JSON library ------------------------
#
# sed on "key":value is enough for three integers, a method name and an
# instance id, and it is what a shell plugin really does. Anything more than
# this (arrays, nesting, strings with escapes in them) is the point where you
# stop and write the plugin in another language. templates/python and
# templates/go are the same plugin with a parser.

number() {
    printf '%s' "$2" | sed -n "s/.*\"$1\"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p"
}

word() {
    printf '%s' "$2" | sed -n "s/.*\"$1\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p"
}

# --- the picture ------------------------------------------------------------

start_media() {
    if [ -n "$GST_PID" ]; then
        return 0
    fi
    # matroskamux streamable=true writes exactly the elements the Python and Go
    # templates write by hand: an EBML header, a Segment with no size, one
    # V_UNCOMPRESSED track whose ColourSpace is I420, then Clusters of
    # SimpleBlocks. fdsink fd=1 puts them on stdout.
    #
    # Change this line and nothing else. ffmpeg does the same job:
    #   ffmpeg -f lavfi -i smptebars=size=${W}x${H}:rate=$FPS \
    #     -pix_fmt yuv420p -c:v rawvideo -f matroska pipe:1
    #
    # gst-launch's own stderr is not JSON-RPC, so it goes nowhere rather than
    # into the control channel. Drop the 2>/dev/null and add -v when a pipeline
    # will not start.
    gst-launch-1.0 -q videotestsrc pattern=smpte is-live=true \
        ! video/x-raw,format=I420,width=$W,height=$H,framerate=$FPS/1 \
        ! matroskamux streamable=true ! fdsink fd=1 </dev/null 2>/dev/null &
    GST_PID=$!
    log info "pipeline started at ${W}x${H}@${FPS}, pid $GST_PID"
}

stop_media() {
    if [ -z "$GST_PID" ]; then
        return 0
    fi
    kill "$GST_PID" 2>/dev/null
    wait "$GST_PID" 2>/dev/null
    GST_PID=""
}

trap 'stop_media; exit 0' INT TERM

# --- the methods the core calls ---------------------------------------------

on_start() {
    line=$1
    id=$2
    transport=$(word transport "$line")
    if [ -n "$transport" ] && [ "$transport" != "container" ]; then
        send "{\"jsonrpc\":\"2.0\",\"id\":${id:-null},\"error\":{\"code\":-32602,\"message\":\"this plugin only speaks the container transport. Declare transports = [container] in gmx-plugin.toml, which is the default.\",\"data\":{\"transport\":\"$(clean "$transport")\",\"retryable\":false}}}"
        return 0
    fi
    read_canvas "$line"
    start_media
    reply "$id" '{"latency_ms":0}'
}

read_canvas() {
    w=$(number width "$1")
    h=$(number height "$1")
    f=$(number fps "$1")
    if [ -n "$w" ]; then W=$w; fi
    if [ -n "$h" ]; then H=$h; fi
    if [ -n "$f" ]; then FPS=$f; fi
}

dispatch() {
    line=$1
    id=$2
    method=$3
    case $method in
        start)
            on_start "$line" "$id"
            ;;
        stop)
            stop_media
            reply "$id" '{}'
            ;;
        configure)
            # The full validated object, not a diff. The pipeline above does
            # not read it: applying a setting in a shell plugin means killing
            # gst-launch and starting it again, which is stop_media followed by
            # start_media right here.
            PARAMS=$line
            reply "$id" '{"applied":true}'
            ;;
        health)
            if [ -z "$GST_PID" ]; then
                reply "$id" '{"state":"ok","detail":"stopped, no pipeline running","latency_ms":0}'
            elif kill -0 "$GST_PID" 2>/dev/null; then
                reply "$id" "{\"state\":\"ok\",\"detail\":\"pipeline $GST_PID at ${W}x${H}@${FPS}\",\"latency_ms\":0}"
            else
                reply "$id" '{"state":"failing","detail":"gst-launch-1.0 exited. Run the pipeline by hand to see why.","latency_ms":0}'
            fi
            ;;
        shutdown)
            stop_media
            reply "$id" '{}'
            return 1
            ;;
        keyframe|initialized)
            reply "$id" '{}'
            ;;
        *)
            what=$(clean "$method")
            send "{\"jsonrpc\":\"2.0\",\"id\":${id:-null},\"error\":{\"code\":-32601,\"message\":\"this plugin has no method '$what'. It implements start, stop, configure, health and shutdown.\",\"data\":{\"method\":\"$what\",\"retryable\":false}}}"
            ;;
    esac
    return 0
}

# --- speak first, then answer until stdin ends ------------------------------

send "{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"initialize\",\"params\":{\"plugin\":\"$NAME\",\"version\":\"$VERSION\",\"api\":1,\"transports\":[\"container\"],\"provides\":[{\"kind\":\"source\",\"id\":\"source\",\"transports\":[\"container\"],\"media\":{\"video\":\"raw\",\"audio\":\"none\",\"alpha\":false,\"thumb\":true},\"capabilities\":[\"restart-in-place\",\"health\"],\"latency_ms\":0,\"settings\":\"schemas/source.json\",\"skill\":\"skills/source/SKILL.md\"}]}}"

while IFS= read -r line; do
    case $line in
        '')
            continue
            ;;
        '{'*)
            ;;
        *)
            log info "ignored a line that was not JSON: $(clean "$line")"
            continue
            ;;
    esac

    id=$(number id "$line")
    method=$(word method "$line")

    if [ -z "$method" ]; then
        # The core's answer to our initialize. Everything else with no method
        # is an answer to something we never asked, so it is dropped.
        if [ "$id" = "0" ]; then
            read_canvas "$line"
            instance=$(word instance "$line")
            log info "$NAME at ${W}x${H}@${FPS} as '$(clean "${instance:-?}")'"
            send "{\"jsonrpc\":\"2.0\",\"method\":\"initialized\",\"params\":{}}"
        fi
        continue
    fi

    if ! dispatch "$line" "$id" "$method"; then
        break
    fi
done

stop_media
exit 0
