"""A throwaway mixer for dev/agents_e2e.py: two test cameras, a token, a free port.

Written the way dev/smoke.sh writes its config: the example config with its
sample sources and outputs commented out, then two test:// sources added so
there is a camera and a wide shot to switch between. Standard library only.
"""

import os
import re
import socket
import subprocess
import tempfile
import time
import urllib.request

SOURCES = '''
[[sources]]
id = "cam1"
name = "Camera"
type = "test/source"
uri = "test://ball"
params = { uri = "test://ball" }

[[sources]]
id = "cam-wide"
name = "Wide"
type = "test/source"
uri = "test://smpte"
params = { uri = "test://smpte" }
'''


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def config(example, port, token, work):
    out, skipping = [], False
    for line in example.splitlines():
        stripped = line.strip()
        if stripped.startswith("[["):
            skipping = stripped in ("[[sources]]", "[[outputs]]")
        elif stripped.startswith("["):
            skipping = False
        out.append("# " + line if skipping and not line.startswith("#") else line)
    text = "\n".join(out) + "\n"
    text = re.sub(r"(?m)^bind = .*$", f'bind = "127.0.0.1:{port}"', text)
    text = re.sub(r"(?m)^# token = .*$", f'token = "{token}"', text)
    text = re.sub(r"(?m)^min_hold_ms = .*$", "min_hold_ms = 0", text)
    plugins = os.path.join(work, "plugins").replace("\\", "/")
    text = re.sub(r"(?m)^# plugins_dir = .*$", f'plugins_dir = "{plugins}"', text)
    return text + SOURCES


class Core:
    """Start a core; stop it on exit. `url` and `token` are what a client needs."""

    def __init__(self, godwinmix):
        self.work = tempfile.mkdtemp(prefix="gmx-agents-e2e-")
        self.token = "e2e-" + os.urandom(6).hex()
        port = free_port()
        self.url = f"http://127.0.0.1:{port}"
        example = subprocess.run([godwinmix, "--example-config"], capture_output=True, text=True, check=True).stdout
        path = os.path.join(self.work, "godwinmix.toml")
        with open(path, "w", encoding="utf-8") as f:
            f.write(config(example, port, self.token, self.work))
        self.log = open(os.path.join(self.work, "core.log"), "w")
        # A home of its own, so `agent.setup` writes where the test can look
        # and never into the runner's real one.
        self.home = os.path.join(self.work, "home")
        os.makedirs(self.home)
        env = dict(os.environ, HOME=self.home, USERPROFILE=self.home,
                   APPDATA=os.path.join(self.home, "AppData", "Roaming"),
                   XDG_CONFIG_HOME=os.path.join(self.home, ".config"),
                   XDG_DATA_HOME=os.path.join(self.home, ".local", "share"))
        self.proc = subprocess.Popen([godwinmix, "--config", path], cwd=self.work, env=env,
                                     stdout=self.log, stderr=subprocess.STDOUT)

    def wait(self, seconds=90):
        deadline = time.time() + seconds
        while time.time() < deadline:
            if self.proc.poll() is not None:
                raise SystemExit(f"the core exited with {self.proc.returncode}; its log is {self.log.name}")
            try:
                req = urllib.request.Request(self.url + "/api/v1/agent/state", headers={"Authorization": "Bearer " + self.token})
                with urllib.request.urlopen(req, timeout=5):
                    return self
            except OSError:
                time.sleep(0.5)
        raise SystemExit(f"the core did not answer in {seconds}s; its log is {self.log.name}")

    def stop(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        self.log.close()
