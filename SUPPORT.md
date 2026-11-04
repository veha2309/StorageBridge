# Storage and recovery support

## A write reports disk full

Check **Overview → Your local storage budget**. This limit is separate from free physical disk space and Google Drive quota.

1. Keep the source of any failed copy. A large operation consists of multiple write requests; earlier requests can succeed before a later one reaches the limit.
2. Close files on the mounted drive so pending uploads can proceed.
3. Wait for upload confirmation. "Saved locally" does not mean uploaded.
4. Use **Free completed copies** to remove eligible clean, closed copies. Pending or open data is protected.
5. If necessary, increase **Settings → Local content limit**, then retry the failed operation from its source.

Do not delete the cache or database to clear an error. If the physical disk is also full, free unrelated space first so the application and its metadata store can operate.

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
