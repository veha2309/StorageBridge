#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
command -v pkg-config >/dev/null || { echo 'Install pkgconf (pkg-config) first.' >&2; exit 1; }
command -v cargo >/dev/null || { echo 'Install Rust 1.88+ first.' >&2; exit 1; }
[[ -d /Library/Filesystems/macfuse.fs ]] || { echo 'Install macFUSE and pkg-config first: https://macfuse.github.io/' >&2; exit 1; }
cargo fmt --all -- --check
# cargo test --locked
# cargo clippy --locked --all-targets -- -D warnings
MACOSX_DEPLOYMENT_TARGET=12.0 cargo build --locked --release
(cd desktop && flutter pub get && flutter analyze && flutter test && flutter build macos --release)
mkdir -p release
APP=release/StorageBridge.app
ditto desktop/build/macos/Build/Products/Release/StorageBridge.app "$APP"
cp target/release/storage-bridge "$APP/Contents/MacOS/storage-bridge"
codesign --force --sign - "$APP/Contents/MacOS/storage-bridge"
codesign --force --deep --sign - "$APP"
"$APP/Contents/MacOS/storage-bridge" --diagnose
ditto -c -k --sequesterRsrc --keepParent "$APP" "release/StorageBridge-macos-$(uname -m).zip"
echo "Built $APP (local ad-hoc signature; distribution requires signing and notarization)."
