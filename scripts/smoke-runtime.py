from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def read_response(stdout, request_id: int) -> dict:
    deadline = time.time() + 30
    while time.time() < deadline:
        line = stdout.readline()
        if not line:
            break
        line = line.strip()
        if not line:
            continue
        payload = json.loads(line)
        if payload.get("id") == request_id:
            return payload
    raise RuntimeError(f"timed out waiting for JSON-RPC response id={request_id}")


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: smoke-runtime.py <runtime-executable>", file=sys.stderr)
        return 2

    executable = Path(sys.argv[1]).resolve()
    if not executable.is_file():
        raise FileNotFoundError(executable)

    with tempfile.TemporaryDirectory(prefix="dsh-desktop-runtime-smoke-") as temp:
        root = Path(temp)
        workspace = root / "workspace"
        harness_home = root / "harness"
        workspace.mkdir()
        harness_home.mkdir()

        environment = dict(**__import__("os").environ)
        environment["DSH_HOME"] = str(harness_home)

        process = subprocess.Popen(
            [str(executable), "--profile", "sdk"],
            cwd=workspace,
            env=environment,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )

        try:
            assert process.stdin is not None
            assert process.stdout is not None

            process.stdin.write(json.dumps({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "__desktop_smoke_unknown_method__",
                "params": {},
            }) + "\n")
            process.stdin.flush()

            response = read_response(process.stdout, 1)
            if response.get("error") is None:
                raise RuntimeError(f"runtime did not return protocol error: {response}")

            process.stdin.write(json.dumps({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "shutdown",
                "params": {},
            }) + "\n")
            process.stdin.flush()

            shutdown = read_response(process.stdout, 2)
            if "result" not in shutdown:
                raise RuntimeError(f"runtime shutdown response was unexpected: {shutdown}")

            exit_code = process.wait(timeout=30)
            if exit_code != 0:
                stderr = process.stderr.read() if process.stderr else ""
                raise RuntimeError(f"runtime exited with {exit_code}: {stderr}")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)

    print(f"runtime smoke test passed: {executable}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
