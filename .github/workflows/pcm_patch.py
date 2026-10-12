import sys

p = sys.argv[1]
s = open(p).read()
a = "    s = socket.create_connection((host, port), timeout=20)\n"
assert a in s
s = s.replace(a, a + "    s.settimeout(4)\n", 1)
b = "def evidence(prefix):\n"
assert b in s
extra = '''    import json, time

    def post(path, body):
        r = urllib.request.Request(f"{base}{path}", data=json.dumps(body).encode(), method="POST")
        r.add_header("Authorization", f"Bearer {token}")
        r.add_header("content-type", "application/json")
        try:
            urllib.request.urlopen(r, timeout=5).read()
        except Exception as e:
            print("log.gst", e)

    post("/api/v1/log/gst", {"categories": "aggregator:6,audioaggregator:6,audiomixer:6,"
                             "audiotestsrc:5,basesrc:5,tee:6,queue:5", "duration_secs": 3})
    time.sleep(3.5)
    for path, suffix in (("/api/v1/pipeline/latency?name=programme", "-latency.json"),
                         ("/api/v1/pipeline/clock?name=programme", "-clock.json")):
        try:
            with urllib.request.urlopen(authed(path), timeout=5) as r:
                open(prefix + suffix, "wb").write(r.read())
        except Exception:
            pass
'''
s = s.replace(b, b + extra, 1)
open(p, "w").write(s)
