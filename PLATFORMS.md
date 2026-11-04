# Platform assessment — 0.4.0

Assessment performed on Windows, 2026-11-04.

The architecture supports separate Windows and macOS binaries using the same Flutter UI and Rust storage logic. A Windows executable cannot run as the Mac app. The shared quota logic is platform-neutral; each adapter translates limit errors into the OS's disk-full error.

| Area | Windows | macOS source |
| --- | --- | --- |
| Desktop UI | Flutter + C++ bridge | Same Dart UI + Swift/AppKit bridge |
| Mounted filesystem | WinFsp asynchronous callbacks | fuser + macFUSE session/ABI integration |
| Credentials | Windows DPAPI | macOS Keychain |
| Local IPC | Per-user named pipe | Private Unix socket directory and instance lock |
| Startup | User Run registry entry | User LaunchAgent |
| Engine and quota | Shared Rust code | Shared Rust code |
| Packaging | Portable bundle + Inno Setup | `.app` bundle + ZIP, local ad-hoc signature |
| Current verification | Windows tests and real WinFsp mount | Source inspection only for this update |

## Findings

- The Mac adapter is wired into `lib.rs` and `main.rs` through target-specific modules and dependencies. It calls the same `Runtime` read/write/resize methods as Windows and therefore uses the new content allowance.
- The Swift bridge invokes the bundled engine for IPC requests, imports a desktop OAuth JSON, launches the daemon, opens Finder, and provides a menu-bar entry. Keychain and LaunchAgent code exist.
- The Mac mount uses a custom libfuse3 session with a `fuser` filesystem. Dynamic library path, symbols, ABI behavior, mount permissions, and driver approval must be tested on the intended macFUSE/macOS versions.
- Windows' asynchronous content/directory queues are specific to its adapter. The Mac adapter services callbacks through a `fuser` session; equivalent Finder responsiveness under slow network operations has not been established.
- The Mac build script previously had Rust tests and Clippy commented out. Both are restored in 0.4.0. A green Windows suite does not execute target-gated Mac tests (`macos_platform.rs` reports zero tests on Windows).
- Local ad-hoc signing is for development. The script does not provide a signed/notarized distribution release or establish Apple Silicon/Intel parity.

## What can be claimed

**Windows: built and tested locally. macOS: an implementation exists and is structurally connected, but operation of this update is unverified.**

This Windows host cannot compile/link the Mac app with Xcode or validate macFUSE, Finder, Keychain prompts, startup, and browser consent. Source portability alone is insufficient to declare Mac support.

## Mac acceptance

On a Mac with the required Rust/Flutter/Xcode/CocoaPods toolchains and compatible macFUSE/libfuse/pkg-config setup:

1. Run `./scripts/build-macos.sh` with Rust tests and Clippy enabled.
2. Run `./scripts/smoke-mount-macos.sh` on disposable local demo data.
3. Exercise Finder enumeration, read/write, seek, replacement saves, folder operations, and unmount.
4. Confirm quota-boundary errors preserve accepted bytes and that pending writes survive daemon termination/restart.
5. Verify Google OAuth/Keychain, ranged reads, uploads, polling, reconnection, and startup on disposable Google data.
6. Test slow/disconnected networking and actual editor workflows before asserting parity with Windows.

Repeat on each Mac architecture and OS/driver combination intended for release. This is the remaining platform acceptance work, not a list of tests already passed.
