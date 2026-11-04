# Storage Bridge

**Your cloud. Your local storage budget.**

Use Google Drive as a drive with streaming reads, durable background writes, and an enforced allowance for local file content. Open cloud files larger than your allowance without creating full read copies on disk. See exactly which pending writes and retained copies use local space.

Storage Bridge is a Windows-first personal cloud-drive preview built with Flutter/Dart, Rust, SQLite, and WinFsp. Mount **My Drive/StorageBridge** as a drive letter (default `G:`), browse in File Explorer, and open or edit files with normal Windows applications. The custom engine talks directly to Google Drive; it needs no rclone installation or hosted backend.

**Current version: 0.4.0. Windows x64 validated; macOS adapter implemented but unverified here.** macOS uses macFUSE, Keychain, a Unix socket, and a native menu bar bridge. Live macOS mounting and Google sign-in still require validation. This is an experimental filesystem preview, not a claim of production-ready or simultaneous multi-device editing support. See [platform assessment](PLATFORMS.md).

## Features

- Browser-based Google desktop OAuth with PKCE and a loopback callback; one account and a scoped My Drive folder.
- Metadata-only directory browsing, stable Drive file identities, and persisted Windows-safe aliases.
- HTTP range reads in 1 MiB blocks through a bounded 32 MiB RAM cache, without whole-file read copies on disk.
- Direct source-backed uploads larger than the managed allowance, with durable retries and provider SHA-256 verification; Windows can remove the original after verification without a second local content copy.
- A dedicated read-only SQLite query tab using version-pinned cloud range reads, without a local database copy.
- Durable local staging for new content and changed-byte journals for edits to uncached files.
- Resumable uploads, persisted pending operations, retries, remote change polling, and explicit upload errors.
- Online-only cleanup, adjustable retained-file cache budget, and separate RAM/disk usage counters.
- Enforced local content allowance (default **1 GiB**), per-file space explanations, safe cleanup, and a copyable storage report that excludes filenames and account details.
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

## Run on Windows

Build the project using the instructions below, then open `release/StorageBridge/storage_bridge.exe` or install `release/StorageBridge-Setup-0.4.0-x64.exe`. Compiled applications, installers, and toolchains are excluded from Git. WinFsp must be installed to mount a drive; the app checks for it. Build targets Windows x64.

1. Click **Start engine**, then **Import OAuth client**.
2. Import your downloaded Google **Desktop app** OAuth client JSON.
3. Click **Sign in with Google** and approve access in your browser.
4. Select an unused drive letter (default `G:`) and click **Mount drive**.
5. Open File Explorer. The drive displays the contents of **My Drive/StorageBridge**. Transfers and Activity distinguish local saves from completed uploads.

The engine locates the exact `StorageBridge` folder directly inside My Drive and creates it if missing. Duplicate folders with that name block connection until you rename the duplicates. Once selected, the folder is remembered by its Drive ID. If it is removed, restore it rather than silently reconnecting to a different folder. Files outside this folder are not exposed by the mount. Changes outside its subtree are excluded from the local index, and pending writes to files moved outside it are blocked. The OAuth grant still requests Drive access; folder scoping is enforced by the application.

To test without Google credentials, click **Try local test drive** before starting a normal engine. Demo data is isolated from Google data. Stop the engine in Settings before switching modes.

Closing the window hides it to the tray. Double-click the tray icon to reopen it. Quitting the desktop UI leaves the engine running. **Settings → Stop background engine** requires closed file handles and an empty upload queue. Startup reconnect is opt-in and runs at user sign-in, without administrator privileges.

## Local storage control (0.4.0)

**Settings → Local content limit** controls a hard allowance for the sum of managed content-file lengths, including new files, changed-byte journals, retained complete copies, and temporary legacy downloads. The default is 1 GiB; the UI accepts 64 MiB–16 TiB. It applies in both online-only and retained-cache modes, across all files together. Growth checks and disk writes share a lock, including conflict-copy duplication by background workers.

Before accepting growth, the engine removes eligible clean, closed copies to make room. If protected content still leaves too little space, it returns a disk-full error to the writing application **before that write request changes content or pending intent**. Earlier successful requests remain staged. A multi-request copy can therefore leave an accepted partial destination when a later request fails. Keep the source and retry after making room. Existing edits are never deleted to satisfy the allowance.

The Overview shows pending, retained, and recovery bytes, with **Why space is used** listing up to 100 files (totals cover every file). **Free completed copies** removes only eligible clean copies. **Copy storage report** copies an aggregate report without filenames, account details, OAuth credentials, or tokens. See [support guide](SUPPORT.md).

Lowering the limit below protected content is refused. Older installations may already have more pending content than the new default; recovery preserves it, reports the excess, and blocks additional growth until uploads or cleanup free space. In-place overwrites that require no growth remain possible. Partial journals are append-only until upload, so repeated changes can use more journal bytes than the final changed region.

This allowance measures logical content-file lengths, conservatively including sparse-file lengths. It is not an exact physical disk-allocation cap: filesystem clusters/metadata, SQLite, credentials, binaries, paging, and other applications are outside it. Pending mounted-file uploads wait for writable handles to close. Explorer/Finder copies and arbitrary application writes still require enough staging allowance. Use the separate direct-transfer workflow below to upload an existing local file larger than that allowance.

## Direct upload and verified move (0.4.0)

For example, upload an existing **3 GB file with a 1 GiB managed allowance** through **Transfers → Choose local file**, select an existing destination folder relative to the mounted root (blank means root), and choose **Upload directly**. The engine reads the original in bounded chunks and uses Google resumable uploads; it never creates a second content copy in its cache. Source bytes already on your disk remain outside the managed allowance and are shown separately in Transfers. Hashing reads the whole original locally; uploading still sends its complete contents over the network.

On Windows, enable **Move after verified upload** to remove the original only after Google reports the expected file size and matching SHA-256 checksum, and a final cloud revision check succeeds. A Windows file-handle lease prevents writers and path replacement during transfer; deletion targets that same handle. Keep the original in place and close it in other applications until completion. The app persists source identity, size, modification time, checksum, remote ID, and transfer state, so it can resume after restart and recognize an upload whose final acknowledgement was lost. If the source changed or was replaced, it is not deleted. Missing checksums, network errors, or mismatches retain it for retry. **Dismiss** stops a failed/inactive job without deleting its source or uploaded cloud file.

Automatic source removal is currently **Windows-only**; macOS direct upload retains the source. This is a dedicated app action: **Explorer Cut/Paste and Finder moves still use normal filesystem staging**, and can hit the allowance. Direct transfer accepts one regular local file at a time, queues up to 32 jobs, refuses occupied destination names, and does not recursively upload folders. It does not provide a zero-disk guarantee for the OS, other applications, metadata, or paging.

## Read-only cloud SQLite (0.4.0)

Use a closed, standalone SQLite backup in **DELETE journal mode**, without `-wal`, `-shm`, or `-journal` sidecars. Upload it with the direct-transfer controls or place it in `My Drive/StorageBridge`. In **SQLite**, enter its path relative to the mounted root (for example `backups/data.sqlite`) and run an indexed query such as `SELECT name FROM items WHERE id=427`. This reader uses a custom read-only SQLite VFS over the shared 1 MiB range cache. Needed pages stream into RAM; no database file is materialized on disk. SQLite also uses its own working memory, separate from the 32 MiB range cache.

The reader pins a cloud revision and discards results if it changes. Writes, ATTACH, user PRAGMAs, extension loading, recursive queries, temporary sorts, and materialized query plans are refused. Results are capped at 200 rows and 256 KiB, text cells at 1024 characters, and blobs display their byte length. SQL is capped at 16 KiB and values at 1 MiB. A 30-second progress/deadline check bounds execution between callbacks; an in-flight network request can take longer. Indexed queries can read a small portion of a large file; scans and aggregate queries may transfer every page. These restrictions intentionally exclude some otherwise valid SELECT statements.

This feature does **not** make a mounted SQLite file safe for live database writes or multi-device transactions. Export a fresh standalone backup when you need new data. The constraints follow SQLite's [immutable database requirements](https://sqlite.org/uri.html#uriimmutable) and [WAL limitations](https://sqlite.org/wal.html). Google supplies binary-file checksums via its [file metadata API](https://developers.google.com/workspace/drive/api/reference/rest/v3/files).

## Streaming reads and hybrid writes

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

GitHub Actions runs checks for every push and pull request, including Windows x64 and macOS Apple Silicon/Intel builds, Rust/Flutter tests, formatting/lint, and real Windows mount/installer smoke tests. The stable **Required checks** gate combines every hosted job. Native Mac mounting is a separate manual acceptance workflow on a dedicated Mac. See [CI configuration and branch-protection setup](.github/CI.md).

Prerequisites:

- Windows x64; Rust stable **1.88+** (edition 2024 and let chains).
- Flutter stable with Dart 3.12.2+ and Windows desktop artifacts.
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
| Windows installer | `release/StorageBridge-Setup-0.4.0-x64.exe` |

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

The 0.4.0 checks cover 67 Rust tests on Windows and five Flutter widget tests, including storage rejection, source-backed transfers, checksum/restart recovery, read-only SQLite ranges, concurrent writes, and report privacy. The mounted Windows smoke test includes a 64 MiB allowance, actual disk-full rejection/recovery, a verified 65 MiB direct move without managed staging growth, and SQLite query IPC. Limited live Google read/write verification was previously completed on 0.3.0; these new feature tests use isolated local remotes. See [VALIDATION.md](VALIDATION.md) for recorded evidence and remaining manual validation.

## macOS build and run

The macOS adapter targets macOS 12+ on Apple Silicon or Intel, built natively for the current machine. Install Xcode and its command-line tools, Rust 1.88+, Flutter with Dart 3.12.2+, [macFUSE 5.4+](https://macfuse.github.io/), and `pkg-config` (for example, `brew install pkgconf`). Follow macFUSE's installation instructions and approve its filesystem extension when required. The macFUSE driver is installed separately; the app does not install it or change system security settings.

From the repository root:

```bash
./scripts/build-macos.sh
open release/StorageBridge.app
```

This runs Rust and Flutter checks, builds the engine and app, places the engine in `Contents/MacOS`, and creates `release/StorageBridge-macos-arm64.zip` or `StorageBridge-macos-x86_64.zip`. The app has a local ad-hoc signature; public distribution still needs Developer ID signing and notarization. Move the app to its permanent location before enabling startup reconnect.

Use **Start engine**, **Import OAuth client**, and **Sign in with Google**, then **Mount drive**. The mount is fixed at `~/StorageBridge`; it must be an empty real directory and cannot already be mounted. It uses macFUSE's `local` option to appear as **Storage Bridge** in Finder; use macFUSE 5.5+ for its Finder integration fixes and enable **External disks** in Finder Settings if hidden. **Open Finder** opens that folder. Close the window to hide it, then reopen it from the menu bar or Dock. Quitting the UI leaves the engine running; use Settings to stop the engine after uploads complete. Startup reconnect writes `~/Library/LaunchAgents/dev.storagebridge.engine.plist` for the next login.

Credentials and upload-session secrets are stored in the macOS login Keychain. Existing `.dpapi` filenames are retained as nonsecret Keychain marker files. Metadata and pending writes live under `~/Library/Application Support/dev.StorageBridge.StorageBridge/google` (or `demo`), protected by a user-only directory. Preserve this directory while uploads remain. The framed JSON protocol is unchanged, transported over a user-only `/tmp/storagebridge-<uid>/v1.sock` socket. `filesystem_available` and `platform` are included in status; `winfsp_installed` remains a compatibility alias for driver availability. macOS ignores the mount request's Windows `letter` parameter.

Diagnostics and an isolated mounted filesystem smoke test:

```bash
target/debug/storage-bridge --diagnose
target/debug/storage-bridge --request '{"version":1,"method":"status"}'
./scripts/smoke-mount-macos.sh
# Or pass release/StorageBridge.app/Contents/MacOS/storage-bridge
```

The smoke test uses a separate local demo instance and no Google credentials. It exercises a 17 MiB streamed read, partial writes, flush, file replacement, rename, delete, and unmount. It shares the fixed mount folder, so unmount any normal instance first. Native extended attributes, symlinks, arbitrary POSIX permissions, Finder cloud badges, and File Provider integration are not implemented. macOS retains the shared engine's Windows-safe filename aliases.

Rust tests and Flutter checks can run without activating the macFUSE extension, but linking the engine requires its libraries. Live mounting, Finder save behavior, Keychain consent, Google authentication, and login reconnect need manual verification before using important files. See [VALIDATION.md](VALIDATION.md).

## Storage semantics

- Browsing uses metadata only. Uncached reads stream requested ranges into RAM. Pending edits and retained complete local writes can be read offline; other reads require internet unless their ranges happen to remain in RAM. New files/folders require a connection to reserve a Drive ID.
- Writes are journaled before modification and flushed locally before acknowledgement. Uploads wait until file handles close. SQLite retains pending work across crashes; resumable session URLs and credentials use Windows DPAPI.
- The optional retained-clean-cache target defaults to **10 GiB** and is bounded by the separate **1 GiB** default content allowance. The retention target is a cleanup preference; the content allowance checks growth before writes. Dirty/open files are protected, and writes requiring space beyond the allowance are refused.
- Remote changes are polled every 30 seconds. When full local content exists, a version change detected before upload preserves a separate conflict copy. When only a partial journal exists, conflicting uploads stop and retain the journal for manual recovery. There is no distributed lock or guarantee against edits occurring after that check. Use one device at a time and wait for uploads before switching.
- Renames preserve Drive IDs. Replacing a destination stages the new version first, then trashes the replaced remote file. Deleting an item moves it to Google Drive Trash; nonempty directories must have their contents removed first.
- Remote duplicate, reserved, case-colliding, or invalid Windows names receive stable persisted aliases. Aliasing alone never renames remote files.
- Google Docs/Sheets/Slides, shortcuts, shared drives, symlinks, arbitrary ACL changes, offline pinning, and Explorer status overlays are excluded.

Live Google sign-in and a limited mounted read/write path have been verified during development. Exhaustive real-service quota/revocation handling, power loss, local disk exhaustion, authenticated startup reconnect, application-specific save patterns, and cross-device operation still need manual validation before relying on this preview for important files. Automated fixtures and local-remote tests do not establish those guarantees.

## Confirming local disk use

Explorer shows the cloud file's logical size and an allocation value supplied by the virtual filesystem. **The mounted drive's “Size on disk” field does not measure the content cache on C:.**

The app separately reports **Your local storage budget** (managed content) and **Read buffer (RAM)**. A full 32 MiB read buffer replaces older blocks as new ones arrive; it does not grow into a full local copy of a large video. Inspect this real local directory to verify content staging:

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
| Disk-full error despite free space on C: | Check the local content allowance in Overview. Close handles, wait for uploads, free completed copies, or raise the allowance before retrying. |

The retention target and enforced content allowance are separate controls. A locally saved file is not yet confirmed in Google Drive until its upload completes. See [product direction](PRODUCT.md) for the intended customer, positioning, and support model.

## Source layout and IPC

- `desktop/`: Flutter UI, C++ Windows named-pipe/tray bridge, and Swift macOS menu bar bridge.
- `desktop/packages/liquid_ui_components/`: vendored UI package, shaders, and license.
- `engine/src/`: Rust engine, Google REST client, OAuth, SQLite store, and cache.
- `engine/src/platform/`: Windows filesystem, DPAPI, startup registration, and IPC; macOS counterparts under `macos/`.
- `engine/tests/`: offline, scope, recovery, streaming, and cache behavior tests.
- `scripts/`: build, IPC diagnostics, and mounted-filesystem smoke test.
- `installer/`: Inno Setup Windows installer source.

The engine's per-user named pipe is `\\.\pipe\StorageBridge-v1-<Windows SID>`. Its protected ACL allows the current user and SYSTEM; remote pipe clients are rejected. Frames are a four-byte little-endian JSON byte length followed by UTF-8 JSON, limited to 1 MiB.

```json
{"version":1,"method":"mount","params":{"letter":"G:"}}
```

Responses contain `version`, `ok`, and either `result` or `error`. Methods are `status`, `configure`, `login`, `mount`, `unmount`, `settings`, `release_cache`, `retry`, `import_file`, `dismiss_import`, `sqlite_query`, and `shutdown`. Status includes queued transfers, active byte progress, cache usage, account/quota metadata, mount state, and recent events. `settings.content_limit` accepts an integer byte allowance. `status.storage` contains allowance, used/available bytes, pending/retained/recovery totals, blocked/over-limit flags, and a bounded per-file explanation. Flutter polls status every two seconds; remote changes are independently polled by the engine.

`import_file` accepts `source_path`, a relative destination `folder` (blank for root), and explicit `move_source: true` for Windows verified removal; it returns a durable job ID. `dismiss_import` accepts that `id` and refuses active transfers. `status.direct_uploads` reports jobs and pending original bytes separately from cache usage; capability flags let older UI/engine combinations degrade safely. `sqlite_query` accepts a relative cloud `path` and read-only `sql`, starts a worker and returns immediately. Poll `status.sqlite_query` for `running`, `complete` with bounded results, or `error`. SQLite result data and original source paths are excluded from the aggregate storage report.

Diagnostics from PowerShell:

```powershell
. .\scripts\ipc.ps1
Invoke-Bridge status | ConvertTo-Json -Depth 8
Invoke-Bridge retry
```

State is under `%LOCALAPPDATA%\StorageBridge\StorageBridge\data\google` (or `demo`), as resolved by the Rust `directories` crate. Keep the database and cache together. Uninstall preserves this state and pending files; never delete it while uploads remain.

The Rust `Remote` trait separates provider behavior from filesystem operations. Windows and macOS adapters reuse the same OAuth, Drive, cache, streaming, and durable queue implementation.

## License

GPL-3.0-only, including the obligations of the GPL WinFsp Rust bindings. See `LICENSE` and `THIRD_PARTY_NOTICES.md`. This project has not been prepared for commercial distribution.

Storage Bridge is independent software and is not affiliated with Google, Microsoft, or Apple. Existing Google Docs/Sheets/Slides, shortcuts, shared drives, pinned offline folders, native cloud-status overlays, and account switching remain outside the current MVP. macOS mounting is implemented but awaits live filesystem validation.
