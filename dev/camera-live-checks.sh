# The checks dev/camera-live.sh runs once the core, the plugins and the fake
# ATEM are up. Sourced, not run: it uses that script's helpers.

# --- ptz -------------------------------------------------------------------

step "ptz: both cameras listed"
n="$(until_is 100 "tool ptz/camera_list | jq_ 'len(d.get(\"cameras\", []))'" 2)" && ok || bad "cameras: $n $(tool ptz/camera_list)"

step "ptz: the joystick moves the camera"
before="$(tool ptz/position '{"camera":"cam-a"}' | jq_ 'd["pan"]')"
for _ in 1 2 3 4 5; do tool ptz/move '{"camera":"cam-a","pan":0.8}' >/dev/null; sleep 0.1; done
after="$(tool ptz/position '{"camera":"cam-a"}' | jq_ 'd["pan"]')"
python3 -c "import sys; sys.exit(0 if float('$after') > float('$before') else 1)" && ok "pan $before to $after" ||
    bad "pan did not grow: $before to $after"

step "ptz: it stops by itself 300 ms after the last move"
sleep 0.6
p1="$(tool ptz/position '{"camera":"cam-a"}' | jq_ 'd["pan"]')"; sleep 0.4
p2="$(tool ptz/position '{"camera":"cam-a"}' | jq_ 'd["pan"]')"
[[ "$p1" == "$p2" ]] && ok || bad "still moving: $p1 then $p2"

step "ptz: a preset saved with a thumbnail, and recalled"
tool ptz/preset_save '{"camera":"cam-a","preset":1,"name":"Wide"}' >"$WORK/preset.json"
thumb="$(tool ptz/preset_list '{"camera":"cam-a","pictures":true}' | jq_ 'len(d["presets"][0].get("thumbnail_data") or "")')"
tool ptz/move '{"camera":"cam-a","pan":-0.8}' >/dev/null; sleep 0.3
tool ptz/preset_recall '{"camera":"cam-a","preset":1}' >/dev/null; sleep 0.5
back="$(tool ptz/position '{"camera":"cam-a"}' | jq_ 'round(d["pan"], 2)')"
want="$(python3 -c "print(round(float('$after'), 2))")"
[[ "$thumb" -gt 1000 ]] && ok "thumbnail $thumb bytes, pan back at $back" || bad "thumbnail $thumb bytes; $(cat "$WORK/preset.json")"

step "ptz: YOLOX finds the two people in cam-b"
people=""
for _ in $(seq 1 90); do
    people="$(tool ptz/track_observe '{"camera":"cam-b","wait_ms":1000}' | tee "$WORK/observe.json" | jq_ 'len(d.get("tracks", []))')"
    det="$(jq_ 'd.get("detector", {}).get("running")' <"$WORK/observe.json")"
    [[ "$det" == "yolox" && "$people" -ge 2 ]] && break
    sleep 1
done
[[ "$det" == "yolox" && "$people" -ge 2 ]] && ok "$people people, detector $det" ||
    bad "detector $det, people $people: $(head -c 600 "$WORK/observe.json")"

step "ptz: it follows one of them and steers"
tool ptz/track_follow '{"camera":"cam-b","target":"largest","framing":"waist"}' >/dev/null
st=""
for _ in $(seq 1 30); do
    st="$(tool ptz/track_state '{"camera":"cam-b"}' | tee "$WORK/state.json" | jq_ 'd["mode"] + " " + str(d.get("target"))')"
    [[ "$st" == following\ person-* ]] && break
    sleep 0.3
done
[[ "$st" == following\ person-* ]] && ok "$st" || bad "$st: $(head -c 600 "$WORK/state.json")"
tool ptz/track_stop '{"camera":"cam-b"}' >/dev/null

# --- atem ------------------------------------------------------------------

step "atem: the switcher connects"
c="$(until_is 100 "tool atem/list | jq_ 'd[\"switchers\"][0][\"connected\"]'" True)" && ok || bad "connected: $c $(tool atem/list)"

step "atem: its inputs and programme are read"
tool atem/state '{"switcher":"atem-main"}' >"$WORK/atem-state.json"
inputs="$(jq_ 'len(d["inputs"])' <"$WORK/atem-state.json")"
[[ "$inputs" -gt 2 ]] && ok "$inputs inputs" || bad "$(head -c 600 "$WORK/atem-state.json")"

atem_pgm() { tool atem/state '{"switcher":"atem-main"}' | jq_ 'd["mes"][0]["program"]'; }
atem_pvw() { tool atem/state '{"switcher":"atem-main"}' | jq_ 'd["mes"][0]["preview"]'; }

step "atem: preview, then cut"
tool atem/preview '{"switcher":"atem-main","me":0,"input":3}' >/dev/null
v="$(until_is 30 atem_pvw 3)" || bad "preview $v"
tool atem/cut '{"switcher":"atem-main","me":0}' >/dev/null
p="$(until_is 30 atem_pgm 3)" && ok || bad "programme after cut: $p"

step "atem: DSK on and fade to black"
tool atem/dsk '{"switcher":"atem-main","keyer":0,"action":"on"}' >/dev/null
d="$(until_is 30 "tool atem/state | jq_ 'd[\"dsk\"][0][\"on_air\"]'" True)" || bad "dsk on air: $d"
tool atem/ftb '{"switcher":"atem-main"}' >/dev/null
f="$(until_is 30 "tool atem/state | jq_ 'd[\"mes\"][0][\"ftb\"][\"on\"]'" True)" && ok || bad "ftb: $f"
tool atem/ftb '{"switcher":"atem-main"}' >/dev/null
tool atem/dsk '{"switcher":"atem-main","keyer":0,"action":"off"}' >/dev/null

step "atem follow: a GodwinMix take cuts the switcher"
tool atem/mode '{"switcher":"atem-main","mode":"follow"}' >/dev/null
take cam-b; p="$(until_is 50 atem_pgm 2)" || { bad "after take cam-b the switcher shows $p"; }
sleep 0.4; take cam-a; p="$(until_is 50 atem_pgm 1)" && ok "cam-b to 2, cam-a to 1" || bad "after take cam-a the switcher shows $p"

step "atem lead: a switcher cut moves GodwinMix"
tool atem/mode '{"switcher":"atem-main","mode":"lead"}' >/dev/null
sleep 0.4
tool atem/program '{"switcher":"atem-main","me":0,"input":2}' >/dev/null
g="$(until_is 50 program cam-b)" && ok || bad "GodwinMix programme is $g, wanted cam-b"

step "atem lead: no loop, the programme settles"
sleep 1.5
n="$(grep -c 'took CPgI' "$WORK/atem.log")"
[[ "$n" -lt 8 ]] && ok "$n programme commands sent to the switcher in all" || bad "$n programme commands: a loop?"
tool atem/mode '{"switcher":"atem-main","mode":"own"}' >/dev/null

step "the plugins are still healthy"
h="$(api plugins | jq_ '",".join(sorted(p["name"] + "=" + str(p.get("state")) for p in (d if isinstance(d, list) else d.get("plugins", [])) if p["name"] in ("ptz", "atem")))')"
[[ "$h" != *failed* && -n "$h" ]] && ok "$h" || bad "$h"
