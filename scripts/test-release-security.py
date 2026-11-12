"""Verify the exported macro removes environment credentials from release executables.

`ai_debugger_config!` reads the credentials only in debug builds of a crate compiled with the
`masamune_ai_debugger` cfg. The security tests run without the cfg, so no profile may embed the
sentinels. A build with the cfg (not run, its assertions expect the cfg to be absent) is the
positive control: its debug executable must embed them and its release executable must not.
"""
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parent.parent
markers = ["release-key-sentinel-7ec285", "release-account-sentinel-7ec285", "release-password-sentinel-7ec285"]
base_env = dict(os.environ, MASAMUNE_AI_DEBUGGER_API_KEY=markers[0],
                MASAMUNE_AI_DEBUGGER_ACCOUNTS=f"{markers[1]}:{markers[2]}")
# The cfg build uses its own target directory so it does not invalidate the regular build.
cfg_env = dict(base_env, RUSTFLAGS=f"{os.environ.get('RUSTFLAGS', '')} --cfg masamune_ai_debugger".strip(),
               CARGO_TARGET_DIR=str(root / "target" / "release-security-cfg"))

for with_cfg in [False, True]:
    for profile in [[], ["--release"]]:
        run_only = ["--no-run"] if with_cfg else []
        result = subprocess.run(["cargo", "test", "-p", "masamune-whisker-ai-debugger", "--test", "security",
                                 "--locked", "--message-format=json", *run_only, *profile], cwd=root,
                                env=cfg_env if with_cfg else base_env,
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
        expected = with_cfg and not profile
        for executable in executables:
            data = Path(executable).read_bytes()
            for marker in markers:
                assert (marker.encode() in data) == expected, "unexpected credential embedding"
        label = f"{'release' if profile else 'debug'} {'with' if with_cfg else 'without'} the cfg"
        print(f"{label} {'includes' if expected else 'excludes'} all three credential sentinels", flush=True)
