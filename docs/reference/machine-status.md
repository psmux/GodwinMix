# The machine: `gpu.status` and `machine.status`

Two read methods about the machine a station or a core runs on, for a
headend view. Both answer on `/rpc` and as `GET /api/v1/gpu/status` and
`GET /api/v1/machine/status`, with the read scope. A station answers them
itself; a single process core answers them too.

Nothing is read until one of them is called. The first call starts one
thread that reads every GPU and the CPU once a second and keeps two
minutes of GPU readings; it ends once neither has been called for ten
seconds, and the readings go with it. The first answer after a start
waits up to a second and a half for the first reading.

## `gpu.status {since_ms?}`

```json
{"source": "nvml", "every_ms": 1000, "gpus": [{
  "index": 0, "name": "NVIDIA GeForce RTX 3060", "driver": "550.144.03",
  "now": {"at_ms": 1792698190000, "gpu_pct": 38, "encoder_pct": 91, "decoder_pct": 44,
          "memory_used_mib": 6241, "memory_total_mib": 12288, "encoder_sessions": 51,
          "encoder_fps": 30, "temperature_c": 63, "power_w": 101.4, "power_limit_w": 170.0,
          "graphics_mhz": 1807, "memory_mhz": 7300, "video_mhz": 1612, "fan_pct": 47},
  "history": [{"at_ms": 1792698189000, "encoder_pct": 90, "...": "..."}]}]}
```

* `source` is `nvml` (the driver's `libnvidia-ml.so.1`, opened at run time
  on Linux, so nothing of NVIDIA's is linked into the binary),
  `nvidia-smi` (one `--query-gpu` a second where NVML cannot be opened,
  on any platform), or `none`.
* `note` is there when `gpus` is empty, and says why in one sentence.
* `history` is one reading a second, oldest first, at most 120. With
  `since_ms`, only the readings after it: a client that holds the rest
  asks for what is new.
* A number the driver will not give is left out of a reading rather than
  sent as zero. `encoder_sessions` counts every process's NVENC sessions
  on the card, not only GodwinMix's.

## `machine.status {}`

```json
{"host_name": "headend-1", "cores": 16, "cpu_pct": 42.5,
 "memory_total_mib": 64000, "memory_available_mib": 41000,
 "uptime_secs": 2985113, "process_uptime_secs": 9120, "load_average": [6.1, 5.8, 5.2]}
```

* `cpu_pct` is every process on the machine, percent of all cores, over
  the last second. It is left out until two readings have been taken.
* `uptime_secs` is since the machine booted (Linux and macOS);
  `process_uptime_secs` since this process started.
* `load_average` is left out where the system keeps none (Windows).

## Testing without a GPU

`crates/godwinmix/src/machine/smi.rs` parses `nvidia-smi`'s answer and is
tested against `crates/godwinmix/tests/fixtures/nvidia-smi.csv`, written
in the shape `nvidia-smi --format=csv,noheader,nounits` prints, with a
second card that has no fan or power reading. `dev/fake-nvidia-smi.py`
answers the station's query with moving numbers, for a page to draw.
The NVML path has been compiled for Linux and not yet run against a card.
