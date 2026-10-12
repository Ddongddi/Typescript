<#
.SYNOPSIS
  Installs (or removes) Typescript: the x64 and x86 text service and the CLI.

  One line, from anywhere (downloads the latest GitHub release, checks its SHA-256):
    irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1 | iex
  Remove it, or pick a version:
    & ([scriptblock]::Create((irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1))) -Uninstall
    & ([scriptblock]::Create((irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1))) -Version v0.2.0

  From a checkout (builds first; needs Rust + the i686-pc-windows-msvc target):
    .\scripts\install.ps1              install or update
    .\scripts\install.ps1 -Uninstall   remove it (settings in %APPDATA%\Typescript are kept)

  Registering a text service writes HKLM, so this asks for administrator rights once (UAC).
  Afterwards pick "Typescript" with Win+Space (or in Settings > Time & language > Language
  & region > English (United States) > Keyboards).
#>
param(
    [switch]$Uninstall,
    # Release tag to install (default: the latest release).
    [string]$Version = 'latest',
    # Internal: the elevated half of the script, and the folder it installs from.
    [switch]$Elevated,
    [string]$Source
)

$GitHubRepo = 'iamallyouneed/Typescript'
$ErrorActionPreference = 'Stop'
$dest = Join-Path $env:ProgramFiles 'Typescript'
$dll64 = Join-Path $dest 'typescript_tip.dll'
$dll86 = Join-Path $dest 'typescript_tip_x86.dll'
$regsvr64 = Join-Path $env:SystemRoot 'System32\regsvr32.exe'
$regsvr86 = Join-Path $env:SystemRoot 'SysWOW64\regsvr32.exe'
$log = Join-Path $env:TEMP 'typescript-install.log'
# The picker entries (CLSID + profile GUID, see crates/tip/src/guids.rs): with and without Tab completion.
# Registered under English (US), not Korean: see LANGID_TYPESCRIPT in guids.rs for why.
$profiles = @(
    '{E231DAD4-326F-436F-B74C-D08F1D66D822}{D7B9871C-E74A-4B4F-9791-CBD1E9E074FE}',
    '{E231DAD4-326F-436F-B74C-D08F1D66D822}{BFC1C2A9-657F-443A-B4DE-ACF8EACA9AEC}'
)

# Adds (or removes) the keyboards under English (US) in the user's language list, adding the
# language if it is missing; existing keyboards stay. Entries of earlier versions (under Korean) go.
function Set-Keyboards([bool]$Present) {
    $list = Get-WinUserLanguageList
    foreach ($ko in @($list | Where-Object { $_.LanguageTag -eq 'ko' })) {
        foreach ($p in $profiles) { [void]$ko.InputMethodTips.Remove("0412:$p") }
    }
    $en = $list | Where-Object { $_.LanguageTag -eq 'en-US' }
    if ($Present -and -not $en) {
        $list.Add('en-US')
        $en = $list | Where-Object { $_.LanguageTag -eq 'en-US' }
    }
    if ($en) {
        foreach ($p in $profiles) {
            $tip = "0409:$p"
            if ($Present -and $en.InputMethodTips -notcontains $tip) { $en.InputMethodTips.Add($tip) }
            if (-not $Present) { [void]$en.InputMethodTips.Remove($tip) }
        }
    }
    Set-WinUserLanguageList $list -Force -WarningAction SilentlyContinue
}

function Invoke-Regsvr([string]$exe, [string]$dll, [switch]$Unregister) {
    $arguments = if ($Unregister) { @('/u', '/s', "`"$dll`"") } else { @('/s', "`"$dll`"") }
    $p = Start-Process $exe -ArgumentList $arguments -Wait -PassThru
    # Unregistering something already gone (a half-finished earlier run) is not an error.
    if ($p.ExitCode -ne 0 -and -not $Unregister) { throw "regsvr32 exited with $($p.ExitCode) for $dll" }
}

# A loaded DLL can be renamed but not overwritten: move it aside, delete leftovers next time.
function Install-File([string]$source, [string]$target) {
    if (Test-Path $target) {
        Move-Item $target "$target.old-$([guid]::NewGuid().ToString('N').Substring(0, 8))" -Force
    }
    Copy-Item $source $target -Force
}

function Remove-Leftovers {
    Get-ChildItem $dest -Filter '*.old-*' -ErrorAction SilentlyContinue | ForEach-Object {
        try { Remove-Item $_.FullName -Force } catch { }
    }
}

function Unregister-Existing {
    if (Test-Path $dll64) { Invoke-Regsvr $regsvr64 $dll64 -Unregister }
    if (Test-Path $dll86) { Invoke-Regsvr $regsvr86 $dll86 -Unregister }
}

if ($Elevated) {
    Start-Transcript -Path $log -Force | Out-Null
    try {
        if ($Uninstall) {
            Unregister-Existing
            if (Test-Path $dest) {
                Remove-Item $dest -Recurse -Force -ErrorAction SilentlyContinue
                if (Test-Path $dest) { Write-Warning "Some files in $dest are in use; they go away after a restart or sign-out." }
            }
        } else {
            New-Item $dest -ItemType Directory -Force | Out-Null
            Unregister-Existing
            Install-File (Join-Path $Source 'typescript_tip.dll') $dll64
            Install-File (Join-Path $Source 'typescript_tip_x86.dll') $dll86
            Install-File (Join-Path $Source 'typescript.exe') (Join-Path $dest 'typescript.exe')
            foreach ($extra in 'typescript.ico', 'THIRD_PARTY_NOTICES.txt', 'install.ps1') {
                Copy-Item (Join-Path $Source $extra) $dest -Force -ErrorAction SilentlyContinue
            }
            # Store apps and sandboxed processes load the DLL only if these groups can read it.
            & icacls.exe $dest /grant '*S-1-15-2-1:(OI)(CI)(RX)' '*S-1-15-2-2:(OI)(CI)(RX)' | Out-Null
            Invoke-Regsvr $regsvr64 $dll64
            Invoke-Regsvr $regsvr86 $dll86
            Remove-Leftovers
        }
    } finally {
        Stop-Transcript | Out-Null
    }
    exit 0
}

# --- the user's side: get the files, ask for elevation, enable the profile ---

# Builds from a checkout into a folder laid out like the release package.
function New-SourceStage {
    $repo = Split-Path -Parent $PSScriptRoot
    Push-Location $repo
    try {
        & cargo build --release -p typescript-tip -p typescript-cli
        if ($LASTEXITCODE) { throw 'cargo build (x64) failed' }
        & cargo build --release -p typescript-tip --target i686-pc-windows-msvc
        if ($LASTEXITCODE) { throw 'cargo build (x86) failed; run: rustup target add i686-pc-windows-msvc' }
    } finally {
        Pop-Location
    }
    $stage = New-TempDirectory
    Copy-Item (Join-Path $repo 'target\release\typescript_tip.dll') $stage
    Copy-Item (Join-Path $repo 'target\i686-pc-windows-msvc\release\typescript_tip.dll') (Join-Path $stage 'typescript_tip_x86.dll')
    Copy-Item (Join-Path $repo 'target\release\typescript.exe') $stage
    Copy-Item (Join-Path $repo 'assets\typescript.ico') $stage
    Copy-Item (Join-Path $repo 'THIRD_PARTY_NOTICES.txt') $stage -ErrorAction SilentlyContinue
    Copy-Item $PSCommandPath $stage
    return $stage
}

function New-TempDirectory {
    $dir = Join-Path $env:TEMP "typescript-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
    New-Item $dir -ItemType Directory | Out-Null
    return $dir
}

# Downloads a release package, checks its SHA-256 and unpacks it.
function Get-ReleaseStage {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $base = if ($Version -eq 'latest') { "https://github.com/$GitHubRepo/releases/latest/download" }
            else { "https://github.com/$GitHubRepo/releases/download/$Version" }
    $stage = New-TempDirectory
    $zip = Join-Path $stage 'typescript-windows.zip'
    Write-Host "Downloading Typescript ($Version)..."
    Invoke-WebRequest "$base/typescript-windows.zip" -OutFile $zip -UseBasicParsing
    # To a file, not .Content: Windows PowerShell 5.1 returns bytes for an octet-stream.
    $sumFile = "$zip.sha256"
    Invoke-WebRequest "$base/typescript-windows.zip.sha256" -OutFile $sumFile -UseBasicParsing
    $expected = ((Get-Content $sumFile -Raw) -split '\s+')[0].Trim()
    $actual = (Get-FileHash $zip -Algorithm SHA256).Hash
    if ($actual -ne $expected) { throw "Checksum mismatch for $zip (expected $expected, got $actual)" }
    Expand-Archive $zip -DestinationPath $stage -Force
    Remove-Item $zip, $sumFile
    return $stage
}

$stage = $null
$inPackage = $PSScriptRoot -and (Test-Path (Join-Path $PSScriptRoot 'typescript_tip.dll'))
$inCheckout = $PSScriptRoot -and (Test-Path (Join-Path (Split-Path -Parent $PSScriptRoot) 'Cargo.toml'))
try {
    if ($inPackage -or ($Uninstall -and $PSCommandPath)) {
        $Source = $PSScriptRoot # nothing to fetch: the files are here, or there is nothing to install
    } elseif ($inCheckout) {
        $stage = New-SourceStage
        $Source = $stage
    } else {
        # Piped from the web: fetch the release package and use the script inside it.
        $stage = Get-ReleaseStage
        $Source = $stage
    }
    $script = if ($PSCommandPath) { $PSCommandPath } else { Join-Path $Source 'install.ps1' }

    $arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$script`"", '-Elevated', '-Source', "`"$Source`"")
    if ($Uninstall) { $arguments += '-Uninstall' }
    Write-Host 'Asking for administrator rights to register the text service...'
    $p = Start-Process powershell.exe -Verb RunAs -ArgumentList $arguments -Wait -PassThru
    if ($p.ExitCode -ne 0) {
        if (Test-Path $log) { Get-Content $log -Tail 20 }
        throw "The elevated step failed (exit $($p.ExitCode)). Log: $log"
    }

    if ($Uninstall) {
        Set-Keyboards $false
        Write-Host 'Typescript removed. Settings stay in %APPDATA%\Typescript.'
    } else {
        # Per user: switch the registered profile on.
        & rundll32.exe $dll64, EnableForUser
        Set-Keyboards $true
        Write-Host 'Typescript installed.'
        Write-Host '  Win+Space  -> "Typescript (autocomplete)" or "Typescript" (no autocomplete)'
        Write-Host '  Pause key  -> the Han/Eng key (VK_HANGUL), per text field. Settings: %APPDATA%\Typescript\settings.json'
        Write-Host '  Apps already running pick the new text service up when you reopen them.'
    }
} finally {
    if ($stage) { Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue }
}
