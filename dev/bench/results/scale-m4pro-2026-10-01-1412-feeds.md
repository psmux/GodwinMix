# Scale run: 200 feeds over multicast on loopback, the generator and the checker alone

| | |
|---|---|
| Machine | Apple M4 Pro, 14 cores, 24 GiB, macOS 26.6.2 |
| Version | godwinmix 0.2.0 |
| Commit | d247eae |
| Date | 2026-10-01 14:13 IST |
| Mode | feeds (multicast) |
| Load average before the run | 13.06 11.10 9.33 |
| Feeds | 200 of hd1080.ts, hd720.ts, sd.ts, mpts.ts, 60 s measured |
| Command | `dev/bench/scale.sh --mode feeds --transport multicast --seconds 60 --no-build --title 200 feeds over multicast on loopback, the generator and the checker alone --note No station. The same feeds on 200 multicast groups over lo0, to show what macOS loopback multicast does at this rate.` |

No station. The same feeds on 200 multicast groups over lo0, to show what macOS loopback multicast does at this rate.

## The numbers

| Measure | Result |
|---|---|
| Feeds generator, alone with the checker | 200 feeds, 1024.71 Mbit/s sent, 148.9% of one core, 28.1 MiB, 0 send errors, latest datagram 5.262 ms late |
| Feeds as sent (checked at the generator) | 200 of 200 streams arrived, 640.0 Mbit/s; 164570 CC errors (928470 packets lost), 6306 PCR jumps, PCR jitter up to 655.4 ms; 7345 keyframes, 3809 GOPs dropped in 200 streams; longest silence 1129.0 ms. The checker used 95.7% of one core |

## The worst streams received

| Stream | kbit/s | CC errors | Packets lost | PCR jumps | PCR gap max ms | Keyframes | GOP ms | GOPs dropped | Silence max ms |
|---|---|---|---|---|---|---|---|---|---|
| 239.77.0.126:5000 | 2507.0 | 702 | 3715 | 35 | 99.6 | 30 | 1019.0 | 28 | 654.0 |
| 239.77.0.70:5000 | 2470.0 | 696 | 3670 | 38 | 99.6 | 30 | 1019.0 | 28 | 1117.0 |
| 239.77.0.46:5000 | 2494.0 | 695 | 3874 | 39 | 99.6 | 30 | 1000.0 | 27 | 531.0 |
| 239.77.0.182:5000 | 2504.0 | 703 | 3944 | 34 | 99.6 | 32 | 1019.0 | 26 | 801.0 |
| 239.77.0.81:5000 | 4943.0 | 653 | 3629 | 32 | 99.8 | 32 | 1000.0 | 26 | 695.0 |
| 239.77.0.17:5000 | 5075.0 | 641 | 3507 | 30 | 99.8 | 32 | 1000.0 | 26 | 305.0 |
| 239.77.0.84:5000 | 3995.0 | 1299 | 7600 | 22 | 100.0 | 33 | 1000.0 | 25 | 649.0 |
| 239.77.0.120:5000 | 4050.0 | 1268 | 7714 | 19 | 100.0 | 33 | 1000.0 | 25 | 657.0 |
| 239.77.0.140:5000 | 4042.0 | 1261 | 7389 | 19 | 100.0 | 33 | 1018.0 | 25 | 646.0 |
| 239.77.0.94:5000 | 2482.0 | 695 | 3769 | 44 | 99.6 | 32 | 1019.0 | 25 | 631.0 |
