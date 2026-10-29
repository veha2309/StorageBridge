param([string]$Engine = "$PSScriptRoot\..\target\debug\storage-bridge.exe", [string]$Letter = 'Z:')
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\ipc.ps1"
if (Test-Path -LiteralPath "$Letter\") { throw "Drive $Letter is already occupied" }
$instance = [guid]::NewGuid().ToString('N')
function Invoke-TestBridge { param([string]$Method, [hashtable]$Params = @{}); Invoke-Bridge -Method $Method -Params $Params -Instance $instance }
function Wait-TestUploads {
    $stable=0
    for ($attempt=0; $attempt -lt 80; $attempt++) {
        $current=Invoke-TestBridge status
        if (-not $current.busy -and $current.pending -eq 0 -and $current.cache_bytes -eq 0 -and $current.open_files -eq 0) { $stable++; if ($stable -ge 2) { return } } else { $stable=0 }
        Start-Sleep -Milliseconds 250
    }
    throw ($current | ConvertTo-Json -Depth 6)
}
$process = Start-Process -FilePath (Resolve-Path -LiteralPath $Engine) -ArgumentList '--daemon','--demo','--test-instance',$instance -WindowStyle Hidden -PassThru
try {
    $ready = $false
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        try { $state = Invoke-TestBridge status; $ready = $true; break } catch { Start-Sleep -Milliseconds 200 }
    }
    if (-not $ready) { throw 'Engine did not start' }
    if (-not $state.demo) { throw 'Refusing filesystem tests against a real Google Drive' }
    Invoke-TestBridge mount @{letter = $Letter} | Out-Null
    $folder = "$Letter\bridge-smoke-$([guid]::NewGuid().ToString('N'))"
    [System.IO.Directory]::CreateDirectory($folder) | Out-Null
    # Wait for the remotely-backed directory to upload before creating children.
    for ($attempt = 0; $attempt -lt 20; $attempt++) {
        if ((Invoke-TestBridge status).pending -eq 0) { break }; Start-Sleep -Milliseconds 250
    }
    $original = Join-Path $folder 'document.txt'
    [System.IO.File]::WriteAllText($original, 'first version')
    if ([System.IO.File]::ReadAllText($original) -ne 'first version') { throw 'Read/write mismatch' }
    Wait-TestUploads
    if ([System.IO.File]::ReadAllText($original) -ne 'first version') { throw 'Streaming read after eviction failed' }
    $edit=[System.IO.File]::Open($original,[System.IO.FileMode]::Open,[System.IO.FileAccess]::Write,[System.IO.FileShare]::None)
    try { $edit.WriteByte([byte][char]'F') } finally { $edit.Dispose() }
    if ([System.IO.File]::ReadAllText($original) -ne 'First version') { throw 'Partial overlay edit failed' }
    Wait-TestUploads
    if ([System.IO.File]::ReadAllText($original) -ne 'First version') { throw 'Partial overlay upload failed' }
    $temp = Join-Path $folder 'save.tmp'
    [System.IO.File]::WriteAllText($temp, 'saved through replacement')
    [System.IO.File]::Replace($temp, $original, [System.Management.Automation.Language.NullString]::Value)
    if ([System.IO.File]::ReadAllText($original) -ne 'saved through replacement') { throw 'Application replacement save failed' }
    $renamed = Join-Path $folder 'renamed.txt'
    [System.IO.File]::Move($original, $renamed)
    if (-not [System.IO.Directory]::GetFiles($folder).Contains($renamed)) { throw 'Directory enumeration failed' }
    $large = Join-Path $folder 'large.bin'
    $payload = New-Object byte[] (17 * 1024 * 1024 + 123)
    $random = [System.Random]::new(42)
    $random.NextBytes($payload)
    [System.IO.File]::WriteAllBytes($large, $payload)
    Wait-TestUploads
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $expected = [Convert]::ToBase64String($sha.ComputeHash($payload))
        $actual = [Convert]::ToBase64String($sha.ComputeHash([System.IO.File]::ReadAllBytes($large)))
        if ($expected -ne $actual) { throw 'Large-file checksum mismatch' }
    } finally { $sha.Dispose() }
    if ((Invoke-TestBridge status).cache_bytes -ne 0) { throw 'Streaming reads created a disk copy' }
    [System.IO.File]::Delete($large)
    $empty = Join-Path $folder 'empty.txt'
    [System.IO.File]::WriteAllBytes($empty, [byte[]]@())
    if ((Get-Item -LiteralPath $empty).Length -ne 0) { throw 'Empty-file size mismatch' }
    [System.IO.File]::Delete($empty)
    [System.IO.File]::Delete($renamed)
    [System.IO.Directory]::Delete($folder)
    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        $state = Invoke-TestBridge status
        if ($state.pending -eq 0) { break }; Start-Sleep -Milliseconds 250
    }
    if ($state.pending -ne 0) { throw ($state.transfers | ConvertTo-Json -Depth 5) }
    Invoke-TestBridge unmount | Out-Null
    Wait-TestUploads
    Invoke-TestBridge shutdown | Out-Null
    'PASS: network drive streaming, partial edits, 17 MiB checksum, zero read-cache disk bytes, replace/rename/delete/unmount'
} finally {
    try { Invoke-TestBridge unmount | Out-Null; Invoke-TestBridge shutdown | Out-Null } catch {}
    if (-not $process.HasExited) { $process.WaitForExit(3000) | Out-Null }
}
