"""hari_client — minimal Python client for the Hari Phase 6 streaming protocol.

The protocol is stdio-JSONL, strictly synchronous request/response. See
``docs/research/phase6-design.md`` §3 for the full spec. This module is a
reference implementation: stdlib-only, ~150 lines, intended to be copied
verbatim into an IX-side codebase.

Usage
-----

    from hari_client import HariSession

    with HariSession.spawn("/path/to/hari-core") as s:
        opened = s.open_session({"dimension": 4, "priority_model": "Flat"})
        rec = s.event({
            "cycle": 1,
            "source": "ix-agent",
            "payload": {"type": "belief_update", "proposition": "p", "value": "Probable"},
        })
        snapshot = s.metrics()
        closed = s.close()

The context manager always shuts down cleanly: stdin is closed, the child
is awaited, and any leaked subprocess is killed on exception.

Errors
------

A ``Response::Error`` from Hari raises :class:`HariProtocolError` carrying the
typed code (``invalid_json`` / ``no_session`` / ``out_of_order_cycle`` / …),
the human-readable message, the originating ``request_op``, and the ``fatal``
flag. Non-fatal errors leave the session usable; fatal ones do not.
"""

from __future__ import annotations

import json
import subprocess
from typing import Any, Optional


class HariProtocolError(RuntimeError):
    """Raised when Hari returns ``{"op": "error", ...}``."""

    def __init__(
        self,
        code: str,
        message: str,
        request_op: Optional[str],
        fatal: bool,
    ) -> None:
        super().__init__(
            f"[{code}] {message} (request_op={request_op!r}, fatal={fatal})"
        )
        self.code = code
        self.message = message
        self.request_op = request_op
        self.fatal = fatal


class HariSession:
    """A single live ``hari-core serve`` subprocess driven over stdio JSONL.

    Construct via :meth:`spawn`. All methods are blocking — Hari emits
    exactly one response per request, so we read one line per write.

    The subprocess holds in-memory state (beliefs, attention, goals); it
    survives until :meth:`close` (or context-manager exit).
    """

    def __init__(self, proc: subprocess.Popen) -> None:
        self._proc = proc
        self._closed = False

    # ------------------------------------------------------------------
    # Construction
    # ------------------------------------------------------------------

    @classmethod
    def spawn(cls, binary: str) -> "HariSession":
        """Spawn ``<binary> serve`` with piped stdio. Binary I/O — line
        endings are ``\\n`` on every platform, no text-mode translation.
        """
        proc = subprocess.Popen(
            [binary, "serve"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        return cls(proc)

    # ------------------------------------------------------------------
    # Protocol primitives
    # ------------------------------------------------------------------

    def _send(self, req: dict) -> dict:
        if self._proc.stdin is None or self._proc.stdout is None:
            raise RuntimeError("subprocess pipes are not available")
        line = (json.dumps(req) + "\n").encode("utf-8")
        self._proc.stdin.write(line)
        self._proc.stdin.flush()
        raw = self._proc.stdout.readline()
        if not raw:
            stderr = b""
            if self._proc.stderr is not None:
                try:
                    stderr = self._proc.stderr.read() or b""
                except Exception:
                    pass
            raise RuntimeError(
                "hari-core closed stdout before responding "
                f"(stderr={stderr.decode('utf-8', errors='replace')!r})"
            )
        resp: dict = json.loads(raw.decode("utf-8"))
        if resp.get("op") == "error":
            raise HariProtocolError(
                code=resp["code"],
                message=resp.get("message", ""),
                request_op=resp.get("request_op"),
                fatal=bool(resp.get("fatal", False)),
            )
        return resp

    # ------------------------------------------------------------------
    # Typed helpers
    # ------------------------------------------------------------------

    def open_session(self, config: Optional[dict] = None) -> dict:
        """Send ``{"op": "open", "config": {...}}``. Must be the first request."""
        return self._send({"op": "open", "config": config or {}})

    def event(self, event: dict) -> dict:
        """Send a single ``ResearchEvent``. Returns the recommendation."""
        return self._send({"op": "event", "event": event})

    def metrics(self) -> dict:
        """Cheap on-demand snapshot. Returns ``{metrics, beliefs, goals}``."""
        return self._send({"op": "metrics"})

    def close(self) -> dict:
        """Finalise the session. Returns ``{final_report, unclean}``.

        Closes stdin and awaits the child process so the OS reclaims the
        pipe. Idempotent — calling it twice is a no-op after the first.
        """
        if self._closed:
            return {}
        resp = self._send({"op": "close"})
        self._closed = True
        if self._proc.stdin is not None:
            try:
                self._proc.stdin.close()
            except Exception:
                pass
        try:
            self._proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self._proc.kill()
            self._proc.wait()
        return resp

    # ------------------------------------------------------------------
    # Context manager — guarantees cleanup on exception
    # ------------------------------------------------------------------

    def __enter__(self) -> "HariSession":
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        if self._closed:
            return
        # Best-effort clean shutdown; on exception we don't care about
        # the final report.
        try:
            if self._proc.poll() is None and self._proc.stdin is not None:
                self._proc.stdin.close()
            self._proc.wait(timeout=5)
        except Exception:
            try:
                self._proc.kill()
                self._proc.wait()
            except Exception:
                pass
