# Shipping a theme with a preset

A preset is a package of configuration, plugins and defaults for one kind of
user: the church preset, the producer preset, the school hall preset. A theme is
one of the things a preset may carry, and it carries it the same way a plugin
carries a panel: a CSS file under the package's `ui/` directory, served by the
core at `/plugins/<name>/ui/`, named in the manifest. The preset's install step
adds that entry to the theme list and sets it as the default; the operator can
still pick another from Settings, and their choice wins from then on, because a
preset chooses where someone starts and never what they are stuck with. Keep the
file to custom properties only, keep `--live` for what is going out, and check
the result against `high-contrast.css` before you ship it: a preset aimed at
volunteers in a hall is exactly the audience whose screens are too bright and
whose eyes have had enough three hours in.
