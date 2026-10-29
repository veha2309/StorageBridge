# Storage Bridge

**Use Google Drive as a Windows drive with on-demand streaming reads and durable background writes.**

Storage Bridge is a Windows-first personal cloud-drive preview built with Flutter/Dart, Rust, SQLite, and WinFsp. Mount **My Drive/StorageBridge** as a drive letter (default `G:`), browse in File Explorer, and open or edit files with normal Windows applications. The custom engine talks directly to Google Drive; it needs no rclone installation or hosted backend.

**Current version: 0.3.0. Windows x64 only.** The Rust core separates provider and platform code for future reuse, but macOS integration and testing are deferred. This is an experimental filesystem preview, not a claim of production-ready or simultaneous multi-device editing support.

## Features

- Browser-based Google desktop OAuth with PKCE and a loopback callback; one account and a scoped My Drive folder.
- Metadata-only directory browsing, stable Drive file identities, and persisted Windows-safe aliases.
- HTTP range reads in 1 MiB blocks through a bounded 32 MiB RAM cache, without whole-file read copies on disk.
- Durable local staging for new content and changed-byte journals for edits to uncached files.
- Resumable uploads, persisted pending operations, retries, remote change polling, and explicit upload errors.
- Online-only cleanup, adjustable retained-file cache budget, and separate RAM/disk usage counters.
- Tray controls, optional startup reconnect, account/storage summary, transfer progress, and Windows installer tooling.

## Architecture

```text
Flutter / Dart desktop UI
          |
  C++ tray + named-pipe bridge
          |
  Per-user, versioned local IPC
          |
Rust background engine ------------------ WinFsp ---------------- File Explorer / apps
  |             |                           G:
  |             +-- SQLite metadata + durable operation queue
  |             +-- Bounded RAM read cache
  |             +-- Disk staging for pending writes
  |             +-- Windows DPAPI credential protection
  |
Google Drive REST API: metadata, ranged reads, resumable uploads, change feed
```

The engine continues running when the UI closes to the tray. Network reads, uploads, and change polling execute without holding the shared metadata mutex during requests. WinFsp dispatches content and directory work through separate queues to keep known-folder browsing responsive.

## Run

Build the project using the instructions below, then open `release/StorageBridge/storage_bridge.exe` or install `release/StorageBridge-Setup-0.3.0-x64.exe`. Compiled applications, installers, and toolchains are excluded from Git. WinFsp must be installed to mount a drive; the app checks for it. Build targets Windows x64.

1. Click **Start engine**, then **Import OAuth client**.
2. Import your downloaded Google **Desktop app** OAuth client JSON.
3. Click **Sign in with Google** and approve access in your browser.
4. Select an unused drive letter (default `G:`) and click **Mount drive**.
5. Open File Explorer. The drive displays the contents of **My Drive/StorageBridge**. Transfers and Activity distinguish local saves from completed uploads.

The engine locates the exact `StorageBridge` folder directly inside My Drive and creates it if missing. Duplicate folders with that name block connection until you rename the duplicates. Once selected, the folder is remembered by its Drive ID. If it is removed, restore it rather than silently reconnecting to a different folder. Files outside this folder are not exposed by the mount. Changes outside its subtree are excluded from the local index, and pending writes to files moved outside it are blocked. The OAuth grant still requests Drive access; folder scoping is enforced by the application.

To test without Google credentials, click **Try local test drive** before starting a normal engine. Demo data is isolated from Google data. Stop the engine in Settings before switching modes.

Closing the window hides it to the tray. Double-click the tray icon to reopen it. Quitting the desktop UI leaves the engine running. **Settings → Stop background engine** requires closed file handles and an empty upload queue. Startup reconnect is opt-in and runs at user sign-in, without administrator privileges.

## Streaming reads and hybrid writes (0.3.0)

Reads from uncached files now use HTTP byte ranges and a **32 MiB RAM cache**, in **1 MiB blocks**, with at most four simultaneous range fetches. Reading or seeking does not create a full local file. Known folder listings are returned from SQLite immediately, with stale metadata refreshed in the background. WinFsp uses asynchronous callbacks and a separate directory queue; uploads and change polling run independently without holding the metadata mutex during network calls. The mount is reported as a network drive and files are marked as not content-indexed.

Editing an uncached file journals only changed bytes on disk. Untouched bytes are streamed through RAM when needed for reads or when assembling a resumable upload. Truncating/replacing a file does not download its old contents. New files still stage the bytes you write. Google Drive requires the completed updated file to be uploaded, so editing a large file can transfer its untouched content through RAM during upload. There is no whole-file read download stored on disk and no second assembled upload file.

On first launch of 0.3.0, **Online-only mode** is enabled to remove clean legacy copies and clean completed writes after close/upload. This preference then persists. Failed uploads and open files remain protected. Disabling it retains complete newly written files under the configured cache budget; ordinary cloud reads still use RAM. Metadata, credentials, application files, OS-managed memory paging, and pending write journals still consume local storage. Explorer's network-drive allocation figure does not measure this PC's cache; the app separately shows disk staging and the RAM read buffer.

Concurrent remote changes detected while a partial write journal is pending block upload and retain the journal for manual conflict recovery. The engine does not silently combine edits against a different cloud revision. Avoid editing the same file on multiple devices simultaneously.

## Google setup

1. Open the [Google Cloud console](https://console.cloud.google.com/) and create or select a project.
2. Enable **Google Drive API** for that project.
3. Configure Google Auth Platform / the OAuth consent screen for your personal application, and add your Google account as a test user when using a testing audience.
4. Create an OAuth client with application type **Desktop app** and download its JSON.
5. In Storage Bridge, use **Import OAuth client**, then **Sign in with Google**. Complete browser consent and return to the app.
6. Mount an unused drive letter. Use disposable files in **My Drive/StorageBridge** for initial read/write testing.

Keep the client JSON private and never commit tokens. OAuth credentials are supplied by each user; none are included in the repository. The installed application protects stored credentials with Windows DPAPI.

The engine requests `https://www.googleapis.com/auth/drive` because it must access existing My Drive files. The narrower `drive.file` scope cannot provide general access to your existing drive. Follow Google's [desktop OAuth guide](https://developers.google.com/identity/protocols/oauth2/native-app) and [Drive scope documentation](https://developers.google.com/workspace/drive/api/guides/api-specific-auth).

External OAuth apps in Testing have seven-day authorizations/refresh tokens for this scope. Use **Reconnect Google account** when required; queued edits are retained. See Google's [audience documentation](https://support.google.com/cloud/answer/15549945). Publishing and verification are separate from building this personal preview.

The cache is bound to one Google account. Reauthorization with a different account is rejected. Account switching and multiple accounts are not implemented.

## Build and test

Prerequisites:

- Windows x64; Rust stable **1.88+** (edition 2024 and let chains).
- Flutter stable with Dart 3.13.4+ and Windows desktop artifacts.
- Visual Studio 2022 with Desktop development with C++, a Windows SDK, and CMake.
- `libclang.dll` for the WinFsp Rust bindings. Set `LIBCLANG_PATH`, or install it locally with `python -m pip install --target .tools libclang`.
- [WinFsp 2.1](https://winfsp.dev/rel/) for live mounts; Inno Setup 6 for the installer.

From PowerShell at the repository root:

```powershell
git clone https://github.com/veha2309/StorageBridge.git
cd StorageBridge

# Resolve Flutter dependencies, including the included UI package.
Push-Location desktop
flutter pub get
Pop-Location

.\scripts\build-windows.ps1 -Installer
```

The UI library is vendored at `desktop/packages/liquid_ui_components` with its MIT license and shader source. Its relative dependency path works in a fresh clone; no personal external source directory is required. Rust and desktop dependency versions are recorded in `Cargo.lock` and `desktop/pubspec.lock`.

The build script runs Rust tests and Clippy, Flutter analysis and widget tests, builds both release executables, bundles the installed MSVC redistributable DLLs when available, and creates the portable ZIP and installer. If your Visual Studio edition has another path, supply the VC++ x64 runtime on the target PC or place the redistributable DLLs beside the application. To build without an installer, omit `-Installer`.

Default build outputs:

| Output | Path |
| --- | --- |
| Portable application and engine | `release/StorageBridge/` |
| Portable ZIP | `release/StorageBridge-windows-x64.zip` |
| Windows installer | `release/StorageBridge-Setup-0.3.0-x64.exe` |

Individual checks, after installing prerequisites and configuring `LIBCLANG_PATH`:

```powershell
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
Push-Location desktop
flutter analyze
flutter test
Pop-Location
```

Run the actual filesystem test with an unused drive letter (the test uses an isolated demo instance and can coexist with the Google engine):

```powershell
# After a debug engine build; defaults to an unused Z:
.\scripts\smoke-mount.ps1
# Or test the packaged release engine:
.\scripts\smoke-mount.ps1 -Engine .\release\StorageBridge\storage-bridge.exe
```

It uses only the disk-backed demo remote and exercises create/read/write, streamed reads after eviction, partial edits, a 17 MiB checksum check, temporary-file replacement, rename, listing, deletion, and unmount. It never runs against a Google account. `engine/tests/recovery.rs` covers crash recovery, cache retention, remote changes, conflicts, moves, and replacement ordering. HTTP fixtures exercise Google pagination, quota errors, and resumed upload offsets.

Version 0.3.0 has passed 48 Rust tests, two Flutter widget tests, static analysis, Clippy, actual WinFsp mounted-drive tests, and limited live Google streamed read/write verification. These are recorded results from the development machine, not a substitute for running tests on your own Windows setup. See [VALIDATION.md](VALIDATION.md) for the evidence and remaining manual validation.

## Storage semantics

- Browsing uses metadata only. Uncached reads stream requested ranges into RAM. Pending edits and retained complete local writes can be read offline; other reads require internet unless their ranges happen to remain in RAM. New files/folders require a connection to reserve a Drive ID.
- Writes are journaled before modification and flushed locally before acknowledgement. Uploads wait until file handles close. SQLite retains pending work across crashes; resumable session URLs and credentials use Windows DPAPI.
- Default cache budget is **10 GiB**. Clean, closed files are evicted in least-recently-used order. Dirty/open files can temporarily exceed the budget; disk-full errors are returned to the writing application.
- Remote changes are polled every 30 seconds. When full local content exists, a version change detected before upload preserves a separate conflict copy. When only a partial journal exists, conflicting uploads stop and retain the journal for manual recovery. There is no distributed lock or guarantee against edits occurring after that check. Use one device at a time and wait for uploads before switching.
- Renames preserve Drive IDs. Replacing a destination stages the new version first, then trashes the replaced remote file. Deleting an item moves it to Google Drive Trash; nonempty directories must have their contents removed first.
- Remote duplicate, reserved, case-colliding, or invalid Windows names receive stable persisted aliases. Aliasing alone never renames remote files.
- Google Docs/Sheets/Slides, shortcuts, shared drives, symlinks, arbitrary ACL changes, offline pinning, and Explorer status overlays are excluded.

Live Google sign-in and a limited mounted read/write path have been verified during development. Exhaustive real-service quota/revocation handling, power loss, local disk exhaustion, authenticated startup reconnect, application-specific save patterns, and cross-device operation still need manual validation before relying on this preview for important files. Automated fixtures and local-remote tests do not establish those guarantees.

## Confirming local disk use

Explorer shows the cloud file's logical size and an allocation value supplied by the virtual filesystem. **The mounted drive's “Size on disk” field does not measure the content cache on C:.**

The app separately reports **Temporary local storage** (disk staging) and **Read buffer (RAM)**. A full 32 MiB read buffer replaces older blocks as new ones arrive; it does not grow into a full local copy of a large video. Inspect this real local directory to verify content staging:

```text
%LOCALAPPDATA%\StorageBridge\StorageBridge\data\google\cache
```

With online-only cleanup enabled, no pending writes, and no retained legacy content, streamed reads leave this directory empty. New or modified content can occupy it until upload succeeds and handles close. Application binaries, SQLite metadata, credentials, and OS/application-managed caches or paging are separate from this counter. Reading still transfers bytes over the internet; “streaming” does not mean no network download.

## Troubleshooting

| Symptom | Action |
| --- | --- |
| Mount unavailable | Install WinFsp, restart the engine, and select an unused letter. |
| Google sign-in fails | Check the client is a Desktop app, Drive API is enabled, and your account is permitted by the consent configuration. |
| Authentication expires | Use Reconnect Google account; keep local pending data intact. |
| Pending uploads remain | Close applications using the file, check connectivity and quota, then inspect Transfers and retry. |
| Partial-edit conflict | Preserve the data directory and journal; manual recovery is required. Do not delete staging to clear an error. |
| Cannot stop or unmount | Close open drive files and allow pending uploads to finish. The engine protects active work. |
| Build cannot locate libclang | Point `LIBCLANG_PATH` at the directory containing `libclang.dll`. |

The cache budget is not a hard ceiling for unsent work. Dirty or open files are protected, so pending writes may exceed the configured budget. A locally saved file is not yet confirmed in Google Drive until its upload completes.

## Source layout and IPC

- `desktop/`: Flutter UI and thin C++ Windows named-pipe/tray bridge.
- `desktop/packages/liquid_ui_components/`: vendored UI package, shaders, and license.
- `engine/src/`: Rust engine, Google REST client, OAuth, SQLite store, and cache.
- `engine/src/platform/`: Windows filesystem, DPAPI, startup registration, and IPC.
- `engine/tests/`: offline, scope, recovery, streaming, and cache behavior tests.
- `scripts/`: build, IPC diagnostics, and mounted-filesystem smoke test.
- `installer/`: Inno Setup Windows installer source.

The engine's per-user named pipe is `\\.\pipe\StorageBridge-v1-<Windows SID>`. Its protected ACL allows the current user and SYSTEM; remote pipe clients are rejected. Frames are a four-byte little-endian JSON byte length followed by UTF-8 JSON, limited to 1 MiB.

```json
{"version":1,"method":"mount","params":{"letter":"G:"}}
```

Responses contain `version`, `ok`, and either `result` or `error`. Methods are `status`, `configure`, `login`, `mount`, `unmount`, `settings`, `retry`, and `shutdown`. Status includes queued transfers, active byte progress, cache usage, account/quota metadata, mount state, and recent events. Flutter polls status every two seconds; remote changes are independently polled by the engine.

Diagnostics from PowerShell:

```powershell
. .\scripts\ipc.ps1
Invoke-Bridge status | ConvertTo-Json -Depth 8
Invoke-Bridge retry
```

State is under `%LOCALAPPDATA%\StorageBridge\StorageBridge\data\google` (or `demo`), as resolved by the Rust `directories` crate. Keep the database and cache together. Uninstall preserves this state and pending files; never delete it while uploads remain.

The Rust `Remote` trait separates provider behavior from filesystem operations. A future macOS adapter can reuse OAuth/Drive/cache/queue logic; credential storage, IPC, startup, and mounting still need macOS implementations and testing on a Mac.

## License

GPL-3.0-only, including the obligations of the GPL WinFsp Rust bindings. See `LICENSE` and `THIRD_PARTY_NOTICES.md`. This project has not been prepared for commercial distribution.

Storage Bridge is independent software and is not affiliated with Google, Microsoft, or Apple. Existing Google Docs/Sheets/Slides, shortcuts, shared drives, pinned offline folders, native cloud-status overlays, account switching, and macOS mounting remain outside the current MVP.
