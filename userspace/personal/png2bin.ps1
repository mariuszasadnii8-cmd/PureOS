param(
    [Parameter(Mandatory=$true)][string]$InputPng,
    [Parameter(Mandatory=$true)][string]$OutputBin
)

Add-Type -AssemblyName System.Drawing

$img = [System.Drawing.Image]::FromFile($InputPng)
$bmp = New-Object System.Drawing.Bitmap($img)
$w = $bmp.Width
$h = $bmp.Height
$bytes = [byte[]]::new($w * $h * 4)
for ($y = 0; $y -lt $h; $y++) {
    for ($x = 0; $x -lt $w; $x++) {
        $c = $bmp.GetPixel($x, $y)
        $off = ($y * $w + $x) * 4
        $bytes[$off]     = $c.B  # BGRA
        $bytes[$off + 1] = $c.G
        $bytes[$off + 2] = $c.R
        $bytes[$off + 3] = $c.A
    }
}
[System.IO.File]::WriteAllBytes($OutputBin, $bytes)
$bmp.Dispose()
$img.Dispose()
Write-Host "$w`x$h -> $OutputBin ($($bytes.Length) bytes)"
