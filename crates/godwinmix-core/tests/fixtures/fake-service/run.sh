#!/bin/sh
# A GodwinMix service and device in shell. Control on stdin and stderr; no
# media at all, which is what makes it a service.
#
# What it does, in order:
#   1. says hello and waits for the core's answer
#   2. if it was configured with `announce`, raises source.appeared for it, so
#      the supervisor has to turn it into a live source
#   3. answers `discover` with one candidate
#   4. answers `tool.call` for the one tool it declares
#   5. answers `source.gone` on demand, through the same tool
say() { printf '%s\n' "$1" >&2; }

say '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"plugin":"fakeservice","version":"0.1.0","api":1,"transports":[],"provides":[]}}'

# The core answers on stdin with the handshake, which carries the params.
read -r ready
say '{"jsonrpc":"2.0","method":"initialized"}'
say '{"jsonrpc":"2.0","method":"log","params":{"level":"info","message":"fakeservice is up"}}'

# `announce` in the params means: pretend something turned up on the network.
announce=$(printf '%s' "$ready" | sed -n 's/.*"announce":"\([^"]*\)".*/\1/p')
if [ -n "$announce" ]; then
  say "{\"jsonrpc\":\"2.0\",\"method\":\"event\",\"params\":{\"name\":\"source.appeared\",\"params\":{\"id\":\"$announce\",\"type\":\"test/source\",\"name\":\"$announce\",\"params\":{\"uri\":\"test://smpte\"},\"confidence\":1.0}}}"
fi

while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"health"'*)
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"state\":\"ok\"}}"
      ;;
    *'"method":"configure"'*)
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"applied\":true}}"
      ;;
    *'"method":"discover"'*)
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"candidates\":[{\"type\":\"test/source\",\"name\":\"Fake Camera 1\",\"params\":{\"uri\":\"test://ball\"},\"confidence\":0.9}]}}"
      ;;
    *'"method":"tool.call"'*)
      # `take_it_away` says the thing it announced has gone; anything else is
      # the echo the manifest declares.
      case "$line" in
        *take_it_away*)
          say "{\"jsonrpc\":\"2.0\",\"method\":\"event\",\"params\":{\"name\":\"source.gone\",\"params\":{\"id\":\"$announce\"}}}"
          ;;
      esac
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"echo\"}],\"isError\":false}}"
      ;;
    *'"method":"stop"'*)
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{}}"
      ;;
    *'"method":"shutdown"'*)
      exit 0
      ;;
  esac
done
