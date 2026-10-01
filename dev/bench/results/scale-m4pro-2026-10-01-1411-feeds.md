# Scale run: 200 feeds over unicast, the generator and the checker alone

| | |
|---|---|
| Machine | Apple M4 Pro, 14 cores, 24 GiB, macOS 26.6.2 |
| Version | godwinmix 0.2.0 |
| Commit | d247eae |
| Date | 2026-10-01 14:12 IST |
| Mode | feeds (unicast) |
| Load average before the run | 10.46 10.35 8.95 |
| Feeds | 200 of hd1080.ts, hd720.ts, sd.ts, mpts.ts, 60 s measured |
| Command | `dev/bench/scale.sh --mode feeds --seconds 60 --no-build --title 200 feeds over unicast, the generator and the checker alone --note No station. What the generator costs to offer 200 feeds over loopback unicast, and whether every one arrives whole.` |

No station. What the generator costs to offer 200 feeds over loopback unicast, and whether every one arrives whole.

## The numbers

| Measure | Result |
|---|---|
| Feeds generator, alone with the checker | 200 feeds, 1024.97 Mbit/s sent, 66.9% of one core, 28.1 MiB, 0 send errors, latest datagram 5.258 ms late |
| Feeds as sent (checked at the generator) | 200 of 200 streams arrived, 1025.1 Mbit/s; 0 CC errors (0 packets lost), 0 PCR jumps, PCR jitter up to 81.1 ms; 11502 keyframes, 0 GOPs dropped in 0 streams; longest silence 61.0 ms. The checker used 93.0% of one core |

## The worst streams received

| Stream | kbit/s | CC errors | Packets lost | PCR jumps | PCR gap max ms | Keyframes | GOP ms | GOPs dropped | Silence max ms |
|---|---|---|---|---|---|---|---|---|---|
| 127.0.0.1:20169 | 4000.0 | 0 | 0 | 0 | 21.4 | 58 | 1000.0 | 0 | 61.0 |
| 127.0.0.1:20171 | 6501.0 | 0 | 0 | 0 | 20.4 | 57 | 1000.0 | 0 | 61.0 |
| 127.0.0.1:20173 | 4000.0 | 0 | 0 | 0 | 21.4 | 57 | 1000.0 | 0 | 61.0 |
| 127.0.0.1:20175 | 6501.0 | 0 | 0 | 0 | 20.4 | 58 | 1000.0 | 0 | 61.0 |
| 127.0.0.1:20177 | 4000.0 | 0 | 0 | 0 | 21.4 | 57 | 1000.0 | 0 | 61.0 |
| 127.0.0.1:20179 | 6501.0 | 0 | 0 | 0 | 20.4 | 58 | 1000.0 | 0 | 61.0 |
| 127.0.0.1:20181 | 4000.0 | 0 | 0 | 0 | 21.4 | 57 | 1000.0 | 0 | 61.0 |
| 127.0.0.1:20183 | 6501.0 | 0 | 0 | 0 | 20.4 | 57 | 1000.0 | 0 | 61.0 |
| 127.0.0.1:20185 | 4000.0 | 0 | 0 | 0 | 21.4 | 58 | 1000.0 | 0 | 61.0 |
| 127.0.0.1:20187 | 6501.0 | 0 | 0 | 0 | 20.4 | 57 | 1000.0 | 0 | 61.0 |
