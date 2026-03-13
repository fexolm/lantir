#!/usr/bin/env python3
"""
Lantir Debug MCP Server
-----------------------
Exposes build / run / dump-frame / log / compare / stop tools so Claude can
drive the rendering engine autonomously without a human at the terminal.

Usage:
    pip install mcp
    python3 mcp/lantir_debug_server.py

Then add the server to ~/.claude/claude_desktop_config.json (or the
Claude Code MCP settings) pointing to this script.
"""

import asyncio
import base64
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp.types import Tool, TextContent, ImageContent

REPO = Path(__file__).parent.parent.resolve()
SCRIPTS = REPO / "scripts"

# ── helpers ───────────────────────────────────────────────────────────────────

def run(cmd: list[str], timeout: int = 300, env: dict | None = None) -> dict:
    """Run a subprocess and return {stdout, stderr, returncode}."""
    merged_env = {**os.environ, **(env or {})}
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            cwd=str(REPO),
            env=merged_env,
        )
        return {
            "stdout": result.stdout,
            "stderr": result.stderr,
            "returncode": result.returncode,
            "ok": result.returncode == 0,
        }
    except subprocess.TimeoutExpired:
        return {"stdout": "", "stderr": "TIMEOUT", "returncode": -1, "ok": False}
    except Exception as e:
        return {"stdout": "", "stderr": str(e), "returncode": -1, "ok": False}


def fmt(r: dict) -> str:
    parts = []
    if r["stdout"]:
        parts.append(r["stdout"].strip())
    if r["stderr"]:
        parts.append("[stderr]\n" + r["stderr"].strip())
    parts.append(f"exit={r['returncode']}")
    return "\n".join(parts)


# ── server ────────────────────────────────────────────────────────────────────

app = Server("lantir-debug")


@app.list_tools()
async def list_tools() -> list[Tool]:
    return [
        Tool(
            name="build",
            description=(
                "Build the Lantir renderer. "
                "profile='debug' (default) or 'release'. "
                "Returns compiler output."
            ),
            inputSchema={
                "type": "object",
                "properties": {
                    "profile": {"type": "string", "enum": ["debug", "release"], "default": "debug"}
                },
            },
        ),
        Tool(
            name="run",
            description=(
                "Start the interactive example in the background. "
                "Returns the PID. Use stop() to kill it."
            ),
            inputSchema={"type": "object", "properties": {}},
        ),
        Tool(
            name="dump_frame",
            description=(
                "Render one deterministic frame of the debug scene "
                "(basicmesh.glb, fixed camera) and save it as a PNG. "
                "Returns the output path. "
                "Optionally pass output_path to override the default."
            ),
            inputSchema={
                "type": "object",
                "properties": {
                    "output_path": {"type": "string", "description": "Where to save the PNG"}
                },
            },
        ),
        Tool(
            name="read_frame",
            description="Read a PNG frame from disk and return it as base64 for visual inspection.",
            inputSchema={
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the PNG (default: debug/frames/latest.png)"}
                },
            },
        ),
        Tool(
            name="collect_logs",
            description=(
                "Run the interactive example for N seconds and capture its log output. "
                "Returns the captured log text."
            ),
            inputSchema={
                "type": "object",
                "properties": {
                    "seconds": {"type": "integer", "default": 5},
                    "log_level": {"type": "string", "default": "debug"},
                },
            },
        ),
        Tool(
            name="compare_frames",
            description=(
                "Compare two PNG frames and report PSNR / pixel diff. "
                "Returns metrics and writes a diff image."
            ),
            inputSchema={
                "type": "object",
                "properties": {
                    "baseline": {"type": "string", "default": "debug/baseline/baseline.png"},
                    "current":  {"type": "string", "default": "debug/frames/latest.png"},
                    "diff_out": {"type": "string", "default": "debug/frames/diff.png"},
                },
            },
        ),
        Tool(
            name="set_baseline",
            description=(
                "Promote a frame to the baseline. "
                "If source_path is omitted, renders a fresh frame first."
            ),
            inputSchema={
                "type": "object",
                "properties": {
                    "source_path": {"type": "string"}
                },
            },
        ),
        Tool(
            name="stop",
            description="Kill any running lantir example or debug_scene process.",
            inputSchema={"type": "object", "properties": {}},
        ),
        Tool(
            name="read_file",
            description="Read a source file from the repo (for code inspection).",
            inputSchema={
                "type": "object",
                "required": ["path"],
                "properties": {
                    "path": {"type": "string", "description": "Relative path from repo root"}
                },
            },
        ),
        Tool(
            name="grep_source",
            description="Search for a pattern in the source tree. Returns matching lines.",
            inputSchema={
                "type": "object",
                "required": ["pattern"],
                "properties": {
                    "pattern": {"type": "string"},
                    "glob":    {"type": "string", "default": "**/*.rs"},
                },
            },
        ),
    ]


@app.call_tool()
async def call_tool(name: str, arguments: dict):
    # ── build ──────────────────────────────────────────────────────────────
    if name == "build":
        profile = arguments.get("profile", "debug")
        r = run([str(SCRIPTS / "build.sh"), profile], timeout=300)
        return [TextContent(type="text", text=fmt(r))]

    # ── run ────────────────────────────────────────────────────────────────
    if name == "run":
        proc = subprocess.Popen(
            [str(SCRIPTS / "run.sh")],
            cwd=str(REPO),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        return [TextContent(type="text", text=f"Started example PID={proc.pid}")]

    # ── dump_frame ─────────────────────────────────────────────────────────
    if name == "dump_frame":
        args = [str(SCRIPTS / "dump-frame.sh")]
        if "output_path" in arguments:
            args.append(arguments["output_path"])
        r = run(args, timeout=120)
        return [TextContent(type="text", text=fmt(r))]

    # ── read_frame ─────────────────────────────────────────────────────────
    if name == "read_frame":
        path = Path(arguments.get("path", "debug/frames/latest.png"))
        if not path.is_absolute():
            path = REPO / path
        if not path.exists():
            return [TextContent(type="text", text=f"File not found: {path}")]
        data = base64.standard_b64encode(path.read_bytes()).decode()
        return [
            ImageContent(type="image", data=data, mimeType="image/png"),
            TextContent(type="text", text=f"Loaded: {path} ({path.stat().st_size} bytes)"),
        ]

    # ── collect_logs ───────────────────────────────────────────────────────
    if name == "collect_logs":
        seconds   = int(arguments.get("seconds", 5))
        log_level = arguments.get("log_level", "debug")
        r = run(
            [str(SCRIPTS / "collect-logs.sh"), str(seconds)],
            timeout=seconds + 30,
            env={"RUST_LOG": log_level},
        )
        # Also return the latest log file contents if it exists
        latest = REPO / "debug" / "logs" / "latest.log"
        log_text = latest.read_text(errors="replace") if latest.exists() else ""
        output = fmt(r)
        if log_text:
            output += f"\n\n── log contents ──\n{log_text[-8000:]}"
        return [TextContent(type="text", text=output)]

    # ── compare_frames ─────────────────────────────────────────────────────
    if name == "compare_frames":
        baseline = arguments.get("baseline", "debug/baseline/baseline.png")
        current  = arguments.get("current",  "debug/frames/latest.png")
        diff_out = arguments.get("diff_out", "debug/frames/diff.png")
        r = run([str(SCRIPTS / "compare-frames.sh"), baseline, current, diff_out], timeout=60)
        return [TextContent(type="text", text=fmt(r))]

    # ── set_baseline ───────────────────────────────────────────────────────
    if name == "set_baseline":
        args = [str(SCRIPTS / "set-baseline.sh")]
        if "source_path" in arguments:
            args.append(arguments["source_path"])
        r = run(args, timeout=120)
        return [TextContent(type="text", text=fmt(r))]

    # ── stop ───────────────────────────────────────────────────────────────
    if name == "stop":
        r = run([str(SCRIPTS / "stop.sh")])
        return [TextContent(type="text", text=fmt(r))]

    # ── read_file ──────────────────────────────────────────────────────────
    if name == "read_file":
        path = REPO / arguments["path"]
        if not path.exists():
            return [TextContent(type="text", text=f"Not found: {path}")]
        try:
            text = path.read_text(errors="replace")
            return [TextContent(type="text", text=text)]
        except Exception as e:
            return [TextContent(type="text", text=f"Error reading {path}: {e}")]

    # ── grep_source ────────────────────────────────────────────────────────
    if name == "grep_source":
        pattern = arguments["pattern"]
        glob    = arguments.get("glob", "**/*.rs")
        r = run(["rg", "--glob", glob, "-n", pattern], timeout=30)
        return [TextContent(type="text", text=fmt(r) or "(no matches)")]

    return [TextContent(type="text", text=f"Unknown tool: {name}")]


# ── entry point ───────────────────────────────────────────────────────────────

async def main():
    async with stdio_server() as (read_stream, write_stream):
        await app.run(read_stream, write_stream, app.create_initialization_options())


if __name__ == "__main__":
    asyncio.run(main())
