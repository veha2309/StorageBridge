$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'This prerequisite installer is intended for disposable GitHub Actions runners' }

python -m pip install --disable-pip-version-check --target .tools 'libclang==18.1.1'
if ($LASTEXITCODE) { throw 'libclang installation failed' }
"LIBCLANG_PATH=$(Join-Path $PWD '.tools\clang\native')" | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8

$installer = Join-Path $env:RUNNER_TEMP 'winfsp-2.1.25156.msi'
curl.exe --silent --show-error --fail --location --retry 3 --connect-timeout 15 --max-time 180 `
    --output $installer 'https://github.com/winfsp/winfsp/releases/download/v2.1/winfsp-2.1.25156.msi'
if ($LASTEXITCODE) { throw 'WinFsp download failed' }
$expected = '073A70E00F77423E34BED98B86E600DEF93393BA5822204FAC57A29324DB9F7A'
if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash -ne $expected) { throw 'WinFsp checksum mismatch' }
if ((Get-AuthenticodeSignature -LiteralPath $installer).Status -ne 'Valid') { throw 'WinFsp signature is not valid' }
$log = Join-Path $env:RUNNER_TEMP 'winfsp-install.log'
$install = Start-Process -FilePath msiexec.exe -ArgumentList '/i',"`"$installer`"",'/qn','/norestart','ADDLOCAL=ALL','/l*v',"`"$log`"" -WindowStyle Hidden -Wait -PassThru
if ($install.ExitCode -notin @(0,3010)) { throw "WinFsp installation failed ($($install.ExitCode)); see $log" }

$compiler = Get-Command iscc -ErrorAction SilentlyContinue
$knownCompiler = @('C:\Program Files (x86)\Inno Setup 6\ISCC.exe','C:\Program Files\Inno Setup 6\ISCC.exe') |
    Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $compiler -and -not $knownCompiler) {
    choco install innosetup --yes --no-progress --limit-output
    if ($LASTEXITCODE) { throw 'Inno Setup installation failed' }
}
