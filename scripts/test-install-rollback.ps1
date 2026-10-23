# Exercise the elevated installer's real file transaction in a temporary folder.
$ErrorActionPreference = 'Stop'
$source = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'install.ps1')
$body = [regex]::Match($source, "(?s)\`$elevatedScript = @'\r?\n(.*?)\r?\n'@").Groups[1].Value
if (-not $body) { throw 'Elevated installer body not found' }
$tokens = $null
$errors = $null
$ast = [Management.Automation.Language.Parser]::ParseInput($body, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($node in $ast.FindAll({
    param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and
        $node.Name -in @('Move-Aside', 'Unregister-Existing', 'Install-Staged')
}, $false)) { . ([scriptblock]::Create($node.Extent.Text)) }

function Invoke-Regsvr($exe, $dll, [switch]$Unregister) {
    if ($Unregister) { return }
    $contents = Get-Content -Raw -LiteralPath $dll
    if ($script:failure -eq 'registration' -and $dll -eq $dll86 -and $contents -like 'new-*') {
        throw 'injected registration failure'
    }
    $script:registered += $contents
}
function icacls.exe { $global:LASTEXITCODE = 0 }
function Move-Item($LiteralPath, $Destination, [switch]$Force) {
    if ($script:failure -eq 'copy' -and $LiteralPath -eq (Join-Path $incoming 'tacit_tip_x86.dll')) {
        throw 'injected replacement failure'
    }
    Microsoft.PowerShell.Management\Move-Item -LiteralPath $LiteralPath -Destination $Destination -Force:$Force
}

$root = Join-Path ([IO.Path]::GetTempPath()) ('tacit-rollback-' + [guid]::NewGuid().ToString('N'))
$required = @('tacit_tip.dll', 'tacit_tip_x86.dll', 'tacit-config.exe')
try {
    foreach ($scenario in @('success', 'copy', 'registration', 'missing', 'first-install')) {
        $dest = Join-Path $root $scenario
        $incoming = Join-Path $dest 'incoming'
        New-Item $incoming -ItemType Directory -Force | Out-Null
        $dll64 = Join-Path $dest 'tacit_tip.dll'
        $dll86 = Join-Path $dest 'tacit_tip_x86.dll'
        $regsvr64 = 'stub-64'
        $regsvr86 = 'stub-86'
        foreach ($name in $required) {
            if ($scenario -ne 'first-install') { [IO.File]::WriteAllText((Join-Path $dest $name), "old-$name") }
            if ($scenario -ne 'missing' -or $name -ne 'tacit-config.exe') {
                [IO.File]::WriteAllText((Join-Path $incoming $name), "new-$name")
            }
        }
        [IO.File]::WriteAllText((Join-Path $dest 'untouched.txt'), 'keep')
        $script:registered = @()
        $script:failure = if ($scenario -eq 'first-install') { 'registration' } else { $scenario }
        $failed = $false
        try { Install-Staged $incoming } catch { $failed = $true }
        if ($failed -ne ($scenario -ne 'success')) { throw "Wrong verdict for $scenario" }
        foreach ($name in $required) {
            $target = Join-Path $dest $name
            if ($scenario -eq 'first-install') {
                if (Test-Path -LiteralPath $target) { throw "Failed first install left $name active" }
            } else {
                $expected = if ($scenario -eq 'success') { "new-$name" } else { "old-$name" }
                if ([IO.File]::ReadAllText($target) -ne $expected) { throw "$scenario did not restore $name" }
            }
        }
        if ($scenario -in @('copy', 'registration')) {
            foreach ($name in @('tacit_tip.dll', 'tacit_tip_x86.dll')) {
                if ($script:registered -notcontains "old-$name") { throw "$scenario did not re-register $name" }
            }
        }
        if ([IO.File]::ReadAllText((Join-Path $dest 'untouched.txt')) -ne 'keep') { throw 'Unrelated file changed' }
        Write-Host "PASS: $scenario"
    }
} finally {
    if (Test-Path -LiteralPath $root) {
        $resolved = (Resolve-Path -LiteralPath $root).Path
        if ((Split-Path $resolved -Parent) -ne ([IO.Path]::GetTempPath()).TrimEnd('\')) { throw 'Unexpected test cleanup path' }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
