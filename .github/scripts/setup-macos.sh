#!/bin/bash
set -euo pipefail

# Installing the user-space SDK enables builds; no driver approval or mounting is attempted.
brew list --formula pkgconf >/dev/null 2>&1 || brew install pkgconf
if [[ ! -f /usr/local/lib/pkgconfig/fuse.pc ]]; then
  brew install --cask macfuse
fi
sdk_path="/usr/local/lib/pkgconfig:$(brew --prefix)/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
export PKG_CONFIG_PATH="$sdk_path"
pkg-config --modversion fuse
printf 'PKG_CONFIG_PATH=%s\n' "$sdk_path" >> "$GITHUB_ENV"
