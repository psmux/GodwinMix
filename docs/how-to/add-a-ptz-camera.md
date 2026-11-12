# Add a PTZ camera

A pan, tilt and zoom camera on the network, moved from GodwinMix: a joystick,
zoom and focus, and presets saved as pictures so the next shot is a tap.

You need the camera's IP address and which protocol it speaks:

| The camera | Protocol |
|---|---|
| Sony, PTZOptics, Lumens, Marshall, Avonic, BirdDog, most church PTZ cameras | `visca-ip` (VISCA over IP, UDP 52381). Switch it on in the camera's network menu |
| A camera whose manual gives a TCP port for VISCA (often 5678) | `visca-tcp` |
| A camera that lists ONVIF Profile S with PTZ | `onvif`, with its user and password |
| Canon CR-N100, CR-N300, CR-N350, CR-N400, CR-N500, CR-N700, CR-X300 | `canon`, with a user that has camera control rights |

## Install the plugin

```sh
cd plugins/ptz && ./build
gmx plugin add ./plugins/ptz
```

## Add the camera

From a source tile (its menu, then **Camera**, asks for the address), an agent, or a shell. Every plugin tool goes through
the mixer's `tool.call`, as `ptz/<tool>`; from a shell that is
`POST /api/v1/tool/call`:

```sh
curl -s http://127.0.0.1:8080/api/v1/tool/call   -H "Authorization: Bearer $GODWINMIX_TOKEN" -H "Content-Type: application/json"   -d '{"name": "ptz/camera_add", "arguments": {"id": "cam-pulpit", "protocol": "visca-ip", "address": "192.0.2.20", "source": "pulpit"}}'
```

The rest of this page gives just the body of each call.

`source` is the mixer source that shows this camera's picture. Without it the
camera still moves, but presets have no pictures and tracking cannot see.

To keep the camera in the plugin's settings instead, which is what a site
that never changes wants:

```json
{
  "cameras": [
    {"id": "cam-pulpit", "protocol": "visca-ip", "address": "192.0.2.20", "source": "pulpit"},
    {"id": "cam-wide", "protocol": "canon", "address": "192.0.2.21", "user": "operator", "password": "camera-password", "source": "wide"}
  ]
}
```

Check it answers:

```json
{"name": "ptz/position", "arguments": {"camera": "cam-pulpit"}}
```

## Move it

`ptz/move` is the joystick. Each call moves the camera for 300 ms at the
speeds given, from -1 to 1; a panel sends it about ten times a second while
the stick is held, and when the calls stop the camera stops by itself. A
closed browser tab can never leave a camera turning.

```json
{"name": "ptz/move", "arguments": {"camera": "cam-pulpit", "pan": 0.3, "tilt": -0.1}}
```

`ptz/zoom`, `ptz/focus` and `ptz/exposure` do what they say;
[the reference](../reference/ptz.md) has every argument.

## Save the shots you use

Frame the shot, then:

```json
{"name": "ptz/preset_save", "arguments": {"camera": "cam-pulpit", "preset": 1, "name": "Pulpit close"}}
```

The camera stores where it points, and the plugin keeps a picture of the shot
taken from the mixer at that moment. `ptz/preset_list` with `pictures: true`
gives them back as images for a grid, and `ptz/preset_recall` goes there.

## Without a camera

`"protocol": "fake"` is a camera that exists only in memory. It moves,
remembers presets and answers where it points, which is enough to try
camera properties or an agent's script before the hardware arrives.

## When it does not move

`ptz/camera_list` shows each camera's `last_error`, which says what to check.
The usual causes: VISCA over IP switched off in the camera's menu, the wrong
port for raw VISCA, an ONVIF login refused because the camera's clock is
wrong, or a Canon user without camera control rights.

The plugin can also run by hand next to the camera, which is the quickest
way to rule the mixer out:

```sh
gmx-ptz --camera cam-pulpit:visca-ip:192.0.2.20
camera_list {}
move {"pan": 0.3}
```
