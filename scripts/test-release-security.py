"""Verify the exported macro removes environment credentials from release executables."""
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parent.parent
markers = ["release-key-sentinel-7ec285", "release-account-sentinel-7ec285", "release-password-sentinel-7ec285"]
env = dict(os.environ, MASAMUNE_AI_DEBUGGER_API_KEY=markers[0],
           MASAMUNE_AI_DEBUGGER_ACCOUNTS=f"{markers[1]}:{markers[2]}")
for profile in [[], ["--release"]]:
    result = subprocess.run(["cargo", "test", "-p", "masamune-whisker-ai-debugger", "--test", "security",
                             "--locked", "--message-format=json", *profile], cwd=root, env=env,
                            text=True, stdout=subprocess.PIPE, check=True)
    executables = []
    for line in result.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") == "compiler-artifact" and message.get("executable") and message["target"]["name"] == "security":
            executables.append(message["executable"])
    assert executables, "no security executable was checked"
    for executable in executables:
        data = Path(executable).read_bytes()
        for marker in markers:
            assert (marker.encode() in data) == (not profile), "unexpected credential embedding"
    print(f"{'release excludes' if profile else 'debug includes'} all three credential sentinels", flush=True)
