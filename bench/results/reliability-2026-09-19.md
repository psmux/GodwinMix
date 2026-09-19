# Reliability checks on macOS

The original nightly one hour run reported 71.6 percent RSS growth and a
51.1 ms programme stall gauge. This work found several separate lifecycle
faults. These measurements do not replace that one hour acceptance run.

* Preview queues could retain a second of full canvas buffers before scaling.
  They now retain at most two buffers. Dynamic thumbnail rate conversion starts
  at the first incoming timestamp rather than generating historical frames.
* Removing a source flushed its reusable compositor pad back into a state that
  waited for another first frame. An EOS event now retires the empty pad until
  a replacement stream starts. Removing only this fix made the focused timing
  regression fail at 49.994402 ms. The optimized fixed run passed at 33.958953 ms.
* Restarting a source sent FLUSH_START across proxy boundaries without matching
  FLUSH_STOP events. The source looked alive while the programme received zero
  frames. Three restart cycles now deliver programme buffers and media status.
* Live test generators already timestamp against the adopted programme clock.
  The aligner added programme age again, putting later sources into the future
  until their queues filled. Sources with this contract can now declare
  `programme-timeline`. A full canvas churn test failed on the second source
  before the change and passed all 12 cycles over 62 seconds afterwards.
* Preview resize teardown removed the compositor before stopping its live
  backdrop. The producer now stops first. Forty resize cycles produced no bus
  stream errors in the regression test.

The first bounded preview run lasted ten minutes and ended with 4.14 percent
less RSS than its warm up sample. Its report was reconstructed from all 120
retained sample rows after the running shell file was edited; its JSON records
that limitation. Its timing gate failed at 34.880713 ms.

A later ten minute run with source retirement and restart fixes, while the
other workers held compilation and testing, ended at 478.1 MiB RSS versus
490.7 MiB after warm up. Descriptors and threads were unchanged. The unchanged
34 ms timing gate still failed at 34.066613 ms. One preview subscription phase
took 15.515 seconds. This run preceded the programme timeline and preview
teardown fixes. Its complete samples remain in
`soak-reliability-quiet-2026-09-19.json`.

Targeted checks after the timeline and teardown fixes passed: 25 multiview tests,
5 restart tests, 15 manifest tests and the full canvas churn regression. Core
Clippy completed for all targets with no warnings.

The final integrated binary at `2d82a3c` ran all phases for three minutes and
36 rounds. RSS was 376.2 MiB after warm up and 376.4 MiB at the end, growth of
0.05 percent. Descriptors stayed at 87 and threads at 105. The slowest preview
subscription phase took 317 ms, compared with 15.515 seconds in the prior run.
There were no source stall or recovery messages and no panics. One source
conversion warning still reported not negotiated during churn.

The final timing result was 34.028375 ms, which fails the unchanged 34 ms bar.
Its complete samples are in `soak-reliability-integrated-2026-09-19.json`. The
console now prints six fractional digits for this value, because rounding a
failure to 34.0 ms made it look indistinguishable from the limit.

No worker built or tested during the final run. An unrelated core that predated
this work remained active on port 54813, along with normal desktop processes.
This is a short macOS measurement, not a one hour or cross platform acceptance
pass. The remaining timing margin and occasional negotiation warning need
further tracing, and the updated lifecycle code still needs the full nightly
endurance run.
