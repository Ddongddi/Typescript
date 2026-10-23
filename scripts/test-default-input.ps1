# Run the installer's keyboard registration without changing the machine's settings.
$ErrorActionPreference = 'Stop'
$installer = Join-Path $PSScriptRoot 'install.ps1'
$tokens = $null
$errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($installer, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw 'Installer syntax errors' }
foreach ($node in $ast.FindAll({
    param($node)
    ($node -is [Management.Automation.Language.AssignmentStatementAst] -and $node.Left.Extent.Text -eq '$profiles') -or
    ($node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Set-Keyboards')
}, $false)) {
    . ([scriptblock]::Create($node.Extent.Text))
}

function Get-WinUserLanguageList { return ,$script:languages }
function Set-WinUserLanguageList($LanguageList, [switch]$Force, $WarningAction) {
    $script:languages = $LanguageList
}
function Set-WinDefaultInputMethodOverride([string]$InputTip) {
    $script:defaultTip = $InputTip
    $script:writes++
}

$autocomplete = "0409:$($profiles[0])"
$plain = "0409:$($profiles[1])"
$system = '0412:{A028AE76-01B1-46C2-99C4-ACD9858AE02F}{B5FE1F02-D5F2-4445-9C03-C568F23C99A1}'
$cases = @(
    @{ Name = 'first install'; Present = $true; Before = ''; After = $autocomplete; Writes = 1 },
    @{ Name = 'replace system default'; Present = $true; Before = $system; After = $autocomplete; Writes = 1 },
    @{ Name = 'keep plain profile on update'; Present = $true; Before = $plain; After = $plain; Writes = 1 },
    @{ Name = 'keep autocomplete on update'; Present = $true; Before = $autocomplete; After = $autocomplete; Writes = 1 },
    @{ Name = 'remove Tacit default'; Present = $false; Before = $plain; After = ''; Writes = 1 },
    @{ Name = 'preserve other default on uninstall'; Present = $false; Before = $system; After = $system; Writes = 0 }
)
foreach ($case in $cases) {
    $script:languages = New-WinUserLanguageList 'ko'
    if (-not $case.Present -or $case.Before -in @($autocomplete, $plain)) {
        $script:languages.Add('en-US')
        foreach ($tip in @($autocomplete, $plain)) { $script:languages[1].InputMethodTips.Add($tip) }
    }
    $script:defaultTip = $case.Before
    # Re-registration can clear a Tacit default before Set-Keyboards restores the saved value.
    if ($case.Before -in @($autocomplete, $plain)) { $script:defaultTip = '' }
    $script:writes = 0
    Set-Keyboards $case.Present $case.Before
    if ($script:defaultTip -cne $case.After -or $script:writes -ne $case.Writes) {
        throw "$($case.Name): expected '$($case.After)', got '$script:defaultTip' ($script:writes writes)"
    }
    $en = $script:languages | Where-Object { $_.LanguageTag -eq 'en-US' }
    foreach ($tip in @($autocomplete, $plain)) {
        if (($en.InputMethodTips -contains $tip) -ne $case.Present) { throw "$($case.Name): wrong keyboard list" }
    }
    if ($script:languages[0].InputMethodTips -notcontains $system) { throw 'System IME was removed' }
    Write-Host "PASS: $($case.Name)"
}
