$ErrorActionPreference = 'Stop'
$version = (Get-Content apps/desktop/src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version
$root = Join-Path $env:RUNNER_TEMP 'ddoktti-upgrade'
New-Item -ItemType Directory -Force $root | Out-Null
$old = Join-Path $root 'previous-setup.exe'
Invoke-WebRequest 'https://github.com/plead-ops/ddoktti-here/releases/download/v0.1.9/ddoktti-here_0.1.9_x64-setup.exe' -OutFile $old
$install = Join-Path $root 'app'
function Install-Checked($file, $arguments) {
    $p = Start-Process -FilePath $file -ArgumentList $arguments -PassThru
    if (-not $p.WaitForExit(120000)) { $p.Kill(); throw 'Installer timed out' }
    if ($p.ExitCode -ne 0) { throw "Installer failed: $($p.ExitCode)" }
}
Install-Checked $old @('/S', "/D=$install")
$exe = Join-Path $install 'ddoktti-here.exe'
if (-not (Test-Path $exe)) { throw 'Previous app was not installed' }
if ((Get-Item $exe).VersionInfo.ProductVersion -notlike '0.1.9*') { throw 'Unexpected previous version' }
$new = Get-ChildItem 'apps/desktop/src-tauri/target/release/bundle/nsis/*-setup.exe' | Select-Object -First 1
Install-Checked $new.FullName @('/S', '/UPDATE', "/D=$install")
if ((Get-Item $exe).VersionInfo.ProductVersion -notlike "$version*") { throw 'Upgrade did not replace the installed app' }
Write-Host "Windows installer upgrade: 0.1.9 -> $version passed"
