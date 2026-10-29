# Windows preview validation

## Repository publication checks

Before the initial source publication on 2026-10-29:

- Replaced the desktop's personal absolute UI dependency path with the identical vendored package at `desktop/packages/liquid_ui_components`; retained its MIT license and shader source.
- Resolved Flutter dependencies using that relative path. Static analysis and both widget tests passed.
- Reran all 48 Rust tests, strict Clippy, and formatting checks successfully in the normal Windows context. The restricted sandbox could not run loopback HTTP fixtures; the normal-context rerun passed all fixtures.
- Reviewed the staged source for credential material and accidental runtime/build outputs. OAuth client files, protected credentials, databases, toolchains, generated builds, and release binaries are excluded from the source commit.
- Expanded README setup, architecture, storage semantics, build/test instructions, troubleshooting, and current limitations. This documentation update does not add new macOS or concurrent-editing guarantees.

## Streaming / hybrid update 0.3.0

- 48 Rust tests passed, including an 8 GiB simulated-file random-read test, bounded RAM cache, stale metadata refresh on first access, per-open read-version pinning, partial overlays, truncation, restart recovery, and metadata-lock availability during blocked reads/uploads.
- Both Flutter widget tests, static analysis, and strict Clippy checks passed.
- Actual WinFsp network-drive test passed with asynchronous callbacks, streamed reads after eviction, partial edits, 17 MiB SHA-256 verification, replacement saves, rename, delete, unmount, and zero file-content disk cache after reads. The isolated test ran alongside the existing Google mount without touching it.
- Installed and activated the update in the normal Windows user context; account and scoped folder configuration were preserved, with online-only cleanup enabled.
- Live Google verification: read 64 bytes from an existing 5,986,326-byte file; created one uniquely named `storagebridge-streaming-check-*.txt` verification file; confirmed a cold one-byte hybrid upload and mounted streamed readback. No existing user file was modified.
- A live test exposed delayed Google metadata-version updates. First-access refresh and version-pinning fixes were added and the mounted write/readback verification then passed.
- Status at completion of the live verification: connected, G: mounted, streaming true, online-only true, pending uploads 0, disk file-cache bytes 0, RAM read-cache bytes 84, no reported error. This is a recorded snapshot; later user activity changes RAM and staging counters.

Read data still transfers over the network into RAM. Pending edits use a durable local journal; Google requires full updated content to be sent during upload, with untouched content assembled through RAM. Concurrent conflicting partial edits require manual recovery. macOS remains deferred.

The earlier validation sections below describe historical builds.

## Online-only update 0.2.0

- 33 Rust tests passed, including cleanup after close/upload, protection for open handles and pending metadata jobs, interrupted uploads, restart recovery, mode persistence, and returning to budgeted caching.
- Both Flutter widget tests passed; the online-only toggle sends the IPC setting, disables cache-budget controls, and updates the temporary-storage display.
- Clippy and Flutter analysis passed.
- The installer is built in `release/StorageBridge-Setup-0.2.0-x64.exe`; a separate portable bundle is in `release/StorageBridge-0.2.0`.
- Activating this update on the running Google mount was deferred: the engine refused unmount because files were still open. The running mount and pending data were left untouched. Close drive files before upgrading. Live online-only cleanup against Google has not yet been verified.

## Initial 0.1.0 validation

Validated on Windows x64 on 2026-10-29.

| Check | Result |
| --- | --- |
| Rust unit/integration tests | 25 passed |
| Clippy, all Rust targets, warnings denied | Passed |
| Rust formatting | Passed |
| Flutter static analysis | No issues |
| Flutter widget test | Passed |
| Release build: Rust engine and Flutter/C++ UI | Passed |
| Actual WinFsp mounted-drive smoke test | Passed |
| 17 MiB + 123 byte read/write SHA-256 check | Passed |
| Empty file and temporary-file replacement | Passed |
| Silent install, installed engine diagnostics, version metadata | Passed |
| Test-copy uninstall | Passed |
| Normal-user Flutter → C++ → named pipe → Rust status | Verified in running UI |

The running Google-mode engine reports `scope_folder: StorageBridge`, `connected: false`, `configured: false`, `mounted: false`, `pending: 0`, and WinFsp installed. The UI displays `Mounted folder: My Drive / StorageBridge` and is ready for importing the user's desktop OAuth client.

Tests cover durable queued edits, offline restart/retry, interrupted download cleanup, eviction protection, remote cache invalidation, conflicts, replacement ordering, repeated-save recovery, Drive pagination, quota errors, resumable upload offsets, duplicate scope folders, and preventing writes to items moved outside the scope.

The mounted-drive test uses the isolated local remote, not a Google account. It performs real Windows filesystem operations for create, read, write, replacement, rename, enumeration, deletion, and unmount. It does not claim validation in Office or other specific editors.

Still requiring user configuration/manual validation: live Google browser consent, token refresh against Google, uploads/downloads in My Drive/StorageBridge, real Google quota/revocation behavior, startup reconnect with authenticated credentials, power-loss/disk-exhaustion testing on a disposable volume, cross-device synchronization, and macOS integration. Version checks preserve detected conflicts but do not provide a distributed lock for concurrent editors.
