# Your icon goes here

Put a square PNG called `icon.png` in this directory, 512x512 or larger, and
set the `icon` line in `build.toml` to `branding/icon.png`. It ships empty.

The workflow generates the `.icns` macOS wants and the `.ico` Windows wants
from that one file, so this is the only image you have to make. A transparent
background works on all three platforms. Anything smaller than 512x512 is
upscaled and looks like it.

Leave `icon` empty in `build.toml` and the build ships the stock GodwinMix
icon, which is fine for a first run and wrong for anything you hand to a
volunteer.
