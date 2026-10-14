<#
.SYNOPSIS
  Installs (or removes) Typescript: the x64 and x86 text service and the CLI.

  One line, from anywhere (downloads the latest GitHub release, checks its SHA-256):
    irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1 | iex
  Remove it, or pick a version:
    & ([scriptblock]::Create((irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1))) -Uninstall
    & ([scriptblock]::Create((irm https://github.com/iamallyouneed/Typescript/releases/latest/download/install.ps1))) -Version v0.2.0

  From a checkout (builds first; needs Rust + the i686-pc-windows-msvc target):
    .\scripts\install.ps1              install or update (English + Korean language packs)
    .\scripts\install.ps1 -Japanese    also install and turn on the Japanese pack
    .\scripts\install.ps1 -Uninstall   remove it (settings in %APPDATA%\Typescript are kept)

  Registering a text service writes HKLM, so this asks for administrator rights once (UAC).
  Afterwards pick "Typescript" with Win+Space (or in Settings > Time & language > Language
  & region > English (United States) > Keyboards).
#>
param(
    [switch]$Uninstall,
    # Release tag to install (default: the latest release).
    [string]$Version = 'latest',
    # Also install and turn on the Japanese language pack (romaji -> kana -> kanji, a 16 MB
    # dictionary). The packs go to %LOCALAPPDATA%\Typescript\packs; settings.json "languages"
    # says which are on, in Esc order.
    [switch]$Japanese,
    # The languages to turn on, in Esc order, e.g. ko,en,ja. Without it a new install asks
    # (typescript-config setup) and an update keeps the languages already on.
    [string]$Languages = '',
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
# The picker entries (CLSID + profile GUID, see platforms/windows/src/guids.rs): with and without Tab completion.
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
            Install-File (Join-Path $Source 'typescript-config.exe') (Join-Path $dest 'typescript-config.exe')
            foreach ($extra in 'typescript.ico', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.txt', 'install.ps1') {
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
        & cargo build --release -p typescript-tip -p typescript-cli -p typescript-config
        if ($LASTEXITCODE) { throw 'cargo build (x64) failed' }
        & cargo build --release -p typescript-tip --target i686-pc-windows-msvc
        if ($LASTEXITCODE) { throw 'cargo build (x86) failed; run: rustup target add i686-pc-windows-msvc' }
        # Out-Host: what it prints must not become part of this function's result (the stage path).
        & cargo run --release -p typescript-packgen --bin build-packs -- crates\core\data target\packs | Out-Host
        if ($LASTEXITCODE) { throw 'building the language packs failed' }
    } finally {
        Pop-Location
    }
    $stage = New-TempDirectory
    Copy-Item (Join-Path $repo 'target\release\typescript_tip.dll') $stage
    Copy-Item (Join-Path $repo 'target\i686-pc-windows-msvc\release\typescript_tip.dll') (Join-Path $stage 'typescript_tip_x86.dll')
    Copy-Item (Join-Path $repo 'target\release\typescript.exe') $stage
    Copy-Item (Join-Path $repo 'target\release\typescript-config.exe') $stage
    Copy-Item (Join-Path $repo 'target\packs') (Join-Path $stage 'packs') -Recurse
    Copy-Item (Join-Path $repo 'assets\typescript.ico') $stage
    foreach ($notice in 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.txt') { Copy-Item (Join-Path $repo $notice) $stage -ErrorAction SilentlyContinue }
    Copy-Item $PSCommandPath $stage
    return $stage
}

function New-TempDirectory {
    $dir = Join-Path $env:TEMP "typescript-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
    New-Item $dir -ItemType Directory | Out-Null
    return $dir
}

# Copies the language packs next to the user's settings and turns them on. Packs live in the
# user's profile (no administrator needed to add one later); AppContainer apps (Store apps,
# sandboxed browsers) load the text service too and must be able to read them.
function Install-Packs([string]$from) {
    $packRoot = Join-Path $env:LOCALAPPDATA 'Typescript\packs'
    New-Item $packRoot -ItemType Directory -Force | Out-Null
    $settingsPath = Join-Path $env:APPDATA 'Typescript\settings.json'
    $settings = [pscustomobject]@{}
    if (Test-Path $settingsPath) {
        try { $settings = Get-Content $settingsPath -Raw | ConvertFrom-Json }
        catch { Write-Warning "settings.json is not valid JSON ($($_.Exception.Message)); it will be rewritten." }
    }
    $hadJapanese = [bool]$settings.PSObject.Properties['japanese'] -and $settings.japanese -eq $true
    $ids = @('en', 'ko')
    if ($Japanese -or $hadJapanese) { $ids += 'ja' }
    if ($settings.PSObject.Properties['languages']) {
        $ids = @($settings.languages)
        if ($Japanese -and $ids -notcontains 'ja') { $ids += 'ja' }
    }
    # Korean and Japanese need the English pack's word list, even with English turned off.
    $need = @($ids) + @('en') | Select-Object -Unique
    # A pack folder is never written in place: running apps keep its files mapped and Windows
    # refuses to overwrite those. A folder that is already there is the same version (versions
    # are immutable), so it is left alone; a new one is copied aside and moved in whole, so a
    # reader never sees a half-copied pack.
    $staging = Join-Path (Split-Path $packRoot) 'packs-incoming'
    Remove-Item $staging -Recurse -Force -ErrorAction SilentlyContinue
    New-Item $staging -ItemType Directory -Force | Out-Null
    foreach ($id in $need) {
        Get-ChildItem (Join-Path $from 'packs') -Directory -Filter "$id-*" -ErrorAction SilentlyContinue | ForEach-Object {
            if (-not (Test-Path (Join-Path $packRoot $_.Name))) {
                Copy-Item $_.FullName (Join-Path $staging $_.Name) -Recurse
                Move-Item (Join-Path $staging $_.Name) (Join-Path $packRoot $_.Name)
            }
        }
    }
    Remove-Item $staging -Recurse -Force -ErrorAction SilentlyContinue
    # On the folder above, so the staging folder and every pack folder inherit it (a folder moved
    # into place keeps the permissions it had).
    & icacls.exe (Split-Path $packRoot) /grant '*S-1-15-2-1:(OI)(CI)(RX)' '*S-1-15-2-2:(OI)(CI)(RX)' /T | Out-Null
    $settings | Add-Member -NotePropertyName languages -NotePropertyValue $ids -Force
    # The versions before language packs read this key; keep it true to what is on.
    $settings | Add-Member -NotePropertyName japanese -NotePropertyValue ($ids -contains 'ja') -Force
    New-Item (Split-Path $settingsPath) -ItemType Directory -Force | Out-Null
    # Not Set-Content -Encoding utf8: Windows PowerShell 5.1 writes a byte order mark with it.
    [IO.File]::WriteAllText($settingsPath, ($settings | ConvertTo-Json), (New-Object Text.UTF8Encoding($false)))
}

# "Typescript <settings>" in Korean, built from code points: Windows PowerShell 5.1 reads a script
# file without a byte order mark as ANSI, so this file stays ASCII.
$settingsLabel = 'Typescript ' + [string][char]0xC124 + [char]0xC815

# The Start menu entry that opens the settings page (typescript-config is not on PATH).
function Set-SettingsShortcut([bool]$Present) {
    $link = Join-Path $env:APPDATA ('Microsoft\Windows\Start Menu\Programs\' + $settingsLabel + '.lnk')
    if (-not $Present) { Remove-Item $link -Force -ErrorAction SilentlyContinue; return }
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($link)
    $shortcut.TargetPath = Join-Path $dest 'typescript-config.exe'
    $shortcut.IconLocation = Join-Path $dest 'typescript.ico'
    $shortcut.Description = 'Typescript: languages, settings, updates'
    $shortcut.Save()
}

# The languages already on in settings.json, as a list (empty on a first install).
function Get-ConfiguredLanguages {
    $path = Join-Path $env:APPDATA 'Typescript\settings.json'
    if (-not (Test-Path $path)) { return @() }
    try { $s = Get-Content $path -Raw | ConvertFrom-Json } catch { return @() }
    if ($s.PSObject.Properties['languages']) { return @($s.languages) }
    if ($s.PSObject.Properties['japanese'] -and $s.japanese -eq $true) { return @('en', 'ko', 'ja') }
    return @('en', 'ko')
}

# A release install: the packs come from the release through typescript-config, which asks
# which languages when this is a first install.
function Install-ReleasePacks {
    $config = Join-Path $dest 'typescript-config.exe'
    if (-not (Test-Path $config)) {
        # A release from before typescript-config (-Version): it has no language packs to fetch.
        Write-Warning 'This release has no typescript-config, so no language packs were installed. Install a newer release to choose languages.'
        return
    }
    $localRoot = Join-Path $env:LOCALAPPDATA 'Typescript'
    New-Item (Join-Path $localRoot 'packs') -ItemType Directory -Force | Out-Null
    # Store apps and sandboxed processes read the packs; set before any pack exists so they inherit it.
    & icacls.exe $localRoot /grant '*S-1-15-2-1:(OI)(CI)(RX)' '*S-1-15-2-2:(OI)(CI)(RX)' /T | Out-Null
    $previousRelease = $env:TYPESCRIPT_RELEASE
    if ($Version -ne 'latest') { $env:TYPESCRIPT_RELEASE = "https://github.com/$GitHubRepo/releases/download/$Version/" }
    $wanted = @($Languages -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
    if (-not $wanted.Count) { $wanted = Get-ConfiguredLanguages }
    if ($Japanese -and $wanted.Count -and $wanted -notcontains 'ja') { $wanted += 'ja' }
    try {
        if ($wanted.Count) { & $config setup --languages ($wanted -join ',') }
        elseif ($Japanese) { & $config setup --languages 'en,ko,ja' }
        else { & $config setup }
    } finally {
        # Do not leave the pin in the user's shell (this runs inside `irm | iex`).
        if ($null -eq $previousRelease) { Remove-Item Env:\TYPESCRIPT_RELEASE -ErrorAction SilentlyContinue } else { $env:TYPESCRIPT_RELEASE = $previousRelease }
    }
    if ($LASTEXITCODE) { throw "typescript-config setup failed (exit $LASTEXITCODE). Run it again from a terminal: typescript-config setup" }
}

# Downloads a release package, checks its SHA-256 and unpacks it.
function Get-ReleaseStage {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $base = if ($Version -eq 'latest') { "https://github.com/$GitHubRepo/releases/latest/download" }
            else { "https://github.com/$GitHubRepo/releases/download/$Version" }
    $stage = New-TempDirectory
    $name = 'typescript-windows.zip'
    $zip = Join-Path $stage $name
    Write-Host "Downloading Typescript ($Version)..."
    Invoke-WebRequest "$base/$name" -OutFile $zip -UseBasicParsing
    # To a file, not .Content: Windows PowerShell 5.1 returns bytes for an octet-stream.
    $sumFile = "$zip.sha256"
    Invoke-WebRequest "$base/$name.sha256" -OutFile $sumFile -UseBasicParsing
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
        Set-SettingsShortcut $false
        Write-Host 'Typescript removed. Settings stay in %APPDATA%\Typescript, language packs in %LOCALAPPDATA%\Typescript.'
    } else {
        if (Test-Path (Join-Path $Source 'packs')) { Install-Packs $Source } else { Install-ReleasePacks }
        # Per user: switch the registered profile on.
        & rundll32.exe $dll64, EnableForUser
        Set-Keyboards $true
        Set-SettingsShortcut $true
        Write-Host 'Typescript installed.'
        Write-Host '  Win+Space  -> "Typescript (autocomplete)" or "Typescript" (no autocomplete)'
        Write-Host '  Switch word language -> Esc or Han/Eng; Pause -> Alt+Han/Eng, per text field. Keys: dashboard settings, or %APPDATA%\Typescript\settings.json'
        Write-Host ('  Languages and settings: Start menu > ' + $settingsLabel + ' (opens a page in your browser), or ' + (Join-Path $dest 'typescript-config.exe'))
        Write-Host '  Apps already running pick the new text service up when you reopen them.'
    }
} finally {
    if ($stage) { Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue }
}
