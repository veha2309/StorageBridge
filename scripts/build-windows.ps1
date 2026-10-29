param([switch]$SkipTests, [switch]$Installer, [string]$BundleName = 'StorageBridge')
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Push-Location $workspace
try {
    if (-not $env:LIBCLANG_PATH) {
        $localClang = Join-Path $workspace '.tools\clang\native'
        if (Test-Path -LiteralPath "$localClang\libclang.dll") { $env:LIBCLANG_PATH = $localClang }
        else { throw 'Set LIBCLANG_PATH to the directory containing libclang.dll, or run: python -m pip install --target .tools libclang' }
    }
    $flutter = (Get-Command flutter -ErrorAction Stop).Source
    $flutterBin = Split-Path -Parent $flutter
    $dart = Join-Path $flutterBin 'cache\dart-sdk\bin\dart.exe'
    $tool = Join-Path $flutterBin 'cache\flutter_tools.snapshot'
    if (-not $SkipTests) {
        cargo test --locked
        if ($LASTEXITCODE) { throw 'Rust tests failed' }
        cargo clippy --locked --all-targets -- -D warnings
        if ($LASTEXITCODE) { throw 'Rust lint failed' }
    }
    cargo build --locked --release
    if ($LASTEXITCODE) { throw 'Engine build failed' }
    Push-Location desktop
    try {
        if (-not $SkipTests) {
            & $dart $tool analyze
            if ($LASTEXITCODE) { throw 'Flutter analysis failed' }
            & $dart $tool test
            if ($LASTEXITCODE) { throw 'Flutter tests failed' }
        }
        & $dart $tool build windows --release
        if ($LASTEXITCODE) { throw 'Desktop build failed' }
    } finally { Pop-Location }
    if ($BundleName -notmatch '^StorageBridge(-[0-9.]+)?$') { throw 'Invalid bundle directory name' }
    $bundle = Join-Path $workspace "release\$BundleName"
    New-Item -ItemType Directory -Force -Path $bundle | Out-Null
    Copy-Item -Path 'desktop\build\windows\x64\runner\Release\*' -Destination $bundle -Recurse -Force
    Copy-Item -LiteralPath 'target\release\storage-bridge.exe','README.md','LICENSE','THIRD_PARTY_NOTICES.md' -Destination $bundle -Force
    $runtimeRoot = 'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Redist\MSVC'
    if (Test-Path -LiteralPath $runtimeRoot) {
        $runtime = Get-ChildItem -Path "$runtimeRoot\*\x64\Microsoft.VC143.CRT" -Directory | Sort-Object FullName -Descending | Select-Object -First 1
        if ($runtime) { Copy-Item -Path "$($runtime.FullName)\*.dll" -Destination $bundle -Force }
    }
    Compress-Archive -LiteralPath $bundle -DestinationPath "release\$BundleName-windows-x64.zip" -Force
    if ($Installer) {
        $compiler = Get-Command iscc -ErrorAction SilentlyContinue
        $compilerPath = if ($compiler) { $compiler.Source } else {
            @('C:\Program Files (x86)\Inno Setup 6\ISCC.exe', 'C:\Program Files\Inno Setup 6\ISCC.exe', "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        }
        if (-not $compilerPath) { throw 'Install Inno Setup 6 to compile the installer' }
        if (-not (Test-Path -LiteralPath $compilerPath)) { throw 'Install Inno Setup 6 to compile the installer' }
        & $compilerPath "/DBundleDir=$bundle" 'installer\storage-bridge.iss'
        if ($LASTEXITCODE) { throw 'Installer compilation failed' }
    }
    Write-Output "Build ready: $bundle\storage_bridge.exe"
} finally { Pop-Location }
