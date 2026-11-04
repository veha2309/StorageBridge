#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
skip_tests=false
case "${1:-}" in
  '') ;;
  --skip-tests) skip_tests=true ;;
  *) echo 'Usage: build-macos.sh [--skip-tests]' >&2; exit 2 ;;
esac
command -v pkg-config >/dev/null || { echo 'Install pkgconf (pkg-config) first.' >&2; exit 1; }
command -v cargo >/dev/null || { echo 'Install Rust 1.88+ first.' >&2; exit 1; }
[[ -d /Library/Filesystems/macfuse.fs ]] || { echo 'Install macFUSE and pkg-config first: https://macfuse.github.io/' >&2; exit 1; }
if [[ "$skip_tests" == false ]]; then
  cargo fmt --all -- --check
  cargo test --locked
  cargo clippy --locked --all-targets -- -D warnings
fi
MACOSX_DEPLOYMENT_TARGET=12.0 cargo build --locked --release
(
  cd desktop
  flutter pub get --enforce-lockfile
  if [[ "$skip_tests" == false ]]; then
    flutter analyze --fatal-infos
    flutter test
  fi
  flutter build macos --release --no-pub
)
mkdir -p release
APP=release/StorageBridge.app
ditto desktop/build/macos/Build/Products/Release/StorageBridge.app "$APP"
cp target/release/storage-bridge "$APP/Contents/MacOS/storage-bridge"
cp README.md LICENSE THIRD_PARTY_NOTICES.md SUPPORT.md PLATFORMS.md PRODUCT.md VALIDATION.md "$APP/Contents/Resources/"
codesign --force --sign - "$APP/Contents/MacOS/storage-bridge"
codesign --force --deep --sign - "$APP"
"$APP/Contents/MacOS/storage-bridge" --diagnose
ditto -c -k --sequesterRsrc --keepParent "$APP" "release/StorageBridge-macos-$(uname -m).zip"
echo "Built $APP (local ad-hoc signature; distribution requires signing and notarization)."
