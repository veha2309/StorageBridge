# Continuous integration

`workflows/ci.yml` runs on every branch/tag push, pull request, merge-queue group, and manual dispatch. There are no path filters. A push event checks its resulting revision; a multi-commit push does not separately test every intermediate ancestor. Push runs have unique concurrency groups so newer pushes cannot cancel queued revisions. Superseded PR runs are cancelled so the latest PR revision is checked.

## Automatic checks

| Job | Checks |
| --- | --- |
| Workflow and repository checks | actionlint 1.7.12, Bash syntax, consistent engine/desktop/installer version |
| Windows x64 | Rust formatting/tests/Clippy, Dart formatting, locked Flutter dependencies, analysis/widget tests/coverage, release app + installer build, real WinFsp mount/quota smoke test, silent install/uninstall |
| macOS Apple Silicon | Rust formatting/tests/Clippy including target-gated platform tests, Dart formatting, locked Flutter dependencies, analysis/widget tests/coverage, app + Rust engine build, ad-hoc signing and diagnostics |
| macOS Intel | Same checks on an Intel runner |
| Required checks | Fails unless the workflow job and every desktop matrix job succeed; failed/cancelled/skipped prerequisites do not produce a passing gate |

The SDK is pinned to Flutter 3.47.5. Rust uses the stable channel and records its version through setup logs. Third-party Actions are pinned to verified revisions. Rust and Flutter dependency caches accelerate subsequent runs; Rust caches are saved only by push runs. Artifacts include Rust test logs, LCOV coverage, installer logs when present, and successfully tested Windows/Mac bundles, retained for seven days.

The Windows prerequisite helper installs libclang, the pinned signed WinFsp release, and Inno Setup if absent. The installer smoke test reads its version from the installer source and refuses to run in an account with an existing Storage Bridge installation. Use disposable CI accounts for installation tests.

The Mac helper installs the macFUSE user-space SDK for compilation/linking. The build script selects Flutter's supported arm64 target on Apple Silicon (rather than Xcode's possible arm64e native subtype) and verifies that the app and bundled Rust engine contain the runner's architecture. Hosted checks do not approve or load the driver and do not establish Finder/native mounting acceptance. Tests use fake HTTP responses and private demo remotes. No Google OAuth credentials, account secrets, release-signing certificates, or personal data are supplied to PR code. Mac build artifacts have local ad-hoc signatures and are not notarized distribution releases.

## Require checks before merging

After the first successful run, add **Required checks** as a required status check in the GitHub ruleset/branch-protection configuration for `main` and other protected branches. The workflow provides this stable gate name; adding a workflow alone does not enable branch protection. Keep the merge-group trigger enabled if using a merge queue.

CI runs pull requests in the ordinary `pull_request` context, with read-only repository permissions and checkout credentials disabled. It does not execute PR code through `pull_request_target` or send reports/comments to other users.

## Native Mac acceptance

`workflows/macos-native.yml` is manual only. Configure a **dedicated** Mac test account/runner with labels `self-hosted`, `macOS`, and `storagebridge-fuse`. Install compatible macFUSE/libfuse, approve the driver, and provide Rust, Flutter, Xcode/CocoaPods, and pkg-config. Keep personal mounts and files out of that account.

Dispatch it against reviewed repository code and explicitly confirm the dedicated-runner checkbox. It builds and tests the app, then runs the mounted smoke test covering Finder/CoreServices visibility, file operations, the enforced content allowance, ENOSPC, and preservation/recovery. It is deliberately outside the required hosted gate and never runs automatically for fork pull requests. No dedicated runner has been provisioned by these files.

Browser consent, real Google quota/revocation handling, power loss, and application-specific workflows still need separate acceptance with disposable data. Passing hosted Mac builds is stronger evidence than source inspection but does not prove actual macFUSE mounting or all Mac configurations.

## Local checks

Run the same Rust/Flutter commands listed in the root README. Validate workflow syntax with `actionlint`; `.github/actionlint.yaml` recognizes the dedicated Mac runner label. Run `python .github/scripts/check-versions.py` after version changes. The Windows/Mac build scripts skip already completed checks only when explicitly passed `-SkipTests` / `--skip-tests` by CI.
