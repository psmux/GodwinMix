# Scale run: 200 unicast UDP feeds into 200 direct shows, one UDP copy output each

| | |
|---|---|
| Machine | Apple M4 Pro, 14 cores, 24 GiB, macOS 26.6.2 |
| Version | godwinmix 0.2.0 |
| Commit | 7c5b39d9 |
| Date | 2026-10-01 16:20 IST |
| Mode | direct (unicast) |
| Load average before the run | 7.76 10.04 11.46 |
| Feeds | 200 of hd1080.ts, hd720.ts, sd.ts, mpts.ts, 60 s measured |
| Processes still running 15 s after SIGTERM to the station | 0 |
| Command | `dev/bench/scale.sh --feeds 200 --seconds 60 --mode direct --port 18961 --title 200 unicast UDP feeds into 200 direct shows, one UDP copy output each --note One show per feed with compositing off, made with one show.add_many, each copied unchanged to its own UDP receiver; alarms on (the default for direct shows), no thumbnails asked for and the wall closed. The ingest plugin was installed through the API while the station ran, into the run's own plugins_dir. Other agents were compiling on this machine during the run, as the load average says.` |

One show per feed with compositing off, made with one show.add_many, each copied unchanged to its own UDP receiver; alarms on (the default for direct shows), no thumbnails asked for and the wall closed. The ingest plugin was installed through the API while the station ran, into the run's own plugins_dir. Other agents were compiling on this machine during the run, as the load average says.

## The numbers

| Measure | Result |
|---|---|
| Shows added | 200 of 200 with show.add_many in 0.28 s |
| Dry run plan | `{"assumed_input":"H.264 1920x1080 30 fps with stereo AAC","cost":{"cpu_millicores":0,"device_millis":0,"device_sessions":0,"egress_kbps":0,"memory_mib":0},"fits":true,"have":{"cpu_millicores":8531,"device_millis":900,"device_sessions":4294967295,"egress_kbps":4294967295,"memory_mib":8288}}` |
| Station | 0.4% of one core on average, 1.0% at peak, 629.3 MiB (1 process) |
| Direct host | 517.5% of one core on average, 539.5% at peak, 205.5 MiB (1 process) |
| Show processes | 1.3% of one core on average, 2.0% at peak, 145.1 MiB (1 process) |
| Station and everything under it | 519.1% of one core on average, 540.5% at peak, 979.9 MiB (3 processes), 173.03% and 326.6 MiB each |
| Feeds generator, alone with the checker | 200 feeds, 1024.93 Mbit/s sent, 55.1% of one core, 28.1 MiB, 0 send errors, latest datagram 5.325 ms late |
| Feeds generator, during the run | 200 feeds, 1023.69 Mbit/s sent, 76.2% of one core, 28.1 MiB, 0 send errors, latest datagram 24.405 ms late |
| Feeds as sent (checked at the generator) | 200 of 200 streams arrived, 1025.0 Mbit/s; 0 CC errors (0 packets lost), 0 PCR jumps, PCR jitter up to 10.8 ms; 3398 keyframes, 0 GOPs dropped in 0 streams; longest silence 10.0 ms, 0 streams silent for a second or more. The checker used 89.4% of one core |
| Outputs received | 200 of 200 streams arrived, 830.2 Mbit/s; 0 CC errors (0 packets lost), 0 PCR jumps, PCR jitter up to 264.0 ms; 11981 keyframes, 0 GOPs dropped in 0 streams; longest silence 112.0 ms, 0 streams silent for a second or more. The checker used 85.7% of one core |
| `show.stats`, once a second | 60 reads, 2.2 ms on average, 7.7 ms at most, 201 shows |
| Shows by state at the end | {"alarm":26,"ok":175} |
| Alarms and outputs | alarms at peak {"freeze":25,"silence":1}, outputs at the end {"live":200} |
| Input as the station counted it | 832548.0 kbit/s, 0.0 CC errors, 0.0 packets lost |

## CPU and memory by role

Percent of one core, sampled once a second. `total` is the station and everything under it.

| Role | Processes | CPU avg % | CPU peak % | RSS avg MiB | RSS peak MiB |
|---|---|---|---|---|---|
| checker | 1 | 85.6 | 93.1 | 3.7 | 3.8 |
| direct host | 1 | 517.5 | 539.5 | 205.5 | 221.8 |
| feeds | 1 | 76.3 | 85.0 | 28.0 | 28.0 |
| shows | 1 | 1.3 | 2.0 | 145.1 | 145.1 |
| station | 1 | 0.4 | 1.0 | 629.3 | 629.5 |
| total | 3 | 519.1 | 540.5 | 979.9 | 996.2 |

## The worst streams received

| Stream | kbit/s | CC errors | Packets lost | PCR jumps | PCR gap max ms | Keyframes | GOP ms | GOPs dropped | Silence max ms |
|---|---|---|---|---|---|---|---|---|---|
| 127.0.0.1:30106 | 1954.0 | 0 | 0 | 0 | 38.0 | 60 | 1000.0 | 0 | 112.0 |
| 127.0.0.1:30037 | 3936.0 | 0 | 0 | 0 | 53.0 | 60 | 1000.0 | 0 | 99.0 |
| 127.0.0.1:30026 | 1954.0 | 0 | 0 | 0 | 38.0 | 60 | 1000.0 | 0 | 98.0 |
| 127.0.0.1:30030 | 1954.0 | 0 | 0 | 0 | 38.0 | 60 | 1000.0 | 0 | 98.0 |
| 127.0.0.1:30097 | 3937.0 | 0 | 0 | 0 | 53.0 | 60 | 1000.0 | 0 | 98.0 |
| 127.0.0.1:30125 | 3935.0 | 0 | 0 | 0 | 53.0 | 60 | 1000.0 | 0 | 98.0 |
| 127.0.0.1:30002 | 1955.0 | 0 | 0 | 0 | 38.0 | 60 | 1000.0 | 0 | 97.0 |
| 127.0.0.1:30054 | 1953.0 | 0 | 0 | 0 | 38.0 | 60 | 1000.0 | 0 | 97.0 |
| 127.0.0.1:30058 | 1954.0 | 0 | 0 | 0 | 38.0 | 60 | 1000.0 | 0 | 97.0 |
| 127.0.0.1:30074 | 1954.0 | 0 | 0 | 0 | 38.0 | 60 | 1000.0 | 0 | 97.0 |

## What this run says

200 direct shows ran clean on this Mac. Every output arrived for the whole
minute, with no continuity error, no PCR jump, no GOP dropped and no output
silent for more than 112 ms, and all 200 said `live` at the end. 200 is the
number the contract asks for and the most this run tried, so it is not a
ceiling.

What it costs is the direct host, one `gmx-ingest` process: about 5.2 cores
and 205 MiB for 200 shows, so about 2.6% of one core and 1 MiB a show at 4 to
8 Mbit/s each. A five second profile of it under this load spends its time in
the socket calls (a UDP receive and send per datagram, about 97,000 datagrams
a second each way) and in the TS demux, not in anything that grows faster than
the number of shows. The station itself used under 1% of one core, and
`show.stats` for 201 shows answered in 2.2 ms on average. On this machine the
next thing to run out is cores: the generator and the checker, which a real
headend does not have, took another 1.6 cores between them.

Things to read correctly:

* The outputs carry 830 Mbit/s for 1025 offered because the clips are
  constant bitrate with null packets padding them to the mux rate, and a copy
  repackages the media into the host's own MPEG-TS muxer, which sends no
  padding. The station counted 833 Mbit/s of media in, which matches.
* The 25 `freeze` alarms are right: those are the shows on the two program
  multiplex taking its second program, which is colour bars that never move.
  The one `silence` alarm is on `main`, the station's own show, which
  composites and has no source, so its programme is silent and the alarm is
  right. It is the 201st show in `show.stats`. A rerun with 8 feeds showed
  it there, with `Programme peak -350 dBFS`. At the time of this run no
  direct show with MPEG layer II sound had its sound measured at all, which
  was fixed afterwards.
* The station's 629 MiB is the encoder calibration it ran when it started,
  because this run's `GODWINMIX_HOME` is a fresh folder with no stored
  calibration. A fresh station climbs from about 130 MiB to 640 MiB during the
  few seconds it measures and keeps that memory afterwards; with a stored
  calibration it stays at about 130 to 180 MiB.
* The ingest plugin was installed with `POST /api/v1/plugins` while the
  station ran, into `plugins_dir` set to the run's own folder in the station's
  config, with `GODWINMIX_HOME` pointed there too. Nothing went into
  `~/.godwinmix`.
