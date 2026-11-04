# Storage Bridge product direction

**Positioning: Your cloud. Your local storage budget.**

The initial customer has a small SSD, a larger Google Drive library, and wants to browse and use it through ordinary desktop applications without unexplained local copies. The first validated platform is Windows. macOS support must pass its own build and runtime acceptance checks.

## What the product promises

- Stream cloud reads through bounded RAM without full read copies on disk.
- Set one enforced allowance for Storage Bridge's managed file-content lengths.
- Know which files consume that allowance and whether they are awaiting upload, open, retained for reuse, or kept for recovery.
- Preserve previously accepted edits when a later write would exceed the allowance.
- Copy an aggregate diagnostic report and follow a documented recovery process.

The allowance is not an absolute cap on every byte the OS or other apps write. Large new-file copies require sufficient staging allowance. Internet speed, provider limits, application-generated temporary files, filesystem allocation overhead, and OS paging remain separate constraints.

## A demonstration that can be measured

1. Set a small content allowance and open a much larger cloud file.
2. Seek/read different regions while the RAM buffer remains bounded and content disk usage remains unchanged.
3. Write new local content up to the allowance.
4. Attempt further growth: observe a clear failure and verify accepted bytes are intact.
5. Close the file, finish its upload, and verify space is released in online-only mode.

Automated tests simulate an 8 GiB remote read workload. The Windows mounted smoke test exercises 17 MiB checksum reads and the 64 MiB allowance boundary. These are different tests; neither is a measured commercial benchmark with a real 50 GiB video.

## Competitive differentiation

Cloud-drive mounting and streaming are established capabilities. The intended differentiation is predictable local content use, comprehensible accounting, and recovery support in one straightforward workflow. Avoid claims such as "first", "only", "zero disk", "no downloads", "unlimited writes", or superiority over Drive for desktop without comparative evidence.

Competitor documentation provides comparison starting points: [RaiDrive cache behavior](https://docs.raidrive.com/gui/settings/cache/), [rclone VFS cache modes and limits](https://rclone.org/commands/rclone_mount/), and [Drive for desktop streaming](https://support.google.com/drive/answer/13401938). Features and plans can change; publish dated, reproducible comparisons rather than permanent exclusivity claims.

## Product and service model

The software uses the customer's Google account and OAuth client. Files go directly between their device and Google Drive. There is no Storage Bridge content-hosting service, and no additional cloud capacity is sold.

A possible service offering is assisted onboarding, configuration checks, updates, and guided diagnosis using the aggregate storage report. The support guide and issue template provide the foundation; no subscription, billing system, remote-support access, hosted support portal, or service-level commitment has been launched.

Do not request refresh tokens, OAuth client JSON, complete metadata databases, or file contents in public support reports. Ask for redacted reproduction steps and the aggregate report first. Recovering unresolved partial-write conflicts remains manual and must preserve the original staging data.

## Acceptance before broader distribution

- Validate quota rejection and retry with real editors, media tools, and Explorer copy workflows.
- Run real Google quota, revocation, network-interruption, low-disk, and crash/power-loss exercises on disposable data.
- Validate the Mac build, its macFUSE ABI integration, Finder behavior, Keychain, startup, and Google consent on actual Macs.
- Measure application responsiveness and bandwidth against specific competitor versions before making performance claims.
- Provide release signing/notarization and a simple OAuth onboarding strategy for distribution.

The 0.4.0 build delivers the enforced allowance and visibility foundation. It does not turn experimental filesystem behavior into a reliability or data-recovery guarantee.
