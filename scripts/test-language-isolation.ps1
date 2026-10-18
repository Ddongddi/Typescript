# Exercise the installer's real edition-selection block without downloading or installing.
$ErrorActionPreference = 'Stop'
$source = Get-Content (Join-Path $PSScriptRoot 'install.ps1') -Raw
$start = $source.IndexOf('# Explicit language selection determines the edition;')
$end = $source.IndexOf('$stage = $null', $start)
if ($start -lt 0 -or $end -lt 0) { throw 'Installer selection block not found' }
$select = [scriptblock]::Create($source.Substring($start, $end - $start))
$dest = Join-Path ([IO.Path]::GetTempPath()) ('tacit-edition-test-' + [guid]::NewGuid().ToString('N'))
New-Item $dest -ItemType Directory | Out-Null

function Get-ConfiguredLanguages { return $script:Configured }
function Check-Selection($Explicit, $List, $UseJapanese, $Previous, $Configured, $Expected, $Remove = $false) {
    $Edition = $Explicit
    $Languages = $List
    $Japanese = $UseJapanese
    $Uninstall = $Remove
    $script:Configured = $Configured
    $marker = Join-Path $dest 'edition'
    if (Test-Path -LiteralPath $marker) { Remove-Item -LiteralPath $marker }
    if ($Previous) { Set-Content -LiteralPath $marker $Previous -Encoding ascii }
    $failure = $null
    try { . $select } catch { $failure = $_ }
    if ($Expected -eq 'reject') {
        if (-not $failure) { throw 'Unsupported selection was accepted' }
    } elseif ($failure -or $Edition -ne $Expected) {
        throw "Expected $Expected, got $Edition ($failure)"
    }
}

try {
    Check-Selection '' '' $false '' @() 'enko'
    Check-Selection '' 'en,ko' $false 'enkoja' @('ja') 'enko'
    Check-Selection '' 'ko,en,ja' $false '' @() 'enkoja'
    Check-Selection '' '' $true '' @() 'enkoja'
    Check-Selection '' '' $false 'enkoja' @('en', 'ko') 'enkoja'
    Check-Selection '' '' $false '' @('ja') 'enkoja'
    Check-Selection 'enko' '' $false 'enkoja' @('ja') 'enko'
    Check-Selection 'enko' 'ja' $false '' @() 'reject'
    Check-Selection '' 'fr' $false '' @() 'reject'
    Check-Selection '' '' $false 'invalid' @() 'reject'
    Check-Selection '' '' $false 'invalid' @() 'enko' $true
    Write-Host 'Installer language isolation: 11 checks passed.'
} finally {
    $marker = Join-Path $dest 'edition'
    if (Test-Path -LiteralPath $marker) { Remove-Item -LiteralPath $marker }
    Remove-Item -LiteralPath $dest
}
