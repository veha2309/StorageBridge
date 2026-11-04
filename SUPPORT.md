# Storage and recovery support

## A write reports disk full

Check **Overview → Your local storage budget**. This limit is separate from free physical disk space and Google Drive quota.

1. Keep the source of any failed copy. A large operation consists of multiple write requests; earlier requests can succeed before a later one reaches the limit.
2. Close files on the mounted drive so pending uploads can proceed.
3. Wait for upload confirmation. "Saved locally" does not mean uploaded.
4. Use **Free completed copies** to remove eligible clean, closed copies. Pending or open data is protected.
5. If necessary, increase **Settings → Local content limit**, then retry the failed operation from its source.

Do not delete the cache or database to clear an error. If the physical disk is also full, free unrelated space first so the application and its metadata store can operate.

## A large original exceeds the allowance

Use **Transfers → Choose local file → Upload directly** to stream the existing original without a second managed content copy. For a verified Windows move, enable **Move after verified upload**. Explorer Cut/Paste still stages the destination and may hit the allowance. The original remains on disk until verification; do not edit, rename, or remove it during a transfer. Only regular local files and existing destination folders are supported.

If verification is unavailable or fails, the original is retained. Retry after networking/authentication recovers. An upload may already exist in Drive; retries verify its saved identity instead of creating another file. Changed/replaced originals are not automatically deleted. Dismissing an inactive failed transfer retains both its source and any uploaded file. Windows holds a read/delete lease during a move; close other apps first. macOS direct uploads retain their source.

## A SQLite snapshot cannot be queried

Export a standalone backup in DELETE journal mode and close the producing application. Local or cloud WAL/SHM/journal sidecars block the supported workflow. Use the app's SQLite tab for read-only queries; avoid live database editing through the mounted drive. Prefer indexed lookups. Full scans can read the entire cloud file; temporary sorts, ATTACH, PRAGMAs, recursion, and writes are refused. Values above 1 MiB may fail, while returned text and result sets are deliberately capped. If the cloud revision changes, retry against a stable snapshot. No local database content copy is created; SQLite working memory and OS paging remain separate.

## Explain the space

**Why space is used** lists the 100 largest managed content files. Totals include all files. Pending journals can contain overwritten patch history until upload. Retained copies may remain because online-only cleanup is disabled or an application still holds a file open. Recovery data is included in the allowance even if it lacks a normal indexed file entry.

On Windows the real content directory is:

```text
%LOCALAPPDATA%\StorageBridge\StorageBridge\data\google\cache
```

The mounted file's Explorer/Finder allocation is a virtual-filesystem value, not a measurement of this directory. The counter measures content-file lengths rather than physical filesystem allocation. The RAM read-buffer counter is not the entire application's memory usage.

## Ask for help

Use **Copy storage report** on Overview. It contains app version, timestamp, streaming/cleanup mode, read-cache counters, and aggregate content-budget totals. It excludes filenames, account identifiers, tokens, OAuth client configuration, and file contents.

Include OS version, app version, the operation that failed, whether networking was disconnected, and the error message with private names redacted. File the report using the repository's storage issue template. Review any additional logs before sharing them publicly.

If a conflict or failed upload remains, preserve the complete engine data directory. A partial journal depends on its metadata and original cloud revision. Manual recovery may require privately assessing that state; the aggregate report alone cannot reconstruct the file. There is no automatic upload of diagnostics or managed support service in this build.

## Platform status

Windows has local automated and mounted-drive validation. The Mac adapter is present in source, but the current release has not been validated on macOS from this Windows environment. See [PLATFORMS.md](PLATFORMS.md).
