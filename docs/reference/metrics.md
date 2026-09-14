# Metrics

`GET /metrics`, Prometheus text exposition format, version 0.0.4.

```
curl -s localhost:8080/metrics
```

Unauthenticated by default, because a Prometheus server scrapes with no
credentials and an operator who has to configure one will not graph anything.
An operator whose control port is reachable from somewhere they do not trust
turns that off, and then the scrape carries the bearer token like every other
call:

```
curl -s -H "Authorization: Bearer $GODWINMIX_TOKEN" localhost:8080/metrics
```

Every metric below exists from the first scrape, at zero, so a dashboard built
against a running show does not break when the box restarts.

## The programme

| Metric | Type | Labels | What it is |
|---|---|---|---|
| `gmx_programme_frames_total` | counter | | Frames leaving the programme mixer. |
| `gmx_programme_frame_interval_ms` | histogram | | Wall clock gap between two programme frames. |

Buckets: 8, 16, 20, 25, 33, 40, 50, 66, 100, 250, 1000 ms. They straddle the
periods of 25, 30, 50 and 60 frames a second, so "frames are landing late" is
visible without the dashboard knowing the canvas rate.

Measured by a pad probe on the raw video tee, which every programme frame
passes exactly once before the encoder and the multiview split apart. The probe
does two relaxed atomic adds and allocates nothing: it runs on the compositor's
streaming thread, where nothing may block.

This is the metric to put on a dashboard first. If the programme is dropping
frames, this shows it before anybody watching does.

## Sources

Labelled `instance`, which is the source id.

| Metric | Type | What it is |
|---|---|---|
| `gmx_source_buffers_total` | counter | Buffers seen from this source. |
| `gmx_source_video_behind_ms` | gauge | Milliseconds since its last video buffer. |
| `gmx_source_queue_buffers` | gauge | Buffers waiting in its queues. |
| `gmx_source_state` | gauge | 0 connecting, 1 live, 2 stalled, 3 failed. |

## Outputs

Labelled `instance`, which is the output id.

| Metric | Type | What it is |
|---|---|---|
| `gmx_output_bytes_total` | counter | Bytes written. |
| `gmx_output_reconnects_total` | counter | Times this output has reconnected. |
| `gmx_output_queue_secs` | gauge | Seconds of encoded data waiting for it. |
| `gmx_output_state` | gauge | 0 connecting, 1 live, 2 reconnecting, 3 failed. |

A `gmx_output_queue_secs` that climbs and stays high means the destination
cannot keep up. It is the earliest warning of an uplink that has gone bad, and
it moves minutes before the output actually drops.

## Control calls

| Metric | Type | Labels | What it is |
|---|---|---|---|
| `gmx_rpc_calls_total` | counter | `method`, `code` | Calls answered. |
| `gmx_rpc_duration_ms` | histogram | `method` | Time to answer one. |

Buckets: 1, 2, 5, 10, 25, 50, 100, 250, 500, 1000, 5000 ms.

`method` is the matched route, not the request path, because a path carries ids
and a label with unbounded values is how a Prometheus server runs out of
memory. `code` is the HTTP status.

## Takes

| Metric | Type | Labels | What it is |
|---|---|---|---|
| `gmx_takes_total` | counter | | Takes that landed. |
| `gmx_takes_refused_total` | counter | `reason` | Takes refused, by reason. |
| `gmx_take_ack_ms` | histogram | | From the take being asked for to it landing. |

Buckets as for `gmx_rpc_duration_ms`. `reason` is a short slug, never a
sentence, for the same label cardinality reason as `method`.

## Multiview

| Metric | Type | What it is |
|---|---|---|
| `gmx_multiview_fps` | gauge | Configured mosaic frame rate. Zero when disabled. |
| `gmx_multiview_subscribers` | gauge | Clients receiving mosaic frames. |

`gmx_multiview_subscribers` at zero with `gmx_multiview_fps` above zero is a
mosaic being encoded for nobody. That is what principle two exists to prevent,
and this is how you catch it.

## Plugins

| Metric | Type | Labels | What it is |
|---|---|---|---|
| `gmx_plugin_restarts_total` | counter | `instance` | Times an instance was rebuilt. |

`gmx_plugin_rss_bytes`, `gmx_plugin_cpu_seconds_total` and
`gmx_plugin_buffers_dropped_total` arrive with the sidecar host, when there is a
process to measure. The names are reserved so a dashboard written now keeps
working.

## When each is sampled

Nothing runs unless asked (principle two), so the numbers reach the registry by
three different routes:

* The programme frame metrics are a pad probe, always on, two atomic adds per
  frame.
* Source, output and take metrics come off the state event broadcast, which
  already exists. One task subscribes to it, folds each event into the registry
  and writes the session log. Nothing else in the mixer knows metrics exist.
* The status gauges and the mosaic subscriber count are read at scrape time. A
  mixer nobody is scraping does no work for them at all.

## Reading one without Prometheus

```
curl -s localhost:8080/metrics | grep '^gmx_source_state'
gmx_source_state{instance="cam1"} 1
gmx_source_state{instance="cam2"} 2

# The programme's frame pacing, as a histogram you can read by eye.
curl -s localhost:8080/metrics | grep '^gmx_programme_frame_interval_ms_bucket'
```

## Prometheus

```yaml
scrape_configs:
  - job_name: godwinmix
    scrape_interval: 15s
    static_configs:
      - targets: ["mixer.local:8080"]
    # Only when metrics_open is off:
    # authorization: { type: Bearer, credentials: "..." }
```

## Grafana

The three panels worth having before any others.

Programme frame pacing, 99th percentile. A line that sits near the canvas
period is healthy; a line that climbs is the encoder or something upstream of
it falling behind.

```promql
histogram_quantile(
  0.99,
  sum(rate(gmx_programme_frame_interval_ms_bucket[1m])) by (le)
)
```

Frames actually leaving, per second. Should equal the canvas rate. Anything
less is dropped frames.

```promql
rate(gmx_programme_frames_total[1m])
```

Sources not live, by name. Empty is the healthy state, which makes it a good
alert.

```promql
gmx_source_state != 1
```

An output whose buffer is filling, which is the uplink warning:

```promql
gmx_output_queue_secs > 5
```

Control calls that failed, by method:

```promql
sum(rate(gmx_rpc_calls_total{code!~"2.."}[5m])) by (method, code)
```

## Adding one

The registry is `src/observe/metrics.rs`, about three hundred lines including
its tests, with no metrics crate behind it. To add a metric, put it in the
`DEFS` table with its type, help text and buckets, then take a handle where you
need it:

```rust
let dropped = observe::metrics::counter("gmx_source_buffers_total", &[("instance", id)]);
dropped.inc();
```

Handles are `Arc`s over atomics. Take one once, outside the loop, and call it on
the hot path; the registry lock is only taken when a series is first created
and when `/metrics` is scraped.

Two rules the tests enforce. A label value must come from a fixed set, never
from user input or an id space that grows without limit. A metric that costs
anything to produce is sampled at scrape time, not on a tick.
