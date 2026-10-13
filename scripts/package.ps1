<#
.SYNOPSIS
  Packs the release builds into dist\ (run after the release builds, see .github\workflows\release.yml):
    typescript-windows.zip         the text service, typescript.exe, typescript-config.exe
    <zip>.sha256                   checked by the one-line installer and by typescript-config update
    install.ps1                    the one-line installer itself (released next to the zip)
    typescript-pack-<id>-<version>.zip, packs.json
                                   the language packs and their list (built by build-packs --release)
#>
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $repo 'dist'
# Empty it rather than recreate it: a folder an editor or indexer is watching can't be deleted.
if (Test-Path $dist) { Get-ChildItem $dist | Remove-Item -Recurse -Force } else { New-Item $dist -ItemType Directory | Out-Null }
# Not Compress-Archive: it opens files without sharing and trips over a virus scanner reading them.
Add-Type -AssemblyName System.IO.Compression.FileSystem

$name = 'typescript-windows'
$tip64 = Join-Path $repo 'target\release\typescript_tip.dll'
if (-not (Test-Path $tip64)) { throw "$tip64 was not built" }

$stage = Join-Path $dist "stage-$name"
New-Item $stage -ItemType Directory | Out-Null
$files = @(
    @('target\release\typescript_tip.dll', 'typescript_tip.dll'),
    @('target\i686-pc-windows-msvc\release\typescript_tip.dll', 'typescript_tip_x86.dll'),
    @('target\release\typescript.exe', 'typescript.exe'),
    @('target\release\typescript-config.exe', 'typescript-config.exe'),
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
& cargo run --release -p typescript-packgen --bin build-packs -- --release (Join-Path $repo 'crates\core\data') $dist
if ($LASTEXITCODE) { throw 'building the language pack release failed' }
Copy-Item (Join-Path $repo 'scripts\install.ps1') $dist
Get-ChildItem $dist | Select-Object Name, Length
