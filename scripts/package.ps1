<#
.SYNOPSIS
  Packs the release builds into dist\ (run after the release builds, see .github\workflows\release.yml):
    tacit-windows.zip         the text service, tacit-config.exe
    <zip>.sha256                   checked by the one-line installer and by tacit-config update
    install.ps1                    the one-line installer itself (released next to the zip)
    tacit-pack-<id>-<version>.zip, packs.json
                                   the language packs and their list (built by build-packs --release)
#>
param([ValidateSet('enko', 'enkoja')][string]$Edition = 'enko')
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $repo 'dist'
$name = if ($Edition -eq 'enkoja') { 'tacit-windows-ja' } else { 'tacit-windows' }
$supported = & (Join-Path $repo 'target/release/tacit-config.exe') supported
$expected = if ($Edition -eq 'enkoja') { 'en,ko,ja' } else { 'en,ko' }
if ($LASTEXITCODE -or $supported.Trim() -ne $expected) { throw 'Build language features do not match the requested package' }
# Empty it rather than recreate it: a folder an editor or indexer is watching can't be deleted.
if (Test-Path $dist) { Get-ChildItem $dist | Remove-Item -Recurse -Force } else { New-Item $dist -ItemType Directory | Out-Null }
# Not Compress-Archive: it opens files without sharing and trips over a virus scanner reading them.
Add-Type -AssemblyName System.IO.Compression.FileSystem

$tip64 = Join-Path $repo 'target\release\tacit_tip.dll'
if (-not (Test-Path $tip64)) { throw "$tip64 was not built" }

$stage = Join-Path $dist "stage-$name"
New-Item $stage -ItemType Directory | Out-Null
$files = @(
    @('target\release\tacit_tip.dll', 'tacit_tip.dll'),
    @('target\i686-pc-windows-msvc\release\tacit_tip.dll', 'tacit_tip_x86.dll'),
    @('target\release\tacit-config.exe', 'tacit-config.exe'),
    @('assets\tacit.ico', 'tacit.ico'),
    @('LICENSE', 'LICENSE'),
    @('NOTICE', 'NOTICE'),
    @('THIRD_PARTY_NOTICES.txt', 'THIRD_PARTY_NOTICES.txt'),
    @('scripts\install.ps1', 'install.ps1')
)
foreach ($f in $files) {
    Copy-Item (Join-Path $repo $f[0]) (Join-Path $stage $f[1])
}

Set-Content (Join-Path $stage 'edition') $Edition -Encoding ascii
$zip = Join-Path $dist "$name.zip"
[IO.Compression.ZipFile]::CreateFromDirectory($stage, $zip)
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
Set-Content "$zip.sha256" "$hash  $name.zip" -Encoding ascii
Remove-Item $stage -Recurse -Force
& cargo run --release -p tacit-packgen --bin build-packs -- --release (Join-Path $repo 'crates\core\data') $dist
if ($LASTEXITCODE) { throw 'building the language pack release failed' }
Copy-Item (Join-Path $repo 'scripts\install.ps1') $dist
Get-ChildItem $dist | Select-Object Name, Length
