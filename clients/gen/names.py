"""Name shapes the three languages want, from one protocol name.

`program.take` is `programTake` in TypeScript, `program_take` in Python and
Rust, and `ProgramTake` when it has to be a type. Everything that turns a
protocol string into an identifier goes through here so the three generators
cannot drift from each other.
"""

# Words Rust will not let us use as a field name. A field called `type` is
# emitted as `r#type` with a serde rename back to the wire spelling.
RUST_KEYWORDS = {
    "as", "async", "await", "box", "break", "const", "continue", "crate", "dyn",
    "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let",
    "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self",
    "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
    "where", "while", "yield",
}

PYTHON_KEYWORDS = {
    "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "False", "finally", "for", "from",
    "global", "if", "import", "in", "is", "lambda", "None", "nonlocal", "not",
    "or", "pass", "raise", "return", "True", "try", "while", "with", "yield",
}


def parts(name):
    """Split a name into words.

    `source.audio.set` -> [source, audio, set], and `ConversionPhase` ->
    [Conversion, Phase], so a type name reaches a Rust constant as
    CONVERSION_PHASE rather than as one run-on word.
    """
    out = []
    for chunk in name.replace("/", ".").replace("-", ".").replace("_", ".").split("."):
        if not chunk:
            continue
        word = ""
        for i, ch in enumerate(chunk):
            starts = ch.isupper() and i > 0 and not chunk[i - 1].isupper()
            if starts and word:
                out.append(word)
                word = ""
            word += ch
        if word:
            out.append(word)
    return out


def camel(name):
    """`program.take` -> `programTake`."""
    words = parts(name)
    if not words:
        return "_"
    return words[0] + "".join(w[:1].upper() + w[1:] for w in words[1:])


def pascal(name):
    """`program.took` -> `ProgramTook`."""
    return "".join(w[:1].upper() + w[1:] for w in parts(name)) or "_"


def snake(name):
    """`program.take` -> `program_take`. Already snake stays as it is."""
    return "_".join(w.lower() for w in parts(name)) or "_"


def rust_field(name):
    """A struct field name Rust accepts, and whether it needed escaping."""
    ident = snake(name)
    if ident in RUST_KEYWORDS:
        return "r#" + ident, True
    return ident, ident != name


def python_arg(name):
    """A keyword argument name Python accepts."""
    if name in PYTHON_KEYWORDS or not name.isidentifier():
        return name + "_"
    return name


def is_identifier(name):
    return name.isidentifier()


def doc_lines(text, prefix):
    """Wrap a schema description as comment lines, one sentence per line kept."""
    if not text:
        return []
    out = []
    for raw in str(text).strip().splitlines():
        out.append((prefix + " " + raw).rstrip())
    return out
