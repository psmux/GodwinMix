# Scale run: 200 unicast UDP feeds into 200 direct shows, one UDP copy output each

| | |
|---|---|
| Machine | Apple M4 Pro, 14 cores, 24 GiB, macOS 26.6.2 |
| Version | godwinmix 0.2.0 |
| Commit | a4f97fc5 |
| Date | 2026-10-01 16:08 IST |
| Mode | direct (unicast) |
| Load average before the run | 12.10 11.99 12.12 |
| Feeds | 200 of hd1080.ts, hd720.ts, sd.ts, mpts.ts, 60 s measured |
| Processes still running 15 s after SIGTERM to the station | 0 |
| Command | `dev/bench/scale.sh --feeds 200 --seconds 60 --mode direct --port 18961 --no-build --title 200 unicast UDP feeds into 200 direct shows, one UDP copy output each` |

## The numbers

| Measure | Result |
|---|---|
| Shows added | 200 of 200 with show.add_many in 0.09 s |
| Dry run plan | `{"assumed_input":"H.264 1920x1080 30 fps with stereo AAC","cost":{"cpu_millicores":0,"device_millis":0,"device_sessions":0,"egress_kbps":0,"memory_mib":0},"fits":true,"have":{"cpu_millicores":6030,"device_millis":900,"device_sessions":4294967295,"egress_kbps":4294967295,"memory_mib":4837}}` |
| Station | 0.5% of one core on average, 1.3% at peak, 70.4 MiB (1 process) |
| Direct host | 520.1% of one core on average, 808.3% at peak, 195.5 MiB (1 process) |
| Show processes | 1.6% of one core on average, 2.1% at peak, 131.2 MiB (1 process) |
| Station and everything under it | 522.2% of one core on average, 810.8% at peak, 397.1 MiB (3 processes), 174.07% and 132.4 MiB each |
| Feeds generator, alone with the checker | 200 feeds, 1024.93 Mbit/s sent, 67.3% of one core, 28.1 MiB, 0 send errors, latest datagram 8.493 ms late |
| Feeds generator, during the run | 200 feeds, 1024.17 Mbit/s sent, 118.0% of one core, 28.0 MiB, 0 send errors, latest datagram 1855.911 ms late |
| Feeds as sent (checked at the generator) | 200 of 200 streams arrived, 1025.0 Mbit/s; 0 CC errors (0 packets lost), 0 PCR jumps, PCR jitter up to 14.3 ms; 3396 keyframes, 0 GOPs dropped in 0 streams; longest silence 15.0 ms, 0 streams silent for a second or more. The checker used 109.5% of one core |
| Outputs received | 200 of 200 streams arrived, 830.3 Mbit/s; 4 CC errors (17 packets lost), 469 PCR jumps, PCR jitter up to 3514.9 ms; 11980 keyframes, 428 GOPs dropped in 142 streams; longest silence 3472.0 ms, 119 streams silent for a second or more. The checker used 76.5% of one core |
| `show.stats`, once a second | 60 reads, 3.0 ms on average, 4.9 ms at most, 201 shows |
| Shows by state at the end | {"alarm":25,"ok":176} |
| Alarms and outputs | alarms at peak {"cc-errors":2,"freeze":25,"loss":3}, outputs at the end {"live":200} |
| Input as the station counted it | 831031.0 kbit/s, 21.0 CC errors, 140.0 packets lost |

## CPU and memory by role

Percent of one core, sampled once a second. `total` is the station and everything under it.

| Role | Processes | CPU avg % | CPU peak % | RSS avg MiB | RSS peak MiB |
|---|---|---|---|---|---|
| checker | 1 | 76.5 | 112.5 | 3.3 | 3.6 |
| direct host | 1 | 520.1 | 808.3 | 195.5 | 218.5 |
| feeds | 1 | 116.3 | 177.6 | 26.2 | 27.9 |
| shows | 1 | 1.6 | 2.1 | 131.2 | 133.5 |
| station | 1 | 0.5 | 1.3 | 70.4 | 369.7 |
| total | 3 | 522.2 | 810.8 | 397.1 | 695.2 |

## The worst streams received

| Stream | kbit/s | CC errors | Packets lost | PCR jumps | PCR gap max ms | Keyframes | GOP ms | GOPs dropped | Silence max ms |
|---|---|---|---|---|---|---|---|---|---|
| 127.0.0.1:30189 | 3936.0 | 0 | 0 | 5 | 53.0 | 60 | 1000.0 | 6 | 1659.0 |
| 127.0.0.1:30193 | 3938.0 | 0 | 0 | 5 | 53.0 | 60 | 999.0 | 5 | 2064.0 |
| 127.0.0.1:30197 | 3940.0 | 0 | 0 | 6 | 53.0 | 60 | 999.0 | 5 | 2064.0 |
| 127.0.0.1:30165 | 3934.0 | 0 | 0 | 5 | 53.0 | 59 | 1000.0 | 5 | 1468.0 |
| 127.0.0.1:30119 | 1999.0 | 2 | 11 | 3 | 52.0 | 60 | 1000.0 | 4 | 2980.0 |
| 127.0.0.1:30141 | 3936.0 | 0 | 0 | 3 | 53.0 | 60 | 1000.0 | 4 | 3176.0 |
| 127.0.0.1:30135 | 1999.0 | 0 | 0 | 3 | 52.0 | 60 | 1000.0 | 4 | 3135.0 |
| 127.0.0.1:30113 | 3938.0 | 0 | 0 | 3 | 53.0 | 60 | 1000.0 | 4 | 2971.0 |
| 127.0.0.1:30111 | 2001.0 | 0 | 0 | 3 | 52.0 | 60 | 1000.0 | 4 | 2932.0 |
| 127.0.0.1:30125 | 3941.0 | 0 | 0 | 5 | 53.0 | 60 | 1000.0 | 4 | 2868.0 |
