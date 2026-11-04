# Storage Bridge desktop

Flutter Windows and macOS UI for the Storage Bridge engine.

The interface uses LiquidGlassSurface, LiquidGlassNavigationBar,
LiquidGlassButton, LiquidGlassIconButton, and LiquidGlassSwitch directly from
`packages/liquid_ui_components` via a repository-relative dependency. The
vendored MIT-licensed source and shader are included in this repository, so
no external personal checkout is required. Balanced optical rendering enables the library's stable
liquid refraction. Its accessibility fallbacks handle reduced motion and
high contrast. Keep the vendored package alongside the desktop source.

Run `flutter pub get`, `flutter test`, and `flutter build windows --release`.
Place the Rust engine beside the desktop executable as described in the root README.

The icon is in `assets/storage_bridge.png` and
`windows/runner/resources/app_icon.ico`. Regenerate both using
`python tool/generate_icon.py` (Pillow required).

For macOS use `./scripts/build-macos.sh` from the repository root as described in the root README. The Swift runner provides OAuth JSON selection, Finder access, engine launch/IPC, and menu bar controls. The packaged engine lives at `StorageBridge.app/Contents/MacOS/storage-bridge`. The desktop app is intentionally not App Sandbox enabled because it launches a separate filesystem daemon.
