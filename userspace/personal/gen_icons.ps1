# Генератор иконок — 48x48 raw BGRA 32bpp
# Каждая иконка рисуется на прозрачном фоне (альфа=0)
$S = 48

function New-BlankIcon {
    $data = [byte[]]::new($S * $S * 4)
    return $data
}

function Set-Pixel($data, $x, $y, $r, $g, $b, $a) {
    if ($x -lt 0 -or $x -ge $S -or $y -lt 0 -or $y -ge $S) { return }
    $off = ($y * $S + $x) * 4
    $data[$off] = $b
    $data[$off+1] = $g
    $data[$off+2] = $r
    $data[$off+3] = $a
}

function Fill-Rect($data, $x1, $y1, $x2, $y2, $r, $g, $b) {
    for ($y = $y1; $y -le $y2; $y++) {
        for ($x = $x1; $x -le $x2; $x++) {
            Set-Pixel $data $x $y $r $g $b 255
        }
    }
}

function Fill-Circle($data, $cx, $cy, $radius, $r, $g, $b) {
    for ($y = ($cy - $radius); $y -le ($cy + $radius); $y++) {
        for ($x = ($cx - $radius); $x -le ($cx + $radius); $x++) {
            $dx = $x - $cx
            $dy = $y - $cy
            if ($dx*$dx + $dy*$dy -le $radius*$radius) {
                Set-Pixel $data $x $y $r $g $b 255
            }
        }
    }
}

# ── Explorer (Folder) ──
$d = New-BlankIcon
# folder body
Fill-Rect $d 8 16 40 42 200 180 120
# folder tab
Fill-Rect $d 8 12 24 16 200 180 120
# folder highlight (top edge)
Fill-Rect $d 8 14 40 14 230 210 160
# folder inner
Fill-Rect $d 10 18 38 40 240 230 190
[System.IO.File]::WriteAllBytes("$PSScriptRoot\explorer.bin", $d)

# ── Paint (Palette) ──
$d = New-BlankIcon
# palette body
Fill-Circle $d 24 24 18 220 180 160
# palette inner
Fill-Circle $d 24 24 14 255 230 210
# paint blob
Fill-Circle $d 24 24 10 255 100 100
# color circles
Fill-Circle $d 14 18 3 255 80 80
Fill-Circle $d 34 18 3 80 255 80
Fill-Circle $d 24 34 3 80 80 255
Fill-Circle $d 34 30 3 255 255 80
[System.IO.File]::WriteAllBytes("$PSScriptRoot\paint.bin", $d)

# ── Snake (S-shape) ──
$d = New-BlankIcon
# S-curve body
for ($i = 0; $i -lt 5; $i++) {
    $x = 16 + $i*4
    $y = 12 + $i*4
    Fill-Circle $d $x $y 3 80 220 120
}
for ($i = 0; $i -lt 4; $i++) {
    $x = 20 + $i*4
    $y = 28 + $i*4
    Fill-Circle $d $x $y 3 80 220 120
}
# eyes
Set-Pixel $d 15 11 255 255 255 255
Set-Pixel $d 15 11 255 255 255 255
Set-Pixel $d 18 11 255 255 255 255
Set-Pixel $d 15 12 0 0 0 255
Set-Pixel $d 18 12 0 0 0 255
[System.IO.File]::WriteAllBytes("$PSScriptRoot\snake.bin", $d)

# ── Settings (Gear) ──
$d = New-BlankIcon
# outer gear ring
for ($a = 0; $a -lt 8; $a++) {
    $angle = $a * [Math]::PI / 4
    $ox = [Math]::Round(24 + [Math]::Cos($angle) * 14)
    $oy = [Math]::Round(24 + [Math]::Sin($angle) * 14)
    Fill-Circle $d $ox $oy 4 160 160 220
}
Fill-Circle $d 24 24 8 160 160 220
Fill-Circle $d 24 24 5 220 220 250
[System.IO.File]::WriteAllBytes("$PSScriptRoot\settingsicon.bin", $d)

# ── Reboot (circular arrow) ──
$d = New-BlankIcon
# circle
for ($a = 0; $a -lt 36; $a++) {
    $angle = $a * [Math]::PI / 18
    $x = [Math]::Round(24 + [Math]::Cos($angle) * 15)
    $y = [Math]::Round(24 + [Math]::Sin($angle) * 15)
    Set-Pixel $d $x $y 200 160 120 255
}
# arrow head at top
Fill-Rect $d 22 6 26 12 200 160 120
Set-Pixel $d 19 8 200 160 120 255
Set-Pixel $d 20 9 200 160 120 255
Set-Pixel $d 21 10 200 160 120 255
Set-Pixel $d 21 11 200 160 120 255
# spinner arc highlight (right side)
for ($a = -4; $a -lt 4; $a++) {
    $angle = $a * [Math]::PI / 18
    $x = [Math]::Round(24 + [Math]::Cos($angle) * 15)
    $y = [Math]::Round(24 + [Math]::Sin($angle) * 15)
    Set-Pixel $d $x $y 240 210 170 255
}
[System.IO.File]::WriteAllBytes("$PSScriptRoot\rebooticon.bin", $d)

# ── Shutdown (power symbol) ──
$d = New-BlankIcon
# vertical bar
Fill-Rect $d 22 6 26 26 240 120 120
# circle arc
for ($a = -8; $a -lt 9; $a++) {
    $angle = $a * [Math]::PI / 18
    $x = [Math]::Round(24 + [Math]::Cos($angle) * 15)
    $y = [Math]::Round(24 + [Math]::Sin($angle) * 15)
    Set-Pixel $d $x $y 240 120 120 255
}
# bottom half of circle
for ($a = -18; $a -le 18; $a++) {
    $angle = $a * [Math]::PI / 18
    $x = [Math]::Round(24 + [Math]::Cos($angle) * 15)
    $y = [Math]::Round(24 + [Math]::Sin($angle) * 15)
    if ($y -ge 24) {
        Set-Pixel $d $x $y 240 120 120 255
    }
}
[System.IO.File]::WriteAllBytes("$PSScriptRoot\shutdown.bin", $d)

Write-Host "Icons generated!"
