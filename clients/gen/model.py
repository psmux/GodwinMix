"""One reading of `protocol.json`, shared by the three generators.

The file on disk is JSON Schema with everything a core knows about itself. What
a client library needs out of it is smaller: a list of named types, a list of
methods with the type of their params and their result, a list of events with
the type of their payload, and the `ext` keys. This module produces that, so
the language backends only have to worry about syntax.
"""

import json

from names import pascal


class Ref:
    """A reference to a named type, resolved by the backends."""

    def __init__(self, name):
        self.name = name


class Model:
    def __init__(self, doc):
        self.doc = doc
        self.api_level = doc["api_level"]
        self.api_compatible = doc["api_compatible"]
        self.types = {}          # name -> schema, in the order they are emitted
        self.methods = []
        self.events = []
        self.ext = doc.get("ext", [])
        self.errors = doc.get("errors", [])
        self._read_types()
        self._read_methods()
        self._read_events()

    # ------------------------------------------------------------------ types

    def _read_types(self):
        for name in sorted(self.doc.get("$defs", {})):
            self.types[name] = self.doc["$defs"][name]

    def _synthesise(self, name, schema):
        """Give an inline object a name, so every language can name it too."""
        self.types[name] = schema
        return name

    def resolve(self, schema):
        """Follow a `$ref` into `$defs`. Anything else comes back untouched."""
        while isinstance(schema, dict) and "$ref" in schema:
            key = schema["$ref"].split("/")[-1]
            schema = self.doc.get("$defs", {}).get(key, {})
        return schema or {}

    # ---------------------------------------------------------------- methods

    def _read_methods(self):
        for m in sorted(self.doc.get("methods", []), key=lambda x: x["name"]):
            name = m["name"]
            params = m.get("params") or {}
            result = m.get("result") or {}
            self.methods.append(
                {
                    "name": name,
                    "summary": m.get("summary", ""),
                    "scope": m.get("scope", "read"),
                    "mutating": bool(m.get("mutating")),
                    "destructive": bool(m.get("destructive")),
                    "idempotent": bool(m.get("idempotent")),
                    "since": m.get("since", ""),
                    "rest": m.get("rest"),
                    "params": self._params_type(name, params),
                    "params_schema": params,
                    "result": self._result_type(name, result),
                    "result_schema": result,
                }
            )

    def _params_type(self, method, schema):
        if "$ref" in schema:
            return Ref(schema["$ref"].split("/")[-1])
        props = schema.get("properties") or {}
        if not props:
            return None  # takes nothing but the envelope keys
        return Ref(self._synthesise(pascal(method) + "Params", schema))

    def _result_type(self, method, schema):
        if "$ref" in schema:
            return Ref(schema["$ref"].split("/")[-1])
        if schema.get("type") == "array" and "$ref" in (schema.get("items") or {}):
            return schema  # a list of a named type: the backends map it directly
        if schema.get("properties"):
            return Ref(self._synthesise(pascal(method) + "Result", schema))
        return schema  # a free object, which every language spells as raw JSON

    # ----------------------------------------------------------------- events

    def _read_events(self):
        for e in self.doc.get("events", []):
            pattern = e.get("pattern") or e["name"].split("/", 1)[-1]
            payload = e.get("payload") or {}
            binary = payload.get("contentEncoding") == "binary"
            self.events.append(
                {
                    "name": e["name"],
                    "pattern": pattern,
                    "summary": e.get("summary", ""),
                    "ext": e.get("ext"),
                    "since": e.get("since", ""),
                    "binary": binary,
                    "payload": None if binary else self._payload_type(pattern, payload),
                }
            )

    def _payload_type(self, pattern, schema):
        if "$ref" in schema:
            return Ref(schema["$ref"].split("/")[-1])
        if schema.get("properties"):
            return Ref(self._synthesise(pascal(pattern) + "Event", schema))
        return schema


def load(path):
    with open(path, "r", encoding="utf-8") as fh:
        return Model(json.load(fh))
