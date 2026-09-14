# zero-dep

The plugin that keeps the protocol honest.

`../zero-dep-source.py` draws colour bars and speaks the whole control
protocol in under two hundred lines of Python that import nothing outside the
standard library. If it ever needs a dependency, the protocol has drifted and
the protocol is wrong, not the file.

This directory is the manifest and the settings schema that make it a plugin
the core can install. The entry point is symlinked in on install, or copied:

    cp examples/zero-dep-source.py examples/zero-dep/main.py
    gmx plugin test examples/zero-dep --quick
    gmx plugin add examples/zero-dep
    gmx source add bars --type zero-dep/source
    gmx ctl status

The harness run on a 2021 laptop, for comparison:

    ok   manifest               zero-dep v0.1.0: 1 provide(s), 0 tool(s), every path and schema in place
    ok   spawn                  hello in 49 ms (the limit is 5 s), api 1, transport container
    ok   playing                reached PLAYING within the timeout
    ok   video caps             video/x-raw, format=(string)I420, width=(int)1280, height=(int)720, ...
    ok   video buffers          91 buffers, none out of order
    ok   audio buffers          not declared, not expected
    ok   stop                   the pipeline is in NULL and the kind let go
    ok   configure              3 example(s), every one answered
