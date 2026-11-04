$ErrorActionPreference = 'Stop'
$workspace = [System.IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$versionText = Get-Content -LiteralPath (Join-Path $workspace 'installer\storage-bridge.iss') -Raw
if ($versionText -notmatch '(?m)^#define AppVersion "(\d+\.\d+\.\d+)"\s*$') { throw 'Cannot read installer version' }
$appVersion = $Matches[1]
# A smoke uninstall must never remove the registration/startup of a personal installation.
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{63AB6BD9-0D40-4F99-A6A7-226807D9B35F}_is1'
if (Test-Path -LiteralPath $uninstallKey) { throw 'Installer smoke test requires a clean account without an existing Storage Bridge installation' }
$destination = [System.IO.Path]::GetFullPath((Join-Path $workspace 'release\InstallerSmoke'))
if (-not $destination.StartsWith("$workspace\release\", [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Installer test path is outside the release directory' }
if (Test-Path -LiteralPath $destination) { throw 'InstallerSmoke already exists; choose a clean test directory before testing' }
$setup = Join-Path $workspace "release\StorageBridge-Setup-$appVersion-x64.exe"
$log = Join-Path $workspace 'release\installer-smoke.log'
$install = Start-Process -FilePath $setup -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/NOICONS',"/DIR=`"$destination`"","/LOG=`"$log`"" -WindowStyle Hidden -Wait -PassThru
if ($install.ExitCode -ne 0) { throw "Installer returned $($install.ExitCode); see $log" }
try {
    $ui = Join-Path $destination 'storage_bridge.exe'
    $engine = Join-Path $destination 'storage-bridge.exe'
    if (-not (Test-Path -LiteralPath $ui) -or -not (Test-Path -LiteralPath $engine)) { throw 'Installed executable is missing' }
    $diagnostics = (& $engine --diagnose | Out-String) | ConvertFrom-Json
    if ($diagnostics.platform -ne 'windows') { throw 'Installed engine did not run' }
    if ((Get-Item -LiteralPath $ui).VersionInfo.ProductVersion -notlike "$appVersion*") { throw 'Installed version metadata mismatch' }
    Write-Output 'PASS: silent install, installed engine startup, and version metadata'
} finally {
    $uninstaller = Join-Path $destination 'unins000.exe'
    if (Test-Path -LiteralPath $uninstaller) {
        $uninstall = Start-Process -FilePath $uninstaller -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART' -WindowStyle Hidden -Wait -PassThru
        if ($uninstall.ExitCode -ne 0) { throw "Uninstaller returned $($uninstall.ExitCode)" }
    }
}
for ($attempt = 0; $attempt -lt 20 -and (Test-Path -LiteralPath (Join-Path $destination 'storage_bridge.exe')); $attempt++) { Start-Sleep -Milliseconds 250 }
if (Test-Path -LiteralPath (Join-Path $destination 'storage_bridge.exe')) { throw 'Uninstall did not remove the test application' }
'PASS: uninstall removes the test application'
