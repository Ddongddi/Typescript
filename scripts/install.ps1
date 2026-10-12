<#
.SYNOPSIS
  Builds, installs and registers Typescript (the x64 and x86 text service, and the CLI).

  .\scripts\install.ps1              install or update
  .\scripts\install.ps1 -Uninstall   remove it (settings in %APPDATA%\Typescript are kept)

  Registering a text service writes HKLM, so this asks for administrator rights once (UAC).
  Afterwards pick "Typescript" with Win+Space (or in Settings > Time & language > Language
  & region > Korean > Keyboards).
#>
param([switch]$Uninstall, [switch]$Elevated)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $env:ProgramFiles 'Typescript'
$dll64 = Join-Path $dest 'typescript_tip.dll'
$dll86 = Join-Path $dest 'typescript_tip_x86.dll'
$regsvr64 = Join-Path $env:SystemRoot 'System32\regsvr32.exe'
$regsvr86 = Join-Path $env:SystemRoot 'SysWOW64\regsvr32.exe'
$log = Join-Path $env:TEMP 'typescript-install.log'
# The picker entries (CLSID + profile GUID, see crates/tip/src/guids.rs): with and without Tab completion.
$tips = @(
    '0412:{E231DAD4-326F-436F-B74C-D08F1D66D822}{D7B9871C-E74A-4B4F-9791-CBD1E9E074FE}',
    '0412:{E231DAD4-326F-436F-B74C-D08F1D66D822}{BFC1C2A9-657F-443A-B4DE-ACF8EACA9AEC}'
)

# Adds (or removes) the keyboards under Korean in the user's language list; existing ones stay.
function Set-Keyboards([bool]$Present) {
    $list = Get-WinUserLanguageList
    $ko = $list | Where-Object { $_.LanguageTag -eq 'ko' }
    if (-not $ko) { return }
    foreach ($tip in $tips) {
        if ($Present -and $ko.InputMethodTips -notcontains $tip) { $ko.InputMethodTips.Add($tip) }
        if (-not $Present) { [void]$ko.InputMethodTips.Remove($tip) }
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
            Install-File (Join-Path $repo 'target\release\typescript_tip.dll') $dll64
            Install-File (Join-Path $repo 'target\i686-pc-windows-msvc\release\typescript_tip.dll') $dll86
            Install-File (Join-Path $repo 'target\release\typescript.exe') (Join-Path $dest 'typescript.exe')
            Copy-Item (Join-Path $repo 'assets\typescript.ico') $dest -Force
            Copy-Item (Join-Path $repo 'THIRD_PARTY_NOTICES.txt') $dest -Force -ErrorAction SilentlyContinue
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

# --- the user's side: build, ask for elevation, enable the profile ---

if (-not $Uninstall) {
    Push-Location $repo
    try {
        & cargo build --release -p typescript-tip -p typescript-cli
        if ($LASTEXITCODE) { throw 'cargo build (x64) failed' }
        & cargo build --release -p typescript-tip --target i686-pc-windows-msvc
        if ($LASTEXITCODE) { throw 'cargo build (x86) failed; run: rustup target add i686-pc-windows-msvc' }
    } finally {
        Pop-Location
    }
}

$arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"", '-Elevated')
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
