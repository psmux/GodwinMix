# Why WASM is not on the frame path

GodwinMix runs `service` and `transition` plugins as WebAssembly components
(tier W). It does not run sources, outputs, filters or encoders that way, and
it never will. This is the reasoning, written down because the question comes
up every time someone reads the tier table: if a component is sandboxed and
fast, why not put a chroma key in one?

## The one rule

The programme output never stops. Nothing added to this mixer may block, stall
or slow the encoder, and nothing may do blocking work on a GStreamer streaming
thread. Every design decision in this document falls out of that.

## What the frame path actually is

At 30 fps a frame is 33.3 milliseconds. The compositor's aggregator wakes on
its clock, takes what every pad has, mixes, and hands the result to the
encoder. If that pass is late, the frame is late, and a late frame on a live
stream is a visible stutter for every viewer at once.

The pass is not just late when something takes 40 ms. It is late when something
takes 5 ms more than it did last time, on a machine that had 6 ms of margin.
The budget on a Raspberry Pi 4 at 720p30 is small enough that the margin is
real and the mixer is built around it.

## What a component would cost there

Three costs, in order of how badly they bite.

**A copy.** The component model boundary is a value boundary. A `list<u8>`
crossing into a component is copied into its linear memory, and the result is
copied back out. A 1920x1080 I420 frame is about 3.1 MB. Two copies per frame
at 30 fps is 186 MB/s of memcpy that the zero copy path (unixfd, a dmabuf
handed between processes) does not pay at all. On a Pi that is most of the
memory bandwidth budget spent moving bytes nowhere.

**Compilation, once, at the wrong moment.** wasmtime compiles a component when
it is loaded. The 100 KB examples in this repository take 1.7 to 2 seconds on a
laptop. That is fine at startup and unacceptable during a show, which means a
filter that is added while live would either stall or need a warm pool nobody
has asked for.

**Unpredictability that cannot be bounded.** This is the one that settles it.
A component can be cut by fuel or by an epoch deadline, and both are excellent
tools. What neither can do is produce a frame. A filter that is cut has no
output, and the compositor's pad has nothing to take. At that point the
options are: stall (forbidden), repeat the last frame (a visible freeze on
every cut), or drop the filter mid show (a visible jump). Every one of those is
worse than the thing the sandbox was protecting against.

A process is different. A sidecar filter that dies is killed, the branch is
rebuilt, and the mixer's freeze frame covers the gap while the rest of the
programme carries on. The isolation a process gives is the isolation a mixer
can actually use, because the recovery is asynchronous. A component's isolation
is synchronous with the call, and a synchronous failure on the frame path is a
dropped frame.

## What is left, and why it is worth having

Take the frame away and what remains is the work that made the tier attractive
in the first place: decisions. Should this take go through? What should the
pads do over the next 300 milliseconds? What does this tool answer? None of
that is on the frame path, all of it is third party code an operator installs
from a marketplace, and all of it benefits from a sandbox.

A `take.before` hook is the clearest case. It is third party code that runs
between an operator pressing a button and the programme changing. It has a 20
millisecond budget by default, one frame at 30 fps, and it cannot touch the
compositor at all: the take is a command on a queue and the pipeline keeps
producing whether or not a decision is pending. A component is exactly the
right shape for that, and the sandbox costs nothing anyone can see.

A transition is the interesting case, because it looks like it is on the frame
path and is not.

## How a transition stays off it

The core asks a transition plugin what the pads should do **before** the
transition window opens, not during it. Up to 65 samples across the window, all
of them taken up front, inside one budget. What comes back becomes a
`GstControlBinding` on each compositor pad: an interpolation control source the
aggregator samples itself, per output frame, with no plugin in the loop.

```text
  take arrives
       |
       |  sample_plugin: up to 65 render calls, one 200 ms budget,
       |  on a worker thread, before the window
       v
  curves -> DirectControlBinding on each pad
       |
       v
  the window: the aggregator samples the control source per frame.
  Nothing calls the plugin. Nothing can.
```

A component that answers `render` with a curve rather than with per frame pad
values is asked **once** per take, and then never again. `examples/wasm-ease`
does that in about twenty lines, and it is the answer worth giving.

So the worst a slow transition component can do is delay the setting up of a
take by its budget, on a thread that is not the mixer's and is certainly not
the compositor's. When it runs out of fuel the take falls back to the built in
cut, the programme is on the new scene either way, and the frame interval never
moves. `crates/godwinmix-wasm/tests/frame_interval.rs` starves a component on
purpose and measures exactly that.

## Why a worker thread and not the caller's

A component call runs on a thread of its own, one per instance, and the caller
sends a message and waits on a channel with its own deadline.

The reason is that the caller is sometimes the mixer. A transition's `render`
is asked for from the mixer's command loop, and a component running there would
hold the command queue for as long as it liked: every other operator command,
every source being added, every take behind it. Off the loop, the mixer waits
at most its deadline and then gives up, and the worker is cut separately by
fuel or by the epoch and goes back to waiting for the next job. Neither end can
hold the other.

The same rule applies to the supervisor's instance table. A component is held
behind an `Arc` that callers clone out before dropping the lock, so a slow
component never makes `plugin.list` or an unrelated tool call wait.

## What this rules out, honestly

There is no way to write a video effect in Rust, compile it to WASM, and have
GodwinMix run it. If that is what you want, write a `sidecar` filter: it is the
same Rust, it runs in a process, media reaches it over unixfd with no copy on
Linux and macOS, and a crash costs one branch and a freeze frame rather than
the programme.

The tier table in the plugin reference says which placements carry media. The
core refuses the other combination outright, with `-32005` and a message that
names them, rather than letting someone discover this at 19:59 on a Sunday.

## What would change this

One thing, and it is not a faster runtime. If the component model grows a way
to hand a buffer across without copying it *and* a way to fail a call that
leaves the previous frame valid and the pipeline unstalled, the argument above
loses two of its three legs. The third, that a synchronous failure on the frame
path is a dropped frame, would still stand, and it is the one that decides.

Until then: logic in a component, pixels in a process.

## See also

* [Plugins as WebAssembly components](../reference/wasm.md) for the world, the
  limits and the numbers.
* [Why plugins are processes](why-plugins-are-processes.md) for the argument
  about tier 2, which this one is the counterpart to.
* [Why the programme never stops](why-the-programme-never-stops.md) for the
  rule everything here follows from.
