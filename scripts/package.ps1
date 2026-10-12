<#
.SYNOPSIS
  Packs the release build into dist\ (run after the release builds, see .github\workflows\release.yml):
    typescript-windows.zip         the files install.ps1 installs
    typescript-windows.zip.sha256  checked by the one-line installer
    install.ps1                    the one-line installer itself (released next to the zip)
#>
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $repo 'dist'
$stage = Join-Path $dist 'stage'
Remove-Item $dist -Recurse -Force -ErrorAction SilentlyContinue
New-Item $stage -ItemType Directory | Out-Null

$files = @{
    'target\release\typescript_tip.dll'                        = 'typescript_tip.dll'
    'target\i686-pc-windows-msvc\release\typescript_tip.dll'   = 'typescript_tip_x86.dll'
    'target\release\typescript.exe'                            = 'typescript.exe'
    'assets\typescript.ico'                                    = 'typescript.ico'
    'THIRD_PARTY_NOTICES.txt'                                  = 'THIRD_PARTY_NOTICES.txt'
    'scripts\install.ps1'                                      = 'install.ps1'
}
foreach ($from in $files.Keys) {
    Copy-Item (Join-Path $repo $from) (Join-Path $stage $files[$from])
}

$zip = Join-Path $dist 'typescript-windows.zip'
# Not Compress-Archive: it opens files without sharing and trips over a virus scanner reading them.
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::CreateFromDirectory($stage, $zip)
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
Set-Content (Join-Path $dist 'typescript-windows.zip.sha256') "$hash  typescript-windows.zip" -Encoding ascii
Copy-Item (Join-Path $repo 'scripts\install.ps1') $dist
Remove-Item $stage -Recurse -Force
Get-ChildItem $dist | Select-Object Name, Length
