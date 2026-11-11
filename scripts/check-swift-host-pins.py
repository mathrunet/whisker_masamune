"""Require standalone Swift Host SDK dependencies to match the Rust lock revision."""
from pathlib import Path
import re
import sys
import tomllib

lock = tomllib.loads(Path(sys.argv[1]).read_text())
source = next(p["source"] for p in lock["package"] if p["name"] == "whisker")
revision = source.split("#", 1)[1]
expected = f'.package(url: "https://github.com/mathrunet/whisker.git", revision: "{revision}")'
count = 0
for manifest in Path("packages").glob("*/Package.swift"):
    text = manifest.read_text()
    if re.search(r'github.com/(?:whiskerrs|mathrunet)/whisker', text):
        if expected not in text or "github.com/whiskerrs/whisker" in text:
            raise SystemExit(f"Host SDK does not match Cargo.lock: {manifest}")
        count += 1
if not count:
    raise SystemExit("No Swift Host SDK dependencies checked")
print(f"ok: {count} Swift Host SDK dependencies match {revision}")
