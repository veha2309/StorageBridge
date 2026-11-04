"""Fail CI when the engine, desktop, and installer disagree on the app version."""
from pathlib import Path
import re

root = Path(__file__).resolve().parents[2]


def version(path, pattern):
    match = re.search(pattern, (root / path).read_text(encoding="utf-8"), re.MULTILINE)
    if not match:
        raise SystemExit(f"Cannot read version from {path}")
    return match.group(1)


versions = {
    "engine": version("engine/Cargo.toml", r'^version = "([^"\n]+)"$'),
    "desktop": version("desktop/pubspec.yaml", r"^version: ([^+\s]+)(?:\+\S+)?$"),
    "installer": version("installer/storage-bridge.iss", r'^#define AppVersion "([^"\n]+)"$'),
}
if len(set(versions.values())) != 1:
    raise SystemExit(f"Application versions disagree: {versions}")
print(f"Application versions agree: {versions['engine']}")
