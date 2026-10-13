<#
.SYNOPSIS
  Packs the release builds into dist\ (run after the release builds, see .github\workflows\release.yml):
    typescript-windows.zip         English + Korean               (built into target\)
    typescript-windows-ja.zip      English + Korean + Japanese    (built into target-ja\ with the japanese feature)
    <zip>.sha256                   checked by the one-line installer
    install.ps1                    the one-line installer itself (released next to the zips)
  An edition that was not built is skipped with a warning.
#>
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $repo 'dist'
Remove-Item $dist -Recurse -Force -ErrorAction SilentlyContinue
New-Item $dist -ItemType Directory | Out-Null
# Not Compress-Archive: it opens files without sharing and trips over a virus scanner reading them.
Add-Type -AssemblyName System.IO.Compression.FileSystem

# zip name -> the target dir its binaries were built into
$editions = [ordered]@{ 'typescript-windows' = 'target'; 'typescript-windows-ja' = 'target-ja' }
foreach ($name in $editions.Keys) {
    $built = $editions[$name]
    $tip64 = Join-Path $repo "$built\release\typescript_tip.dll"
    if (-not (Test-Path $tip64)) { Write-Warning "$name skipped: $tip64 was not built"; continue }

    $stage = Join-Path $dist "stage-$name"
    New-Item $stage -ItemType Directory | Out-Null
    $files = @(
        @("$built\release\typescript_tip.dll", 'typescript_tip.dll'),
        @("$built\i686-pc-windows-msvc\release\typescript_tip.dll", 'typescript_tip_x86.dll'),
        @("$built\release\typescript.exe", 'typescript.exe'),
        @('assets\typescript.ico', 'typescript.ico'),
        @('THIRD_PARTY_NOTICES.txt', 'THIRD_PARTY_NOTICES.txt'),
        @('scripts\install.ps1', 'install.ps1')
    )
    foreach ($f in $files) {
        Copy-Item (Join-Path $repo $f[0]) (Join-Path $stage $f[1])
    }

    $zip = Join-Path $dist "$name.zip"
    [IO.Compression.ZipFile]::CreateFromDirectory($stage, $zip)
    $hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
    Set-Content "$zip.sha256" "$hash  $name.zip" -Encoding ascii
    Remove-Item $stage -Recurse -Force
}
Copy-Item (Join-Path $repo 'scripts\install.ps1') $dist
Get-ChildItem $dist | Select-Object Name, Length
