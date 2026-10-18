<#
.SYNOPSIS
  Draws assets\tacit.ico: the white logo glyph on a dark rounded square, PNG-encoded at
  16/24/32/48/64/256 px. Run after changing the design: .\scripts\make-icon.ps1
#>
Add-Type -AssemblyName System.Drawing
$ErrorActionPreference = 'Stop'
$assets = Join-Path (Split-Path -Parent $PSScriptRoot) 'assets'
New-Item $assets -ItemType Directory -Force | Out-Null

# The logo, from its SVG (viewBox 0 0 45 54):
#   M0 9.24863V0H16.8465V9.24863L28.1535 0L45 0V9.24863H28.1535V45L16.8465 54V9.24863H0Z
$logo = @(
    @(0, 9.24863), @(0, 0), @(16.8465, 0), @(16.8465, 9.24863), @(28.1535, 0), @(45, 0),
    @(45, 9.24863), @(28.1535, 9.24863), @(28.1535, 45), @(16.8465, 54), @(16.8465, 9.24863)
)
$logoWidth = 45.0
$logoHeight = 54.0
$background = [System.Drawing.Color]::FromArgb(255, 24, 24, 27)

function New-Png([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.Clear([System.Drawing.Color]::Transparent)

    $r = [int]($size * 0.22)
    $d = $r * 2
    $card = New-Object System.Drawing.Drawing2D.GraphicsPath
    $card.AddArc(0, 0, $d, $d, 180, 90)
    $card.AddArc($size - $d - 1, 0, $d, $d, 270, 90)
    $card.AddArc($size - $d - 1, $size - $d - 1, $d, $d, 0, 90)
    $card.AddArc(0, $size - $d - 1, $d, $d, 90, 90)
    $card.CloseFigure()
    $g.FillPath((New-Object System.Drawing.SolidBrush $background), $card)

    # The glyph fills about 60% of the height, centered.
    $scale = ($size * 0.60) / $logoHeight
    $ox = ($size - $logoWidth * $scale) / 2
    $oy = ($size - $logoHeight * $scale) / 2
    $points = foreach ($p in $logo) { New-Object System.Drawing.PointF ([float]($ox + $p[0] * $scale)), ([float]($oy + $p[1] * $scale)) }
    $g.FillPolygon([System.Drawing.Brushes]::White, [System.Drawing.PointF[]]$points, [System.Drawing.Drawing2D.FillMode]::Winding)
    $g.Dispose()

    $stream = New-Object System.IO.MemoryStream
    $bmp.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    , $stream.ToArray()
}

$sizes = 16, 24, 32, 48, 64, 256
$images = foreach ($s in $sizes) { , (New-Png $s) }

# ICONDIR + one ICONDIRENTRY per image (PNG payloads), then the images.
$file = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $file
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $dim = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([uint16]1); $w.Write([uint16]32)
    $w.Write([uint32]$images[$i].Length); $w.Write([uint32]$offset)
    $offset += $images[$i].Length
}
foreach ($img in $images) { $w.Write($img) }
$w.Flush()
$target = Join-Path $assets 'tacit.ico'
[System.IO.File]::WriteAllBytes($target, $file.ToArray())
# A 256 px preview of the same drawing, handy for checking the design.
[System.IO.File]::WriteAllBytes((Join-Path $assets 'icon.png'), $images[-1])
Write-Host "wrote $target"
