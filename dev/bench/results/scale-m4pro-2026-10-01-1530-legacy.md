# Scale run: 200 feeds offered, 8 of them into compositing shows, after the output fixes

| | |
|---|---|
| Machine | Apple M4 Pro, 14 cores, 24 GiB, macOS 26.6.2 |
| Version | godwinmix 0.2.0 |
| Commit | 218f60c7 |
| Date | 2026-10-01 15:32 IST |
| Mode | legacy (unicast) |
| Load average before the run | 8.19 9.28 10.17 |
| Feeds | 200 of hd1080.ts, hd720.ts, sd.ts, mpts.ts, 60 s measured |
| Processes still running 15 s after SIGTERM to the station | 0 |
| Command | `dev/bench/scale.sh --mode legacy --legacy-shows 8 --seconds 60 --no-build --port 18931 --keep --machine m4pro --title 200 feeds offered, 8 of them into compositing shows, after the output fixes --note The same run as the 14:29 baseline, on the commit that fixes outputs stopping: the encoder's audio starts at its first buffer, the udp sender holds ten seconds of one stream while it waits for the other, a reconnect waits a bounded time for its old pipeline and restarts a sidecar output that stopped reading, and an output is live only while bytes reach its sink. Other agents were working on this machine throughout, as the load average says.` |

The same run as the 14:29 baseline, on the commit that fixes outputs stopping: the encoder's audio starts at its first buffer, the udp sender holds ten seconds of one stream while it waits for the other, a reconnect waits a bounded time for its old pipeline and restarts a sidecar output that stopped reading, and an output is live only while bytes reach its sink. Other agents were working on this machine throughout, as the load average says.

## The numbers

| Measure | Result |
|---|---|
| Shows added | 8 of 8 with show.add, source.add, program.take, output.add in 4.08 s |
| Station | 0.2% of one core on average, 1.0% at peak, 92.1 MiB (1 process) |
| Direct host | no process of this kind ran |
| Show processes | 128.7% of one core on average, 149.2% at peak, 3698.9 MiB (9 processes), 14.30% and 411.0 MiB each |
| Plugin `gmx-udp` | 21.8% of one core on average, 28.7% at peak, 228.6 MiB (16 processes), 1.36% and 14.3 MiB each |
| Station and everything under it | 150.6% of one core on average, 177.1% at peak, 4019.6 MiB (26 processes), 5.79% and 154.6 MiB each |
| Feeds generator, alone with the checker | 200 feeds, 1024.94 Mbit/s sent, 55.5% of one core, 28.1 MiB, 0 send errors, latest datagram 6.55 ms late |
| Feeds generator, during the run | 200 feeds, 1024.57 Mbit/s sent, 70.4% of one core, 28.0 MiB, 0 send errors, latest datagram 74.686 ms late |
| Feeds as sent (checked at the generator) | 200 of 200 streams arrived, 1025.0 Mbit/s; 0 CC errors (0 packets lost), 0 PCR jumps, PCR jitter up to 11.7 ms; 3394 keyframes, 0 GOPs dropped in 0 streams; longest silence 12.0 ms, 0 streams silent for a second or more. The checker used 91.3% of one core |
| Outputs received | 8 of 8 streams arrived, 48.8 Mbit/s; 0 CC errors (0 packets lost), 0 PCR jumps, PCR jitter up to 2917.5 ms; 227 keyframes, 5 GOPs dropped in 5 streams; longest silence 172.0 ms, 0 streams silent for a second or more. The checker used 2.1% of one core |
| `show.list`, once a second | 60 reads, 44.3 ms on average, 115.1 ms at most, 9 shows |
| Shows by state at the end | {"running":9} |

## CPU and memory by role

Percent of one core, sampled once a second. `total` is the station and everything under it.

| Role | Processes | CPU avg % | CPU peak % | RSS avg MiB | RSS peak MiB |
|---|---|---|---|---|---|
| checker | 1 | 2.1 | 3.0 | 1.8 | 2.0 |
| feeds | 1 | 70.3 | 86.7 | 26.3 | 26.5 |
| plugin gmx-udp | 16 | 21.8 | 28.7 | 228.6 | 234.1 |
| shows | 9 | 128.7 | 149.2 | 3698.9 | 4769.9 |
| station | 1 | 0.2 | 1.0 | 92.1 | 146.7 |
| total | 26 | 150.6 | 177.1 | 4019.6 | 5036.5 |

## The worst streams received

| Stream | kbit/s | CC errors | Packets lost | PCR jumps | PCR gap max ms | Keyframes | GOP ms | GOPs dropped | Silence max ms |
|---|---|---|---|---|---|---|---|---|---|
| 127.0.0.1:30006 | 6098.0 | 0 | 0 | 0 | 67.0 | 28 | 2000.0 | 1 | 155.0 |
| 127.0.0.1:30002 | 6109.0 | 0 | 0 | 0 | 67.0 | 28 | 2000.0 | 1 | 149.0 |
| 127.0.0.1:30003 | 6094.0 | 0 | 0 | 0 | 67.0 | 28 | 2000.0 | 1 | 144.0 |
| 127.0.0.1:30000 | 6091.0 | 0 | 0 | 0 | 67.0 | 28 | 2000.0 | 1 | 141.0 |
| 127.0.0.1:30004 | 6095.0 | 0 | 0 | 0 | 67.0 | 28 | 2000.0 | 1 | 141.0 |
| 127.0.0.1:30007 | 6096.0 | 0 | 0 | 0 | 67.0 | 29 | 2000.0 | 0 | 172.0 |
| 127.0.0.1:30005 | 6086.0 | 0 | 0 | 0 | 67.0 | 29 | 2000.0 | 0 | 144.0 |
| 127.0.0.1:30001 | 6095.0 | 0 | 0 | 0 | 67.0 | 29 | 2000.0 | 0 | 134.0 |
