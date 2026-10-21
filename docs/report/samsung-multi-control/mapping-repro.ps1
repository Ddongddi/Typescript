$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'layout-probe.ps1')
$binary = 'C:\Program Files\WindowsApps\SAMSUNGELECTRONICSCoLtd.MultiControl_2.7.1300.0_x64__wyx1vj98g3asy\SamsungMultiControl\Binary\SamsungMultiControl.exe'
$expectedHash = '6AB6A82EBBAF5F85511E5638897FF590DA9B10DC760A3E137A0A0944A7C73A7E'
if ((Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash -ne $expectedHash) { throw 'Samsung binary differs from the statically inspected version' }
# Only this pure dictionary/conversion helper is invoked; no hooks or input handlers run.
$assembly = [Reflection.Assembly]::LoadFrom($binary)
$converter = $assembly.GetType('MultiControlCore.Hook.KeyboardConverter', $true)
$method = $converter.GetMethod('MapScanCode', [Reflection.BindingFlags]'Public,Static')
$keyType = $method.GetParameters()[0].ParameterType
$protocol = [Reflection.Assembly]::LoadFrom((Join-Path (Split-Path $binary) 'Samsung.MultiControl.Protocol.dll'))
$wireType = $protocol.GetType('Samsung.MultiControl.Protocol.ScanCodeMap', $true)
foreach ($number in @(56, 100, 122, 123)) {
    [pscustomobject]@{ WireKey = $number; Name = [Enum]::GetName($wireType, $number) } | ConvertTo-Json -Compress
}
$results = @(foreach ($layout in $layoutList) {
    $language = $layout.ToInt64() -band 0xFFFF
    if ($language -notin @(0x409, 0x412)) { continue }
    $vk = [Issue9Layout]::MapVirtualKeyExW(0xE038, 3, $layout)
    $wire = $method.Invoke($null, @([Enum]::ToObject($keyType, $vk), [int]0x38, $true))
    [pscustomobject]@{
        Language = ('{0:X4}' -f $language)
        Scan = 'E038'
        WindowsVirtualKey = ('{0:X2}' -f $vk)
        SamsungWireKey = $wire
    }
})
$results | ConvertTo-Json -Compress
$korean = $results | Where-Object Language -eq '0412'
$english = $results | Where-Object Language -eq '0409'
if (-not $korean -or -not $english) { throw 'Both loaded Korean and US layouts are needed' }
if ($korean.SamsungWireKey -ne 122 -or $english.SamsungWireKey -ne 56) { throw 'Measured mapping differs from inspected IL' }
if ($english.SamsungWireKey -ne $korean.SamsungWireKey) {
    throw 'REPRODUCED: the same physical key loses its Samsung Language mapping under the US layout (122 -> 56/LeftMenu).'
}
