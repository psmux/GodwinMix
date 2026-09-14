"""The one error shape.

Every refusal from the core arrives as `{code, message, data}`, and the message
already names the current state and the next step, so a surface shows it as it
came rather than inventing wording of its own.
"""

from __future__ import annotations

from typing import Any, Dict, Optional

#: Codes the core uses. Anything else falls through to the generic branch.
PARSE = -32700
INVALID_REQUEST = -32600
NO_METHOD = -32601
BAD_PARAMS = -32602
INTERNAL = -32603
WRONG_STATE = -32001
NO_SCOPE = -32002
SAFETY = -32003
NOT_FOUND = -32004
NO_PLACEMENT = -32005
PLUGIN_DIED = -32010
LINE_TOO_LONG = -32011
RESTART_REQUIRED = -32012
CONFIRM_REQUIRED = -32020

CODES = {
    "PARSE": PARSE,
    "INVALID_REQUEST": INVALID_REQUEST,
    "NO_METHOD": NO_METHOD,
    "BAD_PARAMS": BAD_PARAMS,
    "INTERNAL": INTERNAL,
    "WRONG_STATE": WRONG_STATE,
    "NO_SCOPE": NO_SCOPE,
    "SAFETY": SAFETY,
    "NOT_FOUND": NOT_FOUND,
    "NO_PLACEMENT": NO_PLACEMENT,
    "PLUGIN_DIED": PLUGIN_DIED,
    "LINE_TOO_LONG": LINE_TOO_LONG,
    "RESTART_REQUIRED": RESTART_REQUIRED,
    "CONFIRM_REQUIRED": CONFIRM_REQUIRED,
}

_TITLES = {
    NO_METHOD: "This core does not have that command",
    BAD_PARAMS: "The command was not filled in correctly",
    WRONG_STATE: "Not ready for that yet",
    NO_SCOPE: "This token is not allowed to do that",
    SAFETY: "Held back by the safety settings",
    NOT_FOUND: "Not found",
    NO_PLACEMENT: "That plugin cannot run there",
    PLUGIN_DIED: "The plugin stopped during the call",
    RESTART_REQUIRED: "The plugin has to be reloaded",
    CONFIRM_REQUIRED: "Confirmation needed",
}


class RpcError(Exception):
    """A refusal from the core, with everything a surface needs to show it."""

    def __init__(self, code: int, message: str = "", data: Optional[Dict[str, Any]] = None):
        super().__init__(message or "the call failed")
        self.code = code
        self.message = message or "the call failed"
        self.data: Dict[str, Any] = data or {}

    @property
    def title(self) -> str:
        """A short heading for a toast. The message stays as the body."""
        return _TITLES.get(self.code, "That did not work")

    @property
    def retryable(self) -> bool:
        """True when trying the same call again could plausibly succeed."""
        flag = self.data.get("retryable")
        if isinstance(flag, bool):
            return flag
        return self.code in (WRONG_STATE, SAFETY, PLUGIN_DIED)

    @property
    def retry_after_ms(self) -> Optional[int]:
        """Milliseconds to wait before a retry, when the core named one."""
        ms = self.data.get("retry_after_ms")
        return ms if isinstance(ms, (int, float)) else None

    @property
    def next_step(self) -> str:
        """The sentence after the last full stop, which by convention is the next step."""
        parts = [p for p in self.message.split(". ") if p.strip()]
        if len(parts) < 2:
            return ""
        last = parts[-1]
        return last if last.endswith(".") else last + "."

    def __str__(self) -> str:
        return f"{self.message} (code {self.code})"


class ConnectionClosed(RpcError):
    """The connection went away, perhaps with a call outstanding."""

    def __init__(self, why: str = "the connection to the mixer closed"):
        super().__init__(PLUGIN_DIED, why, {"retryable": True})
