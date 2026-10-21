$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class Issue9Layout {
    [DllImport("user32.dll")] public static extern int GetKeyboardLayoutList(int count, [Out] IntPtr[] layouts);
    [DllImport("user32.dll")] public static extern uint MapVirtualKeyExW(uint code, uint mode, IntPtr layout);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] public static extern IntPtr GetKeyboardLayout(uint thread);
}
'@
$layoutCount = [Issue9Layout]::GetKeyboardLayoutList(0, $null)
if ($layoutCount -lt 1) { throw 'No keyboard layouts available' }
$layoutList = New-Object IntPtr[] $layoutCount
[void][Issue9Layout]::GetKeyboardLayoutList($layoutCount, $layoutList)
$mapping = @(foreach ($layout in $layoutList) {
    if ([Issue9Layout]::MapVirtualKeyExW(0x3B, 3, $layout) -ne 0x70) { throw 'F1 mapping self-check failed' }
    foreach ($scan in @(0x38, 0xE038, 0x72, 0xF2)) {
        [pscustomobject]@{
            Layout = ('{0:X16}' -f $layout.ToInt64())
            Scan = ('{0:X}' -f $scan)
            VirtualKey = ('{0:X}' -f [Issue9Layout]::MapVirtualKeyExW($scan, 3, $layout))
        }
    }
})
$mapping | ConvertTo-Json -Compress
[uint32]$foregroundProcess = 0
$foregroundThread = [Issue9Layout]::GetWindowThreadProcessId([Issue9Layout]::GetForegroundWindow(), [ref]$foregroundProcess)
[pscustomobject]@{ ForegroundLayout = ('{0:X16}' -f [Issue9Layout]::GetKeyboardLayout($foregroundThread).ToInt64()) } | ConvertTo-Json -Compress
Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\i8042prt\Parameters' -ErrorAction SilentlyContinue |
    Select-Object OverrideKeyboardType, OverrideKeyboardSubtype, LayerDriverKOR | ConvertTo-Json -Compress
