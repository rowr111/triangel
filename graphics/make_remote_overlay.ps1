# Triangel IR remote overlay - L-size sticker sheet for konbini photo print.
# Full-face overlay with no cutouts: the circles mark the buttons, you press through it.
# Body and button geometry measured off the remote; button sizes are DIAMETERS.

Add-Type -AssemblyName System.Drawing

# ---- remote geometry, mm, origin at the top-left of the face, Y down ----
$BODY_W = 35.0; $BODY_H = 55.0
$INSET  = 2.0                        # sticker edge in from the body edge, so the
                                     # sticker sits 31 x 51 on a 35 x 55 face
$CORNER_R = 3.5

# name, x, y (from the TOP), diameter.
# Every Y was measured up from the BOTTOM edge, so these are 55 minus the measurement.
# Up and down come out as 38.5 and 21.5 from the bottom rather than the other way round,
# since otherwise the up arrow would sit below the down arrow.
$CX = 17.0; $CY = 24.0        # centre button, 31mm up from the bottom edge
$ARM = 8.5                    # all four arrows sit this far from the centre button
$BUTTONS = @(
    @{ n = 'gear';  x =  7.5; y =  6.0; d = 5.0  },   # 49 from the bottom
    @{ n = 'tv';    x = 27.5; y =  6.0; d = 5.0  },   # spare, deliberately unlabelled
    @{ n = 'up';    x = $CX;        y = $CY - $ARM; d = 5.0  },
    @{ n = 'left';  x = $CX - $ARM; y = $CY;        d = 5.0  },
    @{ n = 'ctr';   x = $CX;        y = $CY;        d = 10.0 },
    @{ n = 'right'; x = $CX + $ARM; y = $CY;        d = 5.0  },
    @{ n = 'down';  x = $CX;        y = $CY + $ARM; d = 5.0  }
)

$SX0 = $INSET; $SY0 = $INSET; $SX1 = $BODY_W - $INSET; $SY1 = $BODY_H - $INSET

# ---- style, matching the input plate overlay ----
$RING     = 0.4                          # button circle line width
$LAT_A    = 19.0 / [Math]::Sqrt(3)       # same tile size as the input plate overlay
$TILE_GAP = 0.45
$GRAPHIC  = Join-Path $PSScriptRoot "triangel graphic.png"
$GFX_ALPHA = 0.28

Write-Output ("remote {0} x {1} mm, sticker {2:N1} x {3:N1} mm, tile edge {4:N2}mm" -f $BODY_W, $BODY_H, ($SX1-$SX0), ($SY1-$SY0), $LAT_A)

# closest approach between any two buttons, edge to edge
$minGap = 999.0; $minPair = ''
for ($i = 0; $i -lt $BUTTONS.Count; $i++) {
    for ($j = $i + 1; $j -lt $BUTTONS.Count; $j++) {
        $a = $BUTTONS[$i]; $b = $BUTTONS[$j]
        $gap = [Math]::Sqrt([Math]::Pow($a.x - $b.x, 2) + [Math]::Pow($a.y - $b.y, 2)) - $a.d / 2 - $b.d / 2
        if ($gap -lt $minGap) { $minGap = $gap; $minPair = "$($a.n)-$($b.n)" }
    }
}
Write-Output ("closest buttons: {0} at {1:N2}mm apart" -f $minPair, $minGap)

# ---- sheet: L-size, two copies side by side ----
$DPI = 600.0
$SHEET_W = 89.0; $SHEET_H = 127.0
function MM([double]$v) { return $v * $DPI / 25.4 }
$PX_W = [int][Math]::Round((MM $SHEET_W)); $PX_H = [int][Math]::Round((MM $SHEET_H))

$bmp = New-Object System.Drawing.Bitmap($PX_W, $PX_H, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
$bmp.SetResolution($DPI, $DPI)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$g.Clear([System.Drawing.Color]::White)

$black = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::Black)
$white = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::White)
$penRing = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), ([float](MM $RING))
$family = New-Object System.Drawing.FontFamily "Arial"

function P([double]$x, [double]$y) { New-Object System.Drawing.PointF([float](MM $x), [float](MM $y)) }

# ---- pre-scale the graphic and measure its triangle, as on the input plate ----
$gfx = [System.Drawing.Image]::FromFile($GRAPHIC)
$probeN = 128
$probe = New-Object System.Drawing.Bitmap($probeN, $probeN)
$pg = [System.Drawing.Graphics]::FromImage($probe)
$pg.DrawImage($gfx, 0, 0, $probeN, $probeN); $pg.Dispose()
$mnx = $probeN; $mny = $probeN; $mxx = -1; $mxy = -1
for ($py = 0; $py -lt $probeN; $py++) {
    for ($px = 0; $px -lt $probeN; $px++) {
        $c = $probe.GetPixel($px, $py)
        if (($c.R + $c.G + $c.B) -gt 120) {
            if ($px -lt $mnx) { $mnx = $px }; if ($px -gt $mxx) { $mxx = $px }
            if ($py -lt $mny) { $mny = $py }; if ($py -gt $mxy) { $mxy = $py }
        }
    }
}
$probe.Dispose()
$fx0 = $mnx / $probeN; $fx1 = ($mxx + 1) / $probeN
$fy0 = $mny / $probeN; $fy1 = ($mxy + 1) / $probeN

$tileN = 512
$tileImg = New-Object System.Drawing.Bitmap($tileN, $tileN, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$tgc = [System.Drawing.Graphics]::FromImage($tileImg)
$tgc.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$tgc.DrawImage($gfx, 0, 0, $tileN, $tileN)
$tgc.Dispose(); $gfx.Dispose()

$cm = New-Object System.Drawing.Imaging.ColorMatrix
$cm.Matrix33 = $GFX_ALPHA
$ia = New-Object System.Drawing.Imaging.ImageAttributes
$ia.SetColorMatrix($cm)

$triW = $LAT_A - $TILE_GAP * [Math]::Sqrt(3)
$triH = $triW * [Math]::Sqrt(3) / 2.0
$S    = $triW / ($fx1 - $fx0)
$gcx  = ($fx0 + $fx1) / 2
$gcy  = $fy0 + 2.0 * ($fy1 - $fy0) / 3.0

function DrawTile([double]$ccx, [double]$ccy, [bool]$down) {
    $st = $g.Save()
    $v = New-Object 'System.Drawing.PointF[]' 3
    if ($down) {
        $v[0] = P ($ccx - $triW / 2) ($ccy - $triH / 3)
        $v[1] = P ($ccx + $triW / 2) ($ccy - $triH / 3)
        $v[2] = P $ccx               ($ccy + 2 * $triH / 3)
    } else {
        $v[0] = P $ccx               ($ccy - 2 * $triH / 3)
        $v[1] = P ($ccx + $triW / 2) ($ccy + $triH / 3)
        $v[2] = P ($ccx - $triW / 2) ($ccy + $triH / 3)
    }
    $clip = New-Object System.Drawing.Drawing2D.GraphicsPath
    $clip.AddPolygon($v)
    $g.SetClip($clip, [System.Drawing.Drawing2D.CombineMode]::Intersect)
    $clip.Dispose()
    if ($down) {
        $g.TranslateTransform([float](MM $ccx), [float](MM $ccy))
        $g.RotateTransform(180)
        $g.TranslateTransform([float](-(MM $ccx)), [float](-(MM $ccy)))
    }
    $r = New-Object System.Drawing.Rectangle(
        [int][Math]::Round((MM ($ccx - $gcx * $S))), [int][Math]::Round((MM ($ccy - $gcy * $S))),
        [int][Math]::Round((MM $S)), [int][Math]::Round((MM $S)))
    $g.DrawImage($tileImg, $r, 0, 0, $tileImg.Width, $tileImg.Height,
                 [System.Drawing.GraphicsUnit]::Pixel, $ia)
    $g.Restore($st)
}

# Solid pointer, sized to sit inside a 5mm ring with room to spare.
# A triangle's mass sits a third of the way from its base, so centring the bounding box
# leaves it looking shoved toward the base - offset by w/6 to put the centroid on centre.
function TriIcon([double]$tx, [double]$ty, [bool]$pointLeft) {
    $w = 2.4; $h = 2.5
    $ax = if ($pointLeft) { $tx - $w / 6 } else { $tx + $w / 6 }
    $pts = New-Object 'System.Drawing.PointF[]' 3
    if ($pointLeft) {
        $pts[0] = P ($ax - $w / 2) $ty
        $pts[1] = P ($ax + $w / 2) ($ty - $h / 2)
        $pts[2] = P ($ax + $w / 2) ($ty + $h / 2)
    } else {
        $pts[0] = P ($ax + $w / 2) $ty
        $pts[1] = P ($ax - $w / 2) ($ty - $h / 2)
        $pts[2] = P ($ax - $w / 2) ($ty + $h / 2)
    }
    $g.FillPolygon($white, $pts)
}

# Speaker cone plus two arcs, for the sound-mode button
function SpeakerIcon([double]$sx, [double]$sy) {
    $k = 1.25
    $pts = New-Object 'System.Drawing.PointF[]' 6
    $pts[0] = P ($sx - 1.20 * $k) ($sy - 0.35 * $k)
    $pts[1] = P ($sx - 0.70 * $k) ($sy - 0.35 * $k)
    $pts[2] = P ($sx - 0.10 * $k) ($sy - 1.00 * $k)
    $pts[3] = P ($sx - 0.10 * $k) ($sy + 1.00 * $k)
    $pts[4] = P ($sx - 0.70 * $k) ($sy + 0.35 * $k)
    $pts[5] = P ($sx - 1.20 * $k) ($sy + 0.35 * $k)
    $g.FillPolygon($white, $pts)
    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), ([float](MM (0.22 * $k)))
    foreach ($r in @(0.62, 1.02)) {
        $rr = $r * $k
        $g.DrawArc($pen, [float](MM ($sx - 0.10 * $k - $rr)), [float](MM ($sy - $rr)),
                   [float](MM (2 * $rr)), [float](MM (2 * $rr)), -52, 104)
    }
    $pen.Dispose()
}

# Disc plus eight rays. Same glyph at two sizes reads as brighter / dimmer.
function SunIcon([double]$sx, [double]$sy, [double]$scale) {
    $rc = 0.55 * $scale; $r1 = 0.90 * $scale; $r2 = 1.75 * $scale
    $wRay = [Math]::Max(0.24, 0.26 * $scale)      # keep rays thick enough to print
    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), ([float](MM $wRay))
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap   = [System.Drawing.Drawing2D.LineCap]::Round
    $g.FillEllipse($white, [float](MM ($sx - $rc)), [float](MM ($sy - $rc)),
                   [float](MM (2 * $rc)), [float](MM (2 * $rc)))
    for ($k = 0; $k -lt 8; $k++) {
        $a = $k * [Math]::PI / 4
        $g.DrawLine($pen, (P ($sx + $r1 * [Math]::Cos($a)) ($sy + $r1 * [Math]::Sin($a))),
                          (P ($sx + $r2 * [Math]::Cos($a)) ($sy + $r2 * [Math]::Sin($a))))
    }
    $pen.Dispose()
}

function Label([string]$txt, [double]$cx, [double]$cy, [double]$capMM) {
    $p  = New-Object System.Drawing.Drawing2D.GraphicsPath
    $sf = [System.Drawing.StringFormat]::GenericTypographic
    $p.AddString($txt, $family, [int][System.Drawing.FontStyle]::Bold, 200.0,
                 (New-Object System.Drawing.PointF(0, 0)), $sf)
    $b = $p.GetBounds()
    $s = (MM $capMM) / $b.Height
    $m = New-Object System.Drawing.Drawing2D.Matrix
    $m.Scale([float]$s, [float]$s); $p.Transform($m); $m.Dispose()
    $b = $p.GetBounds()
    $m2 = New-Object System.Drawing.Drawing2D.Matrix
    $m2.Translate([float]((MM $cx) - ($b.X + $b.Width / 2)), [float]((MM $cy) - ($b.Y + $b.Height / 2)))
    $p.Transform($m2); $m2.Dispose()
    $g.FillPath($white, $p); $p.Dispose()
}

# ---- draw one overlay at a sheet offset ----
function DrawOverlay([double]$ox, [double]$oy) {
    $st = $g.Save()
    $g.TranslateTransform([float](MM $ox), [float](MM $oy))

    $d = [float](MM (2 * $CORNER_R))
    $outline = New-Object System.Drawing.Drawing2D.GraphicsPath
    $outline.AddArc([float](MM $SX0), [float](MM $SY0), $d, $d, 180, 90)
    $outline.AddArc([float](MM ($SX1 - 2 * $CORNER_R)), [float](MM $SY0), $d, $d, 270, 90)
    $outline.AddArc([float](MM ($SX1 - 2 * $CORNER_R)), [float](MM ($SY1 - 2 * $CORNER_R)), $d, $d, 0, 90)
    $outline.AddArc([float](MM $SX0), [float](MM ($SY1 - 2 * $CORNER_R)), $d, $d, 90, 90)
    $outline.CloseFigure()

    $g.FillPath($black, $outline)
    $g.SetClip($outline, [System.Drawing.Drawing2D.CombineMode]::Intersect)

    $h3 = $LAT_A * [Math]::Sqrt(3) / 2.0
    $lx = 17.0; $ly = 31.0                    # a lattice vertex on the centre button
    for ($j = -9; $j -le 9; $j++) {
        # not $cy: PowerShell is case-insensitive, and that would shadow the $CY the
        # HOLD label needs further down
        $rowY = $ly + ($j + 0.5) * $h3
        if ($rowY -lt -10 -or $rowY -gt $BODY_H + 10) { continue }
        for ($i = -9; $i -le 9; $i++) {
            $cxd = $lx + ($i + 0.5) * $LAT_A + $j * $LAT_A / 2.0
            if ($cxd -gt -10 -and $cxd -lt $BODY_W + 10) { DrawTile $cxd ($rowY - $h3 / 6.0) $true }
            $cxu = $cxd + $LAT_A / 2.0
            if ($cxu -gt -10 -and $cxu -lt $BODY_W + 10) { DrawTile $cxu ($rowY + $h3 / 6.0) $false }
        }
    }
    $g.ResetClip()

    # Knock the pattern out behind every button first - the icons and HOLD need a plain
    # ground to read against.
    foreach ($b in $BUTTONS) {
        if ($b.n -eq 'tv') { continue }     # unused button: leave the pattern unbroken
        $g.FillEllipse($black,
            [float](MM ($b.x - $b.d / 2)), [float](MM ($b.y - $b.d / 2)),
            [float](MM $b.d), [float](MM $b.d))
        $g.DrawEllipse($penRing,
            [float](MM ($b.x - $b.d / 2)), [float](MM ($b.y - $b.d / 2)),
            [float](MM $b.d), [float](MM $b.d))
    }

    SunIcon $CX ($CY - $ARM) 1.00       # up    - brighter
    SunIcon $CX ($CY + $ARM) 0.55       # down  - dimmer
    TriIcon ($CX - $ARM) $CY $true      # left  - previous pattern
    TriIcon ($CX + $ARM) $CY $false     # right - next pattern
    SpeakerIcon 7.5 6.0                 # gear  - sound mode
    Label "HOLD"  $CX $CY 1.6           # inside the big centre ring
    # the TV button keeps its ring and no label

    $outline.Dispose()
    $g.Restore($st)
}

# 2 x 2 on the sheet. Rotating them buys nothing - two 51mm-wide stickers will not fit
# across an 89mm sheet - but a second row does, so four fit instead of two.
$COLS = 2; $ROWS = 2; $GAP = 6.0
$stkW = $SX1 - $SX0; $stkH = $SY1 - $SY0
$blockW = $COLS * $stkW + ($COLS - 1) * $GAP
$blockH = $ROWS * $stkH + ($ROWS - 1) * $GAP
$startX = ($SHEET_W - $blockW) / 2 - $SX0
$startY = ($SHEET_H - $blockH) / 2 - $SY0
Write-Output ("{0} x {1} on the sheet, block {2:N1} x {3:N1} mm, margins {4:N1} / {5:N1} mm" -f `
    $COLS, $ROWS, $blockW, $blockH, (($SHEET_W - $blockW) / 2), (($SHEET_H - $blockH) / 2))
for ($r = 0; $r -lt $ROWS; $r++) {
    for ($c = 0; $c -lt $COLS; $c++) {
        DrawOverlay ($startX + $c * ($stkW + $GAP)) ($startY + $r * ($stkH + $GAP))
    }
}

$out = $PSScriptRoot
$png = Join-Path $out "triangel_remote_overlay_L.png"
$jpg = Join-Path $out "triangel_remote_overlay_L.jpg"
$bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
$enc = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object { $_.MimeType -eq 'image/jpeg' }
$ep = New-Object System.Drawing.Imaging.EncoderParameters 1
$ep.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter ([System.Drawing.Imaging.Encoder]::Quality), ([long]95)
$bmp.Save($jpg, $enc, $ep)
$tileImg.Dispose(); $ia.Dispose(); $g.Dispose(); $bmp.Dispose(); $penRing.Dispose()
Write-Output "wrote $png"
Write-Output "wrote $jpg"
