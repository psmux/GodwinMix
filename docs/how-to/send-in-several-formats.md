# Send in several formats

Each destination can take the programme in the format it wants: YouTube at
1080p, Facebook at 720p, a church's own server at 480p to spare its uplink.
The mixer works out the fewest encoders that make all of them, shares one
encoder between every destination that wants the same thing, and will not
start a format that would make what is already on air drop frames.

The Format row and the Resources tab described below arrive with the
renditions page in this release.

## From the page

Every destination can go out in its own format. You pick it when you add the
destination, and you can change it later from the same form.

### Pick a format

Press **Add destination** on the Outputs panel and pick a platform. Under the
stream key is a **Format** row of cards:

* **Same as the source** is always first and always marked Free. The programme's
  own encode goes out as it is, and nothing more is encoded for this
  destination.
* Then the presets this machine can make, each with its size, frame rate and
  bitrate, and a badge for what it costs: Free, Light or Heavy. Heavy means it
  would take a big share of the CPU.
* A preset this machine cannot make is still shown, dimmed, with the reason
  under it, so you know why 1080p60 is not on offer.
* **Custom** opens a small form under the cards: codec, picture size, frame
  rate, bitrate, keyframe interval and sound.

The platform picks the card to start on. YouTube starts on YouTube 1080p,
Facebook on Facebook 720p, Twitch on the best Twitch preset this machine can
make. When the programme already matches what the platform asks for, the form
starts on Same as the source instead and says so on the card, because a copy
of the right thing costs nothing.

Press **Start sending** and the destination goes out in that format.

A channel's destinations work the same way. On the Channels tab, press a
platform tile under Send on to, paste the key, and choose the format under it.
Same as the source there means the encoder's own bytes, copied with no decode
at all.

### See what the mixer did

Each row on the Outputs panel has a line under its address saying how that
destination is being made:

* Copied, no re-encoding
* Encoded on the GPU (h264-videotoolbox), shared with 2 others
* Encoded on the CPU (x264) because the GPU is full

Shared means several destinations asked for the same format and one encoder
makes it for all of them. A channel's destination tile says the same in two
words under the platform, and the whole sentence when you hover over it.

### When there is no room

If a format would make what is already on air drop frames, the mixer does not
start it. The form stays open and says what the format needs and how much room
is left, in cores, GPU sessions and upload. Under that is a button for each
format that does fit. Press one and the destination starts in that format
instead.

### Resources

The **Resources** tab on the Outputs panel shows what this machine has and
what is using it:

* The CPU as a bar, with a line for each encoder and what it serves, the
  mixer's own work, the room left for more, and the part kept free for what is
  already on air.
* Each GPU the same way, with its encoder sessions as dots, filled for each one
  in use.
* How much is going out over the network. Copies cost no CPU but they do cost
  upload.
* Anything that was dropped to keep the programme going, and why.

**Measure this machine again** times each format on each encoder so the mixer
knows what fits. It cannot run while anything is on air, and the button says
so. The tab only asks the mixer for these numbers while it is on screen.

A mixer older than this page has no formats to offer: the Format row does not
appear, and destinations go out the way they always did.

## What the mixer does with your choices

* A destination left on **Same as the source** reads the programme's own
  encode, exactly as every destination always has. Choosing formats for
  other destinations costs it nothing.
* Destinations that pick the same format share one encoder. Four
  destinations on YouTube 720p cost one 720p encode, not four.
* Each size is scaled once, however many formats use it.
* A GPU encoder is used first when this machine has one with room, the CPU
  after that. The line under each output says which, and why.
* Adding or removing a destination starts or stops only its own encoder.
  Nothing else on air notices: the programme and every other destination go
  on without losing a frame.
* The sizes of an HLS ladder put their keyframes on the same frames, so a
  player moves between them without a pause.

## When the machine runs short on air

If something else on the machine takes the CPU while you are live (another
program, the fans losing to the heat), the mixer gives up the least
important work first: the smallest size of an HLS ladder, then the next.
The programme and the top size of every destination are never dropped. An
alert says what was stopped and why, the destination's row says so too, and
it comes back by itself once there has been room again for ten seconds.

## A machine with no GPU

Everything above works on the CPU alone; it fits fewer formats. To see what
a machine without a GPU encoder would do, open **Mixer settings**, find
**Hardware** and set **Encode** to **Software**, then restart the mixer.
The formats, their costs and the Resources tab then describe the CPU only.

The first time the mixer runs on a machine it measures each encoder for a
few seconds, in the background, once nothing is on air. Until then the
costs it shows are cautious guesses and a refusal says so.

## From a script or an agent

`output.add` and `output.set` take `rendition`: `{"preset": "youtube-720p30"}`,
a custom ladder `{"ladder": [...]}` for HLS, or a whole rendition request.
`rendition.presets` lists what this machine can make and what each costs,
`rendition.plan` shows the encoders and why each was chosen, and
`governor.status` shows what is in use and free. A refusal carries a button
for each format that fits, as `data.advice`. The shapes are in
[the renditions reference](../reference/renditions.md).
