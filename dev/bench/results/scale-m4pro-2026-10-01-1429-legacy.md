# Scale run: 200 feeds offered, 8 of them into compositing shows (the baseline before wave 4)

| | |
|---|---|
| Machine | Apple M4 Pro, 14 cores, 24 GiB, macOS 26.6.2 |
| Version | godwinmix 0.2.0 |
| Commit | d1afd5f |
| Date | 2026-10-01 14:31 IST |
| Mode | legacy (unicast) |
| Load average before the run | 18.08 17.52 15.33 |
| Feeds | 200 of hd1080.ts, hd720.ts, sd.ts, mpts.ts, 60 s measured |
| Processes still running 15 s after SIGTERM to the station | 24 |
| Command | `dev/bench/scale.sh --mode legacy --legacy-shows 8 --seconds 60 --no-build --title 200 feeds offered, 8 of them into compositing shows (the baseline before wave 4) --note Each show is its own process with a udp/source, a programme encode and a udp/output. Direct shows, show.add_many and show.stats do not exist on this commit, so the direct host row is empty and show.list stands in for show.stats. Other agents were compiling and running benchmarks on this machine throughout, as the load average says, so read the CPU numbers as an upper bound.` |

Each show is its own process with a udp/source, a programme encode and a udp/output. Direct shows, show.add_many and show.stats do not exist on this commit, so the direct host row is empty and show.list stands in for show.stats. Other agents were compiling and running benchmarks on this machine throughout, as the load average says, so read the CPU numbers as an upper bound.

## The numbers

| Measure | Result |
|---|---|
| Shows added | 8 of 8 with show.add, source.add, program.take, output.add in 4.92 s |
| Station | 0.4% of one core on average, 1.3% at peak, 40.2 MiB (1 process) |
| Direct host | no process of this kind ran |
| Show processes | 283.7% of one core on average, 491.4% at peak, 3321.9 MiB (9 processes), 31.52% and 369.1 MiB each |
| Plugin `gmx-udp` | 14.3% of one core on average, 26.3% at peak, 196.1 MiB (16 processes), 0.89% and 12.3 MiB each |
| Station and everything under it | 298.4% of one core on average, 514.2% at peak, 3558.3 MiB (26 processes), 11.48% and 136.9 MiB each |
| Feeds generator, alone with the checker | 200 feeds, 878.89 Mbit/s sent, 69.3% of one core, 28.1 MiB, 0 send errors, latest datagram 5.208 ms late |
| Feeds generator, during the run | 200 feeds, 920.86 Mbit/s sent, 78.8% of one core, 28.0 MiB, 0 send errors, latest datagram 5.236 ms late |
| Feeds as sent (checked at the generator) | 200 of 200 streams arrived, 878.5 Mbit/s; 0 CC errors (0 packets lost), 0 PCR jumps, PCR jitter up to 5067.4 ms; 2909 keyframes, 297 GOPs dropped in 166 streams; longest silence 3279.0 ms, 200 streams silent for a second or more. The checker used 50.1% of one core |
| Outputs received | 8 of 8 streams arrived, 30.7 Mbit/s; 0 CC errors (0 packets lost), 0 PCR jumps, PCR jitter up to 2053.6 ms; 144 keyframes, 88 GOPs dropped in 8 streams; longest silence 32763.0 ms, 8 streams silent for a second or more. The checker used 1.7% of one core |
| `show.list`, once a second | 60 reads, 119.3 ms on average, 463.0 ms at most, 9 shows |
| Shows by state at the end | {"running":9} |

## CPU and memory by role

Percent of one core, sampled once a second. `total` is the station and everything under it.

| Role | Processes | CPU avg % | CPU peak % | RSS avg MiB | RSS peak MiB |
|---|---|---|---|---|---|
| checker | 1 | 1.7 | 3.8 | 1.3 | 1.5 |
| feeds | 1 | 87.6 | 172.2 | 24.5 | 26.2 |
| plugin gmx-udp | 16 | 14.3 | 26.3 | 196.1 | 212.9 |
| shows | 9 | 283.7 | 491.4 | 3321.9 | 3826.7 |
| station | 1 | 0.4 | 1.3 | 40.2 | 41.7 |
| total | 26 | 298.4 | 514.2 | 3558.3 | 4070.5 |

## The worst streams received

| Stream | kbit/s | CC errors | Packets lost | PCR jumps | PCR gap max ms | Keyframes | GOP ms | GOPs dropped | Silence max ms |
|---|---|---|---|---|---|---|---|---|---|
| 127.0.0.1:30005 | 2842.0 | 0 | 0 | 0 | 67.0 | 13 | 2000.0 | 16 | 32763.0 |
| 127.0.0.1:30007 | 3088.0 | 0 | 0 | 0 | 67.0 | 15 | 2000.0 | 14 | 29914.0 |
| 127.0.0.1:30002 | 3324.0 | 0 | 0 | 0 | 67.0 | 15 | 2000.0 | 14 | 28027.0 |
| 127.0.0.1:30006 | 3622.0 | 0 | 0 | 0 | 67.0 | 17 | 2000.0 | 12 | 25174.0 |
| 127.0.0.1:30004 | 4122.0 | 0 | 0 | 0 | 67.0 | 20 | 2000.0 | 9 | 20369.0 |
| 127.0.0.1:30001 | 4328.0 | 0 | 0 | 0 | 67.0 | 21 | 2000.0 | 8 | 17701.0 |
| 127.0.0.1:30003 | 4581.0 | 0 | 0 | 0 | 67.0 | 21 | 2000.0 | 8 | 15116.0 |
| 127.0.0.1:30000 | 4822.0 | 0 | 0 | 0 | 67.0 | 22 | 2000.0 | 7 | 12867.0 |
